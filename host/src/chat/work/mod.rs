//! Work a bot does away from its chat, so the chat keeps answering
//! ([the box](../../../../docs/plans/event-box.md), [threads](../../../../docs/features/work.md)).
//!
//! What the bot should work on is a **task** in its box ([`tasks`]). A **thread** works tasks, one at a time: a
//! **thread bot**, a `bots` row of its own (`BotConfig::thread_of`) copied from its bot, with its own actor, agent
//! process, session and transcript, because everything a running turn touches is keyed by bot id. Its memory and
//! instructions are its bot's (`BotConfig::as_owner`). It starts by forking its bot's main session where the agent
//! can (`agent::bot::session`), otherwise from a new session and a brief. [`dispatch`] gives waiting tasks to
//! threads, up to the bot's `maxThreads`; a thread's last message of a turn is its task's result and goes back as a turn
//! in the bot's chat (`Cmd::TaskReport`). A routine run and another bot's request are tasks too ([`sources`]): their
//! result goes to the chat as the routine's message, or to the asking bot as its reply. No client lists a thread bot:
//! they see `work` on the bot and fetch a thread's transcript by id.
//!
//! Here: a thread's life from its turn's end to its report. [`steer`] has what the user and the bot do to a task,
//! [`tools`] the bot's tools, [`prompts`] the words, [`worktree`] the git worktree of a thread that works beside
//! another in one repository.

mod dispatch;
mod prompts;
mod sources;
mod steer;
mod sweep;
mod tasks;
mod thread;
mod tools;
mod worktree;

pub use dispatch::{Wait, dispatch};
pub use prompts::{BRIEF, in_thread, reminder, report_notice, report_prompt};
pub use sources::{access_of, add_request, add_routine, boxed, from_chat, routines_changed, source_of};
pub use steer::{Steer, cut_in, drop_task, first, retry, stop};
#[cfg(test)]
pub use tasks::put_working_for_test;
pub use tasks::{Source, Status, doing, lines, outside, wire, working_threads};
pub use thread::{Refused as ThreadRefused, heard, message, picked_up, thread as thread_of};
pub use tools::{TOOLS, acting_as, call, refuse_chat_only, refuse_in_thread, tools};
pub use worktree::{Worktree, enter as enter_worktree, in_repo};

use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::analytics::errors::host_error;
use crate::analytics::{self, Event};
use crate::hub::Hub;
use crate::store::{BotConfig, Entry, Store, now_ms};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tasks::Task;

/// Tasks added in a row with no message of the user's in between (a report's turn adding the next).
const MAX_HOPS: u8 = 8;
const _: () = assert!(MAX_HOPS == 8, "chat::team::limit::NOTICE and its translations say 8");
/// Tasks a box holds waiting at most.
const MAX_WAITING: usize = 20;
/// An ended task, and a thread's transcript with its last task, are kept this long.
const KEEP_MS: i64 = 7 * 24 * 60 * 60 * 1000;
const MAX_TITLE: usize = 80;
const MAX_BRIEF: usize = 32_000;
/// How much of a thread's last message its task's report carries.
const MAX_REPORT: usize = 8_000;
/// Why a task that a host restart cut off, and that can't pick up, didn't finish.
pub const CUT_OFF: &str = "the computer's Codync restarted while it worked";
/// Why a task whose thread's turn never began didn't finish; the error follows.
const NOT_STARTED: &str = "it could not start: ";

/// How a thread's session began.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Start {
    Fork,
    Brief,
}

/// What a task came to, for its bot.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    // `item`: reads a report stored before this release.
    #[serde(alias = "item")]
    pub task: String,
    pub title: String,
    /// Its thread's last message; `Err` holds why it didn't finish.
    pub outcome: Result<String, String>,
    pub hops: u8,
    /// The git worktree it worked in, if it had one of its own: where its changes are.
    #[serde(default)]
    pub worktree: Option<Worktree>,
    /// What the user wrote to its thread since its last report, word for word: not the thread's words.
    #[serde(default)]
    pub told: Vec<String>,
}

