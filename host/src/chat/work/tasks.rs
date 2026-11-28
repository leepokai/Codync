//! A bot's box: its tasks (kv `work.<bot>`, one JSON list), how they read on the wire and to the bot itself.

use super::dispatch::{self, Standing, Wait};
use super::worktree::{Left, Place, PlaceChoice, Worktree};
use super::{Failure, Report};
use crate::LockExt;
use crate::hub::{BotStatus, Hub};
use crate::store::{BotConfig, Permission, Store, now_ms};
use anyhow::{Result, anyhow, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

/// An ended task shows on the wire and in `work_status` this long.
const RECENT_MS: i64 = 24 * 60 * 60 * 1000;
/// Ended tasks the wire carries at most.
const RECENT_MOST: usize = 20;
/// How much of a task's result the wire carries.
const WIRE_RESULT: usize = 280;

/// Where a task is: the only words for work, on the wire, in every client and in the bot's tools.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// In the box; no thread works on it.
    Waiting,
    Working,
    /// Its thread waits on an approval. Never saved: a working task reads so while its thread's card is open.
    NeedsYou,
    Done,
    Failed,
    Stopped,
}

impl Status {
    /// A thread has it: working, or waiting on the user's approval.
    pub fn live(self) -> bool {
        matches!(self, Self::Working | Self::NeedsYou)
    }

    pub fn ended(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Stopped)
    }
}

/// Where a task came from (`work[].source`, absent for the bot's own).
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Source {
    /// The bot added it (`add_task`).
    #[default]
    Bot,
    /// The user started it themselves, as a thread with the bot (`startThread`): what it comes to is read in that
    /// thread, and never becomes a turn in the chat.
    User,
    /// A run of one of the bot's routines: `id` is the routine.
    Routine { id: String, run: String },
    /// Another bot's `message_bot` request: `id` is the asking bot, when it is one of this computer's.
    Request {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
}

impl Source {
    pub fn is_bot(&self) -> bool {
        *self == Self::Bot
    }

    /// A thread the user started: theirs, so the bot doesn't see or steer it.
    pub fn is_user(&self) -> bool {
        *self == Self::User
    }

    /// It takes one of the bot's seats under `maxThreads` while it works: everything but a thread the user started.
    pub fn seated(&self) -> bool {
        !self.is_user()
    }

    /// The bot's own work, whoever added it (the bot, or the user as a thread): the user may steer it, try it again
    /// and write to it, where a routine's run or another bot's request has an owner elsewhere.
    pub fn own(&self) -> bool {
        matches!(self, Self::Bot | Self::User)
    }

    /// `work_added` and `work_finished`'s `source`.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Bot => "bot",
            Self::User => "user",
            Self::Routine { .. } => "routine",
            Self::Request { .. } => "request",
        }
    }

    /// `work[].source`: `{kind, id?}`; none for the bot's own (a thread the user started is one to every client), so a
    /// client that doesn't know sources shows the row.
    fn wire(&self) -> Option<Value> {
        match self {
            Self::Bot | Self::User => None,
            Self::Routine { id, .. } => Some(json!({"kind": self.kind(), "id": id})),
            Self::Request { id } => Some(match id {
                Some(id) => json!({"kind": self.kind(), "id": id}),
                None => json!({"kind": self.kind()}),
            }),
        }
    }
}

/// What a thread the user starts changes from its bot's setup, for its own task (`startThread`'s `setup`): the
/// agent (a thread without a bot names it) and its login, the folder it works in, its model and how it is approved.
/// What it leaves out is the bot's.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Setup {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    /// The agent's login profile; `""` its default login. None: the bot's (another agent's default login).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<Model>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval: Option<Approval>,
    /// A thread opened beside a workflow to build it by talking (`BotConfig::workflow`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow: Option<String>,
    /// Where its thread works, its folder or a worktree, when the task names it (`add_task`'s and `startThread`'s
    /// `place`); none: the bot's worktree mode decides (`worktree::wanted`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place: Option<PlaceChoice>,
}

/// A model with its own settings (`BotConfig::model`, `model_options`): they go together, as in a bot's setup.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Model {
    /// None: the agent's default.
    pub id: Option<String>,
    #[serde(default)]
    pub options: BTreeMap<String, String>,
}

/// How its requests are approved (`BotConfig::permission`, `mode`).
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Approval {
    pub permission: Permission,
    /// None: the agent's default mode.
    pub mode: Option<String>,
}

impl Setup {
    pub fn is_bots(&self) -> bool {
        *self == Self::default()
    }

    /// `cfg` with what this changes.
    pub fn over(&self, cfg: &BotConfig) -> BotConfig {
        let mut cfg = cfg.clone();
        // Another agent runs on its own login, without the bot's fallbacks, command or Fast mode.
        if let Some(backend) = self.backend.as_ref().filter(|b| **b != cfg.backend) {
            backend.clone_into(&mut cfg.backend);
            cfg.login_profile = None;
            cfg.command = None;
            cfg.fast_mode = false;
            cfg.fallbacks.clear();
        }
        // A login the bot's chain falls back to is the thread's own now: that step goes.
        if let Some(profile) = &self.login_profile {
            cfg.login_profile = Some(profile.clone()).filter(|p| !p.is_empty());
            crate::agent::fallback::tidy(&mut cfg);
        }
        if let Some(cwd) = &self.cwd {
            cfg.cwd.clone_from(cwd);
        }
        if let Some(model) = &self.model {
            cfg.model.clone_from(&model.id);
            cfg.model_options.clone_from(&model.options);
        }
        if let Some(approval) = &self.approval {
            cfg.permission = approval.permission;
            cfg.mode.clone_from(&approval.mode);
        }
        if self.workflow.is_some() {
            cfg.workflow.clone_from(&self.workflow);
        }
        cfg
    }
}

