//! One actor per bot. It owns the bot's ACP process and session, runs turns one
//! at a time, and maps ACP `session/update`s onto transcript entries.
//!
//! Chat model (from Grok Bot): the chat only shows user messages, the *final*
//! agent message of each turn, permission cards and notices. Narration between
//! tool calls, thoughts, tool calls and plans are trace entries shown in the
//! "full conversation" sheet, and surface live as the roster activity line.
//!
//! Context (from Grok Bot too): the bot's instructions and memory are a frozen
//! snapshot per session and compaction epoch (`context`), finished turns feed
//! its dreaming (`memory`), messages sent while it works are folded into
//! its next turn, and a turn cut off by a host restart is resumed.
//!
//! Lanes: besides its own chat, a bot talks in group chats (turns in its main session,
//! written to the group's transcript; see `group`). A routine run talks in its chat
//! from a session of its own (see `session`).

mod compaction;
mod fallback;
mod fast_mode;
mod files;
mod launch;
mod lifecycle;
mod queue;
mod session;
mod turn;
mod updates;

pub(crate) use launch::{launch_commands, select_model, start_agent, system_prompt};

use crate::agent::acp::{self, Acp, Incoming};
use crate::agent::config::FastModeSupport;
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

/// How a notice renders in the chat (wire values: `info` / `error`).
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NoticeStyle {
    Info,
    Error,
}

pub enum Cmd {
    RefreshTools,
    Routine(String),
    /// The user wrote in the bot's chat.
    Send {
        lane: Lane,
        entry_id: String,
        text: String,
    },
    BotRequest(crate::chat::team::BotRequest),
    /// How a request this bot sent came out: a turn of its own, with any others that arrive meanwhile.
    BotReply(crate::chat::team::BotReply),
    /// `send_message` from the bot's `chat` MCP server: a message for the user, now.
    SendToUser {
        text: String,
        reply: tokio::sync::oneshot::Sender<Result<()>>,
    },
    /// `react` from the bot's `chat` MCP server: an emoji on the message the turn answers.
    React {
        emoji: String,
        /// In a room: whose message (see `chat::reactions::in_room`).
        to: Option<String>,
        reply: tokio::sync::oneshot::Sender<Result<()>>,
    },
    SendFile {
        path: String,
        name: Option<String>,
        cancelled: Arc<std::sync::atomic::AtomicBool>,
        reply: tokio::sync::oneshot::Sender<Result<()>>,
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
    /// A Codync tool asks the user before it acts (`chat::manage`), on the agent's kind of approval card.
    Approve {
        title: String,
        detail: String,
        reply: tokio::sync::oneshot::Sender<bool>,
    },
    NewSession,
    Reconfigure(Box<BotConfig>),
    /// The user wants the bot's own agent again (`switchBack`).
    SwitchBack,
    Shutdown,
}

enum Queued {
    Routine(String),
    User {
        lane: Lane,
        entry_id: String,
        text: String,
    },
    BotRequest(crate::chat::team::BotRequest),
    BotReply(crate::chat::team::BotReply),
    /// A compaction the host decided on (see `compaction`); `idle` while the user is away.
    Compact {
        idle: bool,
    },
    Group(GroupTurn),
}

pub struct BotHandle {
    pub tx: mpsc::UnboundedSender<Cmd>,
    /// The actor task; it kills its agent process on the way out.
    pub task: tokio::task::JoinHandle<()>,
}

pub fn spawn(hub: Arc<Hub>, base: BotConfig) -> BotHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    let session_id = hub.store.bot(&base.id).ok().flatten().and_then(|b| b.session_id);
    // The agent its main session runs on: one of its fallbacks, if it was on one when the host stopped.
    let cfg = fallback::resume_step(&hub, &base);
    // A turn the previous host process never finished (crash, update, restart), unless
    // it's stale: an hour-old task is more likely unwanted than awaited (gawkbot's rule).
    let interrupted = hub.store.kv_get(&inflight_key(&cfg.id)).and_then(|v| {
        let started: i64 = v.parse().ok()?;
        (now_ms() - started < STALE_RESUME_MS).then(|| Lane::main(&cfg.id))
    });
    let dreamer = memory::spawn_dreamer(hub.clone(), cfg.id.clone());
    let lane = Lane::main(&cfg.id);
    let fast_support = FastModeSupport::from_stored(hub.store.kv_get(&fast_mode::stored_key(&cfg.id)).as_deref());
    let support = fast_support.clone();
    hub.set_runtime(&cfg.id, |r| r.fast_mode = support);
    let actor = Actor {
        hub,
        base,
        cfg,
        conn: None,
        session_id,
        run_sessions: HashMap::new(),
        lane,
        turn_session: None,
        active_group: None,
        active_routine: None,
        routine_deadline: None,
        routine_timed_out: false,
        routine_completion: None,
        session_fresh: false,
        profile_resend: HashSet::new(),
        compacting: None,
        raw_prompt: false,
        handoff: None,
        compaction_notice: None,
        applied_system: HashMap::new(),
        dreamer,
        turn_text: None,
        turn_entries: Vec::new(),
        turn_memory_revision: String::new(),
        announce: None,
        turn: None,
        queue: VecDeque::new(),
        active_request: None,
        reply_hops: None,
        seg: Seg::None,
        tools: HashMap::new(),
        plan_entry: None,
        last_text: None,
        sent: Vec::new(),
        reacted: Reacted::No,
        files: files::FileShares::default(),
        perms: HashMap::new(),
        stop_requested: false,
        cancel_deadline: None,
        exit_tail: None,
        tools_changed: false,
        restart_after_turn: false,
        fast_support,
        idle_checked: Instant::now(),
        relay: fallback::Relay::default(),
    };
    let task = tokio::spawn(actor.run(rx, interrupted));
    BotHandle { tx, task }
}

