//! The bot asks one of its working tasks a question and gets the answer (`ask_task`): the question goes into the
//! thread's running turn as `tell_task`'s `now` does, marked as the bot asking; the thread's next message after it
//! (`answered`, from its actor as the message ends) is the call's result, not its report. The call waits [`WAIT`];
//! after that it says there is no answer yet, and the answer, when it comes, is a turn of its own in the bot's chat
//! (`Cmd::TaskAnswer`), as a `message_bot` reply is. Refused for a task that waits or ended, a thread the user started,
//! a routine's run and another bot's request; one question at a time a task.

use super::tasks::{self, Status};
use super::thread::{self, Refused};
use super::{MAX_BRIEF, prompts};
use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::analytics::{self, Event};
use crate::hub::Hub;
use crate::store::BotConfig;
use anyhow::Result;
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;

/// The tool's name.
pub const TOOL: &str = "ask_task";
/// How long the bot's call waits for the thread's answer.
pub const WAIT: Duration = Duration::from_mins(1);

/// A question a thread was asked and hasn't answered yet (`Work::asks`, by thread).
pub(super) struct Pending {
    bot: String,
    task: String,
    title: String,
    question: String,
    /// The task's hops: a task the bot adds in the turn a late answer starts carries them on.
    hops: u8,
    /// It reached the thread's agent: the thread's next message is its answer.
    heard: bool,
    /// The bot's call, while it still waits for the answer.
    waiter: Option<oneshot::Sender<String>>,
}

/// How a question came out: `task_asked`'s `outcome`.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum Outcome {
    Answered,
    /// Not within [`WAIT`] (or the task ended first): any answer comes later, as a turn of its own.
    Late,
    Refused,
}

fn counted(hub: &Hub, outcome: Outcome) {
    let properties = json!({"outcome": outcome});
    analytics::capture(hub, Event::TaskAsked, properties.as_object().cloned().unwrap_or_default());
}