/// One piece of work in a bot's box.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[expect(clippy::struct_excessive_bools, reason = "independent facts about a task")]
pub(super) struct Task {
    pub id: String,
    pub title: String,
    /// What its thread is told: the bot's brief, a routine run's prompt (written when it starts) or the request.
    pub brief: String,
    #[serde(default, skip_serializing_if = "Source::is_bot")]
    pub source: Source,
    /// What its task runs with in place of its bot's setup.
    #[serde(default, skip_serializing_if = "Setup::is_bots")]
    pub setup: Setup,
    /// The user's own words that led to it, word for word.
    #[serde(default)]
    pub words: Option<String>,
    /// What the user said about it since, word for word.
    #[serde(default)]
    pub notes: Vec<String>,
    /// What the bot's chat passed on to it since (`tell_task`), word for word: never the user's own.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relayed: Vec<String>,
    pub status: Status,
    /// Its place among the waiting, lowest first.
    pub order: i64,
    /// A task it follows, in that task's session.
    #[serde(default)]
    pub after: Option<String>,
    /// The thread bot that has or had it.
    // `task`: reads a box stored before this release.
    #[serde(default, alias = "task")]
    pub thread: Option<String>,
    /// It gave way to another task once, and starts again in a new session.
    #[serde(default)]
    pub cut_in: bool,
    /// A host restart already sent it back to waiting once: the next one fails it.
    #[serde(default)]
    pub restarted: bool,
    /// Its thread ended a turn while what it started in the background still ran, and was asked to wait for it: the
    /// next such end fails it (`work::hold`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub held: bool,
    #[serde(default)]
    pub hops: u8,
    pub created_at: i64,
    #[serde(default)]
    pub started_at: Option<i64>,
    #[serde(default)]
    pub ended_at: Option<i64>,
    /// What its thread reported, or why it didn't finish.
    #[serde(default)]
    pub result: Option<String>,
    /// Its report, until it has reached where it goes (its bot's turn for it has ended; a routine's result is in
    /// the chat; the asking bot has its reply): a host restart before that sends it again.
    #[serde(default)]
    pub report: Option<Report>,
    /// What kept it from finishing, with its report.
    #[serde(default)]
    pub failure: Option<Failure>,
    /// The host couldn't give it to its thread: `work_finished` says `dispatch` where `failure` says `Start`. Only on
    /// the task as it ends, never saved.
    #[serde(skip)]
    pub undispatched: bool,
    /// Its thread's session began on it (or on the task it follows, in the same session).
    #[serde(default)]
    pub began: bool,
    /// Where its new thread works: in a git worktree of its own when another thread of the bot worked in the
    /// repository as it began, or when it has one from before.
    #[serde(default)]
    pub place: Place,
    /// The worktree its thread works or worked in (`worktree`).
    #[serde(default)]
    pub worktree: Option<Worktree>,
    /// What the user wrote to its thread since its last report, word for word (`thread`): its next report ends
    /// with it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub told: Vec<String>,
    /// What it reported before the user wrote to it again: a new session that picks the work up is told.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub said: Option<String>,
    /// The files the user started it with (`startThread`), as uploaded to its bot (`{id, name, size}`): its thread
    /// gets a copy of each as it starts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<Value>,
    /// The outside app that started it over MCP (`external`), for a thread of the user's: its turns run at the level
    /// the app was given, and its words are the app's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_by: Option<super::external::StartedBy>,
}

/// Where a task's thread works, as `work_added` says it (`folder`), never the path: the bot's own folder, one of
/// its projects, or another folder; for a thread without a bot, its own folder or another.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub(super) enum FolderKind {
    Home,
    Project,
    Own,
    Other,
}

impl FolderKind {
    /// What `folder` is to bot `cfg`.
    pub(super) fn of(cfg: &BotConfig, folder: &str) -> Self {
        if cfg.plain {
            return if super::plain::is_own_folder(folder) { Self::Own } else { Self::Other };
        }
        if cfg.projects.iter().any(|p| p == folder) {
            Self::Project
        } else if folder == cfg.cwd {
            Self::Home
        } else {
            Self::Other
        }
    }
}

impl Task {
    /// The folder its new thread works in (`cfg`: its bot): the one it names, else for the bot's own work and the
    /// user's the bot's default project, else the bot's own folder (a routine's run and another bot's request work
    /// where the bot lives).
    pub(super) fn folder(&self, cfg: &BotConfig) -> String {
        let project = self.source.own().then(|| crate::agent::workspace::default_project(cfg)).flatten();
        self.setup.cwd.as_deref().or(project).unwrap_or(&cfg.cwd).to_owned()
    }

    /// [`Self::folder`], refused once it is gone (a project unmounted or renamed since it was added): its thread
    /// couldn't start there.
    pub(super) fn existing_folder(&self, cfg: &BotConfig) -> Result<String> {
        let folder = self.folder(cfg);
        ensure!(std::path::Path::new(&folder).is_dir(), "project folder does not exist: {folder}");
        Ok(folder)
    }

    /// Whether its thread works in a git worktree of its own (one to come, or one it has), not in its folder.
    pub(super) fn in_worktree(&self) -> bool {
        self.place == Place::Worktree || self.worktree.as_ref().is_some_and(|w| w.left == Left::InUse)
    }

