//! Work a bot does away from its chat, so the chat keeps answering
//! ([the box](../../../../docs/plans/event-box.md), [tasks](../../../../docs/features/background-tasks.md)).
//!
//! What the bot should work on is an **item** in its box ([`items`]). A **task** works items, one at a time: a
//! **task bot**, a `bots` row of its own (`BotConfig::task_of`) copied from its bot, with its own actor, agent
//! process, session and transcript, because everything a running turn touches is keyed by bot id. Its memory and
//! instructions are its bot's (`BotConfig::as_owner`). It starts by forking its bot's main session where the agent
//! can (`agent::bot::session`), otherwise from a new session and a brief. [`dispatch`] gives waiting items to
//! tasks, up to the bot's `maxAgents`; a task's last message of a turn is its item's result and goes back as a turn
//! in the bot's chat (`Cmd::TaskReport`). No client lists a task bot: they see `work` on the bot and fetch a task's
//! transcript by id.
//!
//! Here: a task's life from its turn's end to its report. [`steer`] has what the user and the bot do to an item,
//! [`tools`] the bot's tools, [`prompts`] the words.

mod dispatch;
mod items;
mod prompts;
mod steer;
mod tools;

pub use dispatch::dispatch;
#[cfg(test)]
pub use items::put_working_for_test;
pub use items::{forget, lines, wire, working_tasks};
pub use prompts::{BRIEF, reminder, report_notice, report_prompt};
pub use steer::{cut_in, drop_item, first, retry, stop};
pub use tools::{TOOLS, call, refuse_chat_only, refuse_in_task, tools};

use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::analytics::{self, Event};
use crate::hub::Hub;
use crate::store::{BotConfig, Entry, Store, now_ms};
use anyhow::{Result, anyhow};
use items::{Item, Status};
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
}

/// How a task's turn ended (from its actor).
pub enum Outcome {
    Done(Option<String>),
    /// Why, in words for its bot.
    Failed(Failure, String),
    Stopped,
}

/// What kept an item from finishing: `work_finished`'s `error`.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
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
    /// It finished with nothing to report.
    NoReport,
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

