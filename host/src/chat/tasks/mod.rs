//! Work a bot does away from its chat, so the chat keeps answering
//! ([the box](../../../../docs/plans/event-box.md), [tasks](../../../../docs/features/background-tasks.md)).
//!
//! What the bot should work on is an **item** in its box ([`items`]). A **task** works items, one at a time: a
//! **task bot**, a `bots` row of its own (`BotConfig::task_of`) copied from its bot, with its own actor, agent
//! process, session and transcript, because everything a running turn touches is keyed by bot id. Its memory and
//! instructions are its bot's (`BotConfig::as_owner`). It starts by forking its bot's main session where the agent
//! can (`agent::bot::session`), otherwise from a new session and a brief. [`dispatch`] gives waiting items to
//! tasks, up to the bot's `maxAgents`; a task's last message of a turn is its item's result and goes back as a turn
//! in the bot's chat (`Cmd::TaskReport`). A routine run and another bot's request are items too ([`sources`]): their
//! result goes to the chat as the routine's message, or to the asking bot as its reply. No client lists a task bot:
//! they see `work` on the bot and fetch a task's transcript by id.
//!
//! Here: a task's life from its turn's end to its report. [`steer`] has what the user and the bot do to an item,
//! [`tools`] the bot's tools, [`prompts`] the words, [`worktree`] the git worktree of a task that works beside
//! another in one repository.

mod dispatch;
mod items;
mod prompts;
mod sources;
mod steer;
mod sweep;
mod thread;
mod tools;
mod worktree;

pub use dispatch::{Wait, dispatch};
#[cfg(test)]
pub use items::put_working_for_test;
pub use items::{Source, Status, doing, lines, outside, wire, working_tasks};
pub use prompts::{BRIEF, in_thread, reminder, report_notice, report_prompt};
pub use sources::{access_of, add_request, add_routine, boxed, from_chat, routines_changed, source_of};
pub use steer::{Steer, cut_in, drop_item, first, retry, stop};
pub use thread::{Refused as ThreadRefused, heard, message, picked_up, task as thread_task};
pub use tools::{TOOLS, acting_as, call, refuse_chat_only, refuse_in_task, tools};
pub use worktree::{Worktree, enter as enter_worktree, in_repo};

use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::analytics::{self, Event};
use crate::hub::Hub;
use crate::store::{BotConfig, Entry, Store, now_ms};
use anyhow::{Result, anyhow};
use items::Item;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Items added in a row with no message of the user's in between (a report's turn adding the next).
const MAX_HOPS: u8 = 8;
/// Items a box holds waiting at most.
const MAX_WAITING: usize = 20;
/// An ended item, and a task's transcript with its last item, are kept this long.
const KEEP_MS: i64 = 7 * 24 * 60 * 60 * 1000;
const MAX_TITLE: usize = 80;
const MAX_BRIEF: usize = 32_000;
/// How much of a task's last message its item's report carries.
const MAX_REPORT: usize = 8_000;
/// Why an item that a host restart cut off, and that can't pick up, didn't finish.
pub const CUT_OFF: &str = "the computer's Codync restarted while it worked";
/// Why an item whose task's turn never began didn't finish; the error follows.
const NOT_STARTED: &str = "it could not start: ";

/// How a task's session began.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Start {
    Fork,
    Brief,
}

/// What an item came to, for its bot.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub item: String,
    pub title: String,
    /// Its task's last message; `Err` holds why it didn't finish.
    pub outcome: Result<String, String>,
    pub hops: u8,
    /// The git worktree it worked in, if it had one of its own: where its changes are.
    #[serde(default)]
    pub worktree: Option<Worktree>,
    /// What the user wrote to its thread since its last report, word for word: not the task's words.
    #[serde(default)]
    pub told: Vec<String>,
}

/// How a task's turn ended (from its actor).
pub enum Outcome {
    Done(Option<String>),
    /// Why, in words for its bot.
    Failed(Failure, String),
    Stopped,
}