    /// A new task, waiting; its place is its box's to give.
    pub(super) fn waiting(title: &str, brief: &str, words: Option<String>, after: Option<&str>, hops: u8) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.to_owned(),
            brief: brief.to_owned(),
            source: Source::Bot,
            setup: Setup::default(),
            words,
            notes: Vec::new(),
            relayed: Vec::new(),
            status: Status::Waiting,
            order: 0,
            after: after.map(str::to_owned),
            thread: None,
            cut_in: false,
            restarted: false,
            held: false,
            hops,
            created_at: now_ms(),
            started_at: None,
            ended_at: None,
            result: None,
            report: None,
            failure: None,
            undispatched: false,
            began: false,
            place: Place::Folder,
            worktree: None,
            told: Vec::new(),
            said: None,
            files: Vec::new(),
            started_by: None,
        }
    }

    /// Back to waiting at the front of `tasks`, to start again in a new thread, which may be asked to wait once again.
    pub(super) fn set_aside(&mut self, front: i64) {
        self.status = Status::Waiting;
        self.order = front;
        self.thread = None;
        self.started_at = None;
        self.began = false;
        self.held = false;
    }
}

pub(super) const PREFIX: &str = "work.";

fn key(bot: &str) -> String {
    format!("{PREFIX}{bot}")
}

/// Every bot with a box.
pub(super) fn bots(store: &Store) -> Vec<String> {
    store.kv_prefix(PREFIX).iter().filter_map(|key| key.strip_prefix(PREFIX)).map(str::to_owned).collect()
}

pub(super) fn load(store: &Store, bot: &str) -> Vec<Task> {
    store.kv_get(&key(bot)).and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default()
}

/// Changes `bot`'s box. One change at a time, so two can't write over each other.
// ponytail: one lock for every bot's box; a lock per bot if boxes ever change often enough to wait on each other.
pub(super) fn change<T>(hub: &Hub, bot: &str, change: impl FnOnce(&mut Vec<Task>) -> Result<T>) -> Result<T> {
    let _g = hub.work.boxes.locked();
    let mut tasks = load(&hub.store, bot);
    let out = change(&mut tasks)?;
    hub.store.kv_set(&key(bot), &serde_json::to_string(&tasks)?)?;
    Ok(out)
}

/// A deleted bot's box goes with it.
pub(super) fn forget(store: &Store, bot: &str) {
    if let Err(error) = store.kv_delete(&key(bot)) {
        tracing::warn!(bot, error = format!("{error:#}"), "couldn't drop a bot's work");
    }
}

/// How long an ended task can still be recalled.
pub(super) const PAST_MS: i64 = 30 * 24 * 60 * 60 * 1000;
/// Tasks out of the box (`archive`) `recall_task` reads at most, the newest.
pub(super) const PAST_MOST: usize = 200;

/// Whether a task that ended `at` can still be recalled `now`.
pub(super) fn within_past(at: i64, now: i64) -> bool {
    now - at <= PAST_MS
}

/// An order before every waiting task's.
pub(super) fn front(tasks: &[Task]) -> i64 {
    tasks.iter().filter(|i| i.status == Status::Waiting).map(|i| i.order).min().unwrap_or(0) - 1
}

/// An order after every task's.
pub(super) fn last(tasks: &[Task]) -> i64 {
    tasks.iter().map(|i| i.order).max().unwrap_or(0) + 1
}

/// How many of `tasks` a thread works on.
pub(super) fn working(tasks: &[Task]) -> usize {
    tasks.iter().filter(|i| i.status == Status::Working).count()
}

/// The task `id`.
pub(super) fn find<'a>(tasks: &'a mut [Task], id: &str) -> Result<&'a mut Task> {
    tasks.iter_mut().find(|i| i.id == id).ok_or_else(|| anyhow!("no such task"))
}

/// The task `thread` works on.
pub(super) fn of_thread<'a>(tasks: &'a [Task], thread: &str) -> Option<&'a Task> {
    tasks.iter().find(|i| i.status == Status::Working && i.thread.as_deref() == Some(thread))
}

/// A task as its bot's clients and tools see it now: its status with its thread's card, and what its thread does.
pub(super) struct Shown {
    pub task: Task,
    pub status: Status,
    pub activity: String,
    pub wait: Option<Wait>,
    /// What its thread's agent runs in the background, which the task waits on.
    pub background: Vec<crate::hub::BackgroundTask>,
}

