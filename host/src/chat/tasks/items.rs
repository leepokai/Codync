//! A bot's box: its items (kv `work.<bot>`, one JSON list), how they read on the wire and to the bot itself.

use super::dispatch::{self, Standing, Wait};
use super::{Report, Start};
use crate::LockExt;
use crate::hub::{BotStatus, Hub};
use crate::store::{BotConfig, Store, now_ms};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fmt::Write as _;

/// An ended item shows on the wire and in `work_status` this long.
const RECENT_MS: i64 = 24 * 60 * 60 * 1000;
/// Ended items the wire carries at most.
const RECENT_MOST: usize = 20;
/// How much of an item's result the wire carries.
const WIRE_RESULT: usize = 280;

/// Where an item is: the only words for work, on the wire, in every client and in the bot's tools.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// In the box; no task works on it.
    Waiting,
    Working,
    /// Its task waits on an approval. Never saved: a working item reads so while its task's card is open.
    NeedsYou,
    Done,
    Failed,
    Stopped,
}

impl Status {
    /// A task has it: working, or waiting on the user's approval.
    pub fn live(self) -> bool {
        matches!(self, Self::Working | Self::NeedsYou)
    }

    pub fn ended(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Stopped)
    }
}

/// One piece of work in a bot's box.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(super) struct Item {
    pub id: String,
    pub title: String,
    pub brief: String,
    /// The user's own words that led to it, word for word.
    #[serde(default)]
    pub words: Option<String>,
    /// What the user said about it since, word for word.
    #[serde(default)]
    pub notes: Vec<String>,
    pub status: Status,
    /// Its place among the waiting, lowest first.
    pub order: i64,
    /// An item it follows, in that item's session.
    #[serde(default)]
    pub after: Option<String>,
    /// The task bot that has or had it.
    #[serde(default)]
    pub task: Option<String>,
    /// It gave way to another item once, and starts again in a new session.
    #[serde(default)]
    pub cut_in: bool,
    /// A host restart already sent it back to waiting once: the next one fails it.
    #[serde(default)]
    pub restarted: bool,
    #[serde(default)]
    pub hops: u8,
    pub created_at: i64,
    #[serde(default)]
    pub started_at: Option<i64>,
    #[serde(default)]
    pub ended_at: Option<i64>,
    /// What its task reported, or why it didn't finish.
    #[serde(default)]
    pub result: Option<String>,
    /// Its report, until its bot's turn for it has ended: a host restart before that sends it again.
    #[serde(default)]
    pub report: Option<Report>,
    /// How its task's session began, once known.
    #[serde(default)]
    pub start: Option<Start>,
    /// Its bot's main session, for its new task to fork; none when it starts from a brief.
    #[serde(default)]
    pub fork_from: Option<String>,
    /// The agent refused or didn't answer the fork, so its task started from a brief.
    #[serde(default)]
    pub fork_failed: bool,
}

impl Item {
    /// A new item, waiting; its place is its box's to give.
    pub(super) fn waiting(title: &str, brief: &str, words: Option<String>, after: Option<&str>, hops: u8) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.to_owned(),
            brief: brief.to_owned(),
            words,
            notes: Vec::new(),
            status: Status::Waiting,
            order: 0,
            after: after.map(str::to_owned),
            task: None,
            cut_in: false,
            restarted: false,
            hops,
            created_at: now_ms(),
            started_at: None,
            ended_at: None,
            result: None,
            report: None,
            start: None,
            fork_from: None,
            fork_failed: false,
        }
    }

    /// Back to waiting at the front of `items`, to start again in a new task.
    pub(super) fn set_aside(&mut self, front: i64) {
        self.status = Status::Waiting;
        self.order = front;
        self.task = None;
        self.started_at = None;
        self.start = None;
        self.fork_from = None;
    }
}

pub(super) const PREFIX: &str = "work.";

fn key(bot: &str) -> String {
    format!("{PREFIX}{bot}")
}