/// How a thread's turn ended (from its actor).
pub enum Outcome {
    Done(Option<String>),
    /// Why, in words for its bot.
    Failed(Failure, String),
    /// The host couldn't give the task to its thread, so no agent was ever asked; why, in words for its bot.
    Undispatched(String),
    Stopped,
}

impl Outcome {
    /// Its thread's turn never began, for `error`.
    pub fn not_started(error: &anyhow::Error) -> Self {
        Self::Failed(Failure::Start, format!("{NOT_STARTED}{error:#}"))
    }

    /// The host couldn't give the task to its thread, for `error`: to the user, it could not start either.
    fn not_dispatched(error: &anyhow::Error) -> Self {
        Self::Undispatched(format!("{NOT_STARTED}{error:#}"))
    }
}

/// What kept a task from finishing: `work_finished`'s `error`. Saved with its task, and a host from before a new
/// one reads a box that names it as empty (`tasks::load`): what only `work_finished` tells apart stays out of
/// here, as `dispatch` does ([`Outcome::Undispatched`]).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    /// Its thread's turn never began (the agent didn't start, needs sign-in, opened no session).
    Start,
    /// Its thread's agent failed mid-turn.
    Agent,
    /// Its thread's turn ended short of `end_turn` (output or step limit, a refusal).
    EndedEarly,
    /// A host restart cut it off and it couldn't pick up.
    Restart,
    /// Its thread ran no turn any more and no end ever arrived, twice (`sweep`).
    Lost,
    /// It finished with nothing to report.
    NoReport,
    /// A routine run went past its time limit.
    TimedOut,
}

/// State of the boxes that lives only while the host runs.
#[derive(Default)]
pub struct Work {
    /// Bot → the hops of the reports its running turn answers: a task added there carries them on.
    chain: Mutex<HashMap<String, u8>>,
    /// Bot → the tasks its running turn added: a turn the user stops takes them with it.
    added: Mutex<HashMap<String, Vec<String>>>,
    /// Held across each change to a box (`tasks::change`).
    boxes: Mutex<()>,
    /// Bot → its box's next hourly look (`sweep`), while the box holds work.
    sweeps: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    /// Task → the request of another bot it answers (`sources`), which owns the exchange's notices and the reply.
    /// Gone with the host process: a request is never replayed.
    requests: Mutex<HashMap<String, crate::chat::team::BotRequest>>,
}

impl Work {
    pub fn answering(&self, bot: &str, hops: u8) {
        self.chain.locked().insert(bot.to_owned(), hops);
    }

    fn hops(&self, bot: &str) -> u8 {
        self.chain.locked().get(bot).copied().unwrap_or(0)
    }
}

/// `bot`'s thread bots.
pub fn of(store: &Store, bot: &str) -> Vec<BotConfig> {
    let rows = store.thread_bots().unwrap_or_else(|error| {
        tracing::warn!(error = format!("{error:#}"), "couldn't list threads");
        Vec::new()
    });
    let mut threads: Vec<BotConfig> =
        rows.into_iter().map(|r| r.config).filter(|c| c.thread_of.as_deref() == Some(bot)).collect();
    threads.sort_by_key(|c| c.created_at);
    threads
}

/// `cfg`'s fallbacks changed: its threads fall back the same way from their next agent on, as the bot does. Read,
/// saved and sent under one lock, so nothing a thread saves meanwhile is lost and a later change reaches it after this
/// one.
pub fn fallbacks_changed(hub: &Hub, cfg: &BotConfig) {
    let _g = hub.emit_lock.locked();
    let mut threads = of(&hub.store, &cfg.id);
    for thread in &mut threads {
        thread.fallbacks.clone_from(&cfg.fallbacks);
    }
    if let Err(error) = hub.store.save_bots(&threads) {
        tracing::warn!(bot = %cfg.id, error = format!("{error:#}"), "couldn't give its threads the bot's fallbacks");
        return;
    }
    for thread in threads.into_iter().filter(|thread| hub.has_actor(&thread.id)) {
        let id = thread.id.clone();
        if let Err(error) = hub.send_cmd(&id, Cmd::Reconfigure(Box::new(thread))) {
            tracing::warn!(thread = %id, error = format!("{error:#}"), "couldn't give a thread its bot's fallbacks");
        }
    }
}

