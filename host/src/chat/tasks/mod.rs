//! Tasks: long work a bot does away from its chat, so the chat keeps answering
//! ([design](../../../../docs/plans/background-tasks.md)).
//!
//! A task is a **task bot**: a `bots` row of its own (`BotConfig::task_of`) copied from its bot, with its own actor,
//! agent process, session and transcript, because everything a running turn touches is keyed by bot id. Its memory
//! and instructions are its bot's (`BotConfig::as_owner`). It starts by forking its bot's main session where the
//! agent can (`agent::bot::session`), otherwise from a new session and a brief. Its bot tells it things with
//! `tell_task`; its last message goes back as a turn in the bot's chat (`Cmd::TaskReport`). No client lists a task
//! bot: they see `tasks` on the bot and fetch a task's transcript by id.
//!
//! Here: a task's record and its life from launch to report. [`tools`] has the bot's tools, [`prompts`] the words.

mod prompts;
mod tools;

pub use prompts::{BRIEF, reminder, report_notice, report_prompt};
pub use tools::{TOOLS, call, refuse_chat_only, refuse_in_task, tools};

use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::analytics::{self, Event};
use crate::hub::{BotStatus, Hub};
use crate::store::{BotConfig, Entry, Store, now_ms};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Live tasks one bot may have, and all bots of this computer together.
const PER_BOT: usize = 3;
const PER_COMPUTER: usize = 6;
/// Tasks started in a row with no message of the user's in between (a report's turn starting the next).
const MAX_HOPS: u8 = 8;
/// A finished task and its transcript are kept this long.
const KEEP_MS: i64 = 7 * 24 * 60 * 60 * 1000;
const MAX_TITLE: usize = 80;
const MAX_BRIEF: usize = 32_000;
/// How much of a task's last message its report carries.
const MAX_REPORT: usize = 8_000;
/// Why a task that a host restart cut off, and that can't pick up, didn't finish.
pub const CUT_OFF: &str = "the computer's Codync restarted while it worked";

/// Where a task is. While it works, the capsule shows its actor's status too ([`Live`]).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// Recorded; it launches when the turn that started it ends.
    Starting,
    Working,
    Done,
    Failed,
    Stopped,
}

impl Status {
    fn live(self) -> bool {
        matches!(self, Self::Starting | Self::Working)
    }
}

/// A task that is still going, as the capsule shows it (`bot.tasks[].status`).
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
enum Live {
    Starting,
    Working,
    /// It waits on an approval card in its bot's chat.
    NeedsInput,
}

impl Live {
    fn of(task: Status, actor: BotStatus) -> Self {
        match (task, actor) {
            (Status::Starting, _) => Self::Starting,
            (_, BotStatus::NeedsInput) => Self::NeedsInput,
            _ => Self::Working,
        }
    }
}

/// How a task's session began.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Start {
    Fork,
    Brief,
}

/// A task's own record (kv `task.<id>`); its config is its `bots` row.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct Task {
    title: String,
    brief: String,
    /// The user's own words that led to it, word for word.
    #[serde(default)]
    words: Option<String>,
    status: Status,
    #[serde(default)]
    start: Option<Start>,
    /// Its bot's main session, to fork when its own session opens; none when it starts from a brief.
    #[serde(default)]
    fork_from: Option<String>,
    /// The agent refused or didn't answer the fork, so it started from a brief.
    #[serde(default)]
    fork_failed: bool,
    #[serde(default)]
    hops: u8,
    created_at: i64,
    #[serde(default)]
    ended_at: Option<i64>,
    /// Its report, until its bot's turn for it has ended: a host restart before that sends it again.
    #[serde(default)]
    report: Option<Report>,
}

/// What a task's turn came to, for its bot.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub task: String,
    pub title: String,
    /// Its last message; `Err` holds why it didn't finish.
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

/// What kept a task from finishing: `task_finished`'s `error`.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    /// Its turn never began (the agent didn't start, needs sign-in, opened no session).
    Start,
    /// Its agent failed mid-turn.
    Agent,
    /// Its turn ended short of `end_turn` (output or step limit, a refusal).
    EndedEarly,
    /// A host restart cut it off and it couldn't pick up.
    Restart,
    /// It finished with nothing to report.
    NoReport,
}

