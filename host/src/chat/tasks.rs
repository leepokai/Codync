//! Tasks: long work a bot does away from its chat, so the chat keeps answering
//! ([design](../../../docs/plans/background-tasks.md)).
//!
//! A task is a **task bot**: a `bots` row of its own (`BotConfig::task_of`) copied from its bot, with its own actor,
//! agent process, session and transcript, because everything a running turn touches is keyed by bot id. Its memory
//! and instructions are its bot's (`BotConfig::as_owner`). It starts by forking its bot's main session where the
//! agent can (`agent::bot::session`), otherwise from a new session and a brief. Its bot tells it things with
//! `tell_task`; its last message goes back as a turn in the bot's chat (`Cmd::TaskReport`). No client lists a task
//! bot: they see `tasks` on the bot and fetch a task's transcript by id.

use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::analytics::{self, Event};
use crate::hub::{BotStatus, Hub};
use crate::store::{BotConfig, Entry, Store, TASK_PREFIX, now_ms};
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// The `chat` server's tools that are tasks'.
pub const TOOLS: [&str; 4] = ["start_task", "tell_task", "task_status", "stop_task"];

/// What a bot is told about tasks, in its instructions (and each turn, for a session opened before tasks).
pub const BRIEF: &str = "You do long work in a task, not in this chat: the user waits on this chat and can't reach you \
while a turn runs. Before work that needs more than a few tool calls (reading several files, changing code, running \
builds or tests, research, writing a document), call start_task(title, brief) first, then say in a sentence that it is \
under way and end your turn. The task is another session of you, away from this chat; its report comes back to you as \
a message, and you tell the user what it found. One task at a time: give several things to the same task, and add to \
a running one with tell_task instead of starting another. Only quick answers and work of a few tool calls you do here \
yourself.";

/// Said with each message in the bot's own chat: with the instructions alone, a real agent (Claude, 2026-11-15) did
/// 90 seconds of reading in the chat's turn instead of starting a task.
pub const EACH_TURN: &str = "If this needs more than a few tool calls, don't do it in this turn: call start_task first \
(the chat must stay free for the user), then say briefly that it is under way.";

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

/// Where a task is. While it works, the capsule shows its actor's status (working, or waiting on a card).
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
    /// Its report, until its bot's turn for it has started: a host restart in between sends it again.
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
    Failed(String),
    Stopped,
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
            let status = match (task.status, rt.status) {
                (Status::Starting, _) => "starting",
                (_, BotStatus::NeedsInput) => "needsInput",
                _ => "working",
            };
            json!({
                "id": cfg.id, "title": task.title, "status": status, "activity": rt.activity,
                "startedAt": task.created_at, "start": task.start,
            })
        })
        .collect(),
    )
}

/// The first thing a task is told: what it is, the work, and how its words reach the user.
fn first_prompt(task: &Task) -> String {
    let words = task.words.as_deref().map(|w| format!("\n\n[The user's own words: «{w}»]")).unwrap_or_default();
    format!(
        "[Task] You are continuing as the same bot, away from the chat, on: \"{}\".\n\n{}{words}\n\n\
         [Codync: work until it's done. Your last message is the report the chat gets, so end with what you did and \
         found. Messages you send with send_message show in the task's details, not in the chat. Approvals reach the \
         user in the chat. You can't ask the user a question, share a file, start another task, set a follow-up or \
         message another bot from here: say in your report what you need.]",
        task.title, task.brief
    )
}

/// What a task hears when its bot passes something on.
fn told(message: &str, words: Option<&str>) -> String {
    let words = words.map(|w| format!("\n\n[The user's own words: «{w}»]")).unwrap_or_default();
    format!("[From the chat, passed on by you there] {message}{words}")
}

/// The prompt of the turn that takes `reports` in the bot's chat.
pub fn report_prompt(reports: &[Report]) -> String {
    let parts: Vec<String> = reports
        .iter()
        .map(|r| match &r.outcome {
            Ok(text) => format!("[Codync: your task \"{}\" finished. Its report:]\n{text}", r.title),
            Err(why) => format!("[Codync: your task \"{}\" did not finish: {why}]", r.title),
        })
        .collect();
    format!(
        "{}\n\n[Codync: these are your tasks' reports, not the user speaking. Tell the user what matters in a message \
         of your own; reply with only (pass) if there is nothing to tell.]",
        parts.join("\n\n")
    )
}

