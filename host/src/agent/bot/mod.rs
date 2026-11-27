//! One actor per bot. It owns the bot's ACP process and session, runs turns one
//! at a time, and maps ACP `session/update`s onto transcript entries.
//!
//! Chat model (from Grok Bot): the chat only shows user messages, the *final*
//! agent message of each turn, permission cards and notices. Narration between
//! tool calls, thoughts, tool calls and plans are trace entries shown in the
//! "full conversation" sheet, and surface live as the activity line and the
//! working steps a chat unfolds.
//!
//! Context (from Grok Bot too): the bot's instructions and memory are a frozen
//! snapshot per session and compaction epoch (`context`), finished turns feed
//! its dreaming (`memory`), messages sent while it works are folded into
//! its next turn, and a turn cut off by a host restart is resumed.
//!
//! Lanes: besides its own chat, a bot talks in group chats (turns in its main session,
//! written to the group's transcript; see `group`). A routine run talks in its chat
//! from a session of its own (see `session`).

mod background_work;
mod cmd;
mod codex_section;
mod commands;
mod compaction;
mod decisions;
mod elicitations;
mod fallback;
mod fast_mode;
mod files;
mod inflight;
mod launch;
mod lifecycle;
mod modes;
mod queue;
mod race_gate;
mod session;
mod sign_in;
mod steer;
mod tool_entry;
mod turn;
mod updates;

pub use cmd::Cmd;
use cmd::{Admitted, HostTurn, Queued};
use codync_agent::mcp_names::Renamed;
pub use commands::listed as agent_commands;
pub use compaction::{context_window, offers_threshold};
pub(crate) use fallback::{kept_step, owe_move};
use inflight::Inflight;
pub(crate) use inflight::resumable;
pub(crate) use launch::{launch_commands, select_model, start_agent, start_first, system_prompt};

use crate::chat::context::{Identity, Snapshot};
use crate::chat::group::GroupTurn;
use crate::chat::memory;
use crate::hub::{BotStatus, Hub};
use crate::store::{BotConfig, Entry, EntryKind, Lane};
use anyhow::{Result, anyhow};
use codync_agent::acp::{self, Acp, Capabilities, Incoming};
use codync_agent::config::FastModeSupport;
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

pub struct BotHandle {
    pub tx: mpsc::UnboundedSender<Cmd>,
    /// The actor task; it kills its agent process on the way out.
    pub task: tokio::task::JoinHandle<()>,
}

/// The pause flag and the active run ids, read together. The caller holds the pause transition for this
/// whole read: the flag at one instant and the ids at another would cancel a run admitted in between,
/// or spare one that was already active. A thread has no runs of its own, so its pair is the flag and
/// an empty list, with no second read.
pub(crate) fn startup_pair(hub: &Hub, cfg: &BotConfig) -> (bool, Vec<String>) {
    let was_paused = hub.paused(cfg.owner()) || hub.pause.blocked(cfg.owner());
    let runs = if was_paused && !cfg.is_thread() { hub.routines.active_ids(&cfg.id) } else { Vec::new() };
    (was_paused, runs)
}