impl Outcome {
    /// Its task's turn never began, for `error`.
    pub fn not_started(error: &anyhow::Error) -> Self {
        Self::Failed(Failure::Start, format!("{NOT_STARTED}{error:#}"))
    }
}

/// What kept an item from finishing: `work_finished`'s `error`.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    /// Its task's turn never began (the agent didn't start, needs sign-in, opened no session).
    Start,
    /// Its task's agent failed mid-turn.
    Agent,
    /// Its task's turn ended short of `end_turn` (output or step limit, a refusal).
    EndedEarly,
    /// A host restart cut it off and it couldn't pick up.
    Restart,
    /// Its task ran no turn any more and no end ever arrived, twice (`sweep`).
    Lost,
    /// It finished with nothing to report.
    NoReport,
    /// A routine run went past its time limit.
    TimedOut,
}

/// State of the boxes that lives only while the host runs.
#[derive(Default)]
pub struct Tasks {
    /// Bot → the hops of the reports its running turn answers: an item added there carries them on.
    chain: Mutex<HashMap<String, u8>>,
    /// Bot → the items its running turn added: a turn the user stops takes them with it.
    added: Mutex<HashMap<String, Vec<String>>>,
    /// Held across each change to a box (`items::change`).
    boxes: Mutex<()>,
    /// Bot → its box's next hourly look (`sweep`), while the box holds work.
    sweeps: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    /// Item → the request of another bot it answers (`sources`), which owns the exchange's notices and the reply.
    /// Gone with the host process: a request is never replayed.
    requests: Mutex<HashMap<String, crate::chat::team::BotRequest>>,
}

impl Tasks {
    pub fn answering(&self, bot: &str, hops: u8) {
        self.chain.locked().insert(bot.to_owned(), hops);
    }

    fn hops(&self, bot: &str) -> u8 {
        self.chain.locked().get(bot).copied().unwrap_or(0)
    }
}

/// `bot`'s task bots.
pub fn of(store: &Store, bot: &str) -> Vec<BotConfig> {
    let rows = store.task_bots().unwrap_or_else(|error| {
        tracing::warn!(error = format!("{error:#}"), "couldn't list tasks");
        Vec::new()
    });
    let mut tasks: Vec<BotConfig> =
        rows.into_iter().map(|r| r.config).filter(|c| c.task_of.as_deref() == Some(bot)).collect();
    tasks.sort_by_key(|c| c.created_at);
    tasks
}

/// `cfg`'s fallbacks changed: its tasks fall back the same way from their next agent on, as the bot does. Read,
/// saved and sent under one lock, so nothing a task saves meanwhile is lost and a later change reaches it after this
/// one.
pub fn fallbacks_changed(hub: &Hub, cfg: &BotConfig) {
    let _g = hub.emit_lock.locked();
    let mut tasks = of(&hub.store, &cfg.id);
    for task in &mut tasks {
        task.fallbacks.clone_from(&cfg.fallbacks);
    }
    if let Err(error) = hub.store.save_bots(&tasks) {
        tracing::warn!(bot = %cfg.id, error = format!("{error:#}"), "couldn't give its tasks the bot's fallbacks");
        return;
    }
    for task in tasks.into_iter().filter(|task| hub.has_actor(&task.id)) {
        let id = task.id.clone();
        if let Err(error) = hub.send_cmd(&id, Cmd::Reconfigure(Box::new(task))) {
            tracing::warn!(task = %id, error = format!("{error:#}"), "couldn't give a task its bot's fallbacks");
        }
    }
}

/// The bot `id` is to the user: a task's bot, else `id` itself.
pub fn owner(store: &Store, id: &str) -> String {
    let task_of = store.bot(id).ok().flatten().and_then(|row| row.config.task_of);
    task_of.unwrap_or_else(|| id.to_owned())
}

/// The actor that waits on an approval card's answer: the task that raised it, else the bot that wrote it (in a
/// group, the member that asked), else the chat's own.
pub fn card_actor(card: &Entry) -> &str {
    let task = card.data["task"]["id"].as_str();
    task.or_else(|| card.data["author"].as_str()).unwrap_or(&card.bot_id)
}