/// The user's own typed message the bot's running turn answers.
fn users_words(hub: &Hub, bot: &str) -> Option<String> {
    let entry = hub.store.entry(&hub.runtime(bot).message?)?;
    entry.data["text"].as_str().map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned)
}

/// The actor that waits on an approval card's answer: the task that raised it, else the bot that wrote it (in a
/// group, the member that asked), else the chat's own.
pub fn card_actor(card: &Entry) -> &str {
    let task = card.data["task"]["id"].as_str();
    task.or_else(|| card.data["author"].as_str()).unwrap_or(&card.bot_id)
}

/// A task can't do what only makes sense from the chat. `what`: the tool, as the refusal names it.
pub fn refuse_in_task(hub: &Hub, bot: &str, what: &str) -> Result<()> {
    if hub.store.bot(bot)?.is_some_and(|row| row.config.is_task()) {
        bail!("{what} isn't available in a task: say in your report what you need, and you will handle it in the chat");
    }
    Ok(())
}

pub fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": "start_task",
            "description": "Hand long work to another session of you that works away from this chat; you stay free to answer the user. It starts when this turn ends. Its report comes back to you later as a message. One task at a time: add to a running one with tell_task.",
            "inputSchema": {"type": "object", "properties": {
                "title": {"type": "string", "description": "A few words the user sees while it works, e.g. \"Fix the login redirect\"."},
                "brief": {"type": "string", "description": "Everything the task needs to do the work: the goal, what you already know, what done looks like."},
                "parallel": {"type": "boolean", "description": "Start it beside a task already running. Only when the user wants two unrelated things at once."},
            }, "required": ["title", "brief"]},
            "annotations": {"readOnlyHint": false, "idempotentHint": false},
        }),
        json!({
            "name": "tell_task",
            "description": "Pass something on to your task: a correction, more work, an answer it needs. A finished task takes it as more work and reports again.",
            "inputSchema": {"type": "object", "properties": {
                "message": {"type": "string"},
                "task": {"type": "string", "description": "The task's id; not needed when you have one task."},
            }, "required": ["message"]},
            "annotations": {"readOnlyHint": false, "idempotentHint": false},
        }),
        json!({
            "name": "task_status",
            "description": "What your tasks are doing now: status, current step, how long, their latest messages.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": true},
        }),
        json!({
            "name": "stop_task",
            "description": "Stop a task of yours. What it already changed stays.",
            "inputSchema": {"type": "object", "properties": {
                "task": {"type": "string", "description": "The task's id; not needed when you have one task."},
            }},
            "annotations": {"readOnlyHint": false, "idempotentHint": true},
        }),
    ]
}

pub fn call(hub: &Arc<Hub>, bot: &str, name: &str, args: &Value) -> Result<Value> {
    refuse_in_task(hub, bot, name)?;
    match name {
        "start_task" => start(hub, bot, args),
        "tell_task" => tell(hub, bot, args),
        "task_status" => Ok(status(hub, bot)),
        "stop_task" => {
            let id = target(hub, bot, args["task"].as_str(), |task| task.status.live())?;
            stop(hub, bot, Some(&id))?;
            Ok(json!({"stopped": true}))
        }
        _ => bail!("unknown task tool: {name}"),
    }
}