/// `startup` is `(was_paused, active run ids)` from [`startup_pair`], already captured by the caller.
/// This does not take the pause transition. The scheduler holds that mutex across the admission that
/// reaches here, and `std::sync::Mutex` is not reentrant.
pub fn spawn(hub: Arc<Hub>, base: BotConfig, startup: (bool, Vec<String>)) -> BotHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    let session_id = hub.store.bot(&base.id).ok().flatten().and_then(|b| b.session_id);
    // The agent its main session runs on: one of its fallbacks, if it was on one when the host stopped.
    let cfg = fallback::resume_step(&hub, &base);
    let interrupted = resumable(&hub.store, &cfg.id).then(|| Lane::main(&cfg.id));
    // A thread's chat is its bot's own work, not the user's words: only the bot dreams, over its own chat.
    let dreamer =
        if cfg.is_thread() { mpsc::unbounded_channel().0 } else { memory::spawn_dreamer(hub.clone(), cfg.id.clone()) };
    let lane = Lane::main(&cfg.id);
    let fast_support = FastModeSupport::from_stored(hub.store.kv_get(&fast_mode::stored_key(&cfg.id)).as_deref());
    let support = fast_support.clone();
    hub.set_runtime(&cfg.id, |r| r.fast_mode = support);
    let mut versions = crate::agent::versions::Saves::default();
    versions.ready(&hub, &base);
    // `startup` was read under one pause transition by the caller. Reading the ids again here, or later
    // inside `run`, would cancel a routine admitted after a resume that landed before the task started.
    // `run` decides from this pair, not from the flag or the run list it would see later.
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
        active_workflow: None,
        routine_deadline: None,
        routine_timed_out: false,
        routine_completion: None,
        session_fresh: false,
        unfiled: codex_section::Unfiled::default(),
        profile_resend: HashSet::new(),
        compacting: None,
        raw_prompt: false,
        handoff: None,
        compaction_notice: None,
        applied_system: HashMap::new(),
        dreamer,
        turn_text: None,
        versions,
        turn_entries: Vec::new(),
        turn_call: None,
        turn_memory_revision: String::new(),
        announce: None,
        first_prompt: None,
        turn: None,
        turn_epoch: None,
        queue: VecDeque::new(),
        steers_in_flight: HashMap::new(),
        held_done: None,
        active_request: None,
        task_source: crate::chat::work::Source::Bot,
        reply_hops: None,
        host_turn: None,
        reports: Vec::new(),
        prompt_call: None,
        seg: Seg::None,
        tools: HashMap::new(),
        detached_before: None,
        plan_entry: None,
        last_text: None,
        sent: Vec::new(),
        reacted: Reacted::No,
        joined: false,
        unheard: Vec::new(),
        files: files::FileShares::default(),
        perms: HashMap::new(),
        asked: HashMap::new(),
        open_card: None,
        stop_requested: false,
        stopped_for_message: false,
        cancel_deadline: None,
        exit_tail: None,
        agent_error: None,
        restart_before_turn: false,
        restart_after_turn: false,
        fast_support,
        idle_checked: Instant::now(),
        relay: fallback::Relay::default(),
    };
    let task = tokio::spawn(actor.run(rx, interrupted, startup.0, startup.1));
    BotHandle { tx, task }
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

/// Which adapter answered `initialize`, where Codync treats it apart from the others (Codex's `/compact` keep-list is
/// in `CODEX_CONFIG`).
pub(crate) use codync_agent::acp::Adapter;

/// Whether a turn checks that Codync's MCP servers still run under the agent: OpenCode 2.x drops them from its
/// sessions once it has been idle a while (#508, `Actor::servers_dropped`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ServerWatch {
    /// The agent keeps them.
    Off,
    /// Not seen under this process yet, so their absence says nothing: they may run elsewhere, or never have started.
    Unseen,
    /// Seen running under it: once none does, the agent dropped them.
    Seen,
}

#[allow(clippy::struct_excessive_bools, reason = "independent facts about the agent process")]
pub(crate) struct Conn {
    pub(crate) acp: Arc<Acp>,
    pub(crate) rx: mpsc::UnboundedReceiver<Incoming>,
    /// What the agent said it can do with sessions besides creating them. A reopened session's replay is thrown away
    /// (`drop_replay`), as the host keeps the chat itself.
    sessions: Capabilities,
    servers: ServerWatch,
    /// Sessions already created or loaded in this process, and when each was last used.
    loaded: HashMap<String, Instant>,
    /// A finished routine run's session it couldn't close is still open, with its MCP servers (`release_spent`).
    spent: bool,
    started: Instant,
    /// A `session/new` or `session/load` is waiting for its answer. Still set when a routine's time limit cut it short:
    /// the agent may then open that session, tools and all, under an id the host never learns (`drop_unanswered`).
    opening: bool,
    /// Each live session's Fast mode option, as the agent last listed it.
    fast_options: HashMap<String, Value>,
    /// Each live session's modes, as the agent last listed them, and the sessions already switched to the bot's.
    modes: HashMap<String, codync_agent::modes::Modes>,
    mode_set: HashSet<String>,
    /// The agent reports compactions (`compaction_update`); until it does, a fall in usage stands in for them.
    reports_compaction: bool,
    /// Each session's context use and window size at its last `usage_update` that reported use (a stream's
    /// provisional zero is skipped, see `record_usage`).
    usage: HashMap<String, (u64, u64)>,
    /// Codync's MCP servers each session knows by another name (`codync_agent::mcp_names`), as it was created or loaded with.
    renamed: HashMap<String, Renamed>,
    /// The login profile the process runs on, which isn't removed meanwhile.
    on_profile: Option<crate::agent::login_profiles::Running>,
    /// The `CODEX_HOME` the process started with, when it isn't Codex's own default.
    codex_home: Option<String>,
}

