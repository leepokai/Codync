//! Talking to a thread: the user writes to the task that works, or worked, one of the bot's own items, as to a
//! chat. The task is a bot with an actor, so the message is a message in the task's own chat: it waits behind a
//! running turn, can be sent now or removed, and starts a turn when none runs. An item that ended is reopened by
//! the first message, and reports again when that turn ends. A routine's run and another bot's request take none:
//! their result has an owner already.

use super::items::{self, Item, Source, Status};
use super::{prompts, worktree};
use crate::LockExt;
use crate::hub::{BotStatus, Hub};
use crate::store::{BotConfig, Entry, Store, now_ms};
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;

/// What an item keeps of the user's words for its next report at most.
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

/// The task that takes the user's messages for `item`, or why it takes none.
fn task_of(store: &Store, item: &Item) -> Result<BotConfig> {
    let title = &item.title;
    let read_only = |whose: &str| {
        Refused::error("read_only", format!("\"{title}\" is {whose}: it can be read and stopped, not written to"))
    };
    match &item.source {
        Source::Bot => {}
        Source::Routine { .. } => return Err(read_only("a routine's run")),
        Source::Request { .. } => return Err(read_only("another bot's request")),
    }
    let Some(task) = item.task.as_deref() else {
        let why = format!("\"{title}\" hasn't started yet: there is nothing to write to");
        return Err(Refused::error("not_started", why));
    };
    let row = store.bot(task)?.filter(|row| !row.deleted);
    let gone = || Refused::error("gone", format!("\"{title}\" ended too long ago to write to: add new work instead"));
    Ok(row.ok_or_else(gone)?.config)
}

/// The task behind `bot`'s item `id`, for what is done to a message that waits in its thread (`steer`, `unqueue`).
/// The caller names the bot and the item, never a task: the item must be this bot's, and its own work.
pub fn task(hub: &Hub, bot: &str, id: &str) -> Result<String> {
    let mut items = items::load(&hub.store, bot);
    Ok(task_of(&hub.store, items::find(&mut items, id)?)?.id)
}

/// The user's messages that wait for their turn in `task`'s thread.
fn waiting(store: &Store, task: &str) -> Vec<Entry> {
    store.queued_messages(task).unwrap_or_else(|error| {
        tracing::warn!(task, error = format!("{error:#}"), "couldn't read a thread's waiting messages");
        Vec::new()
    })
}

/// Whether a message of the user's still waits for its turn in `task`'s thread: its item isn't over until that
/// turn is, and nothing ends the task meanwhile.
pub(super) fn unheard(store: &Store, task: &str) -> bool {
    !waiting(store, task).is_empty()
}

/// `item`'s task is closed for the item to start again (it gave way, or got a new brief): what the user wrote to
/// its thread and no turn took yet goes with the item, as what they said since.
pub(super) fn carry(store: &Store, item: &mut Item, task: &str) {
    for entry in waiting(store, task) {
        note(&mut item.notes, entry.data["text"].as_str().unwrap_or_default());
    }
}

fn note(notes: &mut Vec<String>, words: &str) {
    let words = words.trim();
    if !words.is_empty() && notes.last().map(String::as_str) != Some(words) {
        notes.push(words.to_owned());
    }
}

/// An item that ended works again, in the task it had: under Threads, timed from now, counted against the bot's
/// agents at once even past the ceiling (the user asked). Its worktree, if it was kept, is its folder again. A
/// restart that cuts this turn off ends it: the work was done once, and isn't done again from its brief.
fn reopen(hub: &Hub, owner: &BotConfig, item: &mut Item, task: &mut BotConfig) -> Result<()> {
    item.said = item.result.take().or(item.said.take());
    item.status = Status::Working;
    item.restarted = true;
    item.started_at = Some(now_ms());
    item.ended_at = None;
    item.failure = None;
    // A worktree being looked over (`Settling`: its item just ended) is in use again too, which the look then leaves.
    // ponytail: that look may still remove an untouched one; the task then fails to start in it and says so. Hold
    // the look until no message can come if that ever shows.
    if let Some(kept) = item.worktree.as_mut().filter(|w| worktree::usable(w)) {
        kept.left = worktree::Left::InUse;
    } else if !Path::new(&task.cwd).is_dir() {
        // Its worktree had nothing in it and went: the bot's folder, as for a task without one.
        task.cwd.clone_from(&owner.cwd);
        let _g = hub.emit_lock.locked();
        hub.store.save_bot(task)?;
    }
    Ok(())
}

/// The user writes `text` to the thread of `bot`'s item `id`. A turn that runs there makes it wait, as in a chat;
/// an item that ended is reopened and its task's actor started again, which resumes its session where the agent
/// can. `nonce`: the client's, so a message sent twice is one. Answers `{entry, reopened, queued}`: the message as
/// the thread's transcript holds it, whether it reopened the item, and whether it waits behind a running turn
/// (both null for a message sent before).
pub fn message(hub: &Arc<Hub>, bot: &str, id: &str, text: &str, nonce: &str) -> Result<Value> {
    let owner = hub.store.bot(bot)?.filter(|row| !row.deleted).ok_or_else(|| anyhow!("unknown bot"))?.config;
    // Under the box's lock to the end: the item's end and the dispatcher's look at its task come before all of
    // this or after it, so neither ends a task a message is on its way to.
    let (entry, reopened, queued) = items::change(hub, bot, |items| {
        let item = items::find(items, id)?;
        let ended = item.status.ended();
        let mut task = task_of(&hub.store, item)?;
        if let Some(sent) = Some(nonce).filter(|n| !n.is_empty()).and_then(|n| hub.store.find_by_nonce(&task.id, n)) {
            return Ok((sent, None, None));
        }
        // Its task may be on an item that followed it by now: the message is for that session all the same.
        let reopened = ended && items::of_task(items, &task.id).is_none();
        if reopened {
            reopen(hub, &owner, items::find(items, id)?, &mut task)?;
        }
        if !hub.has_actor(&task.id) {
            hub.spawn_bot(task.clone());
        }
        let queued = matches!(hub.runtime(&task.id).status, BotStatus::Working | BotStatus::NeedsInput);
        let data = json!({"text": text, "clientNonce": nonce, "attachments": []});
        Ok((crate::chat::messages::post(hub, &task, data)?, Some(reopened), Some(queued)))
    })?;
    if reopened == Some(true) {
        // It shows under Threads again, and what follows the bot from outside its chat counts it working.
        super::dispatch(hub, bot);
    }
    Ok(json!({"entry": entry, "reopened": reopened, "queued": queued}))
}

