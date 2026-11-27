//! Work a bot does away from its chat, so the chat keeps answering
//! ([the box](../../../../docs/plans/event-box.md), [threads](../../../../docs/features/work.md)).
//!
//! What the bot should work on is a **task** in its box ([`tasks`]). A **thread** works tasks, one at a time: a
//! **thread bot**, a `bots` row of its own (`BotConfig::thread_of`) copied from its bot, with its own actor, agent
//! process, session and transcript, because everything a running turn touches is keyed by bot id. Its memory is its
//! bot's (`BotConfig::as_owner`). It starts in a new session of its own with its task's message, never told the chat;
//! its instructions depend on where the task came from (`chat::context::render`). [`dispatch`] gives waiting tasks to
//! threads, up to the bot's `maxThreads`; a thread's last message of a turn is its task's result and goes back as a turn
//! in the bot's chat (`Cmd::TaskReport`). A routine run and another bot's request are tasks too ([`sources`]): their
//! result goes to the chat as the routine's message, or to the asking bot as its reply. No client lists a thread bot:
//! they see `work` on the bot and fetch a thread's transcript by id.
//!
//! Here: a thread's life from its turn's end to its report. [`steer`] has what the user and the bot do to a task,
//! [`tools`] the bot's tools ([`recall`] the one that looks back, [`ask`] the one that asks a thread and waits for its
//! answer), [`prompts`] the words, [`worktree`] the git worktree of a thread that works beside another in one
//! repository ([`fill`] what a new one is given), [`plain`] threads without a bot, [`watch`] the watchdog that wakes
//! the bot for a thread that looks stuck, [`archive`] the threads that ended over a week ago.

pub mod archive;
pub mod ask;
mod dispatch;
mod fill;
pub mod plain;
mod prompts;
mod recall;
mod sources;
mod steer;
mod sweep;
mod tasks;
mod thread;
mod tools;
mod watch;
mod worktree;

pub use dispatch::{Wait, dispatch};
pub use prompts::{BRIEF, asked, in_thread, relayed, reminder, report_notice, report_prompt};
pub use sources::{access_of, add_request, add_routine, add_thread, boxed, routines_changed, source_of};
pub use steer::{Steer, cut_in, drop_task, first, retry, stop};
#[cfg(test)]
pub use tasks::put_working_for_test;
pub use tasks::{Setup, Source, Status, doing, lines, outside, waiting_threads, wire, working_threads};
pub use thread::{Refused as ThreadRefused, heard, message, picked_up, thread as thread_of, unhear, written};
pub use tools::{TOOLS, acting_as, call, chat_only, refuse_chat_only, refuse_in_thread, tools};
pub use watch::Stuck;
pub use worktree::{Worktree, enter as enter_worktree, in_repo, is_worktree};

use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::analytics::errors::host_error;
use crate::analytics::{self, Event};
use crate::hub::Hub;
use crate::store::{BotConfig, Entry, Store, now_ms};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use tasks::Task;

/// Tasks added in a row with no message of the user's in between (a report's turn adding the next).
const MAX_HOPS: u8 = 8;
const _: () = assert!(MAX_HOPS == 8, "chat::team::limit::NOTICE and its translations say 8");
/// Tasks a box holds waiting at most.
const MAX_WAITING: usize = 20;
/// An ended task stays in its box this long, then goes to the [`archive`] with its thread, kept until the user's
/// `threadKeepDays`; `recall_task` still reads it for [`tasks::PAST_MS`].
const KEEP_MS: i64 = 7 * 24 * 60 * 60 * 1000;
const MAX_TITLE: usize = 80;
const MAX_BRIEF: usize = 32_000;
/// How much of a thread's last message its task's report carries.
const MAX_REPORT: usize = 8_000;
/// Why a task that a host restart cut off, and that can't pick up, didn't finish.
pub const CUT_OFF: &str = "the computer's Codync restarted while it worked";
/// Why a task whose thread's turn never began didn't finish; the error follows.
const NOT_STARTED: &str = "it could not start: ";

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
    /// It ended normally while work it started in the background still runs (`agent::bot::background_work`): what
    /// runs (`running`, in words that fit "while … still runs"), and its last message (`text`). The first time its
    /// task waits on it: that message is a progress line, not a report.
    LeftRunning {
        running: String,
        text: Option<String>,
    },
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
    /// Its thread ended its turn while work it started in the background still ran, after it was asked to wait for
    /// it once (#634).
    Background,
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
    /// Bots one of whose threads began to wait on the user, so gave its seat up: each box is looked over again.
    freed: Mutex<BTreeSet<String>>,
    seat_freed: tokio::sync::Notify,
    /// What the watchdog remembers of the threads it looks at (`watch`).
    watch: Mutex<watch::Memory>,
    /// Thread → the question of its bot's it hasn't answered (`ask`). Gone with the host process: an answer that
    /// comes after a restart is the thread's report's.
    asks: Mutex<HashMap<String, ask::Pending>>,
}