/// The item task `cfg` works on, as its approval card names it: its id and title.
pub fn card_item(store: &Store, cfg: &BotConfig) -> Option<(String, String)> {
    item_of(store, cfg).map(|item| (item.id, item.title))
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

/// Task `cfg`'s turn ended: its item says how, its bot gets the report as a turn in the chat, and the dispatcher
/// decides what the task does next. An item the user stopped reports nothing: they know.
pub fn finished(hub: &Arc<Hub>, cfg: &BotConfig, outcome: Outcome) {
    let Some(bot) = cfg.task_of.as_deref() else { return };
    match items::change(hub, bot, |items| Ok(end(items, &cfg.id, outcome))) {
        Ok(Some((item, failure))) => ended(hub, bot, &item, failure),
        Ok(None) => {}
        Err(error) => tracing::error!(task = %cfg.id, error = format!("{error:#}"), "couldn't save an item's end"),
    }
    dispatch::run(hub, bot, Some(&cfg.id));
}

/// Ends the item `task` works on, in `items`. None: it has none (stopped, or it gave way), or a host restart cut it
/// off for the first time and it waits again.
fn end(items: &mut [Item], task: &str, outcome: Outcome) -> Option<(Item, Option<Failure>)> {
    let front = items::front(items);
    let item = items.iter_mut().find(|i| i.status == Status::Working && i.task.as_deref() == Some(task))?;
    let (status, result, failure) = match outcome {
        Outcome::Failed(Failure::Restart, _) if !item.restarted => {
            item.set_aside(front);
            item.restarted = true;
            item.cut_in = true;
            return None;
        }
        Outcome::Done(text) => {
            let text = text.map(|t| crate::agent::acp::truncate(t.trim(), MAX_REPORT)).filter(|t| !t.is_empty());
            let failure = text.is_none().then_some(Failure::NoReport);
            (Status::Done, Some(text.ok_or_else(|| "it ended without a report".to_owned())), failure)
        }
        Outcome::Failed(failure, why) => (Status::Failed, Some(Err(why)), Some(failure)),
        Outcome::Stopped => (Status::Stopped, None, None),
    };
    item.status = status;
    item.ended_at = Some(now_ms());
    item.result = result.as_ref().map(|r| r.clone().unwrap_or_else(|why| why));
    item.report =
        result.map(|outcome| Report { item: item.id.clone(), title: item.title.clone(), outcome, hops: item.hops });
    Some((item.clone(), failure))
}

/// `work_finished` for `item`, which just ended.
fn finished_properties(item: &Item, failure: Option<Failure>) -> serde_json::Map<String, Value> {
    let mut properties = json!({
        "outcome": item.status, "start": item.start, "forkFailed": item.fork_failed,
        "minutes": (now_ms() - item.started_at.unwrap_or(item.created_at)) / 60_000,
    });
    if let Some(failure) = failure {
        properties["error"] = json!(failure);
    }
    properties.as_object().cloned().unwrap_or_default()
}

/// `item` of `bot` ended: says so (`work_finished`), and hands its report to its bot.
fn ended(hub: &Hub, bot: &str, item: &Item, failure: Option<Failure>) {
    analytics::capture(hub, Event::WorkFinished, finished_properties(item, failure));
    if let Some(report) = item.report.clone()
        && let Err(error) = hub.send_cmd(bot, Cmd::TaskReport(report))
    {
        tracing::warn!(item = %item.id, error = format!("{error:#}"), "couldn't hand an item's report to its bot");
    }
}

/// The bot's turn for these reports is over: they reached the user, from the bot or as the chat's own notice.
pub fn reported(hub: &Hub, bot: &str, reports: &[Report]) {
    let cleared = items::change(hub, bot, |items| {
        for item in items.iter_mut().filter(|i| reports.iter().any(|r| r.item == i.id)) {
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
    for task in hub.store.task_bots().unwrap_or_default() {
        if !task.config.task_of.as_deref().is_some_and(lives)
            && let Err(error) = hub.delete_bot(&task.config.id)
        {
            tracing::warn!(task = %task.config.id, error = format!("{error:#}"), "couldn't remove a task");
        }
    }
    // Every bot with a box, or with a task bot left to clear away.
    let mut bots: Vec<String> =
        hub.store.kv_prefix(items::PREFIX).iter().map(|key| key.trim_start_matches(items::PREFIX).to_owned()).collect();
    bots.extend(hub.store.task_bots().unwrap_or_default().into_iter().filter_map(|task| task.config.task_of));
    bots.sort();
    bots.dedup();
    for bot in &bots {
        if !lives(bot) {
            forget(&hub.store, bot);
            continue;
        }
        let picked_up = items::change(hub, bot, |items| {
            let now = now_ms();
            items.retain(|i| i.ended_at.is_none_or(|at| now - at <= KEEP_MS));
            // A task's actor resumes the turn the last process cut off; when it won't, nothing of the task runs.
            let (resumed, cut_off): (Vec<String>, Vec<String>) = items
                .iter()
                .filter(|i| i.status == Status::Working)
                .filter_map(|i| i.task.clone())
                .partition(|task| crate::agent::bot::resumable(&hub.store, task));
            let failed: Vec<_> = cut_off
                .iter()
                .filter_map(|task| end(items, task, Outcome::Failed(Failure::Restart, CUT_OFF.into())))
                .collect();
            Ok((resumed, failed, items.iter().filter_map(|i| i.report.clone()).collect::<Vec<_>>()))
        });
        let (resumed, failed, reports) = match picked_up {
            Ok(found) => found,
            Err(error) => {
                tracing::error!(bot, error = format!("{error:#}"), "couldn't pick up a bot's work");
                continue;
            }
        };
        for task in resumed {
            if let Ok(Some(row)) = hub.store.bot(&task) {
                hub.spawn_bot(row.config);
            }
        }
        for (item, failure) in &failed {
            analytics::capture(hub, Event::WorkFinished, finished_properties(item, *failure));
        }
        for report in reports {
            let _ = hub.send_cmd(bot, Cmd::TaskReport(report));
        }
        dispatch(hub, bot);
    }
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
        let (done, failure) = end(&mut items, "t1", Outcome::Done(Some(" All green. ".into()))).expect("ended");
        assert_eq!((done.status, done.result.as_deref(), failure), (Status::Done, Some("All green."), None));
        let report = done.report.expect("a report for its bot");
        assert_eq!((report.item.as_str(), report.outcome, report.hops), ("a", Ok("All green.".into()), 2));

        let (silent, failure) = end(&mut items, "t2", Outcome::Done(None)).expect("ended");
        assert_eq!(failure, Some(Failure::NoReport));
        assert_eq!(silent.report.expect("a report").outcome, Err("it ended without a report".into()));

        let (stopped, _) = end(&mut items, "t3", Outcome::Stopped).expect("ended");
        assert!(stopped.status == Status::Stopped && stopped.report.is_none(), "the user knows");
        assert!(end(&mut items, "t3", Outcome::Stopped).is_none(), "its task has no item any more");
        assert_eq!(json!(Failure::EndedEarly), "ended_early");
    }

    #[test]
    fn an_item_a_restart_cut_off_waits_again_once_then_fails() {
        let mut items = [working("a", "t1")];
        assert!(end(&mut items, "t1", Outcome::Failed(Failure::Restart, CUT_OFF.into())).is_none());
        let item = &items[0];
        assert!(item.status == Status::Waiting && item.cut_in && item.task.is_none(), "{item:?}");

        items[0].status = Status::Working;
        items[0].task = Some("t2".into());
        let (failed, failure) =
            end(&mut items, "t2", Outcome::Failed(Failure::Restart, CUT_OFF.into())).expect("ended");
        assert_eq!((failed.status, failure), (Status::Failed, Some(Failure::Restart)));
        assert_eq!(failed.result.as_deref(), Some(CUT_OFF));
    }
}