pub(super) fn load(store: &Store, bot: &str) -> Vec<Item> {
    store.kv_get(&key(bot)).and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default()
}

/// Changes `bot`'s box. One change at a time, so two can't write over each other.
// ponytail: one lock for every bot's box; a lock per bot if boxes ever change often enough to wait on each other.
pub(super) fn change<T>(hub: &Hub, bot: &str, change: impl FnOnce(&mut Vec<Item>) -> Result<T>) -> Result<T> {
    let _g = hub.tasks.boxes.locked();
    let mut items = load(&hub.store, bot);
    let out = change(&mut items)?;
    hub.store.kv_set(&key(bot), &serde_json::to_string(&items)?)?;
    Ok(out)
}

/// A deleted bot's box goes with it.
pub fn forget(store: &Store, bot: &str) {
    if let Err(error) = store.kv_delete(&key(bot)) {
        tracing::warn!(bot, error = format!("{error:#}"), "couldn't drop a bot's work");
    }
}

/// An order before every waiting item's.
pub(super) fn front(items: &[Item]) -> i64 {
    items.iter().filter(|i| i.status == Status::Waiting).map(|i| i.order).min().unwrap_or(0) - 1
}

/// An order after every item's.
pub(super) fn last(items: &[Item]) -> i64 {
    items.iter().map(|i| i.order).max().unwrap_or(0) + 1
}

/// How many of `items` a task works on.
pub(super) fn working(items: &[Item]) -> usize {
    items.iter().filter(|i| i.status == Status::Working).count()
}

/// The item `task` works on.
pub(super) fn of_task<'a>(items: &'a [Item], task: &str) -> Option<&'a Item> {
    items.iter().find(|i| i.status == Status::Working && i.task.as_deref() == Some(task))
}

/// An item as its bot's clients and tools see it now: its status with its task's card, and what its task does.
pub(super) struct Shown {
    pub item: Item,
    pub status: Status,
    pub activity: String,
    pub wait: Option<Wait>,
}

/// `cfg`'s items worth showing: the working ones (oldest first), the waiting in dispatch order, then the ones ended
/// in the last day, newest first.
pub(super) fn shown(hub: &Hub, cfg: &BotConfig) -> Vec<Shown> {
    let items = load(&hub.store, &cfg.id);
    let mut waits: HashMap<String, Wait> = HashMap::new();
    if items.iter().any(|i| i.status == Status::Waiting) {
        let standings = dispatch::standings(&items, &dispatch::places(hub, cfg, &items, None), |task| {
            dispatch::task_state(hub, task, None)
        });
        waits.extend(standings.into_iter().map(|(id, standing)| {
            let wait = match standing {
                Standing::Wait(wait) => wait,
                // About to start, or to be taken.
                Standing::Start => Wait::Next,
                Standing::Join(_) => Wait::After,
            };
            (id, wait)
        }));
    }
    let now = now_ms();
    let (mut live, mut ended): (Vec<Item>, Vec<Item>) = items.into_iter().partition(|i| !i.status.ended());
    live.sort_by_key(|i| {
        (i.status == Status::Waiting, if i.status == Status::Waiting { i.order } else { 0 }, i.started_at)
    });
    ended.retain(|i| i.ended_at.is_some_and(|at| now - at < RECENT_MS));
    ended.sort_by_key(|i| std::cmp::Reverse(i.ended_at));
    ended.truncate(RECENT_MOST);
    live.into_iter()
        .chain(ended)
        .map(|item| {
            let rt = item.task.as_deref().filter(|_| item.status == Status::Working).map(|task| hub.runtime(task));
            let status = match &rt {
                Some(rt) if rt.status == BotStatus::NeedsInput => Status::NeedsYou,
                _ => item.status,
            };
            let wait = waits.remove(&item.id);
            Shown { status, activity: rt.map(|rt| rt.activity).unwrap_or_default(), wait, item }
        })
        .collect()
}