/// What the agent can do with sessions besides creating them.
// Each flag is one capability the agent advertised at `initialize`, independent of the others.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy)]
struct SessionCaps {
    /// `session/load`: resume a session after a restart.
    load: bool,
    /// `session/close`: an idle session can be released while the agent keeps running.
    close: bool,
    /// ACP `systemPrompt`: instructions on `session/new` and `load` that the agent keeps at system level.
    system_prompt: bool,
    /// ACP `additionalDirectories`: folders outside `cwd` the agent may work in, here the memory folders.
    additional_directories: bool,
}

/// How the bot's instructions (identity and memory) reach the agent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Instructions {
    /// Claude's adapter: `_meta.systemPrompt`, swapped in by loading the session again.
    ClaudeMeta,
    /// ACP `systemPrompt` (append), which the agent keeps across compaction.
    SystemPrompt,
    /// A `<bot-profile>` block before the first message, sent again after a compaction.
    FirstMessage,
}

/// Which adapter answered `initialize`, where Codync treats it apart from the others.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Adapter {
    /// claude-agent-acp: takes `_meta.systemPrompt` and `_meta.claudeCode.options`, and `/compact <instructions>`.
    Claude,
    /// codex-acp: `/compact` (its keep-list is in `CODEX_CONFIG`).
    Codex,
    Other,
}

pub(crate) struct Conn {
    pub(crate) acp: Arc<Acp>,
    pub(crate) rx: mpsc::UnboundedReceiver<Incoming>,
    sessions: SessionCaps,
    pub(crate) adapter: Adapter,
    /// Sessions already created or loaded in this process, and when each was last used.
    loaded: HashMap<String, Instant>,
    started: Instant,
    /// Each live session's Fast mode option, as the agent last listed it.
    fast_options: HashMap<String, Value>,
    /// The agent reports compactions (`compaction_update`); until it does, a fall in usage stands in for them.
    reports_compaction: bool,
    /// Each session's context use and window size at its last `usage_update`.
    usage: HashMap<String, (u64, u64)>,
}

impl Conn {
    pub(crate) fn instructions(&self) -> Instructions {
        // Claude's adapter rebuilds the standard field without the `_meta` object's options, so Claude stays on it.
        if self.adapter == Adapter::Claude {
            Instructions::ClaudeMeta
        } else if self.sessions.system_prompt {
            Instructions::SystemPrompt
        } else {
            Instructions::FirstMessage
        }
    }
}