/// The bot `id` is to the user: a thread's bot, else `id` itself.
pub fn owner(store: &Store, id: &str) -> String {
    let thread_of = store.bot(id).ok().flatten().and_then(|row| row.config.thread_of);
    thread_of.unwrap_or_else(|| id.to_owned())
}

/// The actor that waits on an approval card's answer: the thread that raised it, else the bot that wrote it (in a
/// group, the member that asked), else the chat's own.
pub fn card_actor(card: &Entry) -> &str {
    let thread = card.data["thread"]["id"].as_str();
    thread.or_else(|| card.data["author"].as_str()).unwrap_or(&card.bot_id)
}

/// The task thread `cfg` works on.
fn task_of(store: &Store, cfg: &BotConfig) -> Option<Task> {
    tasks::of_thread(&tasks::load(store, cfg.thread_of.as_deref()?), &cfg.id).cloned()
}

/// Changes the task thread `cfg` works on, for a change whose loss only costs a detail (`what`, for the log).
fn update(hub: &Hub, cfg: &BotConfig, what: &str, change: impl FnOnce(&mut Task)) -> bool {
    let Some(bot) = cfg.thread_of.as_deref() else { return false };
    let changed = tasks::change(hub, bot, |tasks| {
        let task = tasks.iter_mut().find(|i| i.status == Status::Working && i.thread.as_deref() == Some(&cfg.id));
        Ok(task.map(change).is_some())
    });
    changed.unwrap_or_else(|error| {
        tracing::warn!(thread = %cfg.id, what, error = format!("{error:#}"), "couldn't save a task's detail");
        false
    })
}

/// The task thread `cfg` works on, as the cards it puts in its bot's chat name it ([`mark`]): its id and title.
pub fn card_task(store: &Store, cfg: &BotConfig) -> Option<(String, String)> {
    task_of(store, cfg).map(|task| (task.id, task.title))
}

/// What marks an entry a thread puts in its bot's chat as its task's (`data.author`, `data.thread`, `data.task`).
pub struct TaskMark {
    /// The thread's bot, whose chat it is.
    pub bot: String,
    thread: String,
    title: String,
    task: Option<String>,
}

impl TaskMark {
    /// The thread it marks as.
    pub fn thread(&self) -> &str {
        &self.thread
    }

    /// Marks `data`: to the user it is the bot's; an answer finds the thread by `data.thread.id` ([`card_actor`]).
    pub fn apply(&self, data: &mut Value) {
        data["author"] = self.bot.as_str().into();
        data["thread"] = json!({"id": self.thread, "title": self.title});
        data["task"] = self.task.clone().into();
    }
}

/// From thread `cfg`: what marks an entry in its bot's chat as its task's, named by what the user knows it by. None
/// for a bot's own.
pub fn mark(store: &Store, cfg: &BotConfig) -> Option<TaskMark> {
    let bot = cfg.thread_of.clone()?;
    let (task, title) = card_task(store, cfg).unzip();
    Some(TaskMark { bot, thread: cfg.id.clone(), title: title.unwrap_or_default(), task })
}

/// The session a thread's first session forks, if any; taken once.
pub fn take_fork_source(hub: &Hub, cfg: &BotConfig) -> Option<String> {
    let mut source = None;
    update(hub, cfg, "its fork", |task| source = task.fork_from.take());
    source
}

