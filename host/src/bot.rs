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

use crate::acp::{self, Acp, Incoming};
use crate::context::{self, Identity, Snapshot};
use crate::group::GroupTurn;
use crate::hub::{BotStatus, Hub};
use crate::memory;
use crate::push::{self, AlertKind};
use crate::store::{BotConfig, Entry, EntryKind, Lane, Permission, lane_key, now_ms};
use anyhow::{Result, anyhow, bail};
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
    /// The user wrote in the bot's chat or one of its threads.
    Send {
        lane: Lane,
        entry_id: String,
        text: String,
    },
    Ask(crate::team::Ask),
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
    User { lane: Lane, entry_id: String, text: String },
    Ask(crate::team::Ask),
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
        session_fresh: false,
        applied_system: None,
        keeper,
        turn_text: None,
        announce: None,
        turn: None,
        queue: VecDeque::new(),
        active_ask: None,
        seg: Seg::None,
        tools: HashMap::new(),
        plan_entry: None,
        last_text: None,
        perms: HashMap::new(),
        stop_requested: false,
        exit_tail: None,
        tools_changed: false,
    };
    let task = tokio::spawn(actor.run(rx, interrupted));
    BotHandle { tx, task }
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
    /// The agent can reach remote (HTTP) MCP servers itself.
    mcp_http: bool,
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
    /// True until the first prompt of a new ACP session has been sent.
    session_fresh: bool,
    /// (session, instructions) last handed to Claude as its system prompt by this process.
    applied_system: Option<(String, String)>,
    keeper: mpsc::UnboundedSender<memory::Exchange>,
    /// The user's words for this turn (None for a hidden turn); feeds memory.
    turn_text: Option<String>,
    /// A profile update this turn carried, recorded once the agent got it.
    announce: Option<(Snapshot, Identity)>,
    turn: Option<i64>,
    queue: VecDeque<Queued>,
    active_ask: Option<crate::team::Ask>,
    seg: Seg,
    tools: HashMap<String, String>,
    plan_entry: Option<String>,
    last_text: Option<String>,
    /// permission entry id -> JSON-RPC request id
    perms: HashMap<String, Value>,
    stop_requested: bool,
    /// Last stderr lines of an agent process that just exited.
    exit_tail: Option<String>,
    /// Connectors changed: restart the agent before the next turn so the
    /// session is resumed with the new MCP servers.
    tools_changed: bool,
}

type Done = Result<Value>;

