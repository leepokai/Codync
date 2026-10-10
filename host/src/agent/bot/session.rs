//! The agent process and its ACP sessions: launch, handshake, new/load/fork, thread sessions.

use super::{Actor, Conn, NoticeStyle, SessionCaps};
use crate::agent::acp::{self, Acp, Incoming};
use crate::chat::context;
use crate::store::{BotConfig, Entry, EntryKind, Lane, lane_key};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::time::Duration;

impl Actor {
    /// Which session a lane's turns use: a thread of the bot's own chat has its own
    /// (by root); its chat and every group lane use the main one.
    pub(super) fn slot(&self, lane: &Lane) -> Option<String> {
        (lane.chat == self.cfg.id).then(|| lane.thread.clone()).flatten()
    }

    pub(super) fn session_for(&mut self, slot: Option<&str>) -> Option<String> {
        let Some(root) = slot else { return self.session_id.clone() };
        if let Some(sid) = self.thread_sessions.get(root) {
            return Some(sid.clone());
        }
        let sid = self.hub.store.kv_get(&lane_key("session", &self.cfg.id, &Lane::in_thread(&self.cfg.id, root)));
        let sid = sid.filter(|s| !s.is_empty())?;
        self.thread_sessions.insert(root.to_owned(), sid.clone());
        Some(sid)
    }

