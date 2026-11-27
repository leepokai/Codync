//! Talking to a thread: the user writes to the thread that works, or worked, one of the bot's own tasks, as to a
//! chat. The thread is a bot with an actor, so the message is a message in the thread's own chat: it waits behind a
//! running turn, can be sent now or removed, and starts a turn when none runs. A task that ended is reopened by
//! the first message, and reports again when that turn ends. The bot's chat passes words on to a task here too
//! (`tell`). A routine's run and another bot's request take none: their result has an owner already.

use super::tasks::{self, Source, Status, Task};
use super::{prompts, worktree};
use crate::LockExt;
use crate::agent::bot::Cmd;
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

/// Whether `task` takes words at all: the bot's own work does; a routine's run and another bot's request have an
/// owner for their result already.
pub(super) fn writable(task: &Task) -> Result<()> {
    let whose = match &task.source {
        Source::Bot | Source::User => return Ok(()),
        Source::Routine { .. } => "a routine's run",
        Source::Request { .. } => "another bot's request",
    };
    let why = format!("\"{}\" is {whose}: it can be read and stopped, not written to", task.title);
    Err(Refused::error("read_only", why))
}

/// The thread that takes the user's messages for `task`, or why it takes none.
pub(super) fn thread_of(store: &Store, task: &Task) -> Result<BotConfig> {
    writable(task)?;
    let title = &task.title;
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
    // What its chat passed on is with the task already (`tell`).
    // ponytail: a waiting message's files stay behind (only their names go); move them to the new thread if users
    // miss them.
    for entry in waiting(store, thread).into_iter().filter(|e| e.data["relay"] != true) {
        note(&mut task.notes, &carried(&entry.data));
    }
}

/// A waiting message as the task that starts again keeps it: its words and the names of the files left behind.
fn carried(data: &Value) -> String {
    let left = data["attachments"].as_array().is_some_and(|a| !a.is_empty());
    format!("{}{}", written(data), if left { " (those files stayed behind)" } else { "" })
}

/// One message the user wrote to a thread, as its task keeps it: the words, and the names of the files sent with them.
pub fn written(data: &Value) -> String {
    let text = data["text"].as_str().unwrap_or_default().trim();
    let names: Vec<&str> =
        data["attachments"].as_array().into_iter().flatten().filter_map(|a| a["name"].as_str()).collect();
    match (text.is_empty(), names.is_empty()) {
        (_, true) => text.to_owned(),
        (true, false) => format!("[sent files: {}]", names.join(", ")),
        (false, false) => format!("{text} [sent files: {}]", names.join(", ")),
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
        // Its worktree had nothing in it and went: the folder a thread without one works in.
        thread.cwd = task.folder(owner);
        let _g = hub.emit_lock.locked();
        hub.store.save_bot(thread)?;
    }
    Ok(())
}

/// The user writes `text` to the thread of `bot`'s task `id`, with the files uploaded to that thread (`uploads`,
/// their ids). A turn that runs there makes it wait, as in a chat; a task that ended is reopened and its thread's
/// actor started again, which resumes its session where the agent can. `nonce`: the client's, so a message sent
/// twice is one. Answers `{entry, reopened, queued}`: the message as the thread's transcript holds it, whether it
/// reopened the task, and whether it waits behind a running turn (both null for a message sent before).
pub fn message(hub: &Arc<Hub>, bot: &str, id: &str, text: &str, uploads: &[String], nonce: &str) -> Result<Value> {
    send(hub, bot, id, text, nonce, Author::User { uploads }, |_, _| Ok(()))
}