/// The item task `cfg` works on.
fn item_of(store: &Store, cfg: &BotConfig) -> Option<Item> {
    items::of_task(&items::load(store, cfg.task_of.as_deref()?), &cfg.id).cloned()
}

/// Changes the item task `cfg` works on, for a change whose loss only costs a detail (`what`, for the log).
fn update(hub: &Hub, cfg: &BotConfig, what: &str, change: impl FnOnce(&mut Item)) -> bool {
    let Some(bot) = cfg.task_of.as_deref() else { return false };
    let changed = items::change(hub, bot, |items| {
        let item = items.iter_mut().find(|i| i.status == Status::Working && i.task.as_deref() == Some(&cfg.id));
        Ok(item.map(change).is_some())
    });
    changed.unwrap_or_else(|error| {
        tracing::warn!(task = %cfg.id, what, error = format!("{error:#}"), "couldn't save an item's detail");
        false
    })
}

/// The item task `cfg` works on, as the cards it puts in its bot's chat name it ([`mark`]): its id and title.
pub fn card_item(store: &Store, cfg: &BotConfig) -> Option<(String, String)> {
    item_of(store, cfg).map(|item| (item.id, item.title))
}

/// What marks an entry a task puts in its bot's chat as its item's (`data.author`, `data.task`, `data.item`).
pub struct ItemMark {
    /// The task's bot, whose chat it is.
    pub bot: String,
    task: String,
    title: String,
    item: Option<String>,
}

impl ItemMark {
    /// The task it marks as.
    pub fn task(&self) -> &str {
        &self.task
    }

    /// Marks `data`: to the user it is the bot's; an answer finds the task by `data.task.id` ([`card_actor`]).
    pub fn apply(&self, data: &mut Value) {
        data["author"] = self.bot.as_str().into();
        data["task"] = json!({"id": self.task, "title": self.title});
        data["item"] = self.item.clone().into();
    }
}

/// From task `cfg`: what marks an entry in its bot's chat as its item's, named by what the user knows it by. None
/// for a bot's own.
pub fn mark(store: &Store, cfg: &BotConfig) -> Option<ItemMark> {
    let bot = cfg.task_of.clone()?;
    let (item, title) = card_item(store, cfg).unzip();
    Some(ItemMark { bot, task: cfg.id.clone(), title: title.unwrap_or_default(), item })
}

/// The session a task's first session forks, if any; taken once.
pub fn take_fork_source(hub: &Hub, cfg: &BotConfig) -> Option<String> {
    let mut source = None;
    update(hub, cfg, "its fork", |item| source = item.fork_from.take());
    source
}

/// Whether `cfg` is a task whose session hasn't begun yet.
pub fn first_session(store: &Store, cfg: &BotConfig) -> bool {
    cfg.is_task() && item_of(store, cfg).is_some_and(|item| item.start.is_none())
}

/// The agent couldn't fork the chat's session for this task.
pub fn fork_failed(hub: &Hub, cfg: &BotConfig) {
    update(hub, cfg, "its failed fork", |item| item.fork_failed = true);
}

/// How the task's session began, once its actor knows.
pub fn started(hub: &Hub, cfg: &BotConfig, start: Start) {
    update(hub, cfg, "how it started", |item| item.start = Some(start));
}

/// What a task that starts from a new session is told of the chat it comes from.
pub fn chat_so_far(store: &Store, bot: &str) -> Option<String> {
    let session = store.bot(bot).ok().flatten()?.session_id.unwrap_or_default();
    let text = crate::chat::handoff::build(store, bot, &session).ok()?;
    Some(format!("[The chat you come from, so far]\n{text}"))
}