/// Whether `cfg` is a thread whose session hasn't begun yet.
pub fn first_session(store: &Store, cfg: &BotConfig) -> bool {
    cfg.is_thread() && task_of(store, cfg).is_some_and(|task| task.start.is_none())
}

/// The agent couldn't fork the chat's session for this thread.
pub fn fork_failed(hub: &Hub, cfg: &BotConfig) {
    update(hub, cfg, "its failed fork", |task| task.fork_failed = true);
}

/// How the thread's session began, once its actor knows.
pub fn started(hub: &Hub, cfg: &BotConfig, start: Start) {
    update(hub, cfg, "how it started", |task| task.start = Some(start));
}

/// What a thread that starts from a new session is told of the chat it comes from.
pub fn chat_so_far(store: &Store, bot: &str) -> Option<String> {
    let session = store.bot(bot).ok().flatten()?.session_id.unwrap_or_default();
    let text = crate::chat::handoff::build(store, bot, &session).ok()?;
    Some(format!("[The chat you come from, so far]\n{text}"))
}

/// `bot`'s own turn ended: the tasks it added may start now (a fork taken mid-turn would carry an unanswered tool
/// call). A turn the user stopped takes the tasks it added with it.
pub fn turn_over(hub: &Arc<Hub>, bot: &str, stopped: bool) {
    hub.work.chain.locked().remove(bot);
    let added = hub.work.added.locked().remove(bot).unwrap_or_default();
    if stopped && !added.is_empty() {
        let dropped = tasks::change(hub, bot, |tasks| {
            tasks.retain(|i| !(i.status == Status::Waiting && added.contains(&i.id)));
            Ok(())
        });
        if let Err(error) = dropped {
            tracing::warn!(bot, error = format!("{error:#}"), "couldn't drop a stopped turn's work");
        }
    }
    dispatch::run(hub, bot, Some(bot));
}

/// Thread `cfg`'s turn ended: its task says how, its result goes where its source reads it (the bot's own work: a
/// report, as a turn in the chat), and the dispatcher decides what the thread does next. A task the user stopped
/// reports nothing: they know. While a message the user wrote to its thread still waits for its turn the task isn't
/// over: that turn's end is its end.
pub fn finished(hub: &Arc<Hub>, cfg: &BotConfig, outcome: Outcome) {
    let Some(bot) = cfg.thread_of.as_deref() else { return };
    let (owner, thread) = (bot.to_owned(), cfg.id.clone());
    let next = move |hub: &Arc<Hub>| dispatch::run(hub, &owner, Some(&thread));
    let ended = tasks::change(hub, bot, |tasks| {
        Ok(if thread::unheard(&hub.store, &cfg.id) { None } else { end_claimed(tasks, &cfg.id, outcome) })
    });
    match ended {
        Ok(Some(over)) => close(hub, bot, over, next),
        Ok(None) => next(hub),
        Err(error) => {
            host_error!(hub, "couldn't save a task's end", thread = %cfg.id, error = format!("{error:#}"));
            next(hub);
        }
    }
}

/// A task that just ended, with its worktree when that is to be looked over before its report says where the
/// work is.
type Over = (Task, Option<Worktree>);

/// [`end`], with the ended task's worktree claimed.
fn end_claimed(tasks: &mut [Task], thread: &str, outcome: Outcome) -> Option<Over> {
    let task = end(tasks, thread, outcome)?;
    let worktree = worktree::claim(tasks, &task.id);
    Some((task, worktree))
}

/// `bot`'s task ended: its worktree is looked over if it has one to, it says so and its result goes where its
/// source reads it ([`ended`]), and `then` follows.
fn close(hub: &Arc<Hub>, bot: &str, (task, worktree): Over, then: impl FnOnce(&Arc<Hub>) + Send + 'static) {
    let Some(worktree) = worktree else {
        ended(hub, bot, &task);
        return then(hub);
    };
    let owner = bot.to_owned();
    worktree::settle_then(hub, bot, &task.id.clone(), worktree, move |hub| {
        let undispatched = task.undispatched;
        let task = tasks::load(&hub.store, &owner).into_iter().find(|i| i.id == task.id).unwrap_or(task);
        ended(hub, &owner, &Task { undispatched, ..task });
        then(hub);
    });
}