impl Conn {
    pub(crate) fn adapter(&self) -> Adapter {
        self.sessions.adapter
    }

    /// An OpenCode older than 2, which never ends a turn its model refused: `start_first` keeps it as a last resort.
    fn superseded(&self) -> bool {
        self.sessions.never_ends_refused_turns()
    }

    pub(crate) fn deletes_sessions(&self) -> bool {
        self.sessions.delete
    }

    pub(crate) fn instructions(&self) -> Instructions {
        // Claude's adapter rebuilds the standard field without the `_meta` object's options, so Claude stays on it.
        if self.adapter() == Adapter::Claude {
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
    active_workflow: Option<String>,
    routine_deadline: Option<Instant>,
    routine_timed_out: bool,
    routine_completion: Option<RoutineCompletion>,
    /// True until the first prompt of a new ACP session has been sent.
    session_fresh: bool,
    /// The new Codex sessions that aren't in Codex's Codync section yet.
    unfiled: codex_section::Unfiled,
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
    /// The versions its turns save in the bot's own folder (`agent::versions`).
    versions: crate::agent::versions::Saves,
    /// The user messages this turn answers (none when it isn't the user's).
    turn_entries: Vec<String>,
    /// The voice call this turn answers while it lasts: its replies carry it (`chat::calls`).
    turn_call: Option<String>,
    /// The memory change this turn's notice told the agent about (see `memory::change_notice`).
    turn_memory_revision: String,
    /// A profile update this turn carried, recorded once the agent got it.
    announce: Option<(Snapshot, Identity)>,
    /// The snapshot of a new session's first prompt, marked delivered once the agent answered it.
    first_prompt: Option<Snapshot>,
    turn: Option<i64>,
    /// The pause generation the running turn was admitted under (`start_turn`). Empty when no turn runs
    /// the check; a finished turn's value is ignored while `turn` is empty.
    turn_epoch: Option<u64>,
    queue: VecDeque<Admitted>,
    /// Queued messages handed to the agent to join the running turn, by entry id, until it answers (`steer`).
    steers_in_flight: HashMap<String, steer::InFlight>,
    /// How the running turn ended, while the agent still owes an answer to a steer (`turn_ended`).
    held_done: Option<Done>,
    active_request: Option<crate::chat::team::BotRequest>,
    /// A thread's running turn works a task that came from here (`chat::work`): a routine's run and another bot's
    /// request keep the rules those turns have in a chat. Anything else: the bot's own.
    task_source: crate::chat::work::Source,
    /// During a turn that takes bot replies: the hops of their chain (see `chat::team::BotReply`).
    reply_hops: Option<u8>,
    /// The running turn is one the host started, with no message of the user's (`Cmd::Prompt`, `Cmd::FollowUp`).
    host_turn: Option<HostTurn>,
    /// The reports the running turn answers (`HostTurn::TaskReport`), kept until it ends (`end_host_turn`).
    reports: Vec<crate::chat::work::Report>,
    /// The call the next host-started turn opens (`Cmd::Prompt`), whose replies are that call's.
    prompt_call: Option<String>,
    seg: Seg,
    tools: HashMap<String, String>,
    /// A thread's: the sessions its agent ran detached in when its task's round began, which aren't what the task
    /// leaves running (`left_running`, #634). Kept while the task goes on with what a turn left running.
    detached_before: Option<Vec<i32>>,
    plan_entry: Option<String>,
    /// What it wrote last this turn, since its last `send_message` or `react`.
    last_text: Option<String>,
    /// Messages the bot sent the user this turn (`send_message`); none: its last text is the reply.
    sent: Vec<String>,
    /// The bot's `react` this turn; with no message sent, it is the reply.
    reacted: Reacted,
    /// A steered message joined this turn: its last text is a reply even after a `send_message` or `react`.
    // ponytail: the host can't tell which message a send answered, so a closing note after one shows too; a bubble too
    // many beats a reply lost.
    joined: bool,
    /// Messages that joined this turn (`steer`) with nothing from the agent since: a turn its agent then cancels on its
    /// own drops them (Codex, on a refused card), so they go out again (`requeue_unheard`).
    // ponytail: any text or tool call counts as the agent's word on them, so a card from the step already under way when
    // one joined, refused, still drops it; ACP doesn't say which step read the message.
    unheard: Vec<Admitted>,
    files: files::FileShares,
    /// permission entry id -> who waits for the answer
    perms: HashMap<String, updates::Answer>,
    /// Decision card id -> the live tool call or agent question (`elicitations`) waiting for this turn's choice.
    asked: HashMap<String, decisions::Asked>,
    /// The chat's choice card whose call stopped waiting: the user's press or next message still answers it.
    open_card: Option<String>,
    stop_requested: bool,
    /// Send now stopped the turn for a waiting message (`steer`): that message going out says so, not a `Stopped.`
    /// line drawn under it.
    stopped_for_message: bool,
    /// A cancelled turn still running past this stops its agent process.
    cancel_deadline: Option<Instant>,
    /// Last stderr lines of an agent process that just exited.
    exit_tail: Option<String>,
    /// The error the agent said its turn ended in while answering it as a success (pi, `codync_agent::pi::turn_result`).
    agent_error: Option<String>,
    /// The agent's process is stale (its tools changed, or the grants or sign-in it started with are gone): a new one
    /// starts before the next turn, which reopens its session. An agent that can't reopen one keeps its process.
    restart_before_turn: bool,
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
    /// The running turn is over, with every steer answered (`turn_ended`): it finishes or goes on with the next agent.
    async fn on_turn_done(&mut self, done: Done, done_tx: &mpsc::UnboundedSender<Done>) {
        // Updates sent before the prompt response are already queued; apply them first.
        while let Some(inc) = self.conn.as_mut().and_then(|c| c.rx.try_recv().ok()) {
            match inc {
                // No agent ends a turn it waits on: a request queued here is for work after it.
                Incoming::Request(request) => self.answer_outside_turn(request).await,
                inc => self.on_incoming(Some(inc)).await,
            }
        }
        let done = codync_agent::pi::turn_result(done, self.agent_error.take());
        // A pause can be saved, and a resume can reopen admission, before this failure is retried.
        race_gate::wait("before_fallback").await;
        if let Err(error) = &done
            && self.fall_back(error, done_tx).await
        {
            return;
        }
        let (session, routine) = (self.turn_session.clone(), self.active_routine.is_some());
        let failed = done.as_ref().is_err_and(acp::may_end_session);
        let left = self.left_running(&done).await;
        if self.finish_turn(&done, left) {
            // Its task waits on what it left running, in this session and this agent: nothing of either goes.
            return self.next_in_queue(done_tx).await;
        }
        match session {
            // A thread's session is spent like a run's; a task that follows reopens it.
            Some(sid) if routine || self.cfg.is_thread() => self.release_spent(&sid).await,
            Some(sid) if failed => self.close_reopenable(&sid).await,
            _ => {}
        }
        self.stop_spent().await;
        // Queued work starts here, before any pause or resume still sitting in the command channel.
        race_gate::wait("before_next_in_queue").await;
        self.next_in_queue(done_tx).await;
    }

    /// `interrupted`: the previous host process stopped a turn in the bot's chat; resume it first.
    /// `was_paused` and `paused_runs` were read together by whoever spawned this actor, before this task ran.
    async fn run(
        mut self,
        mut rx: mpsc::UnboundedReceiver<Cmd>,
        interrupted: Option<Lane>,
        was_paused: bool,
        paused_runs: Vec<String>,
    ) {
        // The gate only lets a test hold the gap after that read and before this branch. The branch does not
        // read `paused()` again: a resume in the gap must not reopen cut-off work, and must not change the ids.
        race_gate::wait("before_startup_pause_read").await;
        let (done_tx, mut done_rx) = mpsc::unbounded_channel::<Done>();
        let mut flush_tick = tokio::time::interval(Duration::from_millis(300));
        if was_paused {
            // Paused while the host was down: nothing resumes now (that would spend). A thread's cut-off task is
            // settled here: one of the bot's own waits again and starts only after an explicit resume, while a
            // routine run or request ends as interrupted. The bot's own run that was going says interrupted, and
            // the runs captured with the pause flag are cancelled. A run admitted after the resume is not one.
            self.set_inflight(false);
            if self.cfg.is_thread() {
                self.turn_over(crate::chat::work::Outcome::Failed(
                    crate::chat::work::Failure::Restart,
                    crate::chat::work::CUT_OFF.into(),
                ));
            } else {
                if let Some(run) = self.hub.routines.recovery(&self.cfg.id) {
                    let ran = crate::routines::Ran { timed_out: false, thread: None };
                    if let Err(error) = self.hub.routines.complete(
                        &self.hub,
                        &run.id,
                        crate::routines::Status::Interrupted,
                        Some("the bot is paused".into()),
                        None,
                        &ran,
                    ) {
                        tracing::error!(error = format!("{error:#}"), run = run.id, "couldn't record the paused run");
                    }
                }
                if let Err(error) = self.hub.routines.cancel_ids_for_pause(&self.hub.store, &paused_runs) {
                    tracing::error!(error = format!("{error:#}"), "couldn't cancel a paused bot's routine runs");
                }
            }
        } else if let Some(run) = self.hub.routines.recovery(&self.cfg.id) {
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
                Some(done) = done_rx.recv() => self.turn_ended(done, &done_tx).await,
                _ = flush_tick.tick() => {
                    self.flush(false);
                    self.expire_abandoned().await;
                    if self.routine_completion.is_some() && self.retry_routine_completion() {
                        self.set_inflight(false);
                        self.next_in_queue(&done_tx).await;
                    }
                    if (self.active_routine.is_some() || self.active_workflow.is_some()) && self.turn.is_some() && self.routine_deadline.is_some_and(|deadline| Instant::now() >= deadline) && !self.routine_timed_out {
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
        // Its cards may sit in another chat (a group's, a thread's parent's), which outlives it.
        self.expire_perms();
        if let Some(c) = self.conn.take() {
            c.acp.kill().await;
        }
        // Unless the thread runs again already: the user wrote to its thread as this actor was on its way out.
        if self.cfg.is_thread() && !self.hub.has_actor(&self.cfg.id) {
            self.hub.forget_runtime(&self.cfg.id);
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

    /// The bot (or, for a thread, its bot) is paused: no turn of its starts, so nothing is spent.
    fn paused(&self) -> bool {
        match self.cfg.thread_of.as_deref() {
            Some(owner) => self.hub.paused(owner) || self.hub.pause.blocked(owner),
            None => self.hub.paused(&self.cfg.id) || self.hub.pause.blocked(&self.cfg.id),
        }
    }

    /// Adds an entry to the current lane, signed by this bot (a group shows who said it).
    fn add(&self, kind: EntryKind, turn: i64, mut data: Value) -> Option<Entry> {
        data["author"] = self.cfg.id.clone().into();
        if let (EntryKind::Agent, Some(call)) = (kind, &self.turn_call) {
            data["call"] = call.clone().into();
        }
        self.hub.add_entry(&self.lane, kind, turn, &data)
    }

    /// A notice in the running turn's lane (or the bot's own chat between turns).
    fn notice(&self, text: &str, style: NoticeStyle) {
        self.notice_data(json!({"text": text, "style": style}));
    }

    /// `notice` with more fields than its text and style.
    fn notice_data(&self, data: Value) {
        let Some(turn) = self.turn else {
            self.add_between_turns(EntryKind::Notice, &data);
            return;
        };
        self.add(EntryKind::Notice, turn, data);
    }

    /// An entry while no turn runs: in the bot's own chat, numbered with its latest turn.
    fn add_between_turns(&self, kind: EntryKind, data: &Value) -> Option<Entry> {
        self.hub.add_entry(&Lane::main(&self.cfg.id), kind, self.hub.store.max_turn(&self.cfg.id), data)
    }
}

/// Told to a bot whose turn the previous host process cut off (Grok Bot's upgrade resume).
const RESUME_PROMPT: &str = "[Codync restarted on this computer and interrupted you mid-turn. You've been resumed with your full conversation intact. Continue exactly where you left off and finish what you were doing. If your previous step already completed an action, do NOT repeat it — just carry on from there.]";

/// Shown in a paused bot's chat when a message of the user's reaches it: nothing runs until it is resumed.
const PAUSED_NOTICE: &str = "This bot is paused. Resume it to respond.";
/// A turn whose setup outlived the start of a pause: no prompt was written, so it ends as a paused start.
const PAUSED_BEFORE_PROMPT: &str = "the bot was paused before its turn began";

/// How long a cancelled turn may take to wind down before its agent process is stopped.
const CANCEL_GRACE: Duration = Duration::from_secs(15);

async fn recv_incoming(conn: &mut Option<Conn>) -> Option<Incoming> {
    match conn {
        Some(c) => c.rx.recv().await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod startup_tests {
    use super::startup_pair;
    use crate::chat::calls::context::tests::{bot, hub};
    use serde_json::json;

    fn run_status(hub: &crate::hub::Hub, id: &str) -> String {
        hub.routines.list("b1")["runs"]
            .as_array()
            .and_then(|runs| runs.iter().find(|run| run["id"] == id))
            .and_then(|run| run["status"].as_str())
            .unwrap_or("")
            .to_owned()
    }

    /// The ids read with the pause flag are the ones a paused startup cancels. A run admitted after that
    /// read is not one of them. This is the pair `Actor::run` passes to `cancel_ids_for_pause`.
    #[test]
    fn startup_cancels_exactly_the_ids_read_with_the_pause_flag() {
        let hub = hub();
        let cfg = crate::store::BotConfig { paused: true, ..bot("b1", "Mika", "/tmp") };
        hub.store.save_bot(&cfg).expect("saved");
        let saved = hub
            .routines
            .save(&hub, "b1", &json!({"name": "Check", "instruction": "x", "triggers": [{"type": "webhook"}]}))
            .expect("routine");
        let routine = saved["routine"]["id"].as_str().expect("id");
        let enqueue = |mark: &str| {
            hub.routines.enqueue(&hub.store, "b1", routine, json!({"source": mark}), None, true).expect("queued")["run"]
                ["id"]
                .as_str()
                .expect("run id")
                .to_owned()
        };
        let first = enqueue("first");
        let second = enqueue("second");
        let startup = {
            let _transition = hub.pause.transition();
            startup_pair(&hub, &cfg)
        };
        let later = enqueue("later");
        assert!(startup.0, "the flag and the ids are one read, and the bot was paused");
        let mut ids = startup.1.clone();
        ids.sort();
        let mut expected = vec![first.clone(), second.clone()];
        expected.sort();
        assert_eq!(ids, expected, "a run admitted after the read is not in the snapshot");
        hub.routines.cancel_ids_for_pause(&hub.store, &startup.1).expect("cancel");
        assert_eq!(run_status(&hub, &first), "cancelled");
        assert_eq!(run_status(&hub, &second), "cancelled");
        assert_eq!(run_status(&hub, &later), "pending");
    }
}