fn start(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value> {
    let started = admit(hub, bot, args);
    let outcome = started.as_ref().map_or_else(|(reason, _)| *reason, |_| "started");
    let properties = json!({"outcome": outcome, "parallel": args["parallel"] == true});
    analytics::capture(hub, Event::TaskStarted, properties.as_object().cloned().unwrap_or_default());
    started.map_err(|(_, error)| error)
}

/// Records a task for `bot`, or says why not: the refusal's `task_started` outcome and its words for the bot.
fn admit(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value, (&'static str, anyhow::Error)> {
    let failed = |error: anyhow::Error| ("failed", error);
    let row = hub.store.bot(bot).map_err(failed)?.filter(|r| !r.deleted);
    let row = row.ok_or_else(|| failed(anyhow!("unknown bot")))?;
    if hub.runtime(bot).lane.is_none_or(|lane| lane.chat != bot) || hub.team.answering(bot) {
        let why = anyhow!("start_task works only in your own chat with the user: its report comes back there");
        return Err(("elsewhere", why));
    }
    let text = |key: &str| args[key].as_str().map(str::trim).filter(|t| !t.is_empty());
    let (Some(title), Some(brief)) = (text("title"), text("brief")) else {
        return Err(("invalid", anyhow!("title and brief are required")));
    };
    if title.chars().count() > MAX_TITLE || brief.len() > MAX_BRIEF {
        let why = anyhow!("title is at most {MAX_TITLE} characters and brief at most {MAX_BRIEF} bytes");
        return Err(("invalid", why));
    }
    let live: Vec<_> = records(&hub.store, bot).into_iter().filter(|(_, t)| t.status.live()).collect();
    if let Some((_, running)) = live.first()
        && args["parallel"] != true
    {
        let why = anyhow!(
            "a task is already running (\"{}\"): add to it with tell_task. Pass parallel: true only when the user \
             wants two unrelated things at once",
            running.title
        );
        return Err(("live", why));
    }
    let everywhere = hub.store.task_bots().map_err(failed)?;
    let everywhere = everywhere.iter().filter(|r| load(&hub.store, &r.config.id).is_some_and(|t| t.status.live()));
    if live.len() >= PER_BOT || everywhere.count() >= PER_COMPUTER {
        let why = anyhow!("too many tasks are running: wait for one to finish, or add to one with tell_task");
        return Err(("limit", why));
    }
    if crate::usage::near_limit(hub, &row.config) {
        let why = anyhow!("your agent is near its usage limit: do the work here, or tell the user it has to wait");
        return Err(("usage", why));
    }
    let hops = hub.tasks.hops(bot) + 1;
    if hops > MAX_HOPS {
        let why =
            anyhow!("too many tasks were started in a row without the user: tell the user where things stand and wait");
        return Err(("hops", why));
    }
    let cfg = BotConfig {
        id: format!("{TASK_PREFIX}{}", uuid::Uuid::new_v4()),
        task_of: Some(bot.to_owned()),
        created_at: now_ms(),
        // Its alerts are its bot's: the cards it raises in the chat, and the turn its report starts there.
        notify: Some(false),
        lead: false,
        pinned: false,
        position: 0,
        section: None,
        auto_name: false,
        created_by: None,
        offered_to_network: false,
        reachable_by_assistants: false,
        // One screen and one team: those stay with the chat.
        computer: false,
        manage_team: false,
        ..row.config
    };
    let task = Task {
        title: title.to_owned(),
        brief: brief.to_owned(),
        words: users_words(hub, bot),
        status: Status::Starting,
        start: None,
        fork_from: None,
        fork_failed: false,
        hops,
        created_at: cfg.created_at,
        ended_at: None,
        report: None,
    };
    {
        let _g = hub.emit_lock.locked();
        hub.store.save_bot(&cfg).map_err(failed)?;
        save(&hub.store, &cfg.id, &task).map_err(failed)?;
    }
    hub.emit_bot(bot);
    Ok(json!({
        "taskId": cfg.id, "status": "starting",
        "note": "It starts when this turn ends. Its report comes back to you later as a message.",
    }))
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
            fail(hub, &cfg, "it could not start");
        }
    }
    hub.emit_bot(bot);
}

fn launch(hub: &Arc<Hub>, bot: &str, cfg: &BotConfig, mut task: Task) -> Result<()> {
    task.fork_from = fork_source(hub, bot);
    task.status = Status::Working;
    save(&hub.store, &cfg.id, &task)?;
    hub.spawn_bot(cfg.clone());
    crate::chat::messages::post(hub, cfg, json!({"text": first_prompt(&task), "from": "host"}))?;
    Ok(())
}

/// `bot`'s main session, when a task may start as a fork of it: the bot is on its own agent, and the session has
/// used under half its window (a fork carries all of it). Whether the agent can fork is its actor's to find out.
fn fork_source(hub: &Hub, bot: &str) -> Option<String> {
    let session = hub.store.bot(bot).ok().flatten()?.session_id.filter(|s| !s.is_empty())?;
    if hub.runtime(bot).agent_step.is_some() {
        return None;
    }
    let usage = hub.store.kv_get(&format!("compaction.{bot}.{session}"));
    let usage: Value = usage.and_then(|u| serde_json::from_str(&u).ok()).unwrap_or_default();
    match (usage["used"].as_u64(), usage["size"].as_u64()) {
        (Some(used), Some(size)) if size > 0 && used * 2 >= size => None,
        _ => Some(session),
    }
}