/// `bot`'s own turn ended: the items it added may start now (a fork taken mid-turn would carry an unanswered tool
/// call). A turn the user stopped takes the items it added with it.
pub fn turn_over(hub: &Arc<Hub>, bot: &str, stopped: bool) {
    hub.tasks.chain.locked().remove(bot);
    let added = hub.tasks.added.locked().remove(bot).unwrap_or_default();
    if stopped && !added.is_empty() {
        let dropped = items::change(hub, bot, |items| {
            items.retain(|i| !(i.status == Status::Waiting && added.contains(&i.id)));
            Ok(())
        });
        if let Err(error) = dropped {
            tracing::warn!(bot, error = format!("{error:#}"), "couldn't drop a stopped turn's work");
        }
    }
    dispatch::run(hub, bot, Some(bot));
}

/// Task `cfg`'s turn ended: its item says how, its result goes where its source reads it (the bot's own work: a
/// report, as a turn in the chat), and the dispatcher decides what the task does next. An item the user stopped
/// reports nothing: they know. While a message the user wrote to its thread still waits for its turn the item isn't
/// over: that turn's end is its end.
pub fn finished(hub: &Arc<Hub>, cfg: &BotConfig, outcome: Outcome) {
    let Some(bot) = cfg.task_of.as_deref() else { return };
    let (owner, task) = (bot.to_owned(), cfg.id.clone());
    let next = move |hub: &Arc<Hub>| dispatch::run(hub, &owner, Some(&task));
    let ended = items::change(hub, bot, |items| {
        Ok(if thread::unheard(&hub.store, &cfg.id) { None } else { end_claimed(items, &cfg.id, outcome) })
    });
    match ended {
        Ok(Some(over)) => close(hub, bot, over, next),
        Ok(None) => next(hub),
        Err(error) => {
            tracing::error!(task = %cfg.id, error = format!("{error:#}"), "couldn't save an item's end");
            next(hub);
        }
    }
}

/// An item that just ended, with its worktree when that is to be looked over before its report says where the
/// work is.
type Over = (Item, Option<Worktree>);

/// [`end`], with the ended item's worktree claimed.
fn end_claimed(items: &mut [Item], task: &str, outcome: Outcome) -> Option<Over> {
    let item = end(items, task, outcome)?;
    let worktree = worktree::claim(items, &item.id);
    Some((item, worktree))
}

/// `bot`'s item ended: its worktree is looked over if it has one to, it says so and its result goes where its
/// source reads it ([`ended`]), and `then` follows.
fn close(hub: &Arc<Hub>, bot: &str, (item, worktree): Over, then: impl FnOnce(&Arc<Hub>) + Send + 'static) {
    let Some(worktree) = worktree else {
        ended(hub, bot, &item);
        return then(hub);
    };
    let owner = bot.to_owned();
    worktree::settle_then(hub, bot, &item.id.clone(), worktree, move |hub| {
        let item = items::load(&hub.store, &owner).into_iter().find(|i| i.id == item.id).unwrap_or(item);
        ended(hub, &owner, &item);
        then(hub);
    });
}

/// Ends the item `task` works on, in `items`. None: it has none (stopped, or it gave way), or a host restart or a
/// lost task cut it off for the first time and it waits again.
fn end(items: &mut [Item], task: &str, outcome: Outcome) -> Option<Item> {
    let front = items::front(items);
    let item = items.iter_mut().find(|i| i.status == Status::Working && i.task.as_deref() == Some(task))?;
    let (status, result, failure) = match outcome {
        // Only the bot's own work starts again: a routine run or a request done twice could act twice.
        Outcome::Failed(Failure::Restart | Failure::Lost, _) if !item.restarted && item.source.is_bot() => {
            item.set_aside(front);
            item.restarted = true;
            item.cut_in = true;
            return None;
        }
        Outcome::Done(text) => {
            // A report to the bot's own turn carries only so much; a routine run's or a request's result is posted
            // whole, as its message.
            let most = if item.source.is_bot() { MAX_REPORT } else { usize::MAX };
            let text = text.map(|t| crate::agent::acp::truncate(t.trim(), most)).filter(|t| !t.is_empty());
            match text {
                Some(text) => (Status::Done, Some(Ok(text)), None),
                // A routine run with nothing to tell says nothing (`(pass)`).
                None if matches!(item.source, Source::Routine { .. }) => (Status::Done, Some(Ok(String::new())), None),
                None => (Status::Done, Some(Err("it ended without a report".to_owned())), Some(Failure::NoReport)),
            }
        }
        Outcome::Failed(failure, why) => (Status::Failed, Some(Err(why)), Some(failure)),
        Outcome::Stopped => (Status::Stopped, None, None),
    };
    item.status = status;
    item.ended_at = Some(now_ms());
    item.result = result.as_ref().map(|r| r.clone().unwrap_or_else(|why| why)).filter(|r| !r.is_empty());
    item.failure = failure;
    item.report = result.map(|outcome| Report {
        item: item.id.clone(),
        title: item.title.clone(),
        outcome,
        hops: item.hops,
        worktree: None,
        told: std::mem::take(&mut item.told),
    });
    Some(item.clone())
}