/// A turn of task `cfg` begins for what the user wrote to its thread (`words`, each message's): the item's next
/// report ends with them, and a session that starts the item again reads them.
pub fn heard(hub: &Hub, cfg: &BotConfig, words: &[String]) {
    super::update(hub, cfg, "what the user wrote to it", |item| {
        for words in words {
            note(&mut item.notes, words);
            item.told.push(words.trim().to_owned());
        }
        let over = item.told.len().saturating_sub(TOLD_KEPT);
        item.told.drain(..over);
    });
}

/// What a new session of task `cfg` is told before the user's message, when the session that did its item can't
/// be picked up: the item, what the user said about it and what it reported.
pub fn picked_up(store: &Store, cfg: &BotConfig) -> Option<String> {
    super::item_of(store, cfg).map(|item| prompts::picked_up(&item))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::calls::context::tests::{bot, hub};

    fn item(more: &Value) -> Item {
        let mut item = json!({"id": "i1", "title": "Fix login", "brief": "x", "status": "done", "order": 1,
            "createdAt": 0, "task": "task-1"});
        item.as_object_mut().expect("an object").extend(more.as_object().cloned().unwrap_or_default());
        serde_json::from_value(item).expect("an item")
    }

    #[test]
    fn only_the_bots_own_work_whose_task_is_there_takes_a_message() {
        let hub = hub();
        let task = BotConfig { task_of: Some("b1".into()), ..bot("task-1", "Mika", "/tmp") };
        let refusal = |item: &Item| task_of(&hub.store, item).expect_err("refused");
        let refused = |item: &Item| refusal(item).to_string();
        let category = |item: &Item| refusal(item).downcast_ref::<Refused>().map(|refused| refused.category);
        assert_eq!(category(&item(&json!({}))), Some("gone"));
        assert_eq!(category(&item(&json!({"source": {"kind": "request"}}))), Some("read_only"));
        assert_eq!(category(&item(&json!({"status": "waiting", "task": null}))), Some("not_started"));
        assert_eq!(refused(&item(&json!({}))), "\"Fix login\" ended too long ago to write to: add new work instead");
        hub.store.save_bot(&task).expect("saved");
        assert_eq!(task_of(&hub.store, &item(&json!({}))).expect("its task").id, "task-1");
        assert_eq!(
            refused(&item(&json!({"source": {"kind": "routine", "id": "r", "run": "u"}}))),
            "\"Fix login\" is a routine's run: it can be read and stopped, not written to"
        );
        assert_eq!(
            refused(&item(&json!({"source": {"kind": "request"}}))),
            "\"Fix login\" is another bot's request: it can be read and stopped, not written to"
        );
        assert_eq!(
            refused(&item(&json!({"status": "waiting", "task": null}))),
            "\"Fix login\" hasn't started yet: there is nothing to write to"
        );
    }

    #[test]
    fn a_reopened_item_works_again_and_keeps_what_it_reported() {
        let hub = hub();
        let owner = bot("b1", "Mika", "/tmp");
        let mut task = BotConfig { task_of: Some("b1".into()), ..bot("task-1", "Mika", "/nowhere/it/went") };
        let mut item = item(&json!({"result": "All green.", "endedAt": 5, "startedAt": 1}));
        reopen(&hub, &owner, &mut item, &mut task).expect("reopened");
        assert_eq!((item.status, item.ended_at, item.result.as_deref()), (Status::Working, None, None));
        assert!(item.restarted, "cut off now, it ends: it isn't done again from its brief");
        assert_eq!(item.said.as_deref(), Some("All green."));
        assert!(item.started_at.is_some_and(|at| at > 1), "timed from now");
        assert_eq!(task.cwd, "/tmp", "a folder that went is the bot's again");
    }

    #[test]
    fn what_the_user_wrote_stays_with_the_item_for_its_report_and_its_next_session() {
        let hub = hub();
        let cfg = BotConfig { task_of: Some("b1".into()), ..bot("task-1", "Mika", "/tmp") };
        crate::chat::tasks::put_working_for_test(&hub.store, "b1", "task-1");
        let words: Vec<String> = (0..=TOLD_KEPT).map(|n| format!(" word {n} ")).collect();
        heard(&hub, &cfg, &words);
        let item = items::load(&hub.store, "b1").remove(0);
        assert_eq!(item.told.len(), TOLD_KEPT, "the newest are kept");
        assert_eq!((item.told[0].as_str(), item.notes[0].as_str()), ("word 1", "word 0"));
    }
}
