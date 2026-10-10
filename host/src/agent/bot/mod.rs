//! One actor per bot. It owns the bot's ACP process and session, runs turns one
//! at a time, and maps ACP `session/update`s onto transcript entries.
//!
//! Chat model (from Grok Bot): the thread only shows user messages, the *final*
//! agent message of each turn, permission cards and notices. Narration between
//! tool calls, thoughts, tool calls and plans are trace entries shown in the
//! "full conversation" sheet, and surface live as the roster activity line.
//!
//! Context (from Grok Bot too): the bot's instructions and memory are a frozen
//! snapshot per session and compaction epoch (`context`), finished exchanges feed
//! its memory keeper (`memory`), messages sent while it works are folded into
//! its next turn, and a turn cut off by a host restart is resumed.
//!
//! Lanes: besides its own chat, a bot talks in threads (each a session of its own,
//! forked from the main one when the agent can fork) and in group chats (turns in
//! its main session, written to the group's transcript; see `group`).

mod files;
mod queue;
mod session;
mod turn;
mod updates;

pub(crate) use session::{launch_commands, select_model, start_agent};

use crate::agent::acp::{Acp, Incoming};
use crate::chat::context::{Identity, Snapshot};
use crate::chat::group::GroupTurn;
use crate::chat::memory;
use crate::hub::{BotStatus, Hub};
use crate::store::{BotConfig, Entry, EntryKind, Lane, now_ms};
use anyhow::{Result, anyhow};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// How a notice renders in the chat (wire values: `info` / `error` / `divider`).
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NoticeStyle {
    Info,
    Error,
    Divider,
}

pub enum Cmd {
    RefreshTools,
    Routine(String),
    /// The user wrote in the bot's chat or one of its threads.
    Send {
        lane: Lane,
        entry_id: String,
        text: String,
    },
    BotRequest(crate::chat::team::BotRequest),
    /// `send_message` from the bot's `chat` MCP server: a message for the user, now.
    SendToUser {
        text: String,
        reply: tokio::sync::oneshot::Sender<Result<()>>,
    },
    SendFile {
        path: String,
        name: Option<String>,
        cancelled: Arc<std::sync::atomic::AtomicBool>,
        reply: tokio::sync::oneshot::Sender<Result<()>>,
    },
    CancelAsk {
        id: String,
    },
    /// Its turn in a group's room turn.
    Group(GroupTurn),
    /// The group's room turns stopped: drop its queued turns, stop a running one.
    CancelGroup {
        chat: String,
    },
    Stop,
    Permission {
        entry_id: String,
        option_id: Option<String>,
    },
    NewSession,
    Reconfigure(Box<BotConfig>),
    Shutdown,
}

enum Queued {
    Routine(String),
    User { lane: Lane, entry_id: String, text: String },
    BotRequest(crate::chat::team::BotRequest),
    Group(GroupTurn),
}

pub struct BotHandle {
    pub tx: mpsc::UnboundedSender<Cmd>,
    /// The actor task; it kills its agent process on the way out.
    pub task: tokio::task::JoinHandle<()>,
}

pub fn spawn(hub: Arc<Hub>, cfg: BotConfig) -> BotHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    let session_id = hub.store.bot(&cfg.id).ok().flatten().and_then(|b| b.session_id);
    // A turn the previous host process never finished (crash, update, restart), unless
    // it's stale: an hour-old task is more likely unwanted than awaited (gawkbot's rule).
    let interrupted = hub.store.kv_get(&inflight_key(&cfg.id)).and_then(|v| {
        let (started, thread) = v.split_once('\t').unwrap_or((&v, ""));
        let started: i64 = started.parse().ok()?;
        (now_ms() - started < STALE_RESUME_MS)
            .then(|| Lane { chat: cfg.id.clone(), thread: (!thread.is_empty()).then(|| thread.to_owned()) })
    });
    let keeper = memory::spawn_keeper(hub.clone(), cfg.id.clone());
    let lane = Lane::main(&cfg.id);
    let actor = Actor {
        hub,
        cfg,
        conn: None,
        session_id,
        thread_sessions: HashMap::new(),
        lane,
        turn_session: None,
        thread_intro: None,
        active_group: None,
        active_routine: None,
        routine_deadline: None,
        routine_timed_out: false,
        routine_completion: None,
        session_fresh: false,
        applied_system: HashMap::new(),
        keeper,
        turn_text: None,
        memory_session: None,
        memory_warned: false,
        turn_memory_revision: String::new(),
        announce: None,
        turn: None,
        queue: VecDeque::new(),
        active_request: None,
        seg: Seg::None,
        tools: HashMap::new(),
        plan_entry: None,
        last_text: None,
        sent: Vec::new(),
        files: files::FileShares::default(),
        perms: HashMap::new(),
        stop_requested: false,
        cancel_deadline: None,
        exit_tail: None,
        tools_changed: false,
    };
    let task = tokio::spawn(actor.run(rx, interrupted));
    BotHandle { tx, task }
}

