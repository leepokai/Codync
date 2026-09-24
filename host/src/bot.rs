//! One actor per bot. It owns the bot's ACP process and session, runs turns one
//! at a time, and maps ACP `session/update`s onto transcript entries.
//!
//! Chat model (from Grok Bot): the thread only shows user messages, the *final*
//! agent message of each turn, permission cards and notices. Narration between
//! tool calls, thoughts, tool calls and plans are trace entries shown in the
//! "full conversation" sheet, and surface live as the roster activity line.

use crate::acp::{self, Acp, Incoming};
use crate::hub::Hub;
use crate::push;
use crate::store::{BotConfig, now_ms};
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub enum Cmd {
    Send { entry_id: String, text: String },
    Stop,
    Permission { entry_id: String, option_id: Option<String> },
    NewSession,
    Reconfigure(Box<BotConfig>),
    Shutdown,
}

pub struct BotHandle {
    pub tx: mpsc::UnboundedSender<Cmd>,
}

pub fn spawn(hub: Arc<Hub>, cfg: BotConfig) -> BotHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    let session_id = hub.store.bot(&cfg.id).ok().flatten().and_then(|b| b.session_id);
    let actor = Actor {
        hub,
        cfg,
        conn: None,
        session_id,
        session_fresh: false,
        turn: None,
        queue: VecDeque::new(),
        seg: Seg::None,
        tools: HashMap::new(),
        plan_entry: None,
        last_text: None,
        perms: HashMap::new(),
        stop_requested: false,
        exit_tail: None,
    };
    tokio::spawn(actor.run(rx));
    BotHandle { tx }
}

struct Conn {
    acp: Arc<Acp>,
    rx: mpsc::UnboundedReceiver<Incoming>,
    can_load: bool,
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
    session_id: Option<String>,
    /// True until the first prompt of a new ACP session has been sent.
    session_fresh: bool,
    turn: Option<i64>,
    queue: VecDeque<(String, String)>,
    seg: Seg,
    tools: HashMap<String, String>,
    plan_entry: Option<String>,
    last_text: Option<String>,
    /// permission entry id -> JSON-RPC request id
    perms: HashMap<String, Value>,
    stop_requested: bool,
    /// Last stderr lines of an agent process that just exited.
    exit_tail: Option<String>,
}

type Done = Result<Value>;

