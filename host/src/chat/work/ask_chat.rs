//! A task's thread asks the bot that handed it out a question and waits for the answer (`ask_chat`), `ask_task`
//! the other way round: the question is a turn of its own in the bot's chat (`Cmd::TaskAnswer`, as a late answer
//! is), which answers it with `answer_task` from what it knows. What only the user can decide, the chat says so and
//! the thread asks them itself (`request_decision`): a host-started turn can't put a card in front of them. The
//! thread's call waits [`WAIT`]; past that it goes on without, and a later `answer_task` is passed on to it as
//! `tell_task`'s `now`. Only for the bot's own tasks; one question at a time a thread.

use super::tasks::{self, Status};
use super::thread::{self, Refused, Told};
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

/// The thread's tool.
pub const TOOL: &str = "ask_chat";
/// The bot's tool that answers it.
pub const ANSWER: &str = "answer_task";
/// How long the thread's call waits: under Codex's 300 s tool timeout, as `request_decision`'s
/// (docs/upstream-agents/codex-acp.md).
pub const WAIT: Duration = Duration::from_secs(270);

/// A thread's question its bot hasn't answered yet (`Work::questions`, by thread).
pub(super) struct Waiting {
    task: String,
    waiter: oneshot::Sender<String>,
}

/// How a question came out: `chat_asked`'s `outcome`.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum Outcome {
    Answered,
    /// Not within [`WAIT`], or the task ended first.
    Unanswered,
    Refused,
}

fn counted(hub: &Hub, outcome: Outcome) {
    let properties = json!({"outcome": outcome});
    analytics::capture(hub, Event::ChatAsked, properties.as_object().cloned().unwrap_or_default());
}

pub fn tool() -> Value {
    json!({
        "name": TOOL,
        "description": "Ask the bot that gave you this task (your chat) a question while you work, and get its answer: what it or the user meant, a choice within the task, a fact you can't find yourself. Use it instead of stopping or guessing on something that matters: your chat answers from what it knows, and when only the user can decide, it says so and you ask them with request_decision. Waits up to four and a half minutes; without an answer by then you get answered: false, so go on with what you can and say in your report what you assumed (a later answer comes to you as a message from your chat). Only in a task your chat handed out.",
        "inputSchema": {"type": "object", "properties": {
            "question": {"type": "string", "description": "What to ask, standing alone: your chat sees only this and the task's title."},
        }, "required": ["question"]},
        "annotations": {"readOnlyHint": false, "idempotentHint": false},
    })
}

pub fn answer_tool() -> Value {
    json!({
        "name": ANSWER,
        "description": "Answer the question one of your working tasks asked you (ask_chat): it waits for this and goes on with it. Answer from what you know (this chat, your memory, the files); what only the user can decide (a permission, their own preference), say so in your answer, and the task asks them itself. An answer after the task stopped waiting is passed on to it as tell_task's now.",
        "inputSchema": {"type": "object", "properties": {
            "task": {"type": "string", "description": "The id of the task that asked."},
            "answer": {"type": "string", "description": "Your answer, as the task should read it."},
        }, "required": ["task", "answer"]},
        "annotations": {"readOnlyHint": false, "idempotentHint": false},
    })
}

/// `ask_chat {question}` from `thread`: `{answered: true, answer}`, or `{answered: false, note}`.
pub async fn ask(hub: &Arc<Hub>, thread: &str, args: &Value) -> Result<Value> {
    let (h, t, a) = (hub.clone(), thread.to_owned(), args.clone());
    let put = tokio::task::spawn_blocking(move || put(&h, &t, &a)).await?;
    let answer = put.inspect_err(|_| counted(hub, Outcome::Refused))?;
    let answer = wait(hub, thread, answer).await;
    counted(hub, if answer.is_some() { Outcome::Answered } else { Outcome::Unanswered });
    Ok(match answer {
        Some(answer) => json!({"answered": true, "answer": answer}),
        None => json!({"answered": false, "note": "Your chat hasn't answered yet. Go on with what you can and say in \
            your report what you assumed; if it answers later, that comes to you as a message from your chat."}),
    })
}