/// Ends the task `thread` works on, in `tasks`. None: it has none (stopped, or it gave way), or a host restart or a
/// lost thread cut it off for the first time and it waits again.
fn end(tasks: &mut [Task], thread: &str, outcome: Outcome) -> Option<Task> {
    let front = tasks::front(tasks);
    let task = tasks.iter_mut().find(|i| i.status == Status::Working && i.thread.as_deref() == Some(thread))?;
    let (status, result, failure) = match outcome {
        // Only the bot's own work starts again: a routine run or a request done twice could act twice.
        Outcome::Failed(Failure::Restart | Failure::Lost, _) if !task.restarted && task.source.is_bot() => {
            task.set_aside(front);
            task.restarted = true;
            task.cut_in = true;
            return None;
        }
        Outcome::Done(text) => {
            // A report to the bot's own turn carries only so much; a routine run's or a request's result is posted
            // whole, as its message.
            let most = if task.source.is_bot() { MAX_REPORT } else { usize::MAX };
            let text = text.map(|t| crate::agent::acp::truncate(t.trim(), most)).filter(|t| !t.is_empty());
            match text {
                Some(text) => (Status::Done, Some(Ok(text)), None),
                // A routine run with nothing to tell says nothing (`(pass)`).
                None if matches!(task.source, Source::Routine { .. }) => (Status::Done, Some(Ok(String::new())), None),
                None => (Status::Done, Some(Err("it ended without a report".to_owned())), Some(Failure::NoReport)),
            }
        }
        Outcome::Failed(failure, why) => (Status::Failed, Some(Err(why)), Some(failure)),
        // To its bot, and in the box, a turn that never began; only `work_finished` tells the two apart.
        Outcome::Undispatched(why) => {
            task.undispatched = true;
            (Status::Failed, Some(Err(why)), Some(Failure::Start))
        }
        Outcome::Stopped => (Status::Stopped, None, None),
    };
    task.status = status;
    task.ended_at = Some(now_ms());
    task.result = result.as_ref().map(|r| r.clone().unwrap_or_else(|why| why)).filter(|r| !r.is_empty());
    task.failure = failure;
    task.report = result.map(|outcome| Report {
        task: task.id.clone(),
        title: task.title.clone(),
        outcome,
        hops: task.hops,
        worktree: None,
        told: std::mem::take(&mut task.told),
    });
    Some(task.clone())
}

/// `work_finished` for `task`, which just ended. `agent`: the one its thread was given, where a thread took it (a turn
/// that fell back names the agent it ran on in `turn_finished`).
fn finished_properties(task: &Task, agent: Option<&str>) -> serde_json::Map<String, Value> {
    let mut properties = json!({
        "outcome": task.status, "start": task.start, "forkFailed": task.fork_failed,
        "minutes": (now_ms() - task.started_at.unwrap_or(task.created_at)) / 60_000,
        "source": task.source.kind(), "worktree": task.worktree.is_some(),
    });
    if let Some(agent) = agent {
        properties["agent"] = agent.into();
    }
    if let Some(failure) = task.failure {
        properties["error"] = if task.undispatched { "dispatch".into() } else { json!(failure) };
    }
    if task.worktree.as_ref().is_some_and(|w| w.left == worktree::Left::Kept) {
        properties["kept"] = true.into();
    }
    if task.place == worktree::Place::NoWorktree {
        properties["worktreeFailed"] = true.into();
    }
    properties.as_object().cloned().unwrap_or_default()
}