/// The session a task's first session forks, if any; taken once.
pub fn take_fork_source(store: &Store, id: &str) -> Option<String> {
    let mut task = load(store, id)?;
    let source = task.fork_from.take()?;
    if let Err(error) = save(store, id, &task) {
        tracing::warn!(task = id, error = format!("{error:#}"), "couldn't record a task's fork");
    }
    Some(source)
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
    let Some(mut task) = load(store, id) else { return };
    task.fork_failed = true;
    if let Err(error) = save(store, id, &task) {
        tracing::warn!(task = id, error = format!("{error:#}"), "couldn't record a task's failed fork");
    }
}

/// How the task's session began, once its actor knows.
pub fn started(hub: &Hub, id: &str, start: Start) {
    let Some(mut task) = load(&hub.store, id) else { return };
    task.start = Some(start);
    if let Err(error) = save(&hub.store, id, &task) {
        tracing::warn!(task = id, error = format!("{error:#}"), "couldn't record how a task started");
    }
    hub.emit_bot(id);
}

/// What a task that starts from a new session is told of the chat it comes from.
pub fn chat_so_far(store: &Store, bot: &str) -> Option<String> {
    let session = store.bot(bot).ok().flatten()?.session_id.unwrap_or_default();
    let text = crate::chat::handoff::build(store, bot, &session).ok()?;
    Some(format!("[The chat you come from, so far]\n{text}"))
}

/// Picks one of `bot`'s tasks: the one named, or the newest that `wanted` when none is.
fn target(hub: &Hub, bot: &str, named: Option<&str>, wanted: impl Fn(&Task) -> bool) -> Result<String> {
    let all = records(&hub.store, bot);
    if let Some(id) = named.map(str::trim).filter(|id| !id.is_empty()) {
        return all
            .iter()
            .find(|(cfg, _)| cfg.id == id)
            .map(|(cfg, _)| cfg.id.clone())
            .ok_or_else(|| anyhow!("no such task"));
    }
    let found = all.iter().rev().find(|(_, task)| wanted(task)).or_else(|| all.last());
    found.map(|(cfg, _)| cfg.id.clone()).ok_or_else(|| anyhow!("you have no task"))
}