/// Who writes a message to a task's thread.
#[derive(Clone, Copy)]
pub(super) enum Author<'a> {
    /// The user, with the files they uploaded to it.
    User { uploads: &'a [String] },
    /// An outside app (`external`), with no files: its words are marked as its own.
    App(&'a super::external::StartedBy),
}

/// [`message`] from `author`; `check` sees the task and its bot under the box's lock before anything is written.
pub(super) fn send(
    hub: &Arc<Hub>,
    bot: &str,
    id: &str,
    text: &str,
    nonce: &str,
    author: Author,
    check: impl FnOnce(&Task, &BotConfig) -> Result<()>,
) -> Result<Value> {
    let owner = hub.store.bot(bot)?.filter(|row| !row.deleted).ok_or_else(|| anyhow!("unknown bot"))?.config;
    // A thread that ended over a week ago is in the archive: it comes back to be written to.
    super::archive::bring_back(hub, bot, id)?;
    // Under the box's lock to the end: the task's end and the dispatcher's look at its thread come before all of
    // this or after it, so neither ends a thread a message is on its way to.
    let (entry, reopened, queued) = tasks::change(hub, bot, |tasks| {
        let task = tasks::find(tasks, id)?;
        check(task, &owner)?;
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
        let uploads = match author {
            Author::User { uploads } => uploads,
            Author::App(_) => &[],
        };
        let root = crate::chat::uploads::root(&thread);
        let attachments = crate::chat::uploads::resolve_all(&root, uploads)?;
        let mut data = json!({"text": text, "clientNonce": nonce, "attachments": attachments});
        if let Author::App(by) = author {
            by.mark(&mut data, text);
        }
        let (entry, queued) = post_to(hub, &thread, data)?;
        Ok((entry, Some(reopened), Some(queued)))
    })?;
    if reopened == Some(true) {
        // It shows under Threads again, and what follows the bot from outside its chat counts it working.
        super::dispatch(hub, bot);
    }
    Ok(json!({"entry": entry, "reopened": reopened, "queued": queued}))
}

/// Posts `data` to `thread`'s chat as a message, its actor started again if it ended. Answers the entry, and whether
/// it waits behind a running turn.
pub(super) fn post_to(hub: &Arc<Hub>, thread: &BotConfig, data: Value) -> Result<(Entry, bool)> {
    if !hub.has_actor(&thread.id) {
        hub.spawn_bot(thread.clone());
    }
    let queued = matches!(hub.runtime(&thread.id).status, BotStatus::Working | BotStatus::NeedsInput);
    Ok((crate::chat::messages::post(hub, thread, data)?, queued))
}

/// Where words passed on to a task went (`tell`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Told {
    /// With the waiting task, for its thread to read when it starts.
    Waiting,
    /// A message in the working task's thread; `queued`: it waits behind a running turn.
    Working { queued: bool },
}

/// The bot passes `text` on to its task `id` (`tell_task`), which keeps it (`Task::relayed`) so a session that
/// starts the task, or starts it again, reads it. A working task also gets it as a message in its thread from the chat
/// (`from: host`, `relay`; every client quotes it there, as what the thread was told), which waits behind a running
/// turn as the user's does and keeps the task going until it is taken. `now` hands it to that turn at its next step,
/// or stops the turn for it where the agent takes no message mid-turn (`agent::bot::steer`).
pub fn tell(hub: &Arc<Hub>, bot: &str, id: &str, text: &str, now: bool) -> Result<Told> {
    let (told, steer) = tasks::change(hub, bot, |tasks| {
        let task = tasks::find(tasks, id)?;
        writable(task)?;
        match task.status {
            Status::Waiting => {
                note(&mut task.relayed, text);
                Ok((Told::Waiting, None))
            }
            Status::Working => {
                let thread = thread_of(&hub.store, task)?;
                note(&mut task.relayed, text);
                let (entry, queued) = post_to(hub, &thread, json!({"text": text, "from": "host", "relay": true}))?;
                Ok((Told::Working { queued }, (now && queued).then_some((thread.id, entry.id))))
            }
            _ => {
                let why = format!(
                    "\"{}\" has ended: its report came to you already. For more of that work, add a new task",
                    task.title
                );
                Err(Refused::error("ended", why))
            }
        }
    })?;
    if let Some((thread, entry_id)) = steer {
        hub.send_cmd(&thread, Cmd::Steer { entry_id })?;
    }
    Ok(told)
}