/// Per-bot state that lives only while the host runs.
#[derive(Default)]
pub struct Tasks {
    /// Bot → the hops of the reports its running turn answers: a task started there carries them on.
    chain: Mutex<HashMap<String, u8>>,
}

impl Tasks {
    pub fn answering(&self, bot: &str, hops: u8) {
        self.chain.locked().insert(bot.to_owned(), hops);
    }

    pub fn turn_ended(&self, bot: &str) {
        self.chain.locked().remove(bot);
    }

    fn hops(&self, bot: &str) -> u8 {
        self.chain.locked().get(bot).copied().unwrap_or(0)
    }
}

fn key(id: &str) -> String {
    format!("task.{id}")
}

fn load(store: &Store, id: &str) -> Option<Task> {
    serde_json::from_str(&store.kv_get(&key(id))?).ok()
}

fn save(store: &Store, id: &str, task: &Task) -> Result<()> {
    store.kv_set(&key(id), &serde_json::to_string(task)?)
}

/// Changes task `id`'s record, for a change whose loss only costs a detail (`what`, for the log). None: no such task.
fn update(store: &Store, id: &str, what: &str, change: impl FnOnce(&mut Task)) -> Option<Task> {
    let mut task = load(store, id)?;
    change(&mut task);
    if let Err(error) = save(store, id, &task) {
        tracing::warn!(task = id, what, error = format!("{error:#}"), "couldn't save a task's record");
    }
    Some(task)
}

