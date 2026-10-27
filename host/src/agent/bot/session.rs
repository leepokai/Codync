//! The bot's ACP sessions: starting its agent, new/load, routine runs' sessions.

use super::fast_mode::Listing;
use super::launch::{claude_meta, launch_commands, select_model, start_agent, system_prompt};
use super::{Actor, Conn, Instructions};
use crate::agent::acp::{self, Incoming};
use crate::agent::own_memory;
use crate::agent::registry::Candidate;
use crate::chat::{context, memory};
use crate::store::run_session_key;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

impl Actor {
    /// Which session the running turn uses: a routine run has one of its own (by its slot); everything else uses
    /// the main one.
    pub(super) fn slot(&self) -> Option<String> {
        self.active_routine.as_deref().map(crate::routines::session_slot)
    }

    pub(super) fn session_for(&mut self, slot: Option<&str>) -> Option<String> {
        let Some(slot) = slot else { return self.session_id.clone() };
        if let Some(sid) = self.run_sessions.get(slot) {
            return Some(sid.clone());
        }
        let sid = self.hub.store.kv_get(&run_session_key(&self.cfg.id, slot));
        let sid = sid.filter(|s| !s.is_empty())?;
        self.run_sessions.insert(slot.to_owned(), sid.clone());
        Some(sid)
    }

    pub(super) fn set_session(&mut self, slot: Option<&str>, sid: Option<&str>) {
        let Some(slot) = slot else {
            self.session_id = sid.map(str::to_owned);
            if let Err(error) = self.hub.store.set_session(&self.cfg.id, sid) {
                tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't store the session");
            }
            self.hub.emit_bot(&self.cfg.id);
            return;
        };
        let key = run_session_key(&self.cfg.id, slot);
        let stored = if let Some(s) = sid {
            self.run_sessions.insert(slot.to_owned(), s.to_owned());
            self.hub.store.kv_set(&key, s)
        } else {
            self.run_sessions.remove(slot);
            self.hub.store.kv_delete(&key)
        };
        if let Err(error) = stored {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't store the routine run's session");
        }
    }

    /// Slots of the routine runs that have a session (stored or loaded).
    pub(super) fn run_slots(&self) -> Vec<String> {
        let prefix = run_session_key(&self.cfg.id, "");
        let mut slots: Vec<String> = self.run_sessions.keys().cloned().collect();
        slots.extend(
            self.hub.store.kv_prefix(&prefix).into_iter().filter_map(|k| k.strip_prefix(&prefix).map(str::to_owned)),
        );
        slots.sort();
        slots.dedup();
        slots
    }

    /// What every session request carries: the folder and MCP servers, and the memory folders for an agent that
    /// takes folders besides its own.
    fn session_params(&self, servers: &Value, session: Option<&str>) -> Value {
        let mut params = json!({"cwd": self.cfg.cwd, "mcpServers": servers});
        if let Some(sid) = session {
            params["sessionId"] = sid.into();
        }
        if self.conn.as_ref().is_some_and(|c| c.sessions.additional_directories) {
            params["additionalDirectories"] = json!(memory::files::folders(&self.cfg));
        }
        params
    }

