//! Talking to a thread: the user writes to the thread that works, or worked, one of the bot's own tasks, as to a
//! chat. The thread is a bot with an actor, so the message is a message in the thread's own chat: it waits behind a
//! running turn, can be sent now or removed, and starts a turn when none runs. A task that ended is reopened by
//! the first message, and reports again when that turn ends. A routine's run and another bot's request take none:
//! their result has an owner already.

use super::tasks::{self, Source, Status, Task};
use super::{prompts, worktree};
use crate::LockExt;
use crate::hub::{BotStatus, Hub};
use crate::store::{BotConfig, Entry, Store, now_ms};
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;

/// What a task keeps of the user's words for its next report at most.
const TOLD_KEPT: usize = 20;

/// A message a thread didn't take, with why for analytics (`category`).
#[derive(Debug)]
pub struct Refused {
    pub category: &'static str,
    message: String,
}

impl Refused {
    pub fn error(category: &'static str, message: String) -> anyhow::Error {
        Self { category, message }.into()
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Refused {}

/// The thread that takes the user's messages for `task`, or why it takes none.
fn thread_of(store: &Store, task: &Task) -> Result<BotConfig> {
    let title = &task.title;
    let read_only = |whose: &str| {
        Refused::error("read_only", format!("\"{title}\" is {whose}: it can be read and stopped, not written to"))
    };
    match &task.source {
        Source::Bot => {}
        Source::Routine { .. } => return Err(read_only("a routine's run")),
        Source::Request { .. } => return Err(read_only("another bot's request")),
    }
    let Some(thread) = task.thread.as_deref() else {
        let why = format!("\"{title}\" hasn't started yet: there is nothing to write to");
        return Err(Refused::error("not_started", why));
    };
    let row = store.bot(thread)?.filter(|row| !row.deleted);
    let gone = || Refused::error("gone", format!("\"{title}\" ended too long ago to write to: add new work instead"));
    Ok(row.ok_or_else(gone)?.config)
}

/// The thread behind `bot`'s task `id`, for what is done to a message that waits in its thread (`steer`, `unqueue`).
/// The caller names the bot and the task, never a thread: the task must be this bot's, and its own work.
pub fn thread(hub: &Hub, bot: &str, id: &str) -> Result<String> {
    let mut tasks = tasks::load(&hub.store, bot);
    Ok(thread_of(&hub.store, tasks::find(&mut tasks, id)?)?.id)
}

/// The user's messages that wait for their turn in `thread`.
fn waiting(store: &Store, thread: &str) -> Vec<Entry> {
    store.queued_messages(thread).unwrap_or_else(|error| {
        tracing::warn!(thread, error = format!("{error:#}"), "couldn't read a thread's waiting messages");
        Vec::new()
    })
}

/// Whether a message of the user's still waits for its turn in `thread`: its task isn't over until that
/// turn is, and nothing ends the thread meanwhile.
pub(super) fn unheard(store: &Store, thread: &str) -> bool {
    !waiting(store, thread).is_empty()
}

/// `task`'s thread is closed for the task to start again (it gave way, or got a new brief): what the user wrote to
/// its thread and no turn took yet goes with the task, as what they said since.
pub(super) fn carry(store: &Store, task: &mut Task, thread: &str) {
    for entry in waiting(store, thread) {
        note(&mut task.notes, entry.data["text"].as_str().unwrap_or_default());
    }
}

fn note(notes: &mut Vec<String>, words: &str) {
    let words = words.trim();
    if !words.is_empty() && notes.last().map(String::as_str) != Some(words) {
        notes.push(words.to_owned());
    }
}

/// A task that ended works again, in the thread it had: under Threads, timed from now, counted against the bot's
/// threads at once even past the ceiling (the user asked). Its worktree, if it was kept, is its folder again. A
/// restart that cuts this turn off ends it: the work was done once, and isn't done again from its brief.
fn reopen(hub: &Hub, owner: &BotConfig, task: &mut Task, thread: &mut BotConfig) -> Result<()> {
    task.said = task.result.take().or(task.said.take());
    task.status = Status::Working;
    task.restarted = true;
    task.started_at = Some(now_ms());
    task.ended_at = None;
    task.failure = None;
    // A worktree being looked over (`Settling`: its task just ended) is in use again too, which the look then leaves.
    // ponytail: that look may still remove an untouched one; the thread then fails to start in it and says so. Hold
    // the look until no message can come if that ever shows.
    if let Some(kept) = task.worktree.as_mut().filter(|w| worktree::usable(w)) {
        kept.left = worktree::Left::InUse;
    } else if !Path::new(&thread.cwd).is_dir() {
        // Its worktree had nothing in it and went: the bot's folder, as for a thread without one.
        thread.cwd.clone_from(&owner.cwd);
        let _g = hub.emit_lock.locked();
        hub.store.save_bot(thread)?;
    }
    Ok(())
}

/// The user writes `text` to the thread of `bot`'s task `id`. A turn that runs there makes it wait, as in a chat;
/// a task that ended is reopened and its thread's actor started again, which resumes its session where the agent
/// can. `nonce`: the client's, so a message sent twice is one. Answers `{entry, reopened, queued}`: the message as
/// the thread's transcript holds it, whether it reopened the task, and whether it waits behind a running turn
/// (both null for a message sent before).
pub fn message(hub: &Arc<Hub>, bot: &str, id: &str, text: &str, nonce: &str) -> Result<Value> {
    let owner = hub.store.bot(bot)?.filter(|row| !row.deleted).ok_or_else(|| anyhow!("unknown bot"))?.config;
    // Under the box's lock to the end: the task's end and the dispatcher's look at its thread come before all of
    // this or after it, so neither ends a thread a message is on its way to.
    let (entry, reopened, queued) = tasks::change(hub, bot, |tasks| {
        let task = tasks::find(tasks, id)?;
        let ended = task.status.ended();
        let mut thread = thread_of(&hub.store, task)?;
        if let Some(sent) = Some(nonce).filter(|n| !n.is_empty()).and_then(|n| hub.store.find_by_nonce(&thread.id, n)) {
            return Ok((sent, None, None));
        }
        // Its thread may be on a task that followed it by now: the message is for that session all the same.
        let reopened = ended && tasks::of_thread(tasks, &thread.id).is_none();
        if reopened {
            reopen(hub, &owner, tasks::find(tasks, id)?, &mut thread)?;
        }
        if !hub.has_actor(&thread.id) {
            hub.spawn_bot(thread.clone());
        }
        let queued = matches!(hub.runtime(&thread.id).status, BotStatus::Working | BotStatus::NeedsInput);
        let data = json!({"text": text, "clientNonce": nonce, "attachments": []});
        Ok((crate::chat::messages::post(hub, &thread, data)?, Some(reopened), Some(queued)))
    })?;
    if reopened == Some(true) {
        // It shows under Threads again, and what follows the bot from outside its chat counts it working.
        super::dispatch(hub, bot);
    }
    Ok(json!({"entry": entry, "reopened": reopened, "queued": queued}))
}

/// A turn of thread `cfg` begins for what the user wrote to its thread (`words`, each message's): the task's next
/// report ends with them, and a session that starts the task again reads them.
pub fn heard(hub: &Hub, cfg: &BotConfig, words: &[String]) {
    super::update(hub, cfg, "what the user wrote to it", |task| {
        for words in words {
            note(&mut task.notes, words);
            task.told.push(words.trim().to_owned());
        }
        let over = task.told.len().saturating_sub(TOLD_KEPT);
        task.told.drain(..over);
    });
}

/// What a new session of thread `cfg` is told before the user's message, when the session that did its task can't
/// be picked up: the task, what the user said about it and what it reported.
pub fn picked_up(store: &Store, cfg: &BotConfig) -> Option<String> {
    super::task_of(store, cfg).map(|task| prompts::picked_up(&task))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::calls::context::tests::{bot, hub};

    fn task(more: &Value) -> Task {
        let mut task = json!({"id": "i1", "title": "Fix login", "brief": "x", "status": "done", "order": 1,
            "createdAt": 0, "thread": "thread-1"});
        task.as_object_mut().expect("an object").extend(more.as_object().cloned().unwrap_or_default());
        serde_json::from_value(task).expect("a task")
    }

    #[test]
    fn only_the_bots_own_work_whose_thread_is_there_takes_a_message() {
        let hub = hub();
        let thread = BotConfig { thread_of: Some("b1".into()), ..bot("thread-1", "Mika", "/tmp") };
        let refusal = |task: &Task| thread_of(&hub.store, task).expect_err("refused");
        let refused = |task: &Task| refusal(task).to_string();
        let category = |task: &Task| refusal(task).downcast_ref::<Refused>().map(|refused| refused.category);
        assert_eq!(category(&task(&json!({}))), Some("gone"));
        assert_eq!(category(&task(&json!({"source": {"kind": "request"}}))), Some("read_only"));
        assert_eq!(category(&task(&json!({"status": "waiting", "thread": null}))), Some("not_started"));
        assert_eq!(refused(&task(&json!({}))), "\"Fix login\" ended too long ago to write to: add new work instead");
        hub.store.save_bot(&thread).expect("saved");
        assert_eq!(thread_of(&hub.store, &task(&json!({}))).expect("its thread").id, "thread-1");
        assert_eq!(
            refused(&task(&json!({"source": {"kind": "routine", "id": "r", "run": "u"}}))),
            "\"Fix login\" is a routine's run: it can be read and stopped, not written to"
        );
        assert_eq!(
            refused(&task(&json!({"source": {"kind": "request"}}))),
            "\"Fix login\" is another bot's request: it can be read and stopped, not written to"
        );
        assert_eq!(
            refused(&task(&json!({"status": "waiting", "thread": null}))),
            "\"Fix login\" hasn't started yet: there is nothing to write to"
        );
    }

    #[test]
    fn a_reopened_task_works_again_and_keeps_what_it_reported() {
        let hub = hub();
        let owner = bot("b1", "Mika", "/tmp");
        let mut thread = BotConfig { thread_of: Some("b1".into()), ..bot("thread-1", "Mika", "/nowhere/it/went") };
        let mut task = task(&json!({"result": "All green.", "endedAt": 5, "startedAt": 1}));
        reopen(&hub, &owner, &mut task, &mut thread).expect("reopened");
        assert_eq!((task.status, task.ended_at, task.result.as_deref()), (Status::Working, None, None));
        assert!(task.restarted, "cut off now, it ends: it isn't done again from its brief");
        assert_eq!(task.said.as_deref(), Some("All green."));
        assert!(task.started_at.is_some_and(|at| at > 1), "timed from now");
        assert_eq!(thread.cwd, "/tmp", "a folder that went is the bot's again");
    }

    #[test]
    fn what_the_user_wrote_stays_with_the_task_for_its_report_and_its_next_session() {
        let hub = hub();
        let cfg = BotConfig { thread_of: Some("b1".into()), ..bot("thread-1", "Mika", "/tmp") };
        crate::chat::work::put_working_for_test(&hub.store, "b1", "thread-1");
        let words: Vec<String> = (0..=TOLD_KEPT).map(|n| format!(" word {n} ")).collect();
        heard(&hub, &cfg, &words);
        let task = tasks::load(&hub.store, "b1").remove(0);
        assert_eq!(task.told.len(), TOLD_KEPT, "the newest are kept");
        assert_eq!((task.told[0].as_str(), task.notes[0].as_str()), ("word 1", "word 0"));
    }
}