impl Work {
    pub fn answering(&self, bot: &str, hops: u8) {
        self.chain.locked().insert(bot.to_owned(), hops);
    }

    fn hops(&self, bot: &str) -> u8 {
        self.chain.locked().get(bot).copied().unwrap_or(0)
    }

    /// A thread of `bot`'s began to wait on the user (`Hub::set_runtime`): its seat is free for the next task. Called
    /// where no `Arc<Hub>` is at hand; the look runs on the task [`restore`] starts.
    pub fn seat_freed(&self, bot: &str) {
        self.freed.locked().insert(bot.to_owned());
        self.seat_freed.notify_one();
    }
}

/// The bot was just paused: a request that had not started is answered now, and a routine task whose run the pause
/// cancelled leaves the box. The threads already working it stop through their own actors.
pub fn paused(hub: &Hub, bot: &str) {
    // One change of the box, so an insert that is checking the pause generation cannot land between the snapshot
    // and the removal.
    let refused = match tasks::change(hub, bot, |tasks| {
        let ids: Vec<String> = tasks
            .iter()
            .filter(|task| task.status == Status::Waiting)
            .filter(|task| matches!(task.source, Source::Request { .. }))
            .map(|task| task.id.clone())
            .collect();
        let refused: Vec<crate::chat::team::BotRequest> = {
            let mut requests = hub.work.requests.locked();
            ids.iter().filter_map(|id| requests.remove(id)).collect()
        };
        tasks.retain(|task| {
            if task.status != Status::Waiting {
                return true;
            }
            match &task.source {
                Source::Request { .. } => !ids.contains(&task.id),
                Source::Routine { run, .. } => hub.routines.waits(run),
                Source::Bot | Source::User => true,
            }
        });
        Ok(refused)
    }) {
        Ok(refused) => refused,
        Err(error) => {
            host_error!(hub, "couldn't clear a paused bot's box", bot, error = format!("{error:#}"));
            return;
        }
    };
    for request in refused {
        request.complete(Err(crate::chat::team::no_reply::not_started("this bot is paused")), false);
    }
}