/// Sends what `task` came to where its source reads it: the bot's own work, as a turn in its chat; a routine run,
/// as the routine's message there; a request, as the asking bot's reply. A report that can't be handed on stays on
/// its task for the next host start.
fn deliver(hub: &Hub, bot: &str, task: &Task) {
    if task.source.is_bot() {
        // With where its work is, as its worktree stands now.
        let report = task.report.clone().map(|report| Report { worktree: task.worktree.clone(), ..report });
        if let Some(report) = report
            && let Err(error) = hub.send_cmd(bot, Cmd::TaskReport(report))
        {
            tracing::warn!(task = task.id, error = format!("{error:#}"), "couldn't hand a task's report to its bot");
        }
    } else if sources::deliver(hub, bot, task) && task.report.is_some() {
        // Nothing of the bot's takes a turn for it: it has arrived.
        reported(hub, bot, std::slice::from_ref(&task.id));
    }
}

/// `task` of `bot` ended: says so (`work_finished`), and its result goes where its source reads it.
fn ended(hub: &Hub, bot: &str, task: &Task) {
    let thread = task.thread.as_deref().and_then(|thread| hub.store.bot(thread).ok().flatten());
    let agent = thread.as_ref().map(|thread| thread.config.backend.as_str());
    analytics::capture(hub, Event::WorkFinished, finished_properties(task, agent));
    deliver(hub, bot, task);
}

/// A deleted bot's box goes with it, and so do the requests it hadn't answered.
pub fn forget(hub: &Hub, bot: &str) {
    let tasks = tasks::load(&hub.store, bot);
    sources::forget_requests(hub, &tasks);
    worktree::leave(&tasks);
    tasks::forget(&hub.store, bot);
}

/// The reports of these tasks of `bot` arrived: the bot's turn for them is over (they reached the user, from the
/// bot or as the chat's own notice), or they went where a routine's or a request's go.
pub fn reported(hub: &Hub, bot: &str, delivered: &[String]) {
    let cleared = tasks::change(hub, bot, |tasks| {
        for task in tasks.iter_mut().filter(|i| delivered.contains(&i.id)) {
            task.report = None;
        }
        Ok(())
    });
    if let Err(error) = cleared {
        tracing::warn!(bot, error = format!("{error:#}"), "couldn't save that reports were delivered");
    }
}

/// At host start: old tasks go, and the others pick up where the last process left them.
pub fn restore(hub: &Arc<Hub>) {
    let lives = |bot: &str| hub.store.bot(bot).ok().flatten().is_some_and(|r| !r.deleted);
    let threads = hub.store.thread_bots().unwrap_or_default();
    for thread in threads.iter().filter(|thread| !thread.config.thread_of.as_deref().is_some_and(lives)) {
        if let Err(error) = hub.delete_bot(&thread.config.id) {
            tracing::warn!(thread = %thread.config.id, error = format!("{error:#}"), "couldn't remove a thread");
        }
    }
    // Every bot with a box, or with a thread bot left to clear away.
    let mut bots: Vec<String> =
        hub.store.kv_prefix(tasks::PREFIX).iter().map(|key| key.trim_start_matches(tasks::PREFIX).to_owned()).collect();
    bots.extend(threads.into_iter().filter_map(|thread| thread.config.thread_of));
    bots.sort();
    bots.dedup();
    for bot in &bots {
        if !lives(bot) {
            forget(hub, bot);
        } else if let Err(error) = pick_up(hub, bot) {
            host_error!(hub, "couldn't pick up a bot's work", bot, error = format!("{error:#}"));
        }
    }
    sources::settle_runs(hub);
    worktree::tidy(hub);
}