/// `work_finished` for `item`, which just ended.
fn finished_properties(item: &Item) -> serde_json::Map<String, Value> {
    let mut properties = json!({
        "outcome": item.status, "start": item.start, "forkFailed": item.fork_failed,
        "minutes": (now_ms() - item.started_at.unwrap_or(item.created_at)) / 60_000,
        "source": item.source.kind(), "worktree": item.worktree.is_some(),
    });
    if let Some(failure) = item.failure {
        properties["error"] = json!(failure);
    }
    if item.worktree.as_ref().is_some_and(|w| w.left == worktree::Left::Kept) {
        properties["kept"] = true.into();
    }
    if item.place == worktree::Place::NoWorktree {
        properties["worktreeFailed"] = true.into();
    }
    properties.as_object().cloned().unwrap_or_default()
}

/// Sends what `item` came to where its source reads it: the bot's own work, as a turn in its chat; a routine run,
/// as the routine's message there; a request, as the asking bot's reply. A report that can't be handed on stays on
/// its item for the next host start.
fn deliver(hub: &Hub, bot: &str, item: &Item) {
    if item.source.is_bot() {
        // With where its work is, as its worktree stands now.
        let report = item.report.clone().map(|report| Report { worktree: item.worktree.clone(), ..report });
        if let Some(report) = report
            && let Err(error) = hub.send_cmd(bot, Cmd::TaskReport(report))
        {
            tracing::warn!(item = item.id, error = format!("{error:#}"), "couldn't hand an item's report to its bot");
        }
    } else if sources::deliver(hub, bot, item) && item.report.is_some() {
        // Nothing of the bot's takes a turn for it: it has arrived.
        reported(hub, bot, std::slice::from_ref(&item.id));
    }
}

/// `item` of `bot` ended: says so (`work_finished`), and its result goes where its source reads it.
fn ended(hub: &Hub, bot: &str, item: &Item) {
    analytics::capture(hub, Event::WorkFinished, finished_properties(item));
    deliver(hub, bot, item);
}

/// A deleted bot's box goes with it, and so do the requests it hadn't answered.
pub fn forget(hub: &Hub, bot: &str) {
    let items = items::load(&hub.store, bot);
    sources::forget_requests(hub, &items);
    worktree::leave(&items);
    items::forget(&hub.store, bot);
}

/// The reports of these items of `bot` arrived: the bot's turn for them is over (they reached the user, from the
/// bot or as the chat's own notice), or they went where a routine's or a request's go.
pub fn reported(hub: &Hub, bot: &str, delivered: &[String]) {
    let cleared = items::change(hub, bot, |items| {
        for item in items.iter_mut().filter(|i| delivered.contains(&i.id)) {
            item.report = None;
        }
        Ok(())
    });
    if let Err(error) = cleared {
        tracing::warn!(bot, error = format!("{error:#}"), "couldn't save that reports were delivered");
    }
}