/// What came of the bot's `react` on the user's message this turn.
#[derive(Default)]
enum Reacted {
    #[default]
    No,
    /// The emoji is on the message.
    With(String),
    /// Refused to keep reactions rare: the bot meant to answer with one, so it says nothing.
    Refused,
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
    /// The bot as the user set it up.
    base: BotConfig,
    /// The bot as the agent it runs on now sees it: `base`, or one of its fallbacks (`agent::fallback::effective`).
    cfg: BotConfig,
    conn: Option<Conn>,
    /// The main session: the bot's own chat and its group turns.
    session_id: Option<String>,
    /// Routine run slot -> the run's session (loaded from kv on first use).
    run_sessions: HashMap<String, String>,
    /// Where the current (or last) turn talks; entries go there.
    lane: Lane,
    /// The session the current turn prompts.
    turn_session: Option<String>,
    active_group: Option<GroupTurn>,
    active_routine: Option<String>,
    routine_deadline: Option<Instant>,
    routine_timed_out: bool,
    routine_completion: Option<RoutineCompletion>,
    /// True until the first prompt of a new ACP session has been sent.
    session_fresh: bool,
    /// Sessions that compacted away the `<bot-profile>` their first message carried; their next prompt carries
    /// a fresh one.
    profile_resend: HashSet<String>,
    /// The running turn is a compaction the host started (`Some(true)` while the user is away).
    compacting: Option<bool>,
    /// The next prompt goes as it is: a command such as `/compact`, with nothing put before or after it.
    raw_prompt: bool,
    /// What the next prompt carries from the conversation it starts over from.
    handoff: Option<String>,
    /// The chat line of the last compaction, which a late summary fills in.
    compaction_notice: Option<String>,
    /// Session -> the instructions this process last handed Claude as its system prompt.
    applied_system: HashMap<String, String>,
    dreamer: mpsc::UnboundedSender<memory::DreamEvent>,
    /// The user's words for this turn (None for a hidden turn or one not from the user); feeds memory.
    turn_text: Option<String>,
    /// The user messages this turn answers (none when it isn't the user's).
    turn_entries: Vec<String>,
    /// The memory change this turn's notice told the agent about (see `memory::change_notice`).
    turn_memory_revision: String,
    /// A profile update this turn carried, recorded once the agent got it.
    announce: Option<(Snapshot, Identity)>,
    turn: Option<i64>,
    queue: VecDeque<Queued>,
    active_request: Option<crate::chat::team::BotRequest>,
    /// During a turn that takes bot replies: the hops of their chain (see `chat::team::BotReply`).
    reply_hops: Option<u8>,
    seg: Seg,
    tools: HashMap<String, String>,
    plan_entry: Option<String>,
    last_text: Option<String>,
    /// Messages the bot sent the user this turn (`send_message`); none: its last text is the reply.
    sent: Vec<String>,
    /// The bot's `react` this turn; with no message sent, it is the reply.
    reacted: Reacted,
    files: files::FileShares,
    /// permission entry id -> who waits for the answer
    perms: HashMap<String, updates::Answer>,
    stop_requested: bool,
    /// A cancelled turn still running past this stops its agent process.
    cancel_deadline: Option<Instant>,
    /// Last stderr lines of an agent process that just exited.
    exit_tail: Option<String>,
    /// Connectors changed: restart the agent before the next turn so the
    /// session is resumed with the new MCP servers.
    tools_changed: bool,
    /// A change that restarts the agent (`restart_agent`) came during a turn: it restarts once the turn ends.
    restart_after_turn: bool,
    /// What the agent says about Fast mode; clients see it in the bot's runtime.
    fast_support: FastModeSupport,
    /// When idle sessions and agents were last looked for.
    idle_checked: Instant,
    /// The running turn's place in the fallback chain (see `fallback`).
    relay: fallback::Relay,
}

struct RoutineCompletion {
    id: String,
    status: crate::routines::Status,
    detail: Option<String>,
    text: Option<String>,
    /// It hit its time limit, so it isn't tried again on its own.
    timed_out: bool,
}

type Done = Result<Value>;

impl Actor {
    /// `interrupted`: the previous host process stopped a turn in the bot's chat; resume it first.
    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<Cmd>, interrupted: Option<Lane>) {
        let (done_tx, mut done_rx) = mpsc::unbounded_channel::<Done>();
        let mut flush_tick = tokio::time::interval(Duration::from_millis(300));
        if let Some(run) = self.hub.routines.recovery(&self.cfg.id) {
            self.active_routine = Some(run.id.clone());
            self.routine_deadline = Some(Instant::now() + self.hub.routines.timeout(&run.id));
            self.resume_interrupted(Lane::main(&self.cfg.id), &done_tx).await;
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
                    if let Err(error) = &done && self.fall_back(error, &done_tx).await {
                        continue;
                    }
                    let (session, routine) = (self.turn_session.clone(), self.active_routine.is_some());
                    let failed = done.as_ref().is_err_and(acp::may_end_session);
                    self.finish_turn(&done);
                    match session {
                        Some(sid) if routine => self.release_spent(&sid).await,
                        Some(sid) if failed => self.close_reloadable(&sid).await,
                        _ => {}
                    }
                    self.next_in_queue(&done_tx).await;
                }
                _ = flush_tick.tick() => {
                    self.flush(false);
                    self.expire_abandoned().await;
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
                    self.release_idle().await;
                    // The idle check may have queued a compaction.
                    if self.turn.is_none() && !self.queue.is_empty() {
                        self.next_in_queue(&done_tx).await;
                    }
                },
            }
        }
        self.files.cancel();
        self.hub.team.end_turn(&self.cfg.id);
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