/// Looks over each box one of whose threads gave its seat up ([`Work::seat_freed`]), for as long as the host runs.
fn look_when_seats_free(hub: &Arc<Hub>) {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
    let hub = hub.clone();
    runtime.spawn(async move {
        loop {
            hub.work.seat_freed.notified().await;
            let bots = std::mem::take(&mut *hub.work.freed.locked());
            for bot in bots {
                dispatch(&hub, &bot);
            }
        }
    });
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
    // A thread keeps the agent it started on: it gets the chain that starts there (the threads' own in Advisor mode,
    // else the chat's), and one that started on another keeps its own.
    let chains = [crate::agent::fallback::threads(cfg), cfg.clone()];
    let login = crate::agent::fallback::login::of;
    for thread in &mut threads {
        if let Some(chain) = chains.iter().find(|chain| login(chain) == login(thread)) {
            thread.fallbacks.clone_from(&chain.fallbacks);
        }
    }
    if let Err(error) = hub.store.save_bots(&threads) {
        tracing::warn!(bot = %cfg.id, error = format!("{error:#}"), "couldn't give its threads the bot's fallbacks");
        return;
    }
    for thread in threads.into_iter().filter(|thread| hub.has_actor(&thread.id)) {
        let id = thread.id.clone();
        let pause_epoch = hub.pause.epoch(thread.owner());
        if let Err(error) = hub.send_cmd(&id, Cmd::Reconfigure { base: Box::new(thread), pause_epoch }) {
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

/// Thread `cfg`'s session began on its task: once, so a later session of the thread (a restart, a fallback) doesn't
/// make the task's worktree again.
pub fn began(hub: &Hub, cfg: &BotConfig) {
    if cfg.is_thread() && task_of(&hub.store, cfg).is_some_and(|task| !task.began) {
        update(hub, cfg, "its start", |task| task.began = true);
    }
}

/// `bot`'s own turn ended: the tasks it added may start now, since stopping that turn drops them. A turn the user
/// stopped takes the tasks it added with it.
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
    dispatch::run(hub, bot, Some(bot), None);
}

/// Thread `cfg`'s turn ended: its task says how, its result goes where its source reads it (the bot's own work: a
/// report, as a turn in the chat), and the dispatcher decides what the thread does next. A task the user stopped
/// reports nothing: they know. While a message the user wrote to its thread still waits for its turn the task isn't
/// over: that turn's end is its end. Nor is it the first time its turn ends while what it started in the background
/// still runs ([`hold`]). Whether it goes on with what the turn left running, either way: then its agent and session
/// stay as they are.
pub fn finished(hub: &Arc<Hub>, cfg: &BotConfig, outcome: Outcome) -> bool {
    let Some(bot) = cfg.thread_of.as_deref() else { return false };
    let (owner, thread) = (bot.to_owned(), cfg.id.clone());
    let next = move |hub: &Arc<Hub>| dispatch::run(hub, &owner, Some(&thread), None);
    let left = matches!(outcome, Outcome::LeftRunning { .. });
    let mut goes_on = false;
    let ended = tasks::change(hub, bot, |tasks| {
        goes_on = thread::unheard(&hub.store, &cfg.id);
        if goes_on {
            return Ok(None);
        }
        if let Outcome::LeftRunning { running, .. } = &outcome {
            goes_on = hold(hub, cfg, tasks, running);
            if goes_on {
                return Ok(None);
            }
        }
        Ok(end_claimed(tasks, &cfg.id, outcome))
    });
    match ended {
        Ok(Some(over)) => close(hub, bot, over, next),
        Ok(None) => next(hub),
        Err(error) => {
            host_error!(hub, "couldn't save a task's end", thread = %cfg.id, error = format!("{error:#}"));
            next(hub);
        }
    }
    goes_on && left
}

/// Thread `cfg` ended its turn while `running` still runs: the first time, its task goes on, and a message from the
/// host in its thread asks it to wait for that and report then (`work_held`). Whether it does. A message that
/// can't be posted ends the task as it is.
fn hold(hub: &Arc<Hub>, cfg: &BotConfig, tasks: &mut [Task], running: &str) -> bool {
    let task = tasks.iter_mut().find(|i| i.status == Status::Working && i.thread.as_deref() == Some(&cfg.id));
    let Some(task) = task.filter(|task| !task.held) else { return false };
    let data = json!({"text": prompts::wait_for(running), "from": "host"});
    if let Err(error) = thread::post_to(hub, cfg, data) {
        host_error!(hub, "couldn't ask a thread to wait for its background work", thread = %cfg.id,
            error = format!("{error:#}"));
        return false;
    }
    task.held = true;
    let properties = json!({"agent": cfg.backend, "source": task.source.kind()});
    analytics::capture(hub, Event::WorkHeld, properties.as_object().cloned().unwrap_or_default());
    true
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
        Outcome::Failed(Failure::Restart | Failure::Lost, _) if !task.restarted && task.source.own() => {
            task.set_aside(front);
            task.restarted = true;
            task.cut_in = true;
            return None;
        }
        Outcome::Done(text) => {
            match report_text(task, text) {
                Some(text) => (Status::Done, Some(Ok(text)), None),
                // A routine run with nothing to tell says nothing (`(pass)`).
                None if matches!(task.source, Source::Routine { .. }) => (Status::Done, Some(Ok(String::new())), None),
                None => (Status::Done, Some(Err("it ended without a report".to_owned())), Some(Failure::NoReport)),
            }
        }
        Outcome::Failed(failure, why) => (Status::Failed, Some(Err(why)), Some(failure)),
        // Asked to wait once already, or the host couldn't ask: what it said is kept, after why.
        Outcome::LeftRunning { running, text } => {
            let why = format!("it ended its turn while {running} still ran");
            let why = match report_text(task, text) {
                Some(text) => format!("{why}. Its last message:\n\n{text}"),
                None => why,
            };
            (Status::Failed, Some(Err(why)), Some(Failure::Background))
        }
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

/// What its thread's last message gives as `task`'s report: one to the bot's own turn carries only so much; a routine
/// run's or a request's result is posted whole, as its message.
fn report_text(task: &Task, text: Option<String>) -> Option<String> {
    let most = if task.source.own() { MAX_REPORT } else { usize::MAX };
    text.map(|t| codync_agent::acp::truncate(t.trim(), most)).filter(|t| !t.is_empty())
}

/// `work_finished` for `task`, which just ended. `agent`: the one its thread was given, where a thread took it (a turn
/// that fell back names the agent it ran on in `turn_finished`).
fn finished_properties(task: &Task, agent: Option<&str>) -> serde_json::Map<String, Value> {
    let mut properties = json!({
        "outcome": task.status,
        "minutes": (now_ms() - task.started_at.unwrap_or(task.created_at)) / 60_000,
        "source": task.source.kind(), "worktree": task.worktree.is_some(),
    });
    if let Some(started) = task.started_at {
        properties["waitedMs"] = (started - task.created_at).max(0).into();
    }
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
    // It was asked to wait for what it left running (#634): `outcome` says whether it then reported.
    if task.held {
        properties["held"] = true.into();
    }
    properties.as_object().cloned().unwrap_or_default()
}

/// Sends what `task` came to where its source reads it: the bot's own work, as a turn in its chat; a routine run,
/// as the routine's message there; a request, as the asking bot's reply; a thread the user started, nowhere (they
/// read it there). A report that can't be handed on stays on its task for the next host start.
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
    if let Some(thread) = &task.thread {
        ask::forget(hub, thread);
    }
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
    plain::forget_folders(&tasks);
    tasks::forget(&hub.store, bot);
    archive::forget(hub, bot);
}

/// `bot`'s tasks that ended over [`KEEP_MS`] ago leave its box for the archive.
fn age(hub: &Hub, bot: &str) {
    let now = now_ms();
    let aged = tasks::change(hub, bot, |tasks| {
        let old = |i: &Task| i.ended_at.is_some_and(|at| now - at > KEEP_MS);
        if !tasks.iter().any(old) {
            return Ok(());
        }
        let (old, kept): (Vec<Task>, Vec<Task>) = std::mem::take(tasks).into_iter().partition(old);
        // On an error `change` saves nothing: the box stays as it was.
        archive::put(hub, bot, &old, &kept)?;
        *tasks = kept;
        Ok(())
    });
    if let Err(error) = aged {
        host_error!(hub, "couldn't archive a bot's old threads", bot, error = format!("{error:#}"));
    }
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
    look_when_seats_free(hub);
    watch::start(hub);
    archive::move_old_past(hub);
    let lives = |bot: &str| hub.store.bot(bot).ok().flatten().is_some_and(|r| !r.deleted);
    let threads = hub.store.thread_bots().unwrap_or_default();
    for thread in threads.iter().filter(|thread| !thread.config.thread_of.as_deref().is_some_and(lives)) {
        if let Err(error) = hub.delete_bot(&thread.config.id) {
            tracing::warn!(thread = %thread.config.id, error = format!("{error:#}"), "couldn't remove a thread");
        }
    }
    // Every bot with a box, or with a thread bot left to clear away.
    let mut bots = tasks::bots(&hub.store);
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
    archive::prune(hub);
    archive::look_daily(hub);
}

/// `bot`'s box at host start: its threads resume the turns the last process cut off, a task whose thread can't ends,
/// and the reports that never arrived are sent again.
fn pick_up(hub: &Arc<Hub>, bot: &str) -> Result<()> {
    age(hub, bot);
    let (resumed, failed, undelivered, recheck) = tasks::change(hub, bot, |tasks| {
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
    // A thread that ended over a week ago is parked, its task in the archive.
    let cfg = archive::thread(&hub.store, bot, id).ok_or_else(|| anyhow!("unknown thread"))?;
    // What it works on, else what it worked on last.
    let mut worked: Vec<Task> =
        tasks::load(&hub.store, bot).into_iter().filter(|i| i.thread.as_deref() == Some(id)).collect();
    if worked.is_empty() {
        worked.extend(archive::last_of(&hub.store, bot, id));
    }
    worked.sort_by_key(|i| (i.status.ended(), i.started_at));
    let task = worked.iter().find(|i| !i.status.ended()).or(worked.last()).ok_or_else(|| anyhow!("unknown thread"))?;
    Ok(json!({
        "thread": {"id": cfg.id, "title": task.title, "status": task.status, "startedAt": task.started_at,
                 "endedAt": task.ended_at},
        "entries": hub.store.history(id, i64::MAX, limit)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_host_start_a_task_ended_over_a_week_ago_leaves_the_box_for_the_archive() {
        use crate::chat::calls::context::tests::{bot, hub};
        let hub = hub();
        hub.store.save_bot(&bot("b1", "Mika", "/tmp")).expect("saved");
        hub.set_runtime("b1", |rt| rt.status = crate::hub::BotStatus::Working);
        let (now, day) = (now_ms(), 24 * 60 * 60 * 1000);
        let ended = |id: &str, ago: i64| {
            json!({"id": id, "title": id, "brief": "x", "status": "done", "order": 0, "createdAt": 0,
                "endedAt": now - ago * day})
        };
        let tasks = json!([ended("old", 8), ended("new", 1)]);
        hub.store.kv_set(&format!("{}b1", tasks::PREFIX), &tasks.to_string()).expect("saved");

        pick_up(&hub, "b1").expect("picked up");
        let ids = |tasks: Vec<Task>| tasks.into_iter().map(|i| i.id).collect::<Vec<_>>();
        assert_eq!(ids(tasks::load(&hub.store, "b1")), ["new"]);
        assert_eq!(ids(archive::recent(&hub.store, "b1", 0, 10)), ["old"]);
    }

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
    fn a_task_that_ends_its_turn_again_while_its_background_work_runs_fails_saying_what_ran() {
        let mut tasks = [Task { held: true, ..working("a", "t1") }];
        let running = "a command started in the background".to_owned();
        let text = Some(" Still waiting for it ".to_owned());
        let failed = end(&mut tasks, "t1", Outcome::LeftRunning { running, text }).expect("ended");
        assert_eq!((failed.status, failed.failure), (Status::Failed, Some(Failure::Background)));
        assert_eq!(
            failed.result.as_deref(),
            Some(
                "it ended its turn while a command started in the background still ran. Its last message:\n\nStill waiting for it"
            )
        );
        let told = finished_properties(&failed, None);
        assert_eq!((&told["error"], &told["held"]), (&json!("background"), &json!(true)));
        let saved = serde_json::to_value(&failed).expect("a task");
        assert_eq!(saved["held"], true);
        assert!(serde_json::to_value(working("b", "t2")).expect("a task").get("held").is_none(), "only once held");
        let mut tried = failed;
        tried.set_aside(0);
        assert!(!tried.held, "tried again, it may be asked to wait again");
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
        assert_eq!(told["waitedMs"], json!(1), "from added to started");
        let never = end(&mut tasks, "t2", Outcome::not_started(&error)).expect("ended");
        let told = finished_properties(&never, None);
        assert_eq!((told["error"].as_str(), told.get("agent")), (Some("start"), None), "no thread, no agent");
        let unstarted = Task { started_at: None, ..never };
        assert_eq!(finished_properties(&unstarted, None).get("waitedMs"), None, "it never started");
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