/// `bot`'s box at host start: its threads resume the turns the last process cut off, a task whose thread can't ends,
/// and the reports that never arrived are sent again.
fn pick_up(hub: &Arc<Hub>, bot: &str) -> Result<()> {
    let (resumed, failed, undelivered, recheck) = tasks::change(hub, bot, |tasks| {
        let now = now_ms();
        tasks.retain(|i| i.ended_at.is_none_or(|at| now - at <= KEEP_MS));
        let recheck = worktree::recheck(tasks);
        // A request lived in the last process (its notices already say it was interrupted): it is never replayed.
        tasks.retain(|i| i.status.ended() || !matches!(i.source, Source::Request { .. }));
        // A thread's actor resumes the turn the last process cut off; when it won't, nothing of the thread runs.
        let (resumed, cut_off): (Vec<String>, Vec<String>) = tasks
            .iter()
            .filter(|i| i.status == Status::Working)
            .filter_map(|i| i.thread.clone())
            .partition(|thread| crate::agent::bot::resumable(&hub.store, thread));
        // The reports that never arrived, before this start's own are added.
        let undelivered: Vec<Task> = tasks.iter().filter(|i| i.report.is_some()).cloned().collect();
        let failed: Vec<Over> = cut_off
            .iter()
            .filter_map(|thread| end_claimed(tasks, thread, Outcome::Failed(Failure::Restart, CUT_OFF.into())))
            .collect();
        Ok((resumed, failed, undelivered, recheck))
    })?;
    for (task, kept) in recheck {
        let owner = bot.to_owned();
        worktree::settle_then(hub, bot, &task, kept, move |hub| dispatch(hub, &owner));
    }
    for thread in resumed {
        if let Ok(Some(row)) = hub.store.bot(&thread) {
            sources::resumed(hub, &row.config);
            hub.spawn_bot(row.config);
        }
    }
    for over in failed {
        close(hub, bot, over, |_| {});
    }
    for task in &undelivered {
        deliver(hub, bot, task);
    }
    dispatch(hub, bot);
    Ok(())
}