/// The bot's `work` on the wire.
pub fn wire(hub: &Hub, cfg: &BotConfig) -> Value {
    Value::Array(
        shown(hub, cfg)
            .into_iter()
            .map(|Shown { item, status, activity, wait }| {
                json!({
                    "id": item.id, "title": item.title, "status": status, "activity": activity, "task": item.task,
                    "wait": wait, "cutIn": item.cut_in, "createdAt": item.created_at, "startedAt": item.started_at,
                    "endedAt": item.ended_at,
                    "result": item.result.map(|r| crate::agent::acp::truncate(&r, WIRE_RESULT)),
                })
            })
            .collect(),
    )
}

/// The box in a few lines: counts and the ceiling, then the live and waiting titles with their ids. None when
/// nothing is live or waiting. `whose`: `Your` for the bot's own turns, `Its` for who asks about the bot (a call).
pub fn lines(hub: &Hub, cfg: &BotConfig, whose: &str) -> Option<String> {
    let live: Vec<Shown> = shown(hub, cfg).into_iter().filter(|s| !s.status.ended()).collect();
    let live: Vec<_> = live.iter().map(|s| (s.status, s.item.title.as_str(), s.item.id.as_str())).collect();
    summary(&live, cfg.max_agents, whose)
}

/// The task bots that work on an item now, on the whole computer.
pub fn working_tasks(store: &Store) -> Vec<BotConfig> {
    let tasks = store.task_bots().unwrap_or_default().into_iter().map(|row| row.config);
    tasks
        .filter(|task| task.task_of.as_deref().is_some_and(|bot| of_task(&load(store, bot), &task.id).is_some()))
        .collect()
}

/// Puts an item that `task` works on in `bot`'s box.
#[cfg(test)]
pub fn put_working_for_test(store: &Store, bot: &str, task: &str) {
    let item = json!({"id": "i1", "title": "Work", "brief": "x", "status": "working", "order": 1, "createdAt": 0,
        "task": task});
    store.kv_set(&key(bot), &json!([item]).to_string()).expect("saved");
}

fn summary(live: &[(Status, &str, &str)], most: u8, whose: &str) -> Option<String> {
    if live.is_empty() {
        return None;
    }
    let count = |wanted: fn(Status) -> bool| live.iter().filter(|(status, ..)| wanted(*status)).count();
    let mut text = format!(
        "{whose} work in the background: {} working, {} waiting; at most {most} run at once, the rest wait their turn.",
        count(|s| s != Status::Waiting),
        count(|s| s == Status::Waiting)
    );
    for (status, title, id) in live {
        let word = match status {
            Status::Waiting => "waiting",
            Status::NeedsYou => "needs the user's approval",
            _ => "working",
        };
        let _ = write!(text, "\n- {word}: \"{title}\" ({id})");
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bot_reads_its_box_in_a_few_lines() {
        assert_eq!(summary(&[], 3, "Your"), None, "an empty box says nothing");
        let live = [
            (Status::Working, "Fix login", "i1"),
            (Status::NeedsYou, "Run the suite", "i2"),
            (Status::Waiting, "Docs", "i3"),
        ];
        let text = summary(&live, 2, "Your");
        assert_eq!(
            text.as_deref(),
            Some(
                "Your work in the background: 2 working, 1 waiting; at most 2 run at once, the rest wait their \
                 turn.\n- working: \"Fix login\" (i1)\n\
                 - needs the user's approval: \"Run the suite\" (i2)\n- waiting: \"Docs\" (i3)"
            )
        );
        let about = summary(&live[..1], 1, "Its").expect("lines");
        assert!(
            about.starts_with("Its work in the background: 1 working, 0 waiting; at most 1 run at once"),
            "{about}"
        );
    }

    #[test]
    fn the_six_words_are_what_the_wire_says() {
        let words: Vec<Value> =
            [Status::Waiting, Status::Working, Status::NeedsYou, Status::Done, Status::Failed, Status::Stopped]
                .into_iter()
                .map(|s| json!(s))
                .collect();
        assert_eq!(words, ["waiting", "working", "needsYou", "done", "failed", "stopped"]);
    }
}