/// The working line for a tool call: composing a message reads as typing.
fn activity(title: &str) -> &str {
    if title.contains("send_message") { "Typing…" } else { title }
}

/// What the agent can do with sessions besides creating them.
#[derive(Clone, Copy)]
struct SessionCaps {
    /// `session/load`: resume a session after a restart.
    load: bool,
    /// `session/fork`: a thread starts as a copy of the main session.
    fork: bool,
}

pub(crate) struct Conn {
    pub(crate) acp: Arc<Acp>,
    pub(crate) rx: mpsc::UnboundedReceiver<Incoming>,
    sessions: SessionCaps,
    /// Claude's adapter: takes `_meta.systemPrompt` and `_meta.claudeCode.options`.
    pub(crate) claude: bool,
    /// Sessions already created or loaded in this process.
    loaded: HashSet<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum SegKind {
    Text,
    Thought,
}

enum Seg {
    None,
    Open { kind: SegKind, entry_id: String, buf: String, flushed: Instant, dirty: bool },
}

// These flags describe independent actor concerns, not interchangeable states.
#[allow(clippy::struct_excessive_bools)]
struct Actor {
    hub: Arc<Hub>,
    cfg: BotConfig,
    conn: Option<Conn>,
    /// The main session: the bot's own chat and its group turns.
    session_id: Option<String>,
    /// Thread root -> the thread's session (loaded from kv on first use).
    thread_sessions: HashMap<String, String>,
    /// Where the current (or last) turn talks; entries go there.
    lane: Lane,
    /// The session the current turn prompts.
    turn_session: Option<String>,
    /// Told on the first message of a new thread session: what the thread is about.
    thread_intro: Option<String>,
    active_group: Option<GroupTurn>,
    active_routine: Option<String>,
    routine_deadline: Option<Instant>,
    routine_timed_out: bool,
    routine_completion: Option<RoutineCompletion>,
    /// True until the first prompt of a new ACP session has been sent.
    session_fresh: bool,
    /// Session -> the instructions this process last handed Claude as its system prompt.
    applied_system: HashMap<String, String>,
    keeper: mpsc::UnboundedSender<memory::KeeperEvent>,
    /// The user's words for this turn (None for a hidden turn or one not from the user); feeds memory.
    turn_text: Option<String>,
    memory_session: Option<String>,
    /// The user was told memory is unavailable.
    memory_warned: bool,
    turn_memory_revision: String,
    /// A profile update this turn carried, recorded once the agent got it.
    announce: Option<(Snapshot, Identity)>,
    turn: Option<i64>,
    queue: VecDeque<Queued>,
    active_request: Option<crate::chat::team::BotRequest>,
    seg: Seg,
    tools: HashMap<String, String>,
    plan_entry: Option<String>,
    last_text: Option<String>,
    /// Messages the bot sent the user this turn (`send_message`); none: its last text is the reply.
    sent: Vec<String>,
    files: files::FileShares,
    /// permission entry id -> JSON-RPC request id
    perms: HashMap<String, Value>,
    stop_requested: bool,
    /// A cancelled turn still running past this stops its agent process.
    cancel_deadline: Option<Instant>,
    /// Last stderr lines of an agent process that just exited.
    exit_tail: Option<String>,
    /// Connectors changed: restart the agent before the next turn so the
    /// session is resumed with the new MCP servers.
    tools_changed: bool,
}

struct RoutineCompletion {
    id: String,
    status: crate::routines::Status,
    detail: Option<String>,
    text: Option<String>,
}

type Done = Result<Value>;

impl Actor {
    /// `interrupted`: the previous host process stopped a turn in this lane (the bot's
    /// chat or one of its threads); resume it first.
    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<Cmd>, interrupted: Option<Lane>) {
        let (done_tx, mut done_rx) = mpsc::unbounded_channel::<Done>();
        let mut flush_tick = tokio::time::interval(Duration::from_millis(300));
        if let Some(run) = self.hub.routines.recovery(&self.cfg.id) {
            self.active_routine = Some(run.id.clone());
            self.routine_deadline = Some(Instant::now() + self.hub.routines.timeout(&run.id));
            if let Some(root) = run.root_id {
                self.resume_interrupted(Lane::in_thread(&self.cfg.id, &root), &done_tx).await;
            } else {
                self.finish_routine(
                    crate::routines::Status::Interrupted,
                    Some("Routine has no saved session; inspect before retrying".into()),
                    None,
                );
            }
        } else if let Some(lane) = interrupted {
            self.resume_interrupted(lane, &done_tx).await;
        }
        loop {
            tokio::select! {
                cmd = rx.recv() => {
                    let Some(cmd) = cmd else { break };
                    if !self.on_cmd(cmd, &done_tx).await { break }
                }
                Some(file) = self.files.rx.recv() => self.finish_file(file),
                inc = recv_incoming(&mut self.conn) => self.on_incoming(inc).await,
                Some(done) = done_rx.recv() => {
                    // Updates sent before the prompt response are already queued; apply them first.
                    while let Some(inc) = self.conn.as_mut().and_then(|c| c.rx.try_recv().ok()) {
                        self.on_incoming(Some(inc)).await;
                    }
                    self.finish_turn(&done);
                    self.next_in_queue(&done_tx).await;
                }
                _ = flush_tick.tick() => {
                    self.flush(false);
                    if self.routine_completion.is_some() && self.retry_routine_completion() {
                        self.set_inflight(false);
                        self.next_in_queue(&done_tx).await;
                    }
                    if self.active_routine.is_some() && self.turn.is_some() && self.routine_deadline.is_some_and(|deadline| Instant::now() >= deadline) && !self.routine_timed_out {
                        self.routine_timed_out = true;
                        self.stop_requested = true;
                        if let Some(c) = self.conn.take() { c.acp.kill().await; }
                    }
                    // The agent ignored the cancel: stop its process so the bot is free again.
                    if self.turn.is_some() && self.cancel_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                        self.cancel_deadline = None;
                        if let Some(c) = self.conn.take() { c.acp.kill().await; }
                    }
                },
            }
        }
        self.files.cancel();
        self.hub.team.cancel_from(&self.cfg.id);
        self.complete_request(Err(anyhow!("recipient shut down")), true);
        self.complete_group(Err(anyhow!("bot shut down")));
        if let Some(c) = self.conn.take() {
            c.acp.kill().await;
        }
    }

    fn id(&self) -> String {
        self.cfg.id.clone()
    }

    fn set_activity(&self, a: &str) {
        if self.hub.runtime(&self.cfg.id).activity != a {
            let a = a.to_owned();
            self.hub.set_runtime(&self.cfg.id, |r| {
                if r.status == BotStatus::Working {
                    r.activity = a;
                }
            });
        }
    }

    /// Adds an entry to the current lane, signed by this bot (a group shows who said it).
    fn add(&self, kind: EntryKind, turn: i64, mut data: Value) -> Option<Entry> {
        data["author"] = self.cfg.id.clone().into();
        self.hub.add_entry(&self.lane, kind, turn, &data)
    }

    /// A notice in the running turn's lane (or the bot's own chat between turns).
    fn notice(&self, text: &str, style: NoticeStyle) {
        let Some(turn) = self.turn else {
            let main = Lane::main(&self.cfg.id);
            let turn = self.hub.store.max_turn(&self.cfg.id);
            self.hub.add_entry(&main, EntryKind::Notice, turn, &json!({"text": text, "style": style}));
            return;
        };
        self.add(EntryKind::Notice, turn, json!({"text": text, "style": style}));
    }
}

/// Told to a bot whose turn the previous host process cut off (Grok Bot's upgrade resume).
const RESUME_PROMPT: &str = "[Codync restarted on this computer and interrupted you mid-task. You've been resumed with your full conversation intact. Continue exactly where you left off and finish what you were doing. If your previous step already completed an action, do NOT repeat it — just carry on from there.]";

/// How long a cancelled turn may take to wind down before its agent process is stopped.
const CANCEL_GRACE: Duration = Duration::from_secs(15);

/// An interrupted turn older than this isn't resumed.
const STALE_RESUME_MS: i64 = 60 * 60 * 1000;

fn inflight_key(bot_id: &str) -> String {
    format!("turn.inflight.{bot_id}")
}

async fn recv_incoming(conn: &mut Option<Conn>) -> Option<Incoming> {
    match conn {
        Some(c) => c.rx.recv().await,
        None => std::future::pending().await,
    }
}