impl Actor {
    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<Cmd>) {
        let (done_tx, mut done_rx) = mpsc::unbounded_channel::<Done>();
        let mut flush_tick = tokio::time::interval(Duration::from_millis(300));
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
                    self.finish_turn(done).await;
                    self.next_in_queue(&done_tx).await;
                }
                _ = flush_tick.tick() => self.flush(false),
            }
        }
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
            Cmd::Send { entry_id, text } => {
                self.queue.push_back((entry_id, text));
                if self.turn.is_none() {
                    self.next_in_queue(done_tx).await;
                }
            }
            Cmd::Stop => self.stop().await,
            Cmd::Permission { entry_id, option_id } => self.answer_permission(&entry_id, option_id).await,
            Cmd::NewSession => {
                if self.turn.is_some() {
                    self.stop().await;
                }
                self.session_id = None;
                let _ = self.hub.store.set_session(&self.cfg.id, None);
                self.notice("New session — the agent starts with a fresh context.", "divider");
            }
            Cmd::Reconfigure(cfg) => {
                let cfg = *cfg;
                let restart = cfg.backend != self.cfg.backend || cfg.command != self.cfg.command || cfg.cwd != self.cfg.cwd;
                self.cfg = cfg;
                if restart {
                    if let Some(c) = self.conn.take() {
                        c.acp.kill().await;
                    }
                    self.session_id = None;
                    let _ = self.hub.store.set_session(&self.cfg.id, None);
                    // A running prompt now fails; finish_turn reports it.
                }
            }
            Cmd::Shutdown => return false,
        }
        true
    }

    async fn stop(&mut self) {
        // Queued messages are dropped too: Stop means "stop everything".
        for (entry_id, _) in self.queue.drain(..) {
            if let Some(mut e) = self.hub.store.entry(&entry_id) {
                e.data["status"] = "cancelled".into();
                let _ = self.hub.set_entry(&entry_id, e.data);
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
        if let (Some(c), Some(sid)) = (&self.conn, &self.session_id) {
            let _ = c.acp.notify("session/cancel", json!({"sessionId": sid})).await;
        }
    }

    async fn next_in_queue(&mut self, done_tx: &mpsc::UnboundedSender<Done>) {
        while self.turn.is_none() {
            let Some((entry_id, text)) = self.queue.pop_front() else { return };
            if let Err(e) = self.start_turn(&entry_id, &text, done_tx).await {
                let mut msg = format!("Couldn't start the agent: {e}");
                if e.to_string().to_lowercase().contains("auth")
                    && let Some(h) = crate::backends::harness(&self.cfg.backend)
                {
                    msg = format!("{} needs you to sign in on your computer first. {}", h.name, h.setup);
                }
                self.notice(&msg, "error");
                self.turn = None;
                self.hub.set_runtime(&self.id(), |r| {
                    r.status = "error".into();
                    r.activity = "Couldn't start".into();
                    r.started_at = None;
                });
            }
        }
    }

    async fn start_turn(&mut self, entry_id: &str, text: &str, done_tx: &mpsc::UnboundedSender<Done>) -> Result<()> {
        let turn = self.hub.store.entry(entry_id).map(|e| e.turn).unwrap_or(0);
        if let Some(mut e) = self.hub.store.entry(entry_id) {
            e.data["status"] = "sent".into();
            self.hub.set_entry(entry_id, e.data)?;
        }
        self.turn = Some(turn);
        self.stop_requested = false;
        self.exit_tail = None;
        self.seg = Seg::None;
        self.tools.clear();
        self.plan_entry = None;
        self.last_text = None;
        self.hub.set_runtime(&self.id(), |r| {
            r.status = "working".into();
            r.activity = "Starting…".into();
            r.started_at = Some(now_ms());
        });

        self.ensure_session().await?;
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
        let conn = self.conn.as_ref().ok_or_else(|| anyhow!("agent not running"))?;
        let mut prompt = text.to_owned();
        if self.session_fresh {
            self.session_fresh = false;
            let mut profile = format!("You are \"{}\", a persistent agent the user delegates work to from their phone.", self.cfg.name);
            if !self.cfg.description.trim().is_empty() {
                profile.push_str(&format!(" Standing instructions:\n{}", self.cfg.description.trim()));
            }
            profile.push_str("\nKeep your final reply short and phone-friendly: say what you did and what needs the user.");
            prompt = format!("<bot-profile>\n{profile}\n</bot-profile>\n\n{text}");
        }
        self.hub.set_runtime(&self.id(), |r| r.activity = "Thinking…".into());
        let acp = conn.acp.clone();
        let params = json!({
            "sessionId": self.session_id,
            "prompt": [{"type": "text", "text": prompt}],
        });
        let done_tx = done_tx.clone();
        tokio::spawn(async move {
            let _ = done_tx.send(acp.request("session/prompt", params).await);
        });
        Ok(())
    }

    async fn ensure_session(&mut self) -> Result<()> {
        if self.conn.is_none() {
            let candidates = match self.cfg.command.as_deref().map(str::trim) {
                Some(c) if !c.is_empty() => vec![c.to_owned()],
                _ => {
                    let hub = self.hub.clone();
                    let id = self.cfg.id.clone();
                    crate::backends::launch_candidates(&self.cfg.backend, move |msg: &str| {
                        let msg = msg.to_owned();
                        hub.set_runtime(&id, |r| r.activity = msg);
                    })
                    .await?
                }
            };
            self.hub.set_runtime(&self.id(), |r| r.activity = "Starting agent…".into());
            let mut last_err = None;
            for (i, command) in candidates.iter().enumerate() {
                let fallback_left = i + 1 < candidates.len();
                // A local CLI too old for ACP just hangs; don't make the user wait long before the fallback.
                let budget = Duration::from_secs(if fallback_left { 20 } else { 180 });
                match start_agent(command, &self.cfg.cwd, budget).await {
                    Ok(conn) => {
                        self.conn = Some(conn);
                        break;
                    }
                    Err(e) => {
                        tracing::info!("`{command}` failed to start: {e}");
                        last_err = Some(e);
                    }
                }
            }
            if self.conn.is_none() {
                return Err(last_err.unwrap_or_else(|| anyhow!("agent failed to start")));
            }
        }
        let conn = self.conn.as_mut().unwrap();
        if let Some(sid) = self.session_id.clone() {
            if conn.loaded.contains(&sid) {
                return Ok(());
            }
            if !conn.can_load {
                self.notice("This agent can't resume sessions after a restart — starting a fresh one.", "divider");
                self.session_id = None;
                return Box::pin(self.ensure_session()).await;
            }
            self.hub.set_runtime(&self.cfg.id, |r| r.activity = "Resuming session…".into());
            let res = conn
                .acp
                .request("session/load", json!({"sessionId": sid, "cwd": self.cfg.cwd, "mcpServers": []}))
                .await;
            // The agent replays history as session/update before answering; drop it.
            while let Ok(inc) = conn.rx.try_recv() {
                if let Incoming::Request { id, .. } = inc {
                    let _ = conn.acp.respond_error(id, -32601, "not supported").await;
                }
            }
            match res {
                Ok(_) => {
                    conn.loaded.insert(sid);
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!("session/load failed, starting fresh: {e}");
                    self.notice("Couldn't resume the previous session — starting a fresh one.", "divider");
                }
            }
        }
        let conn = self.conn.as_mut().unwrap();
        let res = conn
            .acp
            .request("session/new", json!({"cwd": self.cfg.cwd, "mcpServers": []}))
            .await?;
        let sid = res["sessionId"].as_str().ok_or_else(|| anyhow!("agent returned no sessionId"))?.to_owned();
        conn.loaded.insert(sid.clone());
        if let Some(model) = self.cfg.model.clone().filter(|m| !m.is_empty()) {
            let has_model = res["configOptions"].as_array().is_some_and(|o| o.iter().any(|o| o["id"] == "model"));
            let r = if has_model {
                conn.acp
                    .request("session/set_config_option", json!({"sessionId": sid, "configId": "model", "value": model}))
                    .await
            } else {
                conn.acp.request("session/set_model", json!({"sessionId": sid, "modelId": model})).await
            };
            if let Err(e) = r {
                tracing::warn!("couldn't set model {model}: {e}");
            }
        }
        self.session_id = Some(sid.clone());
        self.session_fresh = true;
        self.hub.store.set_session(&self.cfg.id, Some(&sid))?;
        Ok(())
    }

    async fn finish_turn(&mut self, done: Done) {
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
                let _ = self.hub.set_entry(&id, e.data);
            }
        }
        let stopped = self.stop_requested || matches!(&done, Ok(v) if v["stopReason"] == "cancelled");
        // Tool calls the agent never closed out would spin forever in the trace.
        for entry_id in self.tools.values() {
            if let Some(mut e) = self.hub.store.entry(entry_id)
                && matches!(e.data["status"].as_str(), Some("pending" | "in_progress"))
            {
                e.data["status"] = if stopped || done.is_err() { "failed" } else { "completed" }.into();
                let _ = self.hub.set_entry(entry_id, e.data);
            }
        }
        let mut final_text = None;
        if let Some(id) = self.last_text.take().filter(|_| !stopped)
            && let Some(mut e) = self.hub.store.entry(&id)
        {
            e.data["final"] = true.into();
            final_text = e.data["text"].as_str().map(str::to_owned);
            let _ = self.hub.set_entry(&id, e.data);
        }
        let stop_reason = match &done {
            Ok(v) => v["stopReason"].as_str().unwrap_or("end_turn").to_owned(),
            Err(_) => "error".into(),
        };
        match (&done, stop_reason.as_str()) {
            (Err(e), _) if !self.stop_requested => {
                let tail = self.exit_tail.take().filter(|t| !t.is_empty());
                let detail = tail.map(|t| format!("\n\n{}", acp::truncate(&t, 1200))).unwrap_or_default();
                self.notice(&format!("The agent failed: {e}{detail}"), "error")
            }
            (_, "cancelled") | (Err(_), _) => self.notice("Stopped.", "info"),
            (_, "max_tokens") => self.notice("The agent hit its output limit.", "info"),
            (_, "max_turn_requests") => self.notice("The agent hit its step limit for this turn.", "info"),
            (_, "refusal") => self.notice("The agent declined to continue.", "info"),
            _ => {}
        }
        let failed = done.is_err() && !self.stop_requested;
        self.turn = None;
        self.hub.set_runtime(&self.id(), |r| {
            r.status = if failed { "error" } else { "idle" }.into();
            r.activity = String::new();
            r.started_at = None;
        });
        if !self.stop_requested {
            let body = final_text.unwrap_or_else(|| if failed { "The agent failed.".into() } else { "Finished.".into() });
            push::notify(&self.hub, &self.cfg, &self.cfg.name, &body, "done");
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
                if method == "session/update" && params["sessionId"].as_str() == self.session_id.as_deref() && self.turn.is_some() {
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
                if let Ok(e) = self.hub.add_entry(&self.cfg.id, "tool", turn, data) {
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
                let _ = self.hub.set_entry(&entry_id, e.data);
                if status == "in_progress" {
                    self.set_activity(&title);
                }
            }
            "usage_update" => {
                let info = &u["_meta"]["_claude/rateLimit"];
                if info.is_object() {
                    crate::usage::ingest_claude_rate_limit(&self.hub, info);
                }
            }
            "plan" => {
                let data = json!({"entries": u["entries"].clone()});
                match &self.plan_entry {
                    Some(id) => {
                        let _ = self.hub.set_entry(id, data);
                    }
                    None => {
                        if let Ok(e) = self.hub.add_entry(&self.cfg.id, "plan", turn, data) {
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
        if self.cfg.permission == "auto" {
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
        let command = raw["command"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| raw["command"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" ")));
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
        if let Ok(e) = self.hub.add_entry(&self.cfg.id, "permission", turn, data) {
            self.perms.insert(e.id, rpc_id);
        }
        self.hub.set_runtime(&self.cfg.id, |r| {
            r.status = "needsInput".into();
            r.activity = format!("Needs approval: {title}");
        });
        push::notify(&self.hub, &self.cfg, &format!("{} needs you", self.cfg.name), &title, "needsInput");
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
            let _ = self.hub.set_entry(entry_id, e.data);
        }
        if self.perms.is_empty() && self.turn.is_some() {
            self.hub.set_runtime(&self.cfg.id, |r| {
                r.status = "working".into();
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
                SegKind::Text => ("agent", json!({"text": text, "final": false})),
                SegKind::Thought => ("thought", json!({"text": text})),
            };
            if let Ok(e) = self.hub.add_entry(&self.cfg.id, k, self.turn.unwrap_or(0), data) {
                if kind == SegKind::Text {
                    self.last_text = Some(e.id.clone());
                }
                self.seg = Seg::Open { kind, entry_id: e.id, buf: text.to_owned(), flushed: Instant::now(), dirty: false };
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
                SegKind::Text => json!({"text": buf, "final": false}),
                SegKind::Thought => json!({"text": buf}),
            };
            let _ = self.hub.set_entry(entry_id, data);
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
                if r.status == "working" {
                    r.activity = a;
                }
            });
        }
    }

    fn notice(&self, text: &str, style: &str) {
        let turn = self.turn.unwrap_or_else(|| self.hub.store.max_turn(&self.cfg.id));
        let _ = self.hub.add_entry(&self.cfg.id, "notice", turn, json!({"text": text, "style": style}));
    }
}

/// Spawns an ACP agent and completes the `initialize` handshake.
async fn start_agent(command: &str, cwd: &str, budget: Duration) -> Result<Conn> {
    let (acp, rx) = Acp::spawn(command, cwd)?;
    let init = tokio::time::timeout(
        budget,
        acp.request(
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientCapabilities": {"fs": {"readTextFile": false, "writeTextFile": false}, "terminal": false},
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
    let can_load = init["agentCapabilities"]["loadSession"].as_bool().unwrap_or(false);
    Ok(Conn { acp, rx, can_load, loaded: HashSet::new() })
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
    let mut patch = String::new();
    for l in removed {
        patch.push_str(&format!("-{l}\n"));
    }
    for l in added {
        patch.push_str(&format!("+{l}\n"));
    }
    json!({
        "path": path,
        "added": added.len(),
        "removed": removed.len(),
        "isNew": old.is_empty(),
        "startLine": prefix + 1,
        "patch": acp::truncate(&patch, 6000),
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