/// Puts `thread`'s question to its bot's chat: where the answer comes.
fn put(hub: &Hub, thread: &str, args: &Value) -> Result<oneshot::Receiver<String>> {
    let invalid = |why: &str| Refused::error("invalid", why.to_owned());
    let question = args["question"].as_str().map(str::trim).filter(|q| !q.is_empty());
    let question = question.ok_or_else(|| invalid("question is required"))?;
    if question.len() > MAX_BRIEF {
        return Err(invalid(&format!("question is at most {MAX_BRIEF} bytes")));
    }
    let cfg = hub.store.bot(thread)?.map(|row| row.config).filter(BotConfig::is_thread);
    let task = cfg.as_ref().and_then(|cfg| super::task_of(&hub.store, cfg)).filter(|task| task.source.is_bot());
    let (Some(cfg), Some(task)) = (cfg, task) else {
        let why = "ask_chat is for a task your chat handed out: here, say what you need in your message, or call \
                   request_decision when the user must choose";
        return Err(Refused::error("not_handed_out", why.to_owned()));
    };
    let (tx, rx) = oneshot::channel();
    {
        let mut questions = hub.work.questions.locked();
        // A call still waits for the answer (one its stopped turn dropped doesn't).
        if questions.get(thread).is_some_and(|w| !w.waiter.is_closed()) {
            return Err(Refused::error("busy", "a question of yours waits for its answer already".to_owned()));
        }
        questions.insert(thread.to_owned(), Waiting { task: task.id.clone(), waiter: tx });
    }
    let prompt = prompts::question(&task.title, &task.id, question);
    hub.send_cmd(cfg.owner(), Cmd::TaskAnswer { prompt, hops: task.hops })
        .inspect_err(|_| drop(hub.work.questions.locked().remove(thread)))?;
    Ok(rx)
}

/// The bot's answer, if it comes within [`WAIT`]; past that the question goes, and a later answer is told.
async fn wait(hub: &Hub, thread: &str, mut answer: oneshot::Receiver<String>) -> Option<String> {
    if let Ok(answered) = tokio::time::timeout(WAIT, &mut answer).await {
        return answered.ok();
    }
    // `answer` sends under this lock: one sent a moment ago is still this call's.
    hub.work.questions.locked().remove(thread);
    answer.try_recv().ok()
}