fn tell(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value> {
    let message = args["message"].as_str().map(str::trim).filter(|m| !m.is_empty());
    let message = message.ok_or_else(|| anyhow!("message is required"))?;
    if message.len() > MAX_BRIEF {
        bail!("message is at most {MAX_BRIEF} bytes");
    }
    let id = target(hub, bot, args["task"].as_str(), |task| task.status.live())?;
    let cfg = hub.store.bot(&id)?.ok_or_else(|| anyhow!("no such task"))?.config;
    let mut task = load(&hub.store, &id).ok_or_else(|| anyhow!("no such task"))?;
    if task.status == Status::Starting {
        // Not launched yet: it reads this with its brief.
        task.brief = format!("{}\n\n{message}", task.brief);
        save(&hub.store, &id, &task)?;
        return Ok(json!({"told": true, "taskId": id}));
    }
    if !task.status.live() {
        // Finished: this is more work, and it reports again.
        task.status = Status::Working;
        task.ended_at = None;
        save(&hub.store, &id, &task)?;
        if !hub.has_actor(&id) {
            hub.spawn_bot(cfg.clone());
        }
    }
    let words = users_words(hub, bot);
    crate::chat::messages::post(hub, &cfg, json!({"text": told(message, words.as_deref()), "from": "host"}))?;
    analytics::capture(hub, Event::TaskTold, serde_json::Map::new());
    hub.emit_bot(bot);
    Ok(json!({"told": true, "taskId": id}))
}

fn status(hub: &Hub, bot: &str) -> Value {
    let tasks: Vec<Value> = records(&hub.store, bot)
        .into_iter()
        .map(|(cfg, task)| {
            let rt = hub.runtime(&cfg.id);
            let messages: Vec<String> = hub
                .store
                .history(&cfg.id, i64::MAX, 60)
                .unwrap_or_default()
                .iter()
                .filter(|e| e.kind == "agent" && e.data["final"] == true)
                .rev()
                .take(3)
                .filter_map(|e| e.data["text"].as_str().map(|t| crate::agent::acp::truncate(t, 600)))
                .collect();
            let until = task.ended_at.unwrap_or_else(now_ms);
            json!({
                "taskId": cfg.id, "title": task.title, "status": task.status,
                "waitingOnApproval": rt.status == BotStatus::NeedsInput,
                "doingNow": rt.activity, "seconds": (until - task.created_at) / 1000,
                "latestMessages": messages.into_iter().rev().collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({"tasks": tasks})
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
        // With no turn running there is no turn whose end would say it stopped.
        let idle = hub.runtime(&cfg.id).status == BotStatus::Idle;
        if hub.send_cmd(&cfg.id, Cmd::Stop).is_err() || idle {
            finished(hub, &cfg, Outcome::Stopped);
        }
    }
    hub.emit_bot(bot);
    Ok(())
}

fn fail(hub: &Arc<Hub>, cfg: &BotConfig, why: &str) {
    finished(hub, cfg, Outcome::Failed(why.to_owned()));
}

/// A task's turn ended: its record says how, and its bot gets the report as a turn in the chat. A task the user
/// stopped reports nothing: they know.
pub fn finished(hub: &Arc<Hub>, cfg: &BotConfig, outcome: Outcome) {
    let (Some(bot), Some(mut task)) = (cfg.task_of.as_deref(), load(&hub.store, &cfg.id)) else { return };
    let (status, result) = match outcome {
        Outcome::Done(text) => {
            let text = text.map(|t| crate::agent::acp::truncate(t.trim(), MAX_REPORT)).filter(|t| !t.is_empty());
            (Status::Done, Some(text.ok_or_else(|| "it ended without a report".to_owned())))
        }
        Outcome::Failed(why) => (Status::Failed, Some(Err(why))),
        Outcome::Stopped => (Status::Stopped, None),
    };
    task.status = status;
    task.ended_at = Some(now_ms());
    let properties = json!({
        "outcome": status, "start": task.start, "forkFailed": task.fork_failed,
        "minutes": (now_ms() - task.created_at) / 60_000,
    });
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

/// The bot's turn for these reports has started: they are delivered.
pub fn reported(store: &Store, reports: &[Report]) {
    for report in reports {
        let Some(mut task) = load(store, &report.task) else { continue };
        task.report = None;
        if let Err(error) = save(store, &report.task, &task) {
            tracing::warn!(task = %report.task, error = format!("{error:#}"), "couldn't mark a task's report delivered");
        }
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
                    fail(hub, &cfg, "it could not start");
                }
            }
            // Its actor resumes the turn the last process cut off, if it left a mark; with none, nothing of it runs.
            Status::Working if hub.store.kv_get(&format!("turn.inflight.{}", cfg.id)).is_some() => {
                hub.spawn_bot(cfg);
            }
            Status::Working => fail(hub, &cfg, "the computer's Codync restarted while it worked"),
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
    fn a_report_turn_reads_each_task_and_says_who_speaks() {
        let done =
            Report { task: "task-1".into(), title: "Fix login".into(), outcome: Ok("All green.".into()), hops: 1 };
        let failed =
            Report { task: "task-2".into(), title: "Docs".into(), outcome: Err("it was cut off".into()), hops: 1 };
        let prompt = report_prompt(&[done, failed]);
        assert!(prompt.contains("your task \"Fix login\" finished. Its report:]\nAll green."));
        assert!(prompt.contains("your task \"Docs\" did not finish: it was cut off"));
        assert!(prompt.ends_with("reply with only (pass) if there is nothing to tell.]"));
    }

    #[test]
    fn a_task_is_told_its_work_and_the_users_words() {
        let task = Task {
            title: "Fix login".into(),
            brief: "The redirect drops the path.".into(),
            words: Some("fix the login thing".into()),
            status: Status::Starting,
            start: None,
            fork_from: None,
            fork_failed: false,
            hops: 1,
            created_at: 0,
            ended_at: None,
            report: None,
        };
        let prompt = first_prompt(&task);
        assert!(
            prompt.starts_with("[Task] You are continuing as the same bot, away from the chat, on: \"Fix login\".")
        );
        assert!(prompt.contains("The redirect drops the path.\n\n[The user's own words: «fix the login thing»]"));
    }
}