/// `ask_task {task, question}` from `bot`'s chat: `{taskId, answer}`, or `{taskId, status: "no answer yet"}`.
pub(super) async fn ask(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value> {
    let (h, b, a) = (hub.clone(), bot.to_owned(), args.clone());
    let put = tokio::task::spawn_blocking(move || put(&h, &b, &a)).await?;
    let (task, thread, answer) = put.inspect_err(|_| counted(hub, Outcome::Refused))?;
    let answer = wait(hub, &thread, answer).await;
    counted(hub, if answer.is_some() { Outcome::Answered } else { Outcome::Late });
    Ok(match answer {
        Some(answer) => json!({"taskId": task, "answer": answer}),
        None => json!({"taskId": task, "status": "no answer yet",
            "note": "Its answer comes to you later as a message of its own, if it answers before its task ends."}),
    })
}

/// Puts the question to the thread of the task `args` names: the task's id, its thread's, and where the answer comes.
fn put(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<(String, String, oneshot::Receiver<String>)> {
    let text = |key: &str| args[key].as_str().map(str::trim).filter(|t| !t.is_empty());
    let invalid = |why: String| Refused::error("invalid", why);
    let id = text("task").ok_or_else(|| invalid("task is required: its id, from work_status".into()))?;
    let question = text("question").ok_or_else(|| invalid("question is required".into()))?;
    if question.len() > MAX_BRIEF {
        return Err(invalid(format!("question is at most {MAX_BRIEF} bytes")));
    }
    super::tools::the_bots(hub, bot, id)?;
    let (rx, thread, entry, queued) = tasks::change(hub, bot, |tasks| {
        let task = tasks::find(tasks, id)?;
        thread::writable(task)?;
        let title = &task.title;
        match task.status {
            Status::Working => {}
            Status::Waiting => {
                let why =
                    format!("\"{title}\" hasn't started yet: nothing can answer. tell_task passes words on to it");
                return Err(Refused::error("not_started", why));
            }
            _ => return Err(Refused::error("ended", format!("\"{title}\" has ended: its report came to you already"))),
        }
        let thread = thread::thread_of(&hub.store, task)?;
        let (tx, rx) = oneshot::channel();
        {
            let mut asks = hub.work.asks.locked();
            // A call still waits for the answer (one the bot's stopped turn dropped doesn't).
            if asks.get(&thread.id).is_some_and(|p| p.waiter.as_ref().is_some_and(|w| !w.is_closed())) {
                return Err(Refused::error("busy", format!("\"{title}\" has a question of yours to answer already")));
            }
            let pending = Pending {
                bot: bot.to_owned(),
                task: task.id.clone(),
                title: title.clone(),
                question: question.to_owned(),
                hops: task.hops,
                heard: false,
                waiter: Some(tx),
            };
            asks.insert(thread.id.clone(), pending);
        }
        // Never kept with the task (`relayed`): a session that starts it again would read it as an instruction.
        let posted =
            thread::post_to(hub, &thread, json!({"text": question, "from": "host", "relay": true, "ask": true}));
        let (entry, queued) = posted.inspect_err(|_| drop(hub.work.asks.locked().remove(&thread.id)))?;
        Ok((rx, thread.id, entry.id, queued))
    })?;
    if queued {
        hub.send_cmd(&thread, Cmd::Steer { entry_id: entry })?;
    }
    Ok((id.to_owned(), thread, rx))
}

/// The thread's answer, if it comes within [`WAIT`]; past that the question stays, and its answer is late.
async fn wait(hub: &Hub, thread: &str, mut answer: oneshot::Receiver<String>) -> Option<String> {
    if let Ok(answered) = tokio::time::timeout(WAIT, &mut answer).await {
        return answered.ok();
    }
    // From here an answer goes to the bot's chat; one sent a moment ago is still this call's.
    if let Some(pending) = hub.work.asks.locked().get_mut(thread) {
        pending.waiter = None;
    }
    answer.try_recv().ok()
}

/// The entries `entries` reached thread `cfg`'s agent (its turn began with them, or one joined it): a question of its
/// bot's among them makes its next message the answer.
pub fn heard(hub: &Hub, cfg: &BotConfig, entries: &[String]) {
    mark(hub, cfg, entries, true);
}

/// The question in `entry` didn't join the running turn after all: it waits for a turn of its own.
pub fn unheard(hub: &Hub, cfg: &BotConfig, entry: &str) {
    mark(hub, cfg, &[entry.to_owned()], false);
}

fn mark(hub: &Hub, cfg: &BotConfig, entries: &[String], heard: bool) {
    if !cfg.is_thread() || !hub.work.asks.locked().contains_key(&cfg.id) {
        return;
    }
    if entries.iter().any(|id| hub.store.entry(id).is_some_and(|e| e.data["ask"] == true))
        && let Some(pending) = hub.work.asks.locked().get_mut(&cfg.id)
    {
        pending.heard = heard;
    }
}

/// Thread `cfg` wrote `text`, a message of its turn: when it answers its bot's question, the answer goes to the bot
/// (its call, or a turn of its own once the call gave up). Whether it did: then it isn't the turn's report.
pub fn answered(hub: &Hub, cfg: &BotConfig, text: &str) -> bool {
    let pending = {
        let mut asks = hub.work.asks.locked();
        if !asks.get(&cfg.id).is_some_and(|p| p.heard) {
            return false;
        }
        let Some(mut pending) = asks.remove(&cfg.id) else { return false };
        // Sent under the lock, so the call that gives up either gets it or leaves it to its bot's chat (`wait`).
        match pending.waiter.take().map(|waiter| waiter.send(text.trim().to_owned())) {
            Some(Ok(())) => return true,
            Some(Err(_)) | None => pending,
        }
    };
    let prompt = prompts::late_answer(&pending.title, &pending.task, &pending.question, text.trim());
    if let Err(error) = hub.send_cmd(&pending.bot, Cmd::TaskAnswer { prompt, hops: pending.hops }) {
        tracing::warn!(bot = pending.bot, error = format!("{error:#}"), "couldn't give a bot its thread's late answer");
    }
    true
}

/// `thread`'s task ended or was set aside: a question it hadn't answered goes, and a call still waiting hears so.
pub(super) fn forget(hub: &Hub, thread: &str) {
    hub.work.asks.locked().remove(thread);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::calls::context::tests::{bot, hub};
    use crate::store::{EntryKind, Lane};

    fn asked(hub: &Hub, thread: &str) -> (oneshot::Receiver<String>, String) {
        let (tx, rx) = oneshot::channel();
        let pending = Pending {
            bot: "b1".into(),
            task: "i1".into(),
            title: "Docs".into(),
            question: "how far?".into(),
            hops: 0,
            heard: false,
            waiter: Some(tx),
        };
        hub.work.asks.locked().insert(thread.into(), pending);
        let data = json!({"text": "how far?", "from": "host", "relay": true, "ask": true});
        let entry = hub.add_entry(&Lane::main(thread), EntryKind::User, 1, &data).expect("posted");
        (rx, entry.id)
    }

    #[tokio::test]
    async fn the_threads_next_message_after_the_question_is_the_answer() {
        let hub = hub();
        let thread = BotConfig { thread_of: Some("b1".into()), ..bot("thread-1", "Mika", "/tmp") };
        hub.store.save_bot(&thread).expect("saved");
        let (mut rx, entry) = asked(&hub, "thread-1");
        assert!(!answered(&hub, &thread, "still reading"), "before the question reached it");
        heard(&hub, &thread, std::slice::from_ref(&entry));
        unheard(&hub, &thread, &entry);
        assert!(!answered(&hub, &thread, "a steer it didn't take"));
        heard(&hub, &thread, &[entry]);
        assert!(answered(&hub, &thread, " Half done. "));
        assert_eq!(rx.try_recv().expect("the answer"), "Half done.");
        assert!(!answered(&hub, &thread, "the report"), "one message answers it");
    }

    #[test]
    fn a_question_is_only_for_the_bots_own_working_task() {
        let hub = hub();
        hub.store.save_bot(&bot("b1", "Mika", "/tmp")).expect("saved");
        let tasks = json!([
            {"id": "w", "title": "Docs", "brief": "x", "status": "waiting", "order": 1, "createdAt": 0},
            {"id": "d", "title": "Login", "brief": "x", "status": "done", "order": 0, "createdAt": 0, "endedAt": 1},
            {"id": "u", "title": "Mine", "brief": "x", "status": "working", "order": 2, "createdAt": 0,
                "thread": "thread-u", "source": {"kind": "user"}},
            {"id": "r", "title": "Morning", "brief": "x", "status": "working", "order": 3, "createdAt": 0,
                "thread": "thread-r", "source": {"kind": "routine", "id": "r", "run": "u"}},
        ]);
        hub.store.kv_set(&format!("{}b1", tasks::PREFIX), &tasks.to_string()).expect("saved");
        let category = |task: &str, question: &str| {
            let error = put(&hub, "b1", &json!({"task": task, "question": question})).expect_err("refused");
            error.downcast_ref::<Refused>().map(|refused| refused.category)
        };
        assert_eq!(category("w", "how far?"), Some("not_started"));
        assert_eq!(category("d", "how far?"), Some("ended"));
        assert_eq!(category("u", "how far?"), Some("users"));
        assert_eq!(category("r", "how far?"), Some("read_only"));
        assert_eq!(category("w", " "), Some("invalid"));
        assert!(hub.work.asks.locked().is_empty(), "nothing waits for an answer");
    }

    #[tokio::test]
    async fn a_call_that_gave_up_leaves_the_answer_to_the_bots_chat() {
        let hub = hub();
        let thread = BotConfig { thread_of: Some("b1".into()), ..bot("thread-1", "Mika", "/tmp") };
        let (rx, entry) = asked(&hub, "thread-1");
        // As `wait` does past its time.
        hub.work.asks.locked().get_mut("thread-1").expect("pending").waiter = None;
        drop(rx);
        heard(&hub, &thread, &[entry]);
        assert!(answered(&hub, &thread, "Done soon."), "late, to the bot's chat");
        assert!(!hub.work.asks.locked().contains_key("thread-1"));
        let (rx, _) = asked(&hub, "thread-1");
        forget(&hub, "thread-1");
        let mut rx = rx;
        assert!(rx.try_recv().is_err(), "its task ended: the waiting call hears so");
    }
}