/// A turn of thread `cfg` begins for what the user wrote to its thread (`words`, each message's): the task's next
/// report ends with them, and a session that starts the task again reads them.
pub fn heard(hub: &Hub, cfg: &BotConfig, words: &[String]) {
    super::update(hub, cfg, "what the user wrote to it", |task| {
        for words in words.iter().map(|w| w.trim()).filter(|w| !w.is_empty()) {
            note(&mut task.notes, words);
            task.told.push(words.to_owned());
        }
        let over = task.told.len().saturating_sub(TOLD_KEPT);
        task.told.drain(..over);
    });
}

/// What [`heard`] kept of `words` goes: those messages went back to wait (the agent dropped them with its turn), and
/// their own turn hears them again.
pub fn unhear(hub: &Hub, cfg: &BotConfig, words: &[String]) {
    super::update(hub, cfg, "what the user wrote to it", |task| {
        for words in words.iter().rev().map(|words| words.trim()) {
            for kept in [&mut task.told, &mut task.notes] {
                if kept.last().is_some_and(|last| last == words) {
                    kept.pop();
                }
            }
        }
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
    fn a_message_of_files_alone_is_told_by_their_names() {
        let files = json!([{"name": "shot.png"}, {"name": "log.txt"}]);
        assert_eq!(written(&json!({"text": "", "attachments": files})), "[sent files: shot.png, log.txt]");
        assert_eq!(written(&json!({"text": " see ", "attachments": files})), "see [sent files: shot.png, log.txt]");
        assert_eq!(written(&json!({"text": "see", "attachments": []})), "see");
        let carried_files = carried(&json!({"text": "see", "attachments": files}));
        assert_eq!(carried_files, "see [sent files: shot.png, log.txt] (those files stayed behind)");
        assert_eq!(carried(&json!({"text": "see"})), "see");
    }

    #[tokio::test]
    async fn files_sent_to_a_thread_are_its_own_uploads() {
        let hub = hub();
        hub.store.save_bot(&bot("b1", "Mika", "/tmp")).expect("saved");
        let thread = BotConfig { thread_of: Some("b1".into()), ..bot("thread-1", "Mika", "/tmp") };
        hub.store.save_bot(&thread).expect("saved");
        crate::chat::work::put_working_for_test(&hub.store, "b1", "thread-1");
        let root = crate::chat::uploads::root(&thread);
        let upload = uuid::Uuid::new_v4().to_string();
        crate::chat::uploads::append(&root, &upload, "shot.png", 0, b"png", true).expect("uploaded");
        let sent = message(&hub, "b1", "i1", "", std::slice::from_ref(&upload), "n1").expect("sent");
        assert_eq!(sent["entry"]["botId"], "thread-1");
        assert_eq!(sent["entry"]["data"]["attachments"][0]["name"], "shot.png");
        let unknown = [uuid::Uuid::new_v4().to_string()];
        assert!(message(&hub, "b1", "i1", "", &unknown, "n2").is_err(), "only the thread's own uploads");
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

    #[test]
    fn words_that_went_back_to_wait_are_the_tasks_once() {
        let hub = hub();
        let cfg = BotConfig { thread_of: Some("b1".into()), ..bot("thread-1", "Mika", "/tmp") };
        crate::chat::work::put_working_for_test(&hub.store, "b1", "thread-1");
        let words = |said: &[&str]| said.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
        heard(&hub, &cfg, &words(&["earlier"]));
        // Two messages joined a turn its agent dropped: their own turn hears them again.
        heard(&hub, &cfg, &words(&["one", "two"]));
        unhear(&hub, &cfg, &words(&["one", "two"]));
        heard(&hub, &cfg, &words(&["one", "two"]));
        let task = tasks::load(&hub.store, "b1").remove(0);
        assert_eq!(task.told, words(&["earlier", "one", "two"]));
        assert_eq!(task.notes, words(&["earlier", "one", "two"]));
    }
}