/// At host start: old items go, and the others pick up where the last process left them.
pub fn restore(hub: &Arc<Hub>) {
    let lives = |bot: &str| hub.store.bot(bot).ok().flatten().is_some_and(|r| !r.deleted);
    let tasks = hub.store.task_bots().unwrap_or_default();
    for task in tasks.iter().filter(|task| !task.config.task_of.as_deref().is_some_and(lives)) {
        if let Err(error) = hub.delete_bot(&task.config.id) {
            tracing::warn!(task = %task.config.id, error = format!("{error:#}"), "couldn't remove a task");
        }
    }
    // Every bot with a box, or with a task bot left to clear away.
    let mut bots: Vec<String> =
        hub.store.kv_prefix(items::PREFIX).iter().map(|key| key.trim_start_matches(items::PREFIX).to_owned()).collect();
    bots.extend(tasks.into_iter().filter_map(|task| task.config.task_of));
    bots.sort();
    bots.dedup();
    for bot in &bots {
        if !lives(bot) {
            forget(hub, bot);
        } else if let Err(error) = pick_up(hub, bot) {
            tracing::error!(bot, error = format!("{error:#}"), "couldn't pick up a bot's work");
        }
    }
    sources::settle_runs(hub);
}

/// `bot`'s box at host start: its tasks resume the turns the last process cut off, an item whose task can't ends,
/// and the reports that never arrived are sent again.
fn pick_up(hub: &Arc<Hub>, bot: &str) -> Result<()> {
    let (resumed, failed, undelivered) = items::change(hub, bot, |items| {
        let now = now_ms();
        items.retain(|i| i.ended_at.is_none_or(|at| now - at <= KEEP_MS));
        worktree::unsettled(items);
        // A request lived in the last process (its notices already say it was interrupted): it is never replayed.
        items.retain(|i| i.status.ended() || !matches!(i.source, Source::Request { .. }));
        // A task's actor resumes the turn the last process cut off; when it won't, nothing of the task runs.
        let (resumed, cut_off): (Vec<String>, Vec<String>) = items
            .iter()
            .filter(|i| i.status == Status::Working)
            .filter_map(|i| i.task.clone())
            .partition(|task| crate::agent::bot::resumable(&hub.store, task));
        // The reports that never arrived, before this start's own are added.
        let undelivered: Vec<Item> = items.iter().filter(|i| i.report.is_some()).cloned().collect();
        let failed: Vec<Over> = cut_off
            .iter()
            .filter_map(|task| end_claimed(items, task, Outcome::Failed(Failure::Restart, CUT_OFF.into())))
            .collect();
        Ok((resumed, failed, undelivered))
    })?;
    for task in resumed {
        if let Ok(Some(row)) = hub.store.bot(&task) {
            sources::resumed(hub, &row.config);
            hub.spawn_bot(row.config);
        }
    }
    for over in failed {
        close(hub, bot, over, |_| {});
    }
    for item in &undelivered {
        deliver(hub, bot, item);
    }
    dispatch(hub, bot);
    Ok(())
}