/// `answer_task {task, answer}` from `bot`'s chat: the thread waiting for it gets it, or it is told it at its next
/// step.
pub(super) fn answer(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value> {
    let text = |key: &str| args[key].as_str().map(str::trim).filter(|t| !t.is_empty());
    let invalid = |why: String| Refused::error("invalid", why);
    let id = text("task").ok_or_else(|| invalid("task is required: the id of the task that asked".into()))?;
    let answer = text("answer").ok_or_else(|| invalid("answer is required".into()))?;
    if answer.len() > MAX_BRIEF {
        return Err(invalid(format!("answer is at most {MAX_BRIEF} bytes")));
    }
    super::tools::the_bots(hub, bot, id)?;
    let thread = tasks::load(&hub.store, bot).into_iter().find(|i| i.id == id).and_then(|task| task.thread);
    if let Some(thread) = thread {
        let mut questions = hub.work.questions.locked();
        if questions.get(&thread).is_some_and(|w| w.task == id)
            && let Some(waiting) = questions.remove(&thread)
            && waiting.waiter.send(answer.to_owned()).is_ok()
        {
            return Ok(json!({"taskId": id, "status": "answered"}));
        }
    }
    // Nothing waits for it any more: it goes to the task as words passed on.
    let status = match thread::tell(hub, bot, id, answer, true)? {
        Told::Waiting => Status::Waiting,
        Told::Working { .. } => Status::Working,
    };
    let note = "It stopped waiting for your answer, so it gets it at its next step.";
    Ok(json!({"taskId": id, "status": status, "note": note}))
}

/// `thread`'s task ended or was set aside: its question goes, and its call hears so.
pub(super) fn forget(hub: &Hub, thread: &str) {
    hub.work.questions.locked().remove(thread);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::calls::context::tests::{bot, hub};

    fn boxed(hub: &Hub) {
        hub.store.save_bot(&bot("b1", "Mika", "/tmp")).expect("saved");
        let thread = BotConfig { thread_of: Some("b1".into()), ..bot("thread-1", "Mika", "/tmp") };
        hub.store.save_bot(&thread).expect("saved");
        let user = BotConfig { thread_of: Some("b1".into()), ..bot("thread-u", "Mika", "/tmp") };
        hub.store.save_bot(&user).expect("saved");
        let tasks = json!([
            {"id": "i1", "title": "Docs", "brief": "x", "status": "working", "order": 1, "createdAt": 0,
                "thread": "thread-1"},
            {"id": "u", "title": "Mine", "brief": "x", "status": "working", "order": 2, "createdAt": 0,
                "thread": "thread-u", "source": {"kind": "user"}},
        ]);
        hub.store.kv_set(&format!("{}b1", tasks::PREFIX), &tasks.to_string()).expect("saved");
    }

    fn waiting(hub: &Hub) -> oneshot::Receiver<String> {
        let (tx, rx) = oneshot::channel();
        hub.work.questions.locked().insert("thread-1".into(), Waiting { task: "i1".into(), waiter: tx });
        rx
    }

    #[test]
    fn only_a_handed_out_task_asks_and_one_question_at_a_time() {
        let hub = hub();
        boxed(&hub);
        let category = |thread: &str, question: &str| {
            let error = put(&hub, thread, &json!({"question": question})).expect_err("refused");
            error.downcast_ref::<Refused>().map(|refused| refused.category)
        };
        assert_eq!(category("thread-1", " "), Some("invalid"));
        assert_eq!(category("thread-u", "which?"), Some("not_handed_out"), "the user is in that thread");
        assert_eq!(category("b1", "which?"), Some("not_handed_out"), "a bot's own chat");
        let _rx = waiting(&hub);
        assert_eq!(category("thread-1", "which?"), Some("busy"));
    }

    #[tokio::test]
    async fn the_answer_goes_to_the_waiting_call_and_a_later_one_is_told() {
        let hub = hub();
        boxed(&hub);
        let mut rx = waiting(&hub);
        let answered = answer(&hub, "b1", &json!({"task": "i1", "answer": " The staging one. "})).expect("answered");
        assert_eq!(answered["status"], "answered");
        assert_eq!(rx.try_recv().expect("the answer"), "The staging one.");
        assert!(!hub.work.questions.locked().contains_key("thread-1"), "answered once");

        let late = answer(&hub, "b1", &json!({"task": "i1", "answer": "Prod after all."})).expect("told");
        assert_eq!(late["status"], "working");
        let docs = tasks::load(&hub.store, "b1").into_iter().find(|i| i.id == "i1").expect("task");
        assert_eq!(docs.relayed, ["Prod after all."], "passed on as tell_task's words");

        let refused = answer(&hub, "b1", &json!({"task": "u", "answer": "x"})).expect_err("the user's");
        assert_eq!(refused.downcast_ref::<Refused>().map(|r| r.category), Some("users"));
    }

    #[tokio::test]
    async fn an_unanswered_question_goes_after_the_wait_or_with_its_task() {
        let hub = hub();
        tokio::time::pause();
        let rx = waiting(&hub);
        assert_eq!(wait(&hub, "thread-1", rx).await, None);
        assert!(!hub.work.questions.locked().contains_key("thread-1"), "a later answer is told");

        let rx = waiting(&hub);
        forget(&hub, "thread-1");
        assert_eq!(wait(&hub, "thread-1", rx).await, None, "its call hears the task ended");
    }
}