/// `cfg`'s tasks worth showing: the working ones (oldest first), the waiting in dispatch order, then the ones ended
/// in the last day, newest first.
pub(super) fn shown(hub: &Hub, cfg: &BotConfig) -> Vec<Shown> {
    let tasks = load(&hub.store, &cfg.id);
    let mut waits: HashMap<String, Wait> = HashMap::new();
    if tasks.iter().any(|i| i.status == Status::Waiting) {
        let capacity = dispatch::capacity(hub, cfg, &tasks, None);
        let standings = dispatch::standings(
            &tasks,
            &capacity,
            |thread| dispatch::thread_state(hub, thread, None),
            |task| dispatch::held(hub, task),
        );
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
    let (mut live, mut ended): (Vec<Task>, Vec<Task>) = tasks.into_iter().partition(|i| !i.status.ended());
    live.sort_by_key(|i| {
        (i.status == Status::Waiting, if i.status == Status::Waiting { i.order } else { 0 }, i.started_at)
    });
    ended.retain(|i| i.ended_at.is_some_and(|at| now - at < RECENT_MS));
    ended.sort_by_key(|i| std::cmp::Reverse(i.ended_at));
    ended.truncate(RECENT_MOST);
    live.into_iter()
        .chain(ended)
        .map(|task| {
            let rt =
                task.thread.as_deref().filter(|_| task.status == Status::Working).map(|thread| hub.runtime(thread));
            let status = if asks_user(hub, &task) { Status::NeedsYou } else { task.status };
            let wait = waits.remove(&task.id);
            let (activity, background) = rt.map(now_doing).unwrap_or_default();
            Shown { task, status, activity, wait, background }
        })
        .collect()
}

/// What a working task's thread does (`rt`: its runtime): its turn's activity, else between turns what its agent runs
/// in the background, which then is the task's work; and that work.
fn now_doing(rt: crate::hub::Runtime) -> (String, Vec<crate::hub::BackgroundTask>) {
    let between = || rt.background.first().map(|t| t.activity.clone().unwrap_or_else(|| t.name.clone()));
    let activity = Some(rt.activity.clone()).filter(|a| !a.is_empty()).or_else(between).unwrap_or_default();
    (activity, rt.background)
}

/// What the bot sees of its box: [`shown`] without the threads the user started, which are theirs.
pub(super) fn seen_by_bot(hub: &Hub, cfg: &BotConfig) -> Vec<Shown> {
    shown(hub, cfg).into_iter().filter(|s| !s.task.source.is_user()).collect()
}

/// Where `task`'s thread works, of its row's folder `cwd`: not a worktree that had nothing in it and went.
fn folder_of<'a>(task: &Task, cwd: Option<&'a String>) -> Option<&'a String> {
    let gone = |worktree: &Worktree| worktree.left == Left::Removed && cwd == Some(&worktree.cwd);
    cwd.filter(|_| !task.worktree.as_ref().is_some_and(gone))
}

/// The bot's `work` on the wire.
pub fn wire(hub: &Hub, cfg: &BotConfig) -> Value {
    let shown = shown(hub, cfg);
    // The one seat a thread waiting on the user lends: past it, one more waiting gives nothing (`dispatch::free_seats`).
    let mut lent = false;
    // Each thread: where it works (a worktree's folder, or the bot's as it was when the thread began) and on what agent.
    let threads: HashMap<String, BotConfig> = if shown.is_empty() {
        HashMap::new()
    } else {
        super::of(&hub.store, &cfg.id).into_iter().map(|thread| (thread.id.clone(), thread)).collect()
    };
    Value::Array(
        shown
            .into_iter()
            .map(|Shown { task, status, activity, wait, background }| {
                // It takes one of the bot's seats.
                let seat = task.source.seated()
                    && match status {
                        Status::Working => true,
                        Status::NeedsYou => std::mem::replace(&mut lent, true),
                        _ => false,
                    };
                let thread = task.thread.as_ref().and_then(|thread| threads.get(thread));
                // A task waiting for its thread already knows the agent that thread will start on.
                let runs_on = thread.map_or_else(
                    || task.setup.over(&crate::agent::fallback::threads(cfg)).backend,
                    |t| agent_of(&hub.store, t),
                );
                let mut row = row(task, status, &activity, wait, seat, thread.map(|t| &t.cwd), Some(runs_on));
                if !background.is_empty() {
                    row["background"] = json!(background);
                }
                row
            })
            .collect(),
    )
}

/// The agent `thread` runs on now, else the one it ran on last (a backend id): the fallback it is on or ended on, else
/// its own.
pub(super) fn agent_of(store: &Store, thread: &BotConfig) -> String {
    crate::agent::fallback::effective(thread, crate::agent::bot::kept_step(store, thread)).backend
}

/// One of a bot's `work` rows: `task` as it reads now, `cwd` the folder of its thread bot and `runs_on` the agent
/// its thread runs on (or will), when known.
pub(super) fn row(
    task: Task,
    status: Status,
    activity: &str,
    wait: Option<Wait>,
    seat: bool,
    cwd: Option<&String>,
    runs_on: Option<String>,
) -> Value {
    let folder = folder_of(&task, cwd);
    let writable = task.source.own() && cwd.is_some();
    let mut row = json!({
        "id": task.id, "title": task.title, "status": status, "activity": activity, "thread": task.thread,
        "wait": wait, "cutIn": task.cut_in, "createdAt": task.created_at, "startedAt": task.started_at,
        "endedAt": task.ended_at,
        "result": task.result.map(|r| codync_agent::acp::truncate(&r, WIRE_RESULT)),
        // The user may write to its thread: the bot's own work, whose thread is still there (parked too); with files
        // too, which a host from before them doesn't say.
        "talk": writable,
        "talkFiles": writable,
        "seat": seat,
    });
    if let Some(folder) = folder {
        row["folder"] = folder.clone().into();
        // The folder Codync made for a thread without a bot: clients name it "Own folder" and list such threads together.
        if super::plain::is_own_folder(folder) {
            row["ownFolder"] = true.into();
        }
    }
    if let Some(runs_on) = runs_on {
        row["runsOn"] = runs_on.into();
    }
    if let Some(source) = task.source.wire() {
        row["source"] = source;
    }
    if let Some(worktree) = task.worktree.as_ref().and_then(Worktree::wire) {
        row["worktree"] = worktree;
    }
    // The outside app that started it (`external`): every client names it on the thread.
    if let Some(by) = &task.started_by {
        row["startedBy"] = json!({"label": by.label, "client": by.client, "level": by.level});
    }
    // The agent a thread the user started runs on, where it isn't its bot's (always without a bot).
    if let Some(agent) = &task.setup.backend {
        row["agent"] = agent.clone().into();
    }
    row
}