    pub(super) fn set_session(&mut self, slot: Option<&str>, sid: Option<&str>) {
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
    pub(super) fn thread_roots(&self) -> Vec<String> {
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
    pub(super) fn thread_intro(&self, root: &str, forked: bool) -> String {
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
    pub(super) fn mcp_servers(&self) -> Result<Value> {
        let Ok(exe) = std::env::current_exe() else {
            tracing::warn!("can't locate codync-host for MCP servers");
            return Ok(json!([]));
        };
        let port = self.hub.port;
        let mut servers: Vec<Value> =
            (if self.cfg.connectors.is_empty() { Vec::new() } else { crate::market::connectors(&self.hub.store)? })
                .iter()
                .filter(|c| self.cfg.connectors.contains(&c.id))
                .filter_map(|c| c.acp(&exe, port))
                .collect();
        let composio = crate::market::composio::enabled_for(&self.hub.store, &self.cfg.connectors)?;
        let builtin = [
            Some("connectors"),
            Some("chat"),
            Some("team"),
            Some("memory"),
            Some("routines"),
            self.cfg.computer.then_some("computer"),
            composio.then_some("composio"),
        ];
        for name in builtin.into_iter().flatten() {
            let env = if name == "memory" {
                json!([{"name":"CODYNC_MEMORY_LANE", "value":self.slot(&self.lane).unwrap_or_default()}])
            } else {
                json!([])
            };
            servers.push(json!({
                "name": name, "command": exe,
                "args": ["mcp", name, "--bot", self.cfg.id, "--port", port.to_string()],
                "env": env,
            }));
        }
        Ok(Value::Array(servers))
    }

    /// Makes sure the agent runs and `slot`'s session (main, or a thread's) is live;
    /// sets `turn_session`. A thread without one forks the main session when it can.
    pub(super) async fn ensure_session(&mut self, slot: Option<&str>) -> Result<()> {
        // Memory is best-effort: a bot without it still answers.
        if let Err(error) = crate::chat::memory::prepare(&self.cfg.id).await {
            self.memory_unavailable(&error);
        }
        if !self.cfg.connectors.is_empty()
            || self.hub.store.kv_read(&format!("agent-env:{}", self.cfg.backend))?.is_some()
        {
            crate::market::vault::unlock(self.hub.clone()).await?;
        }
        // An agent that can't resume keeps its session; its next new session gets the new tools.
        if std::mem::take(&mut self.tools_changed)
            && self.conn.as_ref().is_some_and(|c| c.sessions.load)
            && let Some(c) = self.conn.take()
        {
            c.acp.kill().await;
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
            let env = crate::agent::auth::env(&self.hub.store, &self.cfg.backend)?;
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
        let servers = self.mcp_servers()?;
        let claude = self.conn.as_ref().is_some_and(|c| c.claude);
        let mut forked = false;
        if let Some(root) = slot
            && self.session_for(slot).is_none()
            && let Some(main) = self.session_id.clone()
            && self.conn.as_ref().is_some_and(|c| c.sessions.fork)
        {
            self.hub.set_runtime(&self.cfg.id, |r| r.activity = "Opening the thread…".into());
            let res = self
                .session_request("session/fork", json!({"sessionId": main, "cwd": self.cfg.cwd, "mcpServers": servers}))
                .await;
            if let Some(sid) = res.ok().and_then(|r| r["sessionId"].as_str().map(str::to_owned))
                && let Some(conn) = self.conn.as_mut()
            {
                conn.loaded.insert(sid.clone());
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
        let Some(conn) = self.conn.as_ref() else { bail!("agent not running") };
        if let Some(sid) = current {
            let prompt_current =
                snapshot.as_ref().is_none_or(|s| self.applied_system.get(&sid).is_some_and(|t| *t == s.system));
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
            let res = self.session_request("session/load", params).await;
            let Some(conn) = self.conn.as_mut() else { return res.map(drop) };
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
                        // Its system prompt now names the bot as rendered.
                        context::mark_announced(&self.hub.store, &self.cfg.id, &s, s.system_identity.clone())?;
                        self.applied_system.insert(sid.clone(), s.system);
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
        let mut params = json!({"cwd": self.cfg.cwd, "mcpServers": servers});
        if claude {
            params["_meta"] = system_meta(&system);
        }
        let res = self.session_request("session/new", params).await?;
        let Some(conn) = self.conn.as_mut() else { bail!("agent not running") };
        let sid = res["sessionId"].as_str().ok_or_else(|| anyhow!("agent returned no sessionId"))?.to_owned();
        conn.loaded.insert(sid.clone());
        select_model(&conn.acp, &res, &sid, &self.cfg).await?;
        self.set_session(slot, Some(&sid));
        self.session_fresh = true;
        self.turn_session = Some(sid.clone());
        if let Some(root) = slot {
            self.thread_intro = Some(self.thread_intro(root, false));
        }
        context::adopt(&self.hub.store, &self.cfg, &sid, system.clone())?;
        if claude {
            self.applied_system.insert(sid, system);
        }
        Ok(())
    }

    /// A session request the agent doesn't answer in time stops the agent: a hung one
    /// would otherwise hold the bot, and its Stop, forever.
    async fn session_request(&mut self, method: &str, params: Value) -> Result<Value> {
        let acp = self.conn.as_ref().ok_or_else(|| anyhow!("agent not running"))?.acp.clone();
        if let Ok(result) = tokio::time::timeout(SESSION_TIMEOUT, acp.request(method, params)).await {
            return result;
        }
        if let Some(c) = self.conn.take() {
            c.acp.kill().await;
        }
        bail!("the agent didn't answer {method} within {}s", SESSION_TIMEOUT.as_secs())
    }

    /// Memory failed to start: said once per actor, then the bot works without it.
    pub(super) fn memory_unavailable(&mut self, error: &anyhow::Error) {
        tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "memory unavailable");
        if !std::mem::replace(&mut self.memory_warned, true) {
            self.notice(
                &format!("Memory is unavailable; the bot works without it for now. {error:#}"),
                NoticeStyle::Info,
            );
        }
    }

    /// Drops the main ACP session so the next turn starts a fresh one.
    pub(super) fn forget_session(&mut self) {
        self.set_session(None, None);
    }
}

/// How long `session/new`, `session/load` and `session/fork` may take.
const SESSION_TIMEOUT: Duration = Duration::from_secs(180);

/// Claude's `_meta`: the bot's instructions appended to Claude Code's own system prompt.
fn system_meta(system: &str) -> Value {
    json!({"systemPrompt": {"append": system}})
}

/// Shell commands that may start the bot's agent over ACP, best first.
/// `progress` receives short status lines ("Downloading Cursor…").
pub(crate) async fn launch_commands(cfg: &BotConfig, progress: impl Fn(&str)) -> Result<Vec<String>> {
    Ok(match cfg.command.as_deref().map(str::trim) {
        Some(c) if !c.is_empty() => vec![c.to_owned()],
        _ => crate::agent::backends::launch_candidates(&cfg.backend, progress)
            .await?
            .iter()
            .map(crate::agent::registry::Cmd::acp)
            .collect(),
    })
}

/// Switches a new session (`session/new` answered `res`) to the bot's chosen model.
pub(crate) async fn select_model(acp: &Acp, res: &Value, sid: &str, cfg: &BotConfig) -> Result<()> {
    let Some(model) = cfg.model.as_deref().filter(|m| !m.is_empty()) else { return Ok(()) };
    let r = match crate::agent::auth::model_config(res).and_then(|c| c["id"].as_str()) {
        Some(config_id) => {
            acp.request("session/set_config_option", json!({"sessionId": sid, "configId": config_id, "value": model}))
                .await
        }
        None => acp.request("session/set_model", json!({"sessionId": sid, "modelId": model})).await,
    };
    if let Err(e) = r {
        tracing::warn!(model, error = format!("{e:#}"), "couldn't set model");
        bail!("Couldn't select model {model}: {e:#}");
    }
    Ok(())
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
    let claude = init["agentCapabilities"]["_meta"]["claudeCode"].is_object();
    Ok(Conn { acp, rx, sessions, claude, loaded: HashSet::new() })
}