/// `taskEntries`: a task's transcript, newest `limit`, for its details. `bot` must be the task's own.
pub fn entries(hub: &Hub, bot: &str, id: &str, limit: i64) -> Result<Value> {
    let row = hub.store.bot(id)?.filter(|r| !r.deleted && r.config.task_of.as_deref() == Some(bot));
    let cfg = row.ok_or_else(|| anyhow!("unknown task"))?.config;
    // What it works on, else what it worked on last.
    let mut worked: Vec<Item> =
        items::load(&hub.store, bot).into_iter().filter(|i| i.task.as_deref() == Some(id)).collect();
    worked.sort_by_key(|i| (i.status.ended(), i.started_at));
    let item = worked.iter().find(|i| !i.status.ended()).or(worked.last()).ok_or_else(|| anyhow!("unknown task"))?;
    Ok(json!({
        "task": {"id": cfg.id, "title": item.title, "status": item.status, "startedAt": item.started_at,
                 "endedAt": item.ended_at, "start": item.start},
        "entries": hub.store.history(id, i64::MAX, limit)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn working(id: &str, task: &str) -> Item {
        serde_json::from_value(json!({
            "id": id, "title": id, "brief": "x", "status": "working", "order": 5, "task": task, "createdAt": 0,
            "startedAt": 1, "hops": 2,
        }))
        .expect("an item")
    }

    #[test]
    fn an_item_ends_as_its_tasks_turn_did() {
        let mut items = [working("a", "t1"), working("b", "t2"), working("c", "t3")];
        let done = end(&mut items, "t1", Outcome::Done(Some(" All green. ".into()))).expect("ended");
        assert_eq!((done.status, done.result.as_deref(), done.failure), (Status::Done, Some("All green."), None));
        let report = done.report.expect("a report for its bot");
        assert_eq!((report.item.as_str(), report.outcome, report.hops), ("a", Ok("All green.".into()), 2));

        let silent = end(&mut items, "t2", Outcome::Done(None)).expect("ended");
        assert_eq!(silent.failure, Some(Failure::NoReport));
        assert_eq!(silent.report.expect("a report").outcome, Err("it ended without a report".into()));

        let stopped = end(&mut items, "t3", Outcome::Stopped).expect("ended");
        assert!(stopped.status == Status::Stopped && stopped.report.is_none(), "the user knows");
        assert!(end(&mut items, "t3", Outcome::Stopped).is_none(), "its task has no item any more");
        assert_eq!(json!(Failure::EndedEarly), "ended_early");
    }

    #[test]
    fn only_the_bots_own_report_is_cut() {
        let long = "x".repeat(MAX_REPORT + 10);
        let mut items = [working("a", "t1"), working("b", "t2")];
        items[1].source = Source::Routine { id: "r".into(), run: "1".into() };
        let report = end(&mut items, "t1", Outcome::Done(Some(long.clone()))).expect("ended");
        assert!(report.result.expect("a result").len() < long.len());
        let routine = end(&mut items, "t2", Outcome::Done(Some(long.clone()))).expect("ended");
        assert_eq!(routine.result, Some(long), "a routine's message is posted whole");
    }

    #[test]
    fn an_item_a_restart_cut_off_waits_again_once_then_fails() {
        let mut items = [working("a", "t1")];
        assert!(end(&mut items, "t1", Outcome::Failed(Failure::Restart, CUT_OFF.into())).is_none());
        let item = &items[0];
        assert!(item.status == Status::Waiting && item.cut_in && item.task.is_none(), "{item:?}");

        items[0].status = Status::Working;
        items[0].task = Some("t2".into());
        let failed = end(&mut items, "t2", Outcome::Failed(Failure::Restart, CUT_OFF.into())).expect("ended");
        assert_eq!((failed.status, failed.failure), (Status::Failed, Some(Failure::Restart)));
        assert_eq!(failed.result.as_deref(), Some(CUT_OFF));
    }

    #[test]
    fn a_routine_run_or_a_request_is_never_done_twice_and_a_silent_run_is_no_failure() {
        let routine = Source::Routine { id: "r".into(), run: "run".into() };
        let mut items = [
            Item { source: routine.clone(), ..working("a", "t1") },
            Item { source: Source::Request { id: None }, ..working("b", "t2") },
            Item { source: routine, ..working("c", "t3") },
        ];
        for task in ["t1", "t2"] {
            let cut = end(&mut items, task, Outcome::Failed(Failure::Lost, "lost".into())).expect("it ends at once");
            assert_eq!((cut.status, cut.failure), (Status::Failed, Some(Failure::Lost)), "{task}");
        }
        let silent = end(&mut items, "t3", Outcome::Done(None)).expect("ended");
        assert_eq!((silent.status, silent.failure, silent.result), (Status::Done, None, None));
        assert_eq!(silent.report.expect("the run's end still goes to its routine").outcome, Ok(String::new()));
        assert_eq!(json!(Failure::TimedOut), "timed_out");
    }
}