/// The box in a few lines: counts and the ceiling, then the live and waiting titles with their ids. None when
/// nothing is live or waiting. `whose`: `Your` for the bot's own turns, `Its` for who asks about the bot (a call).
pub fn lines(hub: &Hub, cfg: &BotConfig, whose: &str) -> Option<String> {
    let live: Vec<Shown> = seen_by_bot(hub, cfg).into_iter().filter(|s| !s.status.ended()).collect();
    let live: Vec<_> = live.iter().map(|s| (s.status, s.task.title.as_str(), s.task.id.as_str())).collect();
    summary(&live, cfg.max_threads, whose)
}

/// The tasks still to come whose report comes to the bot's own chat (its `add_task`'s: not a routine's run, another
/// bot's request or a thread the user started), as `(id, title)`.
pub fn reporting(store: &Store, bot: &str) -> Vec<(String, String)> {
    let mine = load(store, bot).into_iter().filter(|t| t.source.is_bot() && !t.status.ended());
    mine.map(|t| (t.id, t.title)).collect()
}

/// What one of a bot's tasks does now, for who follows the bot from outside its chat (a call's voice).
pub struct Doing {
    pub task: String,
    pub title: String,
    /// Its thread's activity line; empty when it says none.
    pub activity: String,
}

/// What each of `cfg`'s tasks at work does now, oldest first. One that waits on the user isn't at work.
pub fn doing(hub: &Hub, cfg: &BotConfig) -> Vec<Doing> {
    let mut tasks = load(&hub.store, &cfg.id);
    tasks.sort_by_key(|i| i.started_at);
    let at_work = tasks.into_iter().filter(|i| i.status == Status::Working).filter_map(|task| {
        let rt = hub.runtime(task.thread.as_deref()?);
        (rt.status == BotStatus::Working).then_some(Doing { task: task.id, title: task.title, activity: rt.activity })
    });
    at_work.collect()
}

/// Whether `task` works and its thread waits on the user (a card, a sign-in): it needs them, and lends its seat
/// (`dispatch::free_seats`).
pub(super) fn asks_user(hub: &Hub, task: &Task) -> bool {
    task.status == Status::Working
        && task.thread.as_deref().is_some_and(|thread| hub.runtime(thread).status == BotStatus::NeedsInput)
}

/// How `cfg` stands to what shows it outside its chat (a Live Activity, an ongoing notification), given its
/// chat's own runtime: it needs the user while one of its tasks does, and works while its box holds anything
/// that isn't over: a task working or waiting, or one whose report its chat hasn't taken yet. Otherwise the
/// chat's own status. With it, when that work began.
pub fn outside(hub: &Hub, cfg: &BotConfig, chat: &crate::hub::Runtime) -> (BotStatus, Option<i64>) {
    let tasks = load(&hub.store, &cfg.id);
    let open: Vec<&Task> =
        tasks.iter().filter(|i| !i.status.ended() || (i.source.is_bot() && i.report.is_some())).collect();
    let status = if chat.status == BotStatus::NeedsInput || open.iter().any(|i| asks_user(hub, i)) {
        BotStatus::NeedsInput
    } else if open.is_empty() {
        chat.status
    } else {
        BotStatus::Working
    };
    let began = open.iter().map(|i| i.started_at.unwrap_or(i.created_at)).min();
    (status, chat.started_at.or(began))
}

/// The thread bots that work on a task now, on the whole computer.
pub fn working_threads(store: &Store) -> Vec<BotConfig> {
    let threads = store.thread_bots().unwrap_or_default().into_iter().map(|row| row.config);
    threads
        .filter(|thread| {
            thread.thread_of.as_deref().is_some_and(|bot| of_thread(&load(store, bot), &thread.id).is_some())
        })
        .collect()
}

/// The setups the user's waiting threads will run with (`Setup::over` their bot's) where they name a login, each named
/// by its bot (a thread without one by its title): a login profile they run on must still be there when they start.
pub fn waiting_threads(store: &Store) -> Vec<BotConfig> {
    let bots = store.bots().unwrap_or_default().into_iter().filter(|row| !row.deleted);
    bots.flat_map(|row| {
        let cfg = row.config;
        let waiting = load(store, &cfg.id).into_iter();
        let named = waiting.filter(|t| t.status == Status::Waiting && t.setup.login_profile.is_some());
        let name = |t: &Task| if cfg.plain { t.title.clone() } else { cfg.name.clone() };
        named
            .map(|t| BotConfig { name: name(&t), ..t.setup.over(&crate::agent::fallback::threads(&cfg)) })
            .collect::<Vec<_>>()
    })
    .collect()
}