    /// ACP connectors, team collaboration, and optional team management and computer control.
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
            self.cfg.manage_team.then_some("manage"),
            Some("memory"),
            Some("routines"),
            self.cfg.computer.then_some("computer"),
            composio.then_some("composio"),
        ];
        for name in builtin.into_iter().flatten() {
            servers.push(crate::mcp::acp_server(name, &exe, &[name, "--bot", &self.cfg.id], port));
        }
        Ok(Value::Array(servers))
    }

    /// Starts the agent one way: resolved first (a registry build can download), with the harness's own memory set
    /// for the bot.
    async fn start_candidate(
        &self,
        candidate: Candidate,
        env: &[(String, String)],
        budget: Duration,
        progress: impl Fn(&str),
    ) -> Result<Conn> {
        let mut command = [candidate.resolve(progress).await?.acp()];
        self.hub.set_runtime(&self.id(), |r| r.activity = "Starting agent…".into());
        let mut env = env.to_vec();
        own_memory::apply(&self.cfg, &mut env, &mut command).await;
        start_agent(&command[0], &self.cfg.cwd, &env, budget).await.inspect_err(|e| {
            tracing::info!(command = command[0].as_str(), error = format!("{e:#}"), "agent failed to start");
        })
    }

    /// `ensure_session` for the turn: a routine run's setup must end by its time limit, and setup cut short
    /// mustn't leave the agent opening a session the host never learns of.
    pub(super) async fn ensure_turn_session(&mut self, slot: Option<&str>) -> Result<()> {
        let Some(deadline) = self.routine_deadline else { return self.ensure_session(slot).await };
        let setup = tokio::time::timeout_at(deadline.into(), self.ensure_session(slot)).await;
        if setup.is_err() {
            self.drop_unanswered().await;
        }
        setup.context("routine session setup timed out")?
    }

    /// Makes sure the agent runs and `slot`'s session (main, or a routine run's) is live;
    /// sets `turn_session`.
    async fn ensure_session(&mut self, slot: Option<&str>) -> Result<()> {
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
            let progress = move |msg: &str| {
                let msg = msg.to_owned();
                hub.set_runtime(&id, |r| r.activity = msg);
            };
            let candidates = launch_commands(&self.cfg)?;
            // Keys saved from an "environment variable" sign-in, and what the harness needs to start.
            let mut env = crate::agent::auth::launch_env(&self.hub.store, &self.cfg.backend)?;
            if self.cfg.backend == "codex" && !env.iter().any(|(k, _)| k == "CODEX_CONFIG") {
                // Codex's `/compact` takes no instructions: the keep-list is its config's `compact_prompt`.
                let config = json!({"compact_prompt": codync_mem::compaction::KEEP});
                env.push(("CODEX_CONFIG".into(), config.to_string()));
            }
            let mut last_err = None;
            let count = candidates.len();
            for (i, candidate) in candidates.into_iter().enumerate() {
                let fallback_left = i + 1 < count;
                // A local CLI too old for ACP just hangs; don't make the user wait long before the fallback.
                let budget = Duration::from_secs(if fallback_left { 20 } else { 180 });
                match self.start_candidate(candidate, &env, budget, &progress).await {
                    Ok(conn) => {
                        self.conn = Some(conn);
                        break;
                    }
                    Err(e) => last_err = Some(e),
                }
            }
            if self.conn.is_none() {
                return Err(last_err.unwrap_or_else(|| anyhow!("agent failed to start")));
            }
        }
        let servers = self.mcp_servers()?;
        let instructions = self.conn.as_ref().map_or(Instructions::FirstMessage, Conn::instructions);
        // Claude gets the frozen instructions as its system prompt. When a compaction
        // re-rendered them, loading the session again swaps the prompt in (the adapter
        // rebuilds its query and resumes the same conversation).
        let current = self.session_for(slot);
        let snapshot = match (&current, instructions) {
            (Some(sid), Instructions::ClaudeMeta) => Some(self.snapshot(sid).await?),
            _ => None,
        };
        let Some(conn) = self.conn.as_ref() else { bail!("agent not running") };
        if let Some(sid) = current {
            let prompt_current =
                snapshot.as_ref().is_none_or(|s| self.applied_system.get(&sid).is_some_and(|t| *t == s.system));
            if conn.loaded.contains_key(&sid) && prompt_current {
                self.turn_session = Some(sid);
                return Ok(());
            }
            if !conn.sessions.load {
                tracing::info!(bot = %self.cfg.id, "the agent can't resume sessions; starting a fresh one");
                self.set_session(slot, None);
                return Box::pin(self.ensure_session(slot)).await;
            }
            self.hub.set_runtime(&self.cfg.id, |r| r.activity = "Resuming session…".into());
            let mut params = self.session_params(&servers, Some(&sid));
            if let Some(s) = &snapshot {
                params["_meta"] = claude_meta(&self.cfg, &s.system);
            }
            if instructions == Instructions::SystemPrompt {
                params["systemPrompt"] = system_prompt(&self.snapshot(&sid).await?.system);
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
                Ok(res) => {
                    // A resumed session can come back on the agent's last-used model (Cursor
                    // keeps one for all its sessions), so the bot's own is set again.
                    let res = if res.get("configOptions").is_some() || res.get("models").is_some() {
                        select_model(&conn.acp, &res, &sid, &self.cfg).await?
                    } else {
                        res
                    };
                    conn.loaded.insert(sid.clone(), Instant::now());
                    self.note_options(&sid, &res, Listing::Session);
                    if let Some(s) = snapshot {
                        // Its system prompt now names the bot and holds memory as rendered.
                        context::mark_announced(&self.hub.store, &self.cfg.id, &s, s.system_identity.clone())?;
                        memory::opened(&self.hub.store, &self.cfg.id, &sid, &s.memory)?;
                        self.applied_system.insert(sid.clone(), s.system);
                    }
                    self.turn_session = Some(sid);
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!(bot = %self.cfg.id, error = format!("{e:#}"), "session/load failed; starting fresh");
                }
            }
        }
        let (revision, system) = {
            let (hub, cfg) = (self.hub.clone(), self.cfg.clone());
            tokio::task::spawn_blocking(move || {
                (memory::current_revision(&hub.store, &cfg.id), context::render(&hub.store, &cfg))
            })
            .await?
        };
        let mut params = self.session_params(&servers, None);
        match instructions {
            Instructions::ClaudeMeta => params["_meta"] = claude_meta(&self.cfg, &system),
            Instructions::SystemPrompt => params["systemPrompt"] = system_prompt(&system),
            Instructions::FirstMessage => {}
        }
        let res = self.session_request("session/new", params).await?;
        let Some(conn) = self.conn.as_mut() else { bail!("agent not running") };
        let sid = res["sessionId"].as_str().ok_or_else(|| anyhow!("agent returned no sessionId"))?.to_owned();
        conn.loaded.insert(sid.clone(), Instant::now());
        let options = select_model(&conn.acp, &res, &sid, &self.cfg).await?;
        self.note_options(&sid, &options, Listing::Session);
        if slot.is_none() {
            crate::chat::context_usage::clear(&self.hub.store, &self.cfg, &sid)?;
        }
        self.set_session(slot, Some(&sid));
        self.session_fresh = true;
        self.turn_session = Some(sid.clone());
        context::adopt(&self.hub.store, &self.cfg, &sid, system.clone(), revision)?;
        if instructions == Instructions::ClaudeMeta {
            self.applied_system.insert(sid, system);
        }
        Ok(())
    }

    /// A session request the agent doesn't answer in time stops the agent: a hung one
    /// would otherwise hold the bot, and its Stop, forever.
    pub(super) async fn session_request(&mut self, method: &str, params: Value) -> Result<Value> {
        let conn = self.conn.as_mut().ok_or_else(|| anyhow!("agent not running"))?;
        conn.opening = matches!(method, "session/new" | "session/load");
        let answered = tokio::time::timeout(SESSION_TIMEOUT, conn.acp.request(method, params)).await;
        conn.opening = false;
        if let Ok(result) = answered {
            return result;
        }
        if let Some(c) = self.conn.take() {
            c.acp.kill().await;
        }
        let message = format!("the agent didn't answer {method} within {}s", SESSION_TIMEOUT.as_secs());
        Err(acp::Failure::error(acp::FailureKind::Timeout, message))
    }

    /// A routine's time limit cut a session request short. An agent that can resume stops, so a session it might
    /// still open goes with its process; one that can't keeps running rather than lose the sessions it has.
    async fn drop_unanswered(&mut self) {
        let Some(conn) = self.conn.as_mut() else { return };
        if !std::mem::take(&mut conn.opening) {
            return;
        }
        if !conn.sessions.load {
            // ponytail: that session lasts until the agent next stops; keep the cut-short request's Answer past the
            // time limit and session/close the id it returns, if that ever matters.
            tracing::warn!(bot = %self.cfg.id, "a session request was cut short; the agent can't resume, so it keeps running");
            return;
        }
        if let Some(c) = self.conn.take() {
            c.acp.kill().await;
        }
    }

    /// Drops the main ACP session so the next turn starts a fresh one.
    pub(super) fn forget_session(&mut self) {
        if let Some(sid) = &self.session_id {
            crate::chat::context_usage::record(&self.hub, &self.cfg, sid, 0, 0);
        }
        self.set_session(None, None);
    }
}

/// How long `session/new` and `session/load` may take.
const SESSION_TIMEOUT: Duration = Duration::from_mins(3);