/// A deleted bot's task record goes with it (a no-op for a bot that isn't a task).
pub fn forget(store: &Store, id: &str) {
    if let Err(error) = store.kv_delete(&key(id)) {
        tracing::warn!(task = id, error = format!("{error:#}"), "couldn't drop a task's record");
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

/// `bot`'s tasks with their records, oldest first.
fn records(store: &Store, bot: &str) -> Vec<(BotConfig, Task)> {
    of(store, bot).into_iter().filter_map(|cfg| load(store, &cfg.id).map(|task| (cfg, task))).collect()
}

/// The bot's `tasks` on the wire: the ones still going, oldest first.
pub fn wire(hub: &Hub, bot: &str) -> Value {
    let live = records(&hub.store, bot).into_iter().filter(|(_, task)| task.status.live());
    Value::Array(
        live.map(|(cfg, task)| {
            let rt = hub.runtime(&cfg.id);
            json!({
                "id": cfg.id, "title": task.title, "status": Live::of(task.status, rt.status),
                "activity": rt.activity, "startedAt": task.created_at, "start": task.start,
            })
        })
        .collect(),
    )
}

/// The actor that waits on an approval card's answer: the task that raised it, else the bot that wrote it (in a
/// group, the member that asked), else the chat's own.
pub fn card_actor(card: &Entry) -> &str {
    let task = card.data["task"]["id"].as_str();
    task.or_else(|| card.data["author"].as_str()).unwrap_or(&card.bot_id)
}

/// `bot`'s turn ended: the tasks it started launch now (a fork taken mid-turn would carry an unanswered tool call).
/// A turn the user stopped takes them with it.
pub fn launch_pending(hub: &Arc<Hub>, bot: &str, stopped: bool) {
    for (cfg, task) in records(&hub.store, bot) {
        if task.status != Status::Starting {
            continue;
        }
        let launched = if stopped { hub.delete_bot(&cfg.id) } else { launch(hub, bot, &cfg, task) };
        if let Err(error) = launched {
            tracing::error!(task = %cfg.id, error = format!("{error:#}"), "couldn't launch a task");
            could_not_start(hub, &cfg);
        }
    }
    hub.emit_bot(bot);
}

fn launch(hub: &Arc<Hub>, bot: &str, cfg: &BotConfig, mut task: Task) -> Result<()> {
    task.fork_from = fork_source(hub, bot);
    task.status = Status::Working;
    save(&hub.store, &cfg.id, &task)?;
    hub.spawn_bot(cfg.clone());
    crate::chat::messages::post(hub, cfg, json!({"text": prompts::first_prompt(&task), "from": "host"}))?;
    Ok(())
}

fn could_not_start(hub: &Arc<Hub>, cfg: &BotConfig) {
    finished(hub, cfg, Outcome::Failed(Failure::Start, "it could not start".into()));
}

/// `bot`'s main session, when a task may start as a fork of it: the bot is on its own agent, and the session has
/// used under half its window (a fork carries all of it). Whether the agent can fork is its actor's to find out.
fn fork_source(hub: &Hub, bot: &str) -> Option<String> {
    let session = hub.store.bot(bot).ok().flatten()?.session_id.filter(|s| !s.is_empty())?;
    if hub.runtime(bot).agent_step.is_some() {
        return None;
    }
    let chat = crate::agent::bot::conversation(&hub.store, bot, &session);
    (chat.size == 0 || chat.used * 2 < chat.size).then_some(session)
}

/// The session a task's first session forks, if any; taken once.
pub fn take_fork_source(store: &Store, id: &str) -> Option<String> {
    let mut source = None;
    update(store, id, "its fork", |task| source = task.fork_from.take());
    source
}

/// Whether `cfg` is a task whose session hasn't begun yet.
pub fn first_session(store: &Store, cfg: &BotConfig) -> bool {
    cfg.is_task() && load(store, &cfg.id).is_some_and(|task| task.start.is_none())
}

/// A task's title, for what it puts in its bot's chat.
pub fn title(store: &Store, id: &str) -> String {
    load(store, id).map(|task| task.title).unwrap_or_default()
}

/// The agent couldn't fork the chat's session for this task.
pub fn fork_failed(store: &Store, id: &str) {
    update(store, id, "its failed fork", |task| task.fork_failed = true);
}

/// How the task's session began, once its actor knows.
pub fn started(hub: &Hub, id: &str, start: Start) {
    if update(&hub.store, id, "how it started", |task| task.start = Some(start)).is_some() {
        hub.emit_bot(id);
    }
}

/// What a task that starts from a new session is told of the chat it comes from.
pub fn chat_so_far(store: &Store, bot: &str) -> Option<String> {
    let session = store.bot(bot).ok().flatten()?.session_id.unwrap_or_default();
    let text = crate::chat::handoff::build(store, bot, &session).ok()?;
    Some(format!("[The chat you come from, so far]\n{text}"))
}

/// Stops one of `bot`'s tasks, or all of them (`None`): the user's Stop on the capsule, or the bot's `stop_task`.
pub fn stop(hub: &Arc<Hub>, bot: &str, id: Option<&str>) -> Result<()> {
    for (cfg, task) in records(&hub.store, bot) {
        if id.is_some_and(|id| id != cfg.id) || !task.status.live() {
            continue;
        }
        if task.status == Status::Starting {
            hub.delete_bot(&cfg.id)?;
            continue;
        }
        // With no turn running (it ended, or never started) there is no turn whose end would say it stopped.
        let turn_runs = matches!(hub.runtime(&cfg.id).status, BotStatus::Working | BotStatus::NeedsInput);
        if hub.send_cmd(&cfg.id, Cmd::Stop).is_err() || !turn_runs {
            finished(hub, &cfg, Outcome::Stopped);
        }
    }
    hub.emit_bot(bot);
    Ok(())
}

/// A task's turn ended: its record says how, and its bot gets the report as a turn in the chat. A task the user
/// stopped reports nothing: they know.
pub fn finished(hub: &Arc<Hub>, cfg: &BotConfig, outcome: Outcome) {
    let (Some(bot), Some(mut task)) = (cfg.task_of.as_deref(), load(&hub.store, &cfg.id)) else { return };
    let (status, result, failure) = match outcome {
        Outcome::Done(text) => {
            let text = text.map(|t| crate::agent::acp::truncate(t.trim(), MAX_REPORT)).filter(|t| !t.is_empty());
            let failure = text.is_none().then_some(Failure::NoReport);
            (Status::Done, Some(text.ok_or_else(|| "it ended without a report".to_owned())), failure)
        }
        Outcome::Failed(failure, why) => (Status::Failed, Some(Err(why)), Some(failure)),
        Outcome::Stopped => (Status::Stopped, None, None),
    };
    task.status = status;
    task.ended_at = Some(now_ms());
    let mut properties = json!({
        "outcome": status, "start": task.start, "forkFailed": task.fork_failed,
        "minutes": (now_ms() - task.created_at) / 60_000,
    });
    if let Some(failure) = failure {
        properties["error"] = json!(failure);
    }
    analytics::capture(hub, Event::TaskFinished, properties.as_object().cloned().unwrap_or_default());
    task.report =
        result.map(|outcome| Report { task: cfg.id.clone(), title: task.title.clone(), outcome, hops: task.hops });
    if let Err(error) = save(&hub.store, &cfg.id, &task) {
        tracing::error!(task = %cfg.id, error = format!("{error:#}"), "couldn't save a task's end");
    }
    if let Some(report) = task.report
        && let Err(error) = hub.send_cmd(bot, Cmd::TaskReport(report))
    {
        tracing::warn!(task = %cfg.id, error = format!("{error:#}"), "couldn't hand a task's report to its bot");
    }
    hub.emit_bot(bot);
}

/// The bot's turn for these reports is over: they reached the user, from the bot or as the chat's own notice.
pub fn reported(store: &Store, reports: &[Report]) {
    for report in reports {
        update(store, &report.task, "its delivered report", |task| task.report = None);
    }
}

/// At host start: old finished tasks go, and the others pick up where the last process left them.
pub fn restore(hub: &Arc<Hub>) {
    let rows = hub.store.task_bots().unwrap_or_default();
    for cfg in rows.into_iter().map(|r| r.config) {
        let Some(bot) = cfg.task_of.clone() else { continue };
        let owner_lives = hub.store.bot(&bot).ok().flatten().is_some_and(|r| !r.deleted);
        let task = load(&hub.store, &cfg.id).filter(|_| owner_lives);
        let expired = |t: &Task| t.ended_at.is_some_and(|at| now_ms() - at > KEEP_MS);
        let Some(task) = task.filter(|t| !expired(t)) else {
            if let Err(error) = hub.delete_bot(&cfg.id) {
                tracing::warn!(task = %cfg.id, error = format!("{error:#}"), "couldn't remove an old task");
            }
            continue;
        };
        match task.status {
            Status::Starting => {
                if let Err(error) = launch(hub, &bot, &cfg, task) {
                    tracing::error!(task = %cfg.id, error = format!("{error:#}"), "couldn't launch a task");
                    could_not_start(hub, &cfg);
                }
            }
            // Its actor resumes the turn the last process cut off; when it won't, nothing of the task runs.
            Status::Working if crate::agent::bot::resumable(&hub.store, &cfg.id) => hub.spawn_bot(cfg),
            Status::Working => finished(hub, &cfg, Outcome::Failed(Failure::Restart, CUT_OFF.into())),
            Status::Done | Status::Failed | Status::Stopped => {
                if let Some(report) = task.report {
                    let _ = hub.send_cmd(&bot, Cmd::TaskReport(report));
                }
            }
        }
    }
}

/// `taskEntries`: a task's transcript, newest `limit`, for its details. `bot` must be the task's own.
pub fn entries(hub: &Hub, bot: &str, id: &str, limit: i64) -> Result<Value> {
    let row = hub.store.bot(id)?.filter(|r| !r.deleted && r.config.task_of.as_deref() == Some(bot));
    let cfg = row.ok_or_else(|| anyhow!("unknown task"))?.config;
    let task = load(&hub.store, &cfg.id).ok_or_else(|| anyhow!("unknown task"))?;
    Ok(json!({
        "task": {"id": cfg.id, "title": task.title, "status": task.status, "startedAt": task.created_at,
                 "endedAt": task.ended_at, "start": task.start},
        "entries": hub.store.history(id, i64::MAX, limit)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_capsule_shows_a_card_over_the_work_and_a_start_over_both() {
        assert_eq!(Live::of(Status::Starting, BotStatus::NeedsInput), Live::Starting);
        assert_eq!(Live::of(Status::Working, BotStatus::NeedsInput), Live::NeedsInput);
        assert_eq!(Live::of(Status::Working, BotStatus::Error), Live::Working);
        assert_eq!(json!(Live::NeedsInput), "needsInput");
        assert_eq!(json!(Failure::EndedEarly), "ended_early");
    }
}