/// Puts a task that `thread` works on in `bot`'s box.
#[cfg(test)]
pub fn put_working_for_test(store: &Store, bot: &str, thread: &str) {
    let task = json!({"id": "i1", "title": "Work", "brief": "x", "status": "working", "order": 1, "createdAt": 0,
        "thread": thread});
    store.kv_set(&key(bot), &json!([task]).to_string()).expect("saved");
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
            // An approval, a choice or a sign-in: its card in the chat says which.
            Status::NeedsYou => "needs the user (its card in the chat)",
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
    fn only_the_bots_own_tasks_still_to_come_report_to_its_chat() {
        let hub = crate::chat::calls::context::tests::hub();
        let tasks = json!([
            {"id": "a", "title": "Mine", "brief": "x", "status": "working", "order": 1, "createdAt": 1},
            {"id": "b", "title": "Reported", "brief": "x", "status": "done", "order": 2, "createdAt": 1},
            {"id": "c", "title": "Run", "brief": "x", "status": "waiting", "order": 3, "createdAt": 1,
                "source": {"kind": "routine", "id": "r", "run": "1"}},
        ]);
        hub.store.kv_set(&key("b1"), &tasks.to_string()).expect("saved");
        assert_eq!(reporting(&hub.store, "b1"), [("a".to_owned(), "Mine".to_owned())]);
    }

    #[test]
    fn a_task_works_in_its_folder_else_the_default_project_else_where_the_bot_lives() {
        let bot: BotConfig = serde_json::from_value(
            json!({"id": "b", "name": "B", "backend": "claude", "cwd": "/home", "projects": ["/p1", "/p2"]}),
        )
        .expect("a bot");
        let task = |source: Source, cwd: Option<&str>| Task {
            source,
            setup: Setup { cwd: cwd.map(str::to_owned), ..Setup::default() },
            ..Task::waiting("t", "b", None, None, 0)
        };
        assert_eq!(task(Source::Bot, None).folder(&bot), "/p1");
        assert_eq!(task(Source::User, None).folder(&bot), "/p1");
        assert_eq!(task(Source::Bot, Some("/p2")).folder(&bot), "/p2");
        assert_eq!(
            task(Source::Request { id: None }, None).folder(&bot),
            "/home",
            "a request works where the bot lives"
        );
        let alone = BotConfig { projects: Vec::new(), ..bot.clone() };
        assert_eq!(task(Source::Bot, None).folder(&alone), "/home");

        let kind = |folder| FolderKind::of(&bot, folder);
        assert_eq!(
            (kind("/p2"), kind("/home"), kind("/tmp")),
            (FolderKind::Project, FolderKind::Home, FolderKind::Other)
        );
    }

    #[test]
    fn a_row_says_when_its_thread_works_in_a_folder_codync_made_for_it() {
        let task = || Task::waiting("t", "b", None, None, 0);
        let own = super::super::plain::own_folder(&uuid::Uuid::new_v4().to_string()).expect("an own folder");
        let own = own.to_string_lossy().into_owned();
        let row = |cwd: &String| row(task(), Status::Done, "", None, false, Some(cwd), None);
        assert_eq!((row(&own)["folder"].as_str(), row(&own)["ownFolder"].as_bool()), (Some(own.as_str()), Some(true)));
        assert!(row(&"/p/app".to_owned()).get("ownFolder").is_none(), "a folder the user picked is named as it is");
    }

    #[test]
    fn a_threads_setup_changes_only_what_it_names() {
        let bot: BotConfig = serde_json::from_value(json!({"id": "b", "name": "B", "backend": "claude", "cwd": "/bot",
            "model": "m1", "modelOptions": {"effort": "low"}, "permission": "auto", "createdAt": 0}))
        .expect("a bot");
        let setup: Setup = serde_json::from_value(json!({"model": {"id": null}, "approval": {"permission": "ask",
            "mode": "plan"}}))
        .expect("a setup");
        let run = setup.over(&bot);
        assert_eq!((run.cwd.as_str(), run.model, run.model_options.len()), ("/bot", None, 0));
        assert_eq!((run.permission, run.mode.as_deref()), (Permission::Ask, Some("plan")));
        assert!(Setup::default().is_bots() && !setup.is_bots());
        let on =
            |login: &str| Setup { login_profile: Some(login.to_owned()), ..Setup::default() }.over(&bot).login_profile;
        assert_eq!((on("work"), on("")), (Some("work".to_owned()), None), "a login profile, or the default login");
        // The login it picks leaves the bot's chain; another agent leaves the bot's Fast mode too.
        let chained = BotConfig {
            fast_mode: true,
            fallbacks: vec![
                serde_json::from_value(json!({"backend": "claude", "loginProfile": "work"})).expect("a step"),
            ],
            ..bot.clone()
        };
        let work = Setup { login_profile: Some("work".into()), ..Setup::default() }.over(&chained);
        assert!(work.fallbacks.is_empty() && work.fast_mode, "{work:?}");
        let codex = Setup { backend: Some("codex".into()), ..Setup::default() }.over(&chained);
        assert!(!codex.fast_mode && codex.fallbacks.is_empty());
        assert_eq!(serde_json::to_value(Setup::default().over(&bot)).ok(), serde_json::to_value(&bot).ok());
    }

    #[test]
    fn a_waiting_thread_on_a_login_is_named_with_it() {
        let hub = crate::chat::calls::context::tests::hub();
        let cfg = crate::chat::calls::context::tests::bot("b1", "Mika", "/tmp");
        hub.store.save_bot(&cfg).expect("saved");
        let on = |login: Option<&str>, status| Task {
            setup: Setup { login_profile: login.map(str::to_owned), ..Setup::default() },
            status,
            ..Task::waiting("Fix it", "x", None, None, 0)
        };
        let tasks = [on(Some("work"), Status::Waiting), on(None, Status::Waiting), on(Some("old"), Status::Done)];
        hub.store.kv_set(&key("b1"), &serde_json::to_string(&tasks).expect("json")).expect("saved");
        let waiting: Vec<_> = waiting_threads(&hub.store).into_iter().map(|c| (c.name, c.login_profile)).collect();
        assert_eq!(waiting, [("Mika".to_owned(), Some("work".to_owned()))]);
    }

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
                 - needs the user (its card in the chat): \"Run the suite\" (i2)\n- waiting: \"Docs\" (i3)"
            )
        );
        let about = summary(&live[..1], 1, "Its").expect("lines");
        assert!(
            about.starts_with("Its work in the background: 1 working, 0 waiting; at most 1 run at once"),
            "{about}"
        );
    }

    #[test]
    fn a_worktree_that_was_removed_is_no_folder_to_show() {
        let mut task: Task = serde_json::from_value(json!({
            "id": "i", "title": "t", "brief": "b", "status": "done", "order": 1, "createdAt": 0, "thread": "thread-1",
            "worktree": {"branch": "codync/t", "path": "/w", "cwd": "/w/app", "base": "abc", "left": "kept"},
        }))
        .expect("a task");
        let (worktree, workspace) = ("/w/app".to_owned(), "/repo/app".to_owned());
        assert_eq!(folder_of(&task, Some(&worktree)), Some(&worktree), "kept: it is there to go to");
        task.worktree.as_mut().expect("a worktree").left = Left::Removed;
        assert_eq!(folder_of(&task, Some(&worktree)), None, "it went with its branch");
        assert_eq!(folder_of(&task, Some(&workspace)), Some(&workspace), "reopened: its thread is in the bot's folder");
        assert_eq!(folder_of(&task, None), None);
    }

    #[test]
    fn a_task_says_where_it_came_from_unless_the_bot_added_it() {
        assert_eq!(Source::Bot.wire(), None, "absent: an older client shows the row as the bot's own");
        let routine = Source::Routine { id: "r1".into(), run: "run1".into() };
        assert_eq!(routine.wire(), Some(json!({"kind": "routine", "id": "r1"})));
        assert_eq!(Source::Request { id: Some("b2".into()) }.wire(), Some(json!({"kind": "request", "id": "b2"})));
        assert_eq!(Source::Request { id: None }.wire(), Some(json!({"kind": "request"})), "asked from elsewhere");
        // A task saved before sources is the bot's own.
        let old: Task = serde_json::from_value(json!({
            "id": "i", "title": "t", "brief": "b", "status": "waiting", "order": 1, "createdAt": 0,
        }))
        .expect("a task");
        assert!(old.source.is_bot());
        assert!(json!(old).get("source").is_none());
    }

    #[test]
    fn a_box_stored_before_tasks_and_threads_were_renamed_reads_and_is_saved_under_the_new_names() {
        let hub = crate::chat::calls::context::tests::hub();
        // As the last release wrote it: the session under `task` (id `task-…`), a report's task under `item`.
        let old = json!([{
            "id": "i1", "title": "Fix login", "brief": "x", "status": "done", "order": 1, "createdAt": 0,
            "task": "task-9", "endedAt": 5,
            "report": {"item": "i1", "title": "Fix login", "outcome": {"Ok": "All green."}, "hops": 1},
        }]);
        hub.store.kv_set(&key("b1"), &old.to_string()).expect("saved");
        let tasks = load(&hub.store, "b1");
        assert_eq!(tasks[0].thread.as_deref(), Some("task-9"), "its thread keeps the id it had");
        assert_eq!(tasks[0].report.as_ref().map(|r| r.task.as_str()), Some("i1"));
        assert!(of_thread(&tasks, "task-9").is_none() && crate::store::is_thread_id("task-9"));
        change(&hub, "b1", |_| Ok(())).expect("saved again");
        let saved: Value = serde_json::from_str(&hub.store.kv_get(&key("b1")).expect("a box")).expect("json");
        assert_eq!((&saved[0]["thread"], &saved[0]["report"]["task"]), (&json!("task-9"), &json!("i1")));
        assert!(saved[0].get("task").is_none() && saved[0]["report"].get("item").is_none(), "{saved}");
    }

    #[test]
    fn outside_its_chat_a_bot_works_while_its_box_does_and_needs_the_user_when_a_task_does() {
        let hub = crate::chat::calls::context::tests::hub();
        let cfg = crate::chat::calls::context::tests::bot("b-out", "Mika", "/tmp");
        hub.store.save_bot(&cfg).expect("saved");
        let stands = || hub.outside_runtime(&cfg).status;
        assert_eq!(stands(), BotStatus::Idle);
        hub.set_runtime(&cfg.id, |r| r.status = BotStatus::Error);
        assert_eq!(stands(), BotStatus::Error, "the chat's own, with an empty box");

        let task = |status: &str, more: Value| {
            let mut task = json!({"id": "i1", "title": "Work", "brief": "x", "status": status, "order": 1,
                "createdAt": 5, "thread": "thread-out"});
            task.as_object_mut().expect("an object").extend(more.as_object().cloned().unwrap_or_default());
            hub.store.kv_set(&key(&cfg.id), &json!([task]).to_string()).expect("saved");
        };
        task("waiting", json!({}));
        assert_eq!(stands(), BotStatus::Working, "what waits will start");
        task("working", json!({"startedAt": 9}));
        let rt = hub.outside_runtime(&cfg);
        assert_eq!((rt.status, rt.started_at), (BotStatus::Working, Some(9)), "since its task began");
        hub.set_runtime("thread-out", |r| r.status = BotStatus::NeedsInput);
        assert_eq!(stands(), BotStatus::NeedsInput, "its thread's card waits");
        // The same on the wire, beside the chat's own status.
        let row = hub.store.bot(&cfg.id).expect("read").expect("a bot");
        let wire = hub.bot_json(&row);
        assert_eq!(
            (&wire["status"], &wire["outside"]),
            (&json!("error"), &json!({"status": "needsInput", "startedAt": 9}))
        );
        // Done, but the chat hasn't taken its report yet: the bot still has that to say.
        let report = json!({"task": "i1", "title": "Work", "outcome": {"Ok": "done"}, "hops": 0});
        task("done", json!({"endedAt": 10, "report": report}));
        assert_eq!(stands(), BotStatus::Working);
        task("done", json!({"endedAt": 10}));
        assert_eq!(stands(), BotStatus::Error, "the box is empty: the chat's own again");
        task("stopped", json!({"endedAt": 10}));
        hub.set_runtime(&cfg.id, |r| r.status = BotStatus::Idle);
        assert_eq!(stands(), BotStatus::Idle);
    }

    #[test]
    fn a_row_names_the_agent_its_thread_runs_on() {
        let hub = crate::chat::calls::context::tests::hub();
        let mut cfg = crate::chat::calls::context::tests::bot("b-agent", "Mika", "/tmp");
        let mut thread = crate::chat::calls::context::tests::bot("thread-a", "Mika", "/tmp");
        thread.thread_of = Some(cfg.id.clone());
        thread.backend = "claude".into();
        thread.fallbacks = vec![crate::store::Fallback { backend: "codex".into(), ..Default::default() }];
        hub.store.save_bot(&cfg).expect("saved");
        hub.store.save_bot(&thread).expect("saved");
        let tasks = json!([{"id": "a", "title": "a", "brief": "x", "status": "working", "order": 1, "createdAt": 1,
            "startedAt": 1, "thread": "thread-a"}]);
        hub.store.kv_set(&key(&cfg.id), &tasks.to_string()).expect("saved");
        let runs_on = || wire(&hub, &cfg)[0]["runsOn"].clone();
        assert_eq!(runs_on(), "claude", "its own agent");
        // On its fallback, as the actor keeps it: still so once the thread's actor and its runtime are gone.
        hub.store.kv_set("fallback.on.thread-a", &crate::agent::fallback::login(&thread, 1)).expect("saved");
        assert_eq!(runs_on(), "codex", "the fallback it switched to");
        // Waiting for a seat, without a thread yet: the agent its thread will start on, in Advisor mode the threads'.
        cfg.thread_agent = Some(crate::store::Fallback { backend: "gemini".into(), ..Default::default() });
        let tasks = json!([{"id": "w", "title": "w", "brief": "x", "status": "waiting", "order": 1, "createdAt": 1}]);
        hub.store.kv_set(&key(&cfg.id), &tasks.to_string()).expect("saved");
        assert_eq!(wire(&hub, &cfg)[0]["runsOn"], "gemini");
    }

    #[test]
    fn between_turns_a_task_shows_and_does_what_its_agent_runs_in_the_background() {
        let hub = crate::chat::calls::context::tests::hub();
        let cfg = crate::chat::calls::context::tests::bot("b-bg", "Mika", "/tmp");
        hub.store.save_bot(&cfg).expect("saved");
        let tasks = json!([{"id": "a", "title": "a", "brief": "x", "status": "working", "order": 1, "createdAt": 1,
            "startedAt": 1, "thread": "thread-bg"}]);
        hub.store.kv_set(&key(&cfg.id), &tasks.to_string()).expect("saved");
        let workflow = crate::hub::BackgroundTask {
            id: "w1".into(),
            kind: crate::hub::BackgroundKind::Workflow,
            name: "after-change".into(),
            activity: None,
            started_at: 5,
        };
        hub.set_runtime("thread-bg", |r| r.background = vec![workflow.clone()]);
        let row = wire(&hub, &cfg)[0].clone();
        assert_eq!(
            row["background"],
            json!([{"id": "w1", "kind": "workflow", "name": "after-change", "startedAt": 5}])
        );
        assert_eq!(row["activity"], "after-change", "no turn runs: the workflow is what it does");
        hub.set_runtime("thread-bg", |r| r.background[0].activity = Some("Review: diff".into()));
        assert_eq!(wire(&hub, &cfg)[0]["activity"], "Review: diff");
        // A turn's own activity comes first.
        hub.set_runtime("thread-bg", |r| {
            r.status = BotStatus::Working;
            r.activity = "Thinking…".into();
        });
        assert_eq!(wire(&hub, &cfg)[0]["activity"], "Thinking…");
    }

    #[test]
    fn only_one_thread_waiting_on_the_user_lends_its_seat() {
        let hub = crate::chat::calls::context::tests::hub();
        let cfg = crate::chat::calls::context::tests::bot("b-seat", "Mika", "/tmp");
        hub.store.save_bot(&cfg).expect("saved");
        let task = |id: &str, at: i64| {
            json!({"id": id, "title": id, "brief": "x", "status": "working", "order": 1, "createdAt": at,
                "startedAt": at, "thread": format!("thread-{id}")})
        };
        let tasks = json!([task("a", 1), task("b", 2), task("c", 3)]);
        hub.store.kv_set(&key(&cfg.id), &tasks.to_string()).expect("saved");
        let seats =
            || -> Vec<Value> { wire(&hub, &cfg).as_array().expect("rows").iter().map(|r| r["seat"].clone()).collect() };
        assert_eq!(seats(), [true, true, true]);
        hub.set_runtime("thread-a", |r| r.status = BotStatus::NeedsInput);
        hub.set_runtime("thread-b", |r| r.status = BotStatus::NeedsInput);
        assert_eq!(seats(), [false, true, true], "one seat is lent, as the dispatcher counts it");

        let mut tasks = tasks;
        tasks[2]["source"] = json!({"kind": "user"});
        hub.store.kv_set(&key(&cfg.id), &tasks.to_string()).expect("saved");
        hub.set_runtime("thread-c", |r| r.status = BotStatus::NeedsInput);
        hub.set_runtime("thread-a", |r| r.status = BotStatus::Working);
        assert_eq!(seats(), [true, false, false], "the user's thread takes no seat and leaves the lent one to b");
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