impl Actor {
    /// `interrupted`: the previous host process stopped a turn in this lane (the bot's
    /// chat or one of its threads); resume it first.
    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<Cmd>, interrupted: Option<Lane>) {
        let (done_tx, mut done_rx) = mpsc::unbounded_channel::<Done>();
        let mut flush_tick = tokio::time::interval(Duration::from_millis(300));
        if let Some(lane) = interrupted {
            self.resume_interrupted(lane, &done_tx).await;
        }
        loop {
            tokio::select! {
                cmd = rx.recv() => {
                    let Some(cmd) = cmd else { break };
                    if !self.on_cmd(cmd, &done_tx).await { break }
                }
                inc = recv_incoming(&mut self.conn) => self.on_incoming(inc).await,
                Some(done) = done_rx.recv() => {
                    // Updates sent before the prompt response are already queued; apply them first.
                    while let Some(inc) = self.conn.as_mut().and_then(|c| c.rx.try_recv().ok()) {
                        self.on_incoming(Some(inc)).await;
                    }
                    self.finish_turn(&done);
                    self.next_in_queue(&done_tx).await;
                }
                _ = flush_tick.tick() => self.flush(false),
            }
        }
        self.hub.team.cancel_from(&self.cfg.id);
        self.complete_ask(Err(anyhow!("recipient shut down")));
        self.complete_group(Err(anyhow!("bot shut down")));
        if let Some(c) = self.conn.take() {
            c.acp.kill().await;
        }
    }

    fn id(&self) -> String {
        self.cfg.id.clone()
    }

    // MARK: commands

    async fn on_cmd(&mut self, cmd: Cmd, done_tx: &mpsc::UnboundedSender<Done>) -> bool {
        match cmd {
            Cmd::Send { lane, entry_id, text } => {
                self.queue.push_back(Queued::User { lane, entry_id, text });
                if self.turn.is_none() {
                    self.next_in_queue(done_tx).await;
                }
            }
            Cmd::Group(turn) => {
                self.queue.push_back(Queued::Group(turn));
                if self.turn.is_none() {
                    self.next_in_queue(done_tx).await;
                }
            }
            Cmd::CancelGroup { chat } => {
                // Dropping a queued turn's reply tells the room it won't come.
                self.queue.retain(|q| !matches!(q, Queued::Group(g) if g.lane.chat == chat));
                if self.active_group.as_ref().is_some_and(|g| g.lane.chat == chat) {
                    self.stop().await;
                }
            }
            Cmd::Ask(ask) => {
                self.queue.push_back(Queued::Ask(ask));
                if self.turn.is_none() {
                    self.next_in_queue(done_tx).await;
                }
            }
            Cmd::CancelAsk { id } => {
                self.queue.retain(|q| !matches!(q, Queued::Ask(a) if a.id == id));
                if self.active_ask.as_ref().is_some_and(|a| a.id == id) {
                    self.hub.team.cancel_from(&self.cfg.id);
                    self.stop_requested = true;
                    // The request has expired. Kill only its process so a harness
                    // ignoring cooperative cancellation cannot block the queue.
                    if let Some(c) = self.conn.take() {
                        c.acp.kill().await;
                    }
                }
            }
            Cmd::Stop => self.stop().await,
            Cmd::Permission { entry_id, option_id } => self.answer_permission(&entry_id, option_id).await,
            Cmd::NewSession => {
                if self.turn.is_some() {
                    self.stop().await;
                }
                self.forget_session();
                // Like Grok Bot's clearConversation: the half-written episode goes too; memory stays.
                if let Err(error) = memory::set_pending_episode(&self.hub.store, &self.cfg.id, &[]) {
                    tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't clear the pending episode");
                }
                self.notice("New session — the agent starts with a fresh context.", NoticeStyle::Divider);
            }
            Cmd::Reconfigure(cfg) => {
                let cfg = *cfg;
                let restart =
                    cfg.backend != self.cfg.backend || cfg.command != self.cfg.command || cfg.cwd != self.cfg.cwd;
                self.tools_changed |= cfg.connectors != self.cfg.connectors || cfg.computer != self.cfg.computer;
                // Name, description and skill changes reach the agent as a profile update
                // on the next message (see `context`).
                self.cfg = cfg;
                if restart {
                    if let Some(c) = self.conn.take() {
                        c.acp.kill().await;
                    }
                    for root in self.thread_roots() {
                        self.set_session(Some(&root), None);
                    }
                    if self.session_id.is_some() {
                        self.forget_session();
                        self.notice(
                            "The agent, command or folder changed — the agent starts with a fresh context.",
                            NoticeStyle::Divider,
                        );
                    }
                    // A running prompt now fails; finish_turn reports it.
                }
            }
            Cmd::Shutdown => return false,
        }
        true
    }

    async fn stop(&mut self) {
        self.hub.team.cancel_from(&self.cfg.id);
        // Queued messages are dropped too: Stop means "stop everything".
        for queued in self.queue.drain(..) {
            let Queued::User { entry_id, .. } = queued else { continue };
            if let Some(mut e) = self.hub.store.entry(&entry_id) {
                e.data["status"] = "cancelled".into();
                self.hub.set_entry(&entry_id, &e.data);
            }
        }
        if self.turn.is_none() {
            return;
        }
        self.stop_requested = true;
        let ids: Vec<String> = self.perms.keys().cloned().collect();
        for id in ids {
            self.answer_permission(&id, None).await;
        }
        if let (Some(c), Some(sid)) = (&self.conn, &self.turn_session) {
            let _ = c.acp.notify("session/cancel", json!({"sessionId": sid})).await;
        }
    }

    async fn next_in_queue(&mut self, done_tx: &mpsc::UnboundedSender<Done>) {
        let main = Lane::main(&self.cfg.id);
        while self.turn.is_none() && !self.queue.is_empty() {
            match self.queue.front() {
                Some(Queued::Ask(_)) => {
                    let Some(Queued::Ask(ask)) = self.queue.pop_front() else { unreachable!("front is an ask") };
                    if ask.reply.is_closed() {
                        continue;
                    }
                    let ids = [ask.entry_id.clone()];
                    let prompt = ask.prompt.clone();
                    self.active_ask = Some(ask);
                    let result = self.start_turn(main.clone(), &ids, &prompt, false, done_tx).await;
                    // Another bot's words are not facts learned from the user.
                    self.turn_text = None;
                    if let Err(e) = result {
                        self.start_failed(&e);
                    }
                    continue;
                }
                Some(Queued::Group(_)) => {
                    let Some(Queued::Group(turn)) = self.queue.pop_front() else {
                        unreachable!("front is a group turn")
                    };
                    if turn.reply.is_closed() {
                        continue;
                    }
                    let (lane, prompt) = (turn.lane.clone(), turn.prompt.clone());
                    self.active_group = Some(turn);
                    let result = self.start_turn(lane, &[], &prompt, false, done_tx).await;
                    // The room is not the user's private chat: nothing here feeds memory.
                    self.turn_text = None;
                    if let Err(e) = result {
                        self.start_failed(&e);
                    }
                    continue;
                }
                _ => {}
            }
            // Messages sent while the agent worked go out together as its next turn
            // without crossing a bot request or another lane: each gets its own reply.
            let Some(Queued::User { lane, .. }) = self.queue.front() else { continue };
            let lane = lane.clone();
            let mut batch = Vec::new();
            while let Some(Queued::User { lane: l, .. }) = self.queue.front()
                && *l == lane
            {
                if let Some(Queued::User { entry_id, text, .. }) = self.queue.pop_front() {
                    batch.push((entry_id, text));
                }
            }
            let ids: Vec<String> = batch.iter().map(|(id, _)| id.clone()).collect();
            let text = batch.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("\n\n");
            if let Err(e) = self.start_turn(lane, &ids, &text, false, done_tx).await {
                self.start_failed(&e);
            }
        }
    }

    fn start_failed(&mut self, e: &anyhow::Error) {
        self.complete_ask(Err(anyhow!("couldn't start recipient: {e:#}")));
        self.complete_group(Err(anyhow!("couldn't start: {e:#}")));
        let mut msg = format!("Couldn't start the agent: {e}");
        if e.to_string().to_lowercase().contains("auth")
            && let Some(h) = crate::backends::harness(&self.cfg.backend)
        {
            msg = format!("{0} needs you to sign in. Open {0} in Marketplace to sign in.", h.name);
        }
        self.notice(&msg, NoticeStyle::Error);
        self.turn = None;
        self.set_inflight(false);
        self.hub.set_runtime(&self.id(), |r| {
            r.status = BotStatus::Error;
            r.activity = "Couldn't start".into();
            r.started_at = None;
            r.lane = None;
        });
        self.hub.team.cancel_from(&self.cfg.id);
    }

    fn complete_ask(&mut self, result: Result<String>) {
        if let Some(ask) = self.active_ask.take() {
            let _ = ask.reply.send(result);
        }
    }

    fn complete_group(&mut self, result: Result<Option<String>>) {
        if let Some(turn) = self.active_group.take() {
            let _ = turn.reply.send(result);
        }
    }

    /// Grok Bot's upgrade resume: the previous host process stopped mid-turn, so
    /// the agent is told, in the same session, to finish without redoing steps.
    async fn resume_interrupted(&mut self, lane: Lane, done_tx: &mpsc::UnboundedSender<Done>) {
        self.set_inflight(false);
        if self.session_for(lane.thread.as_deref()).is_none() {
            return;
        }
        if let Err(e) = self.start_turn(lane, &[], RESUME_PROMPT, true, done_tx).await {
            self.start_failed(&e);
        }
    }

    /// Marks a turn in the bot's own chat or a thread as running across host restarts
    /// (see [`Self::resume_interrupted`]); the thread root rides along.
    fn set_inflight(&self, on: bool) {
        let value = match (&self.lane.thread, on) {
            (_, false) => String::new(),
            (Some(root), true) => format!("{}\t{root}", now_ms()),
            (None, true) => now_ms().to_string(),
        };
        if let Err(error) = self.hub.store.kv_set(&inflight_key(&self.cfg.id), &value) {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't record the running turn");
        }
    }

    /// This session's frozen instructions (reads memory files when re-rendering).
    async fn snapshot(&self, session: &str) -> Result<Snapshot> {
        let (hub, cfg, session) = (self.hub.clone(), self.cfg.clone(), session.to_owned());
        tokio::task::spawn_blocking(move || context::resolve(&hub.store, &cfg, &session)).await?
    }

    /// Starts a turn in `lane` for the given user entries (none for a hidden turn).
    async fn start_turn(
        &mut self,
        lane: Lane,
        entry_ids: &[String],
        text: &str,
        hidden: bool,
        done_tx: &mpsc::UnboundedSender<Done>,
    ) -> Result<()> {
        let turn = entry_ids
            .last()
            .and_then(|id| self.hub.store.entry(id))
            .map_or_else(|| self.hub.store.max_turn(&lane.chat) + i64::from(self.active_group.is_some()), |e| e.turn);
        for id in entry_ids {
            if let Some(mut e) = self.hub.store.entry(id) {
                e.data["status"] = "sent".into();
                self.hub.set_entry(id, &e.data);
            }
        }
        self.lane = lane.clone();
        self.turn = Some(turn);
        self.turn_text = (!hidden).then(|| text.to_owned());
        self.announce = None;
        self.stop_requested = false;
        self.hub.team.start_turn(&self.cfg.id);
        self.exit_tail = None;
        self.seg = Seg::None;
        self.tools.clear();
        self.plan_entry = None;
        self.last_text = None;
        // A delegated or group turn has no live waiter after a host restart. Its persisted
        // notices are marked interrupted instead of silently repeating work.
        self.set_inflight(self.active_ask.is_none() && self.active_group.is_none());
        self.hub.set_runtime(&self.id(), |r| {
            r.status = BotStatus::Working;
            r.activity = if hidden { "Picking up where it left off…" } else { "Starting…" }.into();
            r.started_at = Some(now_ms());
            r.lane = Some(lane.clone());
        });

        let slot = self.slot(&lane);
        self.ensure_session(slot.as_deref()).await?;
        if hidden && self.session_fresh {
            // The interrupted session couldn't be resumed, so there is nothing to continue.
            self.turn = None;
            self.set_inflight(false);
            self.hub.team.cancel_from(&self.cfg.id);
            self.hub.set_runtime(&self.id(), |r| {
                r.status = BotStatus::Idle;
                r.activity = String::new();
                r.started_at = None;
                r.lane = None;
            });
            return Ok(());
        }
        // Session-start chatter (banners, command lists) isn't part of the reply.
        // Some adapters (pi-acp) send it on a timer right after session/new.
        if self.session_fresh {
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
        if let Some(conn) = self.conn.as_mut() {
            while let Ok(inc) = conn.rx.try_recv() {
                if let Incoming::Request { id, .. } = inc {
                    let _ = conn.acp.respond_error(id, -32601, "not supported").await;
                }
            }
        }
        let claude = self.conn.as_ref().is_some_and(|c| c.claude);
        let sid = self.turn_session.clone().ok_or_else(|| anyhow!("no session"))?;
        let snapshot = self.snapshot(&sid).await?;
        let mut prompt = text.to_owned();
        if let Some(intro) = self.thread_intro.take() {
            prompt = format!("{intro}\n\n{prompt}");
        }
        if self.session_fresh {
            self.session_fresh = false;
            // Claude already has the instructions as its system prompt.
            if !claude {
                prompt = format!("<bot-profile>\n{}\n</bot-profile>\n\n{prompt}", snapshot.system);
            }
        }
        if let Some((update, identity)) = context::profile_update(&self.hub.store, &snapshot, &self.cfg) {
            prompt = format!("{prompt}\n\n{update}");
            self.announce = Some((snapshot, identity));
        }
        self.hub.set_runtime(&self.id(), |r| r.activity = "Thinking…".into());
        let acp = self.conn.as_ref().ok_or_else(|| anyhow!("agent not running"))?.acp.clone();
        let params = json!({
            "sessionId": sid,
            "prompt": [{"type": "text", "text": prompt}],
        });
        let done_tx = done_tx.clone();
        tokio::spawn(async move {
            let _ = done_tx.send(acp.request("session/prompt", params).await);
        });
        Ok(())
    }

    /// Which session a lane's turns use: a thread of the bot's own chat has its own
    /// (by root); its chat and every group lane use the main one.
    fn slot(&self, lane: &Lane) -> Option<String> {
        (lane.chat == self.cfg.id).then(|| lane.thread.clone()).flatten()
    }

    fn session_for(&mut self, slot: Option<&str>) -> Option<String> {
        let Some(root) = slot else { return self.session_id.clone() };
        if let Some(sid) = self.thread_sessions.get(root) {
            return Some(sid.clone());
        }
        let sid = self.hub.store.kv_get(&lane_key("session", &self.cfg.id, &Lane::in_thread(&self.cfg.id, root)));
        let sid = sid.filter(|s| !s.is_empty())?;
        self.thread_sessions.insert(root.to_owned(), sid.clone());
        Some(sid)
    }

    fn set_session(&mut self, slot: Option<&str>, sid: Option<&str>) {
        let Some(root) = slot else {
            self.session_id = sid.map(str::to_owned);
            if let Err(error) = self.hub.store.set_session(&self.cfg.id, sid) {
                tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't store the session");
            }
            return;
        };
        match sid {
            Some(s) => self.thread_sessions.insert(root.to_owned(), s.to_owned()),
            None => self.thread_sessions.remove(root),
        };
        let key = lane_key("session", &self.cfg.id, &Lane::in_thread(&self.cfg.id, root));
        if let Err(error) = self.hub.store.kv_set(&key, sid.unwrap_or_default()) {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't store the thread session");
        }
    }

    /// Roots of the threads that have a session (stored or loaded).
    fn thread_roots(&self) -> Vec<String> {
        let prefix = lane_key("session", &self.cfg.id, &Lane::main(&self.cfg.id)) + "/";
        let mut roots: Vec<String> = self.thread_sessions.keys().cloned().collect();
        roots.extend(
            self.hub.store.kv_prefix(&prefix).into_iter().filter_map(|k| k.strip_prefix(&prefix).map(str::to_owned)),
        );
        roots.sort();
        roots.dedup();
        roots
    }

    /// What a new thread session is told first: where the thread branches off.
    fn thread_intro(&self, root: &str, forked: bool) -> String {
        let quote = |e: &Entry| {
            let who = if e.kind == EntryKind::User.as_str() { "the user" } else { "you" };
            format!("{who}: \"{}\"", acp::truncate(e.data["text"].as_str().unwrap_or_default(), 1500))
        };
        let Some(root_entry) = self.hub.store.entry(root) else {
            return "[Thread] The user opened a thread off your main chat. It is a separate branch of the conversation.".into();
        };
        if forked {
            return format!(
                "[Thread] The user opened a thread on this message from your main chat — {}. This thread is its own \
                 branch: it starts from everything said so far, and nothing said here reaches the main chat. \
                 Just continue the conversation; there's no need to mention this.",
                quote(&root_entry)
            );
        }
        // A fresh session: bring the lines leading up to the message along.
        let before = self.hub.store.history(&self.cfg.id, root_entry.seq, 12).unwrap_or_default();
        let lines: Vec<String> = before
            .iter()
            .chain(std::iter::once(&root_entry))
            .filter(|e| {
                e.kind == EntryKind::User.as_str() || (e.kind == EntryKind::Agent.as_str() && e.data["final"] == true)
            })
            .map(quote)
            .collect();
        format!(
            "[Thread] The user opened a thread on the last of these messages from your main chat (oldest first):\n{}\n\
             This thread is its own branch of the conversation; nothing said here reaches the main chat. \
             Just continue the conversation; there's no need to mention this.",
            lines.join("\n")
        )
    }

    /// ACP connectors, team collaboration, and optional computer control.
    fn mcp_servers(&self, http_ok: bool) -> Value {
        let all = crate::market::connectors(&self.hub.store);
        let mut servers: Vec<Value> =
            all.iter().filter(|c| self.cfg.connectors.contains(&c.id)).filter_map(|c| c.acp(http_ok)).collect();
        match std::env::current_exe() {
            Ok(exe) => {
                let composio = crate::composio::enabled_for(&self.hub.store, &self.cfg.connectors);
                let builtin = [Some("team"), self.cfg.computer.then_some("computer"), composio.then_some("composio")];
                for name in builtin.into_iter().flatten() {
                    servers.push(json!({
                        "name": name, "command": exe,
                        "args": ["mcp", name, "--bot", self.cfg.id, "--port", self.hub.port.to_string()],
                        "env": [],
                    }));
                }
            }
            Err(error) => tracing::warn!(%error, "can't locate codync-host for built-in MCP servers"),
        }
        Value::Array(servers)
    }

    /// Makes sure the agent runs and `slot`'s session (main, or a thread's) is live;
    /// sets `turn_session`. A thread without one forks the main session when it can.
    async fn ensure_session(&mut self, slot: Option<&str>) -> Result<()> {
        if self.tools_changed && self.turn.is_none() {
            self.tools_changed = false;
            if let Some(c) = self.conn.take() {
                c.acp.kill().await;
            }
        }
        if self.conn.is_none() {
            let (hub, id) = (self.hub.clone(), self.cfg.id.clone());
            let candidates = launch_commands(&self.cfg, move |msg: &str| {
                let msg = msg.to_owned();
                hub.set_runtime(&id, |r| r.activity = msg);
            })
            .await?;
            self.hub.set_runtime(&self.id(), |r| r.activity = "Starting agent…".into());
            // Keys saved from an "environment variable" sign-in.
            let env = crate::auth::env(&self.hub.store, &self.cfg.backend);
            let mut last_err = None;
            for (i, command) in candidates.iter().enumerate() {
                let fallback_left = i + 1 < candidates.len();
                // A local CLI too old for ACP just hangs; don't make the user wait long before the fallback.
                let budget = Duration::from_secs(if fallback_left { 20 } else { 180 });
                match start_agent(command, &self.cfg.cwd, &env, budget).await {
                    Ok(conn) => {
                        self.conn = Some(conn);
                        break;
                    }
                    Err(e) => {
                        tracing::info!(command, error = format!("{e:#}"), "agent failed to start");
                        last_err = Some(e);
                    }
                }
            }
            if self.conn.is_none() {
                return Err(last_err.unwrap_or_else(|| anyhow!("agent failed to start")));
            }
        }
        let servers = self.mcp_servers(self.conn.as_ref().is_some_and(|c| c.mcp_http));
        let claude = self.conn.as_ref().is_some_and(|c| c.claude);
        let mut forked = false;
        if let Some(root) = slot
            && self.session_for(slot).is_none()
            && let Some(main) = self.session_id.clone()
            && self.conn.as_ref().is_some_and(|c| c.sessions.fork)
        {
            self.hub.set_runtime(&self.cfg.id, |r| r.activity = "Opening the thread…".into());
            let conn = self.conn.as_ref().ok_or_else(|| anyhow!("agent not running"))?;
            let res = conn
                .acp
                .request("session/fork", json!({"sessionId": main, "cwd": self.cfg.cwd, "mcpServers": servers}))
                .await;
            if let Some(sid) = res.ok().and_then(|r| r["sessionId"].as_str().map(str::to_owned)) {
                self.set_session(slot, Some(&sid));
                forked = true;
            } else {
                tracing::warn!(bot = %self.cfg.id, "session/fork failed; the thread starts fresh");
            }
            if forked {
                self.thread_intro = Some(self.thread_intro(root, true));
            }
        }
        // Claude gets the frozen instructions as its system prompt. When a compaction
        // re-rendered them, loading the session again swaps the prompt in (the adapter
        // rebuilds its query and resumes the same conversation).
        let current = self.session_for(slot);
        let snapshot = match (&current, claude) {
            (Some(sid), true) => Some(self.snapshot(sid).await?),
            _ => None,
        };
        let Some(conn) = self.conn.as_mut() else { bail!("agent not running") };
        if let Some(sid) = current {
            let prompt_current = snapshot
                .as_ref()
                .is_none_or(|s| self.applied_system.as_ref().is_some_and(|(a, t)| *a == sid && *t == s.system));
            if conn.loaded.contains(&sid) && prompt_current {
                self.turn_session = Some(sid);
                return Ok(());
            }
            if !conn.sessions.load {
                self.notice(
                    "This agent can't resume sessions after a restart — starting a fresh one.",
                    NoticeStyle::Divider,
                );
                self.set_session(slot, None);
                return Box::pin(self.ensure_session(slot)).await;
            }
            self.hub.set_runtime(&self.cfg.id, |r| r.activity = "Resuming session…".into());
            let mut params = json!({"sessionId": sid, "cwd": self.cfg.cwd, "mcpServers": servers});
            if let Some(s) = &snapshot {
                params["_meta"] = system_meta(&s.system);
            }
            let res = conn.acp.request("session/load", params).await;
            // The agent replays history as session/update before answering; drop it.
            while let Ok(inc) = conn.rx.try_recv() {
                if let Incoming::Request { id, .. } = inc {
                    let _ = conn.acp.respond_error(id, -32601, "not supported").await;
                }
            }
            match res {
                Ok(_) => {
                    conn.loaded.insert(sid.clone());
                    if let Some(s) = snapshot {
                        self.applied_system = Some((sid.clone(), s.system));
                    }
                    self.turn_session = Some(sid);
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!(error = format!("{e:#}"), "session/load failed; starting fresh");
                    self.thread_intro = None;
                    self.notice("Couldn't resume the previous session — starting a fresh one.", NoticeStyle::Divider);
                }
            }
        }
        let system = {
            let (hub, cfg) = (self.hub.clone(), self.cfg.clone());
            tokio::task::spawn_blocking(move || context::render(&hub.store, &cfg)).await?
        };
        let Some(conn) = self.conn.as_mut() else { bail!("agent not running") };
        let mut params = json!({"cwd": self.cfg.cwd, "mcpServers": servers});
        if claude {
            params["_meta"] = system_meta(&system);
        }
        let res = conn.acp.request("session/new", params).await?;
        let sid = res["sessionId"].as_str().ok_or_else(|| anyhow!("agent returned no sessionId"))?.to_owned();
        conn.loaded.insert(sid.clone());
        if let Some(model) = self.cfg.model.clone().filter(|m| !m.is_empty()) {
            let has_model = res["configOptions"].as_array().is_some_and(|o| o.iter().any(|o| o["id"] == "model"));
            let r = if has_model {
                conn.acp
                    .request(
                        "session/set_config_option",
                        json!({"sessionId": sid, "configId": "model", "value": model}),
                    )
                    .await
            } else {
                conn.acp.request("session/set_model", json!({"sessionId": sid, "modelId": model})).await
            };
            if let Err(e) = r {
                tracing::warn!(model, error = format!("{e:#}"), "couldn't set model");
            }
        }
        self.set_session(slot, Some(&sid));
        self.session_fresh = true;
        self.turn_session = Some(sid.clone());
        if let Some(root) = slot {
            self.thread_intro = Some(self.thread_intro(root, false));
        }
        context::adopt(&self.hub.store, &self.cfg, &sid, system.clone())?;
        if claude {
            self.applied_system = Some((sid, system));
        }
        Ok(())
    }

    fn finish_turn(&mut self, done: &Done) {
        if self.turn.is_none() {
            return;
        }
        self.flush(true);
        self.seg = Seg::None;
        // Unanswered permission cards can't be answered after the turn.
        let ids: Vec<String> = self.perms.drain().map(|(k, _)| k).collect();
        for id in ids {
            if let Some(mut e) = self.hub.store.entry(&id) {
                e.data["status"] = "expired".into();
                self.hub.set_entry(&id, &e.data);
            }
        }
        let stopped = self.stop_requested || matches!(&done, Ok(v) if v["stopReason"] == "cancelled");
        // Tool calls the agent never closed out would spin forever in the trace.
        for entry_id in self.tools.values() {
            if let Some(mut e) = self.hub.store.entry(entry_id)
                && matches!(e.data["status"].as_str(), Some("pending" | "in_progress"))
            {
                e.data["status"] = if stopped || done.is_err() { "failed" } else { "completed" }.into();
                self.hub.set_entry(entry_id, &e.data);
            }
        }
        let mut final_text = None;
        let grouped = self.active_group.is_some();
        if let Some(id) = self.last_text.take().filter(|_| !stopped)
            && let Some(mut e) = self.hub.store.entry(&id)
        {
            let text = e.data["text"].as_str().map(str::to_owned);
            // A pass in a room stays in the trace; the room doesn't see it.
            if !(grouped && text.as_deref().is_none_or(crate::group::is_pass)) {
                e.data["final"] = true.into();
                self.hub.set_entry(&id, &e.data);
                final_text = text;
            }
        }
        let stop_reason = match &done {
            Ok(v) => v["stopReason"].as_str().unwrap_or("end_turn").to_owned(),
            Err(_) => "error".into(),
        };
        match (&done, stop_reason.as_str()) {
            (Err(e), _) if !self.stop_requested => {
                let tail = self.exit_tail.take().filter(|t| !t.is_empty());
                let detail = tail.map(|t| format!("\n\n{}", acp::truncate(&t, 1200))).unwrap_or_default();
                self.notice(&format!("The agent failed: {e}{detail}"), NoticeStyle::Error);
            }
            (_, "cancelled") | (Err(_), _) => self.notice("Stopped.", NoticeStyle::Info),
            (_, "max_tokens") => self.notice("The agent hit its output limit.", NoticeStyle::Info),
            (_, "max_turn_requests") => self.notice("The agent hit its step limit for this turn.", NoticeStyle::Info),
            (_, "refusal") => self.notice("The agent declined to continue.", NoticeStyle::Info),
            _ => {}
        }
        let failed = done.is_err() && !self.stop_requested;
        let delegated = self.active_ask.is_some();
        let reply = if stopped {
            Err(anyhow!("recipient was stopped; partial work may have happened"))
        } else if stop_reason != "end_turn" {
            Err(anyhow!("recipient did not finish ({stop_reason}); partial work may have happened"))
        } else {
            final_text
                .clone()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| anyhow!("recipient finished without a text reply"))
        };
        self.complete_ask(reply);
        self.complete_group(if stopped || failed { Err(anyhow!("stopped")) } else { Ok(final_text.clone()) });
        self.turn = None;
        self.set_inflight(false);
        // The message reached the agent: the profile update it carried is now known to it.
        if let (Ok(_), Some((snapshot, identity))) = (&done, self.announce.take())
            && let Err(error) = context::mark_announced(&self.hub.store, &self.cfg.id, &snapshot, identity)
        {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't record the profile update");
        }
        if let (Some(user), Some(agent), "end_turn") = (self.turn_text.take(), &final_text, stop_reason.as_str())
            && memory::is_memorable(&user)
        {
            let _ = self.keeper.send(memory::Exchange { user, agent: agent.clone(), at: now_ms() });
        }
        self.hub.set_runtime(&self.id(), |r| {
            r.status = if failed { BotStatus::Error } else { BotStatus::Idle };
            r.activity = String::new();
            r.started_at = None;
            r.lane = None;
        });
        self.hub.team.cancel_from(&self.cfg.id);
        // A room turn's news comes from the group once the room is done (see `group`).
        if !self.stop_requested && !delegated && !grouped {
            let body =
                final_text.unwrap_or_else(|| if failed { "The agent failed.".into() } else { "Finished.".into() });
            push::notify(&self.hub, &self.cfg, &self.cfg.name, &body, AlertKind::Done);
        }
        push::live_activity_end(&self.hub, &self.cfg.id);
    }

    // MARK: agent → client

    async fn on_incoming(&mut self, inc: Option<Incoming>) {
        match inc {
            None | Some(Incoming::Closed { .. }) => {
                let tail = match inc {
                    Some(Incoming::Closed { stderr_tail }) => stderr_tail,
                    _ => String::new(),
                };
                self.conn = None;
                // The in-flight prompt request resolves with an error; finish_turn reports it with this output.
                self.exit_tail = Some(tail);
            }
            Some(Incoming::Notification { method, params }) => {
                if method == "session/update"
                    && params["sessionId"].as_str() == self.turn_session.as_deref()
                    && self.turn.is_some()
                {
                    self.on_update(&params["update"]);
                }
            }
            Some(Incoming::Request { id, method, params }) => {
                let Some(conn) = &self.conn else { return };
                let acp = conn.acp.clone();
                if method == "session/request_permission" && self.turn.is_some() {
                    self.on_permission(id, params).await;
                } else {
                    let _ = acp.respond_error(id, -32601, &format!("{method} is not supported by Codync")).await;
                }
            }
        }
    }

    fn on_update(&mut self, u: &Value) {
        let turn = self.turn.unwrap_or(0);
        match u["sessionUpdate"].as_str().unwrap_or_default() {
            "agent_message_chunk" => {
                let t = acp::content_text(&u["content"]);
                self.append(SegKind::Text, &t);
                self.set_activity("Writing a reply…");
            }
            "agent_thought_chunk" => {
                let t = acp::content_text(&u["content"]);
                self.append(SegKind::Thought, &t);
                self.set_activity("Thinking…");
            }
            "tool_call" => {
                self.close_seg();
                let tool_id = u["toolCallId"].as_str().unwrap_or_default().to_owned();
                let mut data = json!({
                    "toolCallId": tool_id,
                    "title": u["title"].as_str().unwrap_or("Tool"),
                    "toolKind": u["kind"].as_str().unwrap_or("other"),
                    "status": u["status"].as_str().unwrap_or("pending"),
                    "output": "",
                    "diffs": [],
                    "locations": u["locations"].clone(),
                });
                merge_tool_content(&mut data, &u["content"]);
                let title = data["title"].as_str().unwrap_or("Tool").to_owned();
                if let Some(e) = self.add(EntryKind::Tool, turn, data) {
                    self.tools.insert(tool_id, e.id);
                }
                self.set_activity(&title);
            }
            "tool_call_update" => {
                let tool_id = u["toolCallId"].as_str().unwrap_or_default();
                let Some(entry_id) = self.tools.get(tool_id).cloned() else { return };
                let Some(mut e) = self.hub.store.entry(&entry_id) else { return };
                for (k, key) in [("title", "title"), ("kind", "toolKind"), ("status", "status")] {
                    if let Some(v) = u[k].as_str() {
                        e.data[key] = v.into();
                    }
                }
                if !u["locations"].is_null() {
                    e.data["locations"] = u["locations"].clone();
                }
                merge_tool_content(&mut e.data, &u["content"]);
                let status = e.data["status"].as_str().unwrap_or_default().to_owned();
                let title = e.data["title"].as_str().unwrap_or_default().to_owned();
                self.hub.set_entry(&entry_id, &e.data);
                if status == "in_progress" {
                    self.set_activity(&title);
                }
            }
            // The agent summarized its context: the next turn gets freshly rendered
            // instructions and memory (see `context`).
            "compaction_update" if u["status"] == "completed" => {
                let Some(sid) = &self.turn_session else { return };
                let id = u["compactionId"].as_str().unwrap_or_default();
                match context::bump_epoch(&self.hub.store, &self.cfg.id, sid, id) {
                    Ok(true) => self.notice("Earlier context was summarized to make room.", NoticeStyle::Info),
                    Ok(false) => {}
                    Err(error) => {
                        tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't record the compaction");
                    }
                }
            }
            "usage_update" => {
                let info = &u["_meta"]["_claude/rateLimit"];
                if info.is_object() {
                    crate::usage::ingest_claude_rate_limit(&self.hub, info);
                }
            }
            "plan" => {
                let data = json!({"entries": u["entries"].clone(), "author": self.cfg.id});
                match &self.plan_entry {
                    Some(id) => {
                        self.hub.set_entry(id, &data);
                    }
                    None => {
                        if let Some(e) = self.add(EntryKind::Plan, turn, data) {
                            self.plan_entry = Some(e.id);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    async fn on_permission(&mut self, rpc_id: Value, params: Value) {
        let options = params["options"].as_array().cloned().unwrap_or_default();
        let tool = &params["toolCall"];
        if self.cfg.permission == Permission::Auto {
            let pick = ["allow_once", "allow_always"]
                .iter()
                .find_map(|k| options.iter().find(|o| o["kind"] == *k))
                .or_else(|| options.first());
            if let (Some(o), Some(conn)) = (pick, &self.conn) {
                let _ = conn
                    .acp
                    .respond(rpc_id, json!({"outcome": {"outcome": "selected", "optionId": o["optionId"]}}))
                    .await;
                return;
            }
        }
        let title = tool["title"]
            .as_str()
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                let eid = self.tools.get(tool["toolCallId"].as_str()?)?;
                self.hub.store.entry(eid)?.data["title"].as_str().map(str::to_owned)
            })
            .unwrap_or_else(|| "Use a tool".into());
        let mut detail = json!({"output": "", "diffs": []});
        merge_tool_content(&mut detail, &tool["content"]);
        let raw = &tool["rawInput"];
        let command = raw["command"].as_str().map(str::to_owned).or_else(|| {
            raw["command"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "))
        });
        let data = json!({
            "title": title,
            "toolKind": tool["kind"].as_str().unwrap_or("other"),
            "command": command,
            "detail": detail["output"],
            "diffs": detail["diffs"],
            "cwd": self.cfg.cwd,
            "options": options.iter().map(|o| json!({"optionId": o["optionId"], "name": o["name"], "kind": o["kind"]})).collect::<Vec<_>>(),
            "status": "pending",
            "selected": Value::Null,
        });
        let turn = self.turn.unwrap_or(0);
        self.close_seg();
        if let Some(e) = self.add(EntryKind::Permission, turn, data) {
            self.perms.insert(e.id, rpc_id);
        }
        self.hub.set_runtime(&self.cfg.id, |r| {
            r.status = BotStatus::NeedsInput;
            r.activity = format!("Needs approval: {title}");
        });
        // Tapping it opens where the card is: the group, for a room turn.
        let target = self.hub.store.bot(&self.lane.chat).ok().flatten().map_or_else(|| self.cfg.clone(), |r| r.config);
        push::notify(&self.hub, &target, &format!("{} needs you", self.cfg.name), &title, AlertKind::NeedsInput);
    }

    async fn answer_permission(&mut self, entry_id: &str, option_id: Option<String>) {
        let Some(rpc_id) = self.perms.remove(entry_id) else { return };
        let outcome = match &option_id {
            Some(o) => json!({"outcome": {"outcome": "selected", "optionId": o}}),
            None => json!({"outcome": {"outcome": "cancelled"}}),
        };
        if let Some(conn) = &self.conn {
            let _ = conn.acp.respond(rpc_id, outcome).await;
        }
        if let Some(mut e) = self.hub.store.entry(entry_id) {
            e.data["status"] = if option_id.is_some() { "answered" } else { "cancelled" }.into();
            e.data["selected"] = option_id.into();
            self.hub.set_entry(entry_id, &e.data);
        }
        if self.perms.is_empty() && self.turn.is_some() {
            self.hub.set_runtime(&self.cfg.id, |r| {
                r.status = BotStatus::Working;
                r.activity = "Continuing…".into();
            });
        }
    }

    // MARK: streaming text segments

    fn append(&mut self, kind: SegKind, text: &str) {
        if text.is_empty() {
            return;
        }
        let same = matches!(&self.seg, Seg::Open { kind: k, .. } if *k == kind);
        if !same {
            self.close_seg();
            let (k, data) = match kind {
                SegKind::Text => (EntryKind::Agent, json!({"text": text, "final": false})),
                SegKind::Thought => (EntryKind::Thought, json!({"text": text})),
            };
            if let Some(e) = self.add(k, self.turn.unwrap_or(0), data) {
                if kind == SegKind::Text {
                    self.last_text = Some(e.id.clone());
                }
                self.seg =
                    Seg::Open { kind, entry_id: e.id, buf: text.to_owned(), flushed: Instant::now(), dirty: false };
            }
            return;
        }
        if let Seg::Open { buf, dirty, .. } = &mut self.seg {
            buf.push_str(text);
            *dirty = true;
        }
    }

    /// Persists streamed text at most every 300 ms (or immediately with `force`).
    fn flush(&mut self, force: bool) {
        if let Seg::Open { kind, entry_id, buf, flushed, dirty } = &mut self.seg
            && *dirty
            && (force || flushed.elapsed() >= Duration::from_millis(300))
        {
            let data = match kind {
                SegKind::Text => json!({"text": buf, "final": false, "author": self.cfg.id}),
                SegKind::Thought => json!({"text": buf, "author": self.cfg.id}),
            };
            self.hub.set_entry(entry_id, &data);
            *flushed = Instant::now();
            *dirty = false;
        }
    }

    fn close_seg(&mut self) {
        self.flush(true);
        self.seg = Seg::None;
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

    /// Drops the main ACP session so the next turn starts a fresh one.
    fn forget_session(&mut self) {
        self.set_session(None, None);
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

/// An interrupted turn older than this isn't resumed.
const STALE_RESUME_MS: i64 = 60 * 60 * 1000;

fn inflight_key(bot_id: &str) -> String {
    format!("turn.inflight.{bot_id}")
}

/// Claude's `_meta`: the bot's instructions appended to Claude Code's own system prompt.
fn system_meta(system: &str) -> Value {
    json!({"systemPrompt": {"append": system}})
}

/// Shell commands that may start the bot's agent over ACP, best first.
/// `progress` receives short status lines ("Downloading Cursor…").
pub(crate) async fn launch_commands(cfg: &BotConfig, progress: impl Fn(&str)) -> Result<Vec<String>> {
    Ok(match cfg.command.as_deref().map(str::trim) {
        Some(c) if !c.is_empty() => vec![c.to_owned()],
        _ => crate::backends::launch_candidates(&cfg.backend, progress)
            .await?
            .iter()
            .map(crate::registry::Cmd::acp)
            .collect(),
    })
}

/// Spawns an ACP agent and completes the `initialize` handshake.
pub(crate) async fn start_agent(command: &str, cwd: &str, env: &[(String, String)], budget: Duration) -> Result<Conn> {
    let (acp, rx) = Acp::spawn(command, cwd, env)?;
    let init = tokio::time::timeout(
        budget,
        acp.request(
            "initialize",
            json!({
                "protocolVersion": 1,
                // `session.compaction`: tell us when the agent summarizes its context.
                "clientCapabilities": {
                    "fs": {"readTextFile": false, "writeTextFile": false},
                    "terminal": false,
                    "session": {"compaction": {}},
                },
                "clientInfo": {"name": "codync", "title": "Codync", "version": env!("CARGO_PKG_VERSION")},
            }),
        ),
    )
    .await;
    let init = match init {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            acp.kill().await;
            return Err(e);
        }
        Err(_) => {
            acp.kill().await;
            return Err(anyhow!("the agent didn't answer the ACP handshake within {}s", budget.as_secs()));
        }
    };
    let sessions = SessionCaps {
        load: init["agentCapabilities"]["loadSession"].as_bool().unwrap_or(false),
        fork: init["agentCapabilities"]["sessionCapabilities"]["fork"].is_object(),
    };
    let mcp_http = init["agentCapabilities"]["mcpCapabilities"]["http"].as_bool().unwrap_or(false);
    let claude = init["agentCapabilities"]["_meta"]["claudeCode"].is_object();
    Ok(Conn { acp, rx, sessions, mcp_http, claude, loaded: HashSet::new() })
}

async fn recv_incoming(conn: &mut Option<Conn>) -> Option<Incoming> {
    match conn {
        Some(c) => c.rx.recv().await,
        None => std::future::pending().await,
    }
}

/// Folds ACP tool-call `content` into `{output, diffs}` on an entry's data.
pub fn merge_tool_content(data: &mut Value, content: &Value) {
    let Some(items) = content.as_array() else { return };
    let mut out = String::new();
    let mut diffs = vec![];
    for item in items {
        match item["type"].as_str() {
            Some("diff") => {
                let path = item["path"].as_str().unwrap_or_default();
                let old = item["oldText"].as_str().unwrap_or_default();
                let new = item["newText"].as_str().unwrap_or_default();
                diffs.push(diff_summary(path, old, new));
            }
            Some("terminal") => {}
            _ => {
                let t = acp::content_text(item);
                if !t.is_empty() {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(&t);
                }
            }
        }
    }
    if !out.is_empty() {
        data["output"] = acp::truncate(&out, 6000).into();
    }
    if !diffs.is_empty() {
        data["diffs"] = diffs.into();
    }
}

/// Changed-region summary: trims the common prefix/suffix and reports the middle.
pub fn diff_summary(path: &str, old: &str, new: &str) -> Value {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..].iter().rev().zip(b[prefix..].iter().rev()).take_while(|(x, y)| x == y).count();
    let removed = &a[prefix..a.len() - suffix];
    let added = &b[prefix..b.len() - suffix];
    let mut unified = String::new();
    for (sign, lines) in [('-', removed), ('+', added)] {
        for l in lines {
            unified.push(sign);
            unified.push_str(l);
            unified.push('\n');
        }
    }
    json!({
        "path": path,
        "added": added.len(),
        "removed": removed.len(),
        "isNew": old.is_empty(),
        "startLine": prefix + 1,
        "patch": acp::truncate(&unified, 6000),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_counts_changed_middle() {
        let d = diff_summary("f", "a\nb\nc\n", "a\nB\nB2\nc\n");
        assert_eq!(d["added"], 2);
        assert_eq!(d["removed"], 1);
        assert_eq!(d["startLine"], 2);
        assert_eq!(d["patch"], "-b\n+B\n+B2\n");
    }

    #[test]
    fn tool_content_merges_text_and_diffs() {
        let mut data = json!({"output": "", "diffs": []});
        merge_tool_content(
            &mut data,
            &json!([
                {"type": "content", "content": {"type": "text", "text": "ok"}},
                {"type": "diff", "path": "/a.rs", "oldText": "", "newText": "x\n"}
            ]),
        );
        assert_eq!(data["output"], "ok");
        assert_eq!(data["diffs"][0]["isNew"], true);
    }
}