/// `threadEntries`: a thread's transcript, newest `limit`, for its details. `bot` must be the thread's own.
pub fn entries(hub: &Hub, bot: &str, id: &str, limit: i64) -> Result<Value> {
    let row = hub.store.bot(id)?.filter(|r| !r.deleted && r.config.thread_of.as_deref() == Some(bot));
    let cfg = row.ok_or_else(|| anyhow!("unknown thread"))?.config;
    // What it works on, else what it worked on last.
    let mut worked: Vec<Task> =
        tasks::load(&hub.store, bot).into_iter().filter(|i| i.thread.as_deref() == Some(id)).collect();
    worked.sort_by_key(|i| (i.status.ended(), i.started_at));
    let task = worked.iter().find(|i| !i.status.ended()).or(worked.last()).ok_or_else(|| anyhow!("unknown thread"))?;
    Ok(json!({
        "thread": {"id": cfg.id, "title": task.title, "status": task.status, "startedAt": task.started_at,
                 "endedAt": task.ended_at, "start": task.start},
        "entries": hub.store.history(id, i64::MAX, limit)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn working(id: &str, thread: &str) -> Task {
        serde_json::from_value(json!({
            "id": id, "title": id, "brief": "x", "status": "working", "order": 5, "thread": thread, "createdAt": 0,
            "startedAt": 1, "hops": 2,
        }))
        .expect("a task")
    }

    #[test]
    fn a_task_ends_as_its_threads_turn_did() {
        let mut tasks = [working("a", "t1"), working("b", "t2"), working("c", "t3")];
        let done = end(&mut tasks, "t1", Outcome::Done(Some(" All green. ".into()))).expect("ended");
        assert_eq!((done.status, done.result.as_deref(), done.failure), (Status::Done, Some("All green."), None));
        let report = done.report.expect("a report for its bot");
        assert_eq!((report.task.as_str(), report.outcome, report.hops), ("a", Ok("All green.".into()), 2));

        let silent = end(&mut tasks, "t2", Outcome::Done(None)).expect("ended");
        assert_eq!(silent.failure, Some(Failure::NoReport));
        assert_eq!(silent.report.expect("a report").outcome, Err("it ended without a report".into()));

        let stopped = end(&mut tasks, "t3", Outcome::Stopped).expect("ended");
        assert!(stopped.status == Status::Stopped && stopped.report.is_none(), "the user knows");
        assert!(end(&mut tasks, "t3", Outcome::Stopped).is_none(), "its thread has no task any more");
        assert_eq!(json!(Failure::EndedEarly), "ended_early");
    }

    #[test]
    fn work_finished_names_the_agent_and_what_the_host_could_not_hand_over() {
        let mut tasks = [working("a", "t1"), working("b", "t2")];
        let error = anyhow!("no such chat");
        let lost = end(&mut tasks, "t1", Outcome::not_dispatched(&error)).expect("ended");
        assert_eq!(lost.result.as_deref(), Some("it could not start: no such chat"), "to its bot, it could not start");
        assert_eq!(lost.failure, Some(Failure::Start), "as its task says, for its routine or request");
        let saved = serde_json::to_string(&tasks).expect("a box");
        assert!(!saved.contains("dispatch"), "a host from before `dispatch` still reads the box: {saved}");
        let told = finished_properties(&lost, Some("claude"));
        assert_eq!(
            (&told["outcome"], &told["error"], &told["agent"]),
            (&json!("failed"), &json!("dispatch"), &json!("claude"))
        );
        let never = end(&mut tasks, "t2", Outcome::not_started(&error)).expect("ended");
        let told = finished_properties(&never, None);
        assert_eq!((told["error"].as_str(), told.get("agent")), (Some("start"), None), "no thread, no agent");
    }

    #[test]
    fn only_the_bots_own_report_is_cut() {
        let long = "x".repeat(MAX_REPORT + 10);
        let mut tasks = [working("a", "t1"), working("b", "t2")];
        tasks[1].source = Source::Routine { id: "r".into(), run: "1".into() };
        let report = end(&mut tasks, "t1", Outcome::Done(Some(long.clone()))).expect("ended");
        assert!(report.result.expect("a result").len() < long.len());
        let routine = end(&mut tasks, "t2", Outcome::Done(Some(long.clone()))).expect("ended");
        assert_eq!(routine.result, Some(long), "a routine's message is posted whole");
    }

    #[test]
    fn a_task_a_restart_cut_off_waits_again_once_then_fails() {
        let mut tasks = [working("a", "t1")];
        assert!(end(&mut tasks, "t1", Outcome::Failed(Failure::Restart, CUT_OFF.into())).is_none());
        let task = &tasks[0];
        assert!(task.status == Status::Waiting && task.cut_in && task.thread.is_none(), "{task:?}");

        tasks[0].status = Status::Working;
        tasks[0].thread = Some("t2".into());
        let failed = end(&mut tasks, "t2", Outcome::Failed(Failure::Restart, CUT_OFF.into())).expect("ended");
        assert_eq!((failed.status, failed.failure), (Status::Failed, Some(Failure::Restart)));
        assert_eq!(failed.result.as_deref(), Some(CUT_OFF));
    }

    #[test]
    fn a_routine_run_or_a_request_is_never_done_twice_and_a_silent_run_is_no_failure() {
        let routine = Source::Routine { id: "r".into(), run: "run".into() };
        let mut tasks = [
            Task { source: routine.clone(), ..working("a", "t1") },
            Task { source: Source::Request { id: None }, ..working("b", "t2") },
            Task { source: routine, ..working("c", "t3") },
        ];
        for thread in ["t1", "t2"] {
            let cut = end(&mut tasks, thread, Outcome::Failed(Failure::Lost, "lost".into())).expect("it ends at once");
            assert_eq!((cut.status, cut.failure), (Status::Failed, Some(Failure::Lost)), "{thread}");
        }
        let silent = end(&mut tasks, "t3", Outcome::Done(None)).expect("ended");
        assert_eq!((silent.status, silent.failure, silent.result), (Status::Done, None, None));
        assert_eq!(silent.report.expect("the run's end still goes to its routine").outcome, Ok(String::new()));
        assert_eq!(json!(Failure::TimedOut), "timed_out");
    }
}
