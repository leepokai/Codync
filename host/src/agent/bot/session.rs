//! The bot's ACP sessions: starting its agent, new/load, routine runs' sessions.

use super::fast_mode::Listing;
use super::launch::{claude_meta, launch_commands, select_model, start_agent, system_prompt};
use super::{Actor, Adapter, Conn, Instructions, codex_names, pi_tools};
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

    /// What the turn's session calls Codync's MCP servers that it knows by another name.
    pub(super) fn renamed(&self) -> &codex_names::Renamed {
        static NONE: codex_names::Renamed = codex_names::Renamed::new();
        let sid = self.turn_session.as_ref();
        self.conn.as_ref().zip(sid).and_then(|(c, sid)| c.renamed.get(sid)).unwrap_or(&NONE)
    }

    /// A folder saved for the bot's agent (`setAgentEnv`) as `key`, such as where its own config is (`CODEX_HOME`).
    fn saved_env(&self, key: &str) -> Option<std::path::PathBuf> {
        let saved = crate::agent::auth::env(&self.hub.store, &self.cfg.backend).unwrap_or_else(|e| {
            tracing::warn!(error = format!("{e:#}"), "couldn't read the agent's saved environment");
            Vec::new()
        });
        saved.into_iter().find(|(k, _)| k == key).map(|(_, v)| v.into())
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
        &mut self,
        candidate: Candidate,
        env: &[(String, String)],
        budget: Duration,
        progress: impl Fn(&str),
    ) -> Result<Conn> {
        let mut command = [candidate.resolve(progress).await?.acp()];
        self.hub.set_runtime(&self.id(), |r| r.activity = "Starting agent…".into());
        let mut env = env.to_vec();
        own_memory::apply(&self.cfg, &mut env, &mut command).await;
        let cwd = self.cfg.cwd.clone();
        let started = start_agent(&command[0], &cwd, &env, budget, |acp| self.starting = Some(acp)).await;
        if started.is_ok() {
            self.starting = None;
        }
        started.inspect_err(|e| {
            tracing::info!(command = command[0].as_str(), error = format!("{e:#}"), "agent failed to start");
        })
    }

    /// `ensure_session` for the turn: a routine run's setup must end by its time limit, and setup cut short
    /// mustn't leave the agent opening a session the host never learns of.
    pub(super) async fn ensure_turn_session(&mut self, slot: Option<&str>) -> Result<()> {
        let Some(deadline) = self.routine_deadline else { return self.ensure_session(slot).await };
        let setup = tokio::time::timeout_at(deadline.into(), self.ensure_session(slot)).await;
        if setup.is_err() {
            if self.starting.is_some() {
                self.stop_agent().await;
            }
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
        if self.tools_changed && self.conn.as_ref().is_some_and(|c| c.sessions.load) {
            self.recycle_agent("tools_changed").await?;
            self.tools_changed = false;
        }
        self.prepare_allocation(slot).await?;
        if let Some(sid) = self.session_for(slot)
            && self.conn.as_ref().is_some_and(|c| c.pending_models.contains_key(&sid))
        {
            self.finish_model_setup(&sid).await?;
        }
        let mut servers = self.mcp_servers()?;
        // pi gets its servers when it starts (`pi_tools`), so they take pi's names before that.
        let pi = self.cfg.backend == "pi";
        let mut renamed = if pi {
            pi_tools::avoid_clashes(&mut servers, &self.cfg.cwd, self.saved_env("PI_CODING_AGENT_DIR")).await
        } else {
            codex_names::Renamed::new()
        };
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
            if pi {
                env.extend(pi_tools::env(&servers).await?);
            }
            let mut last_err = None;
            let count = candidates.len();
            for (i, candidate) in candidates.into_iter().enumerate() {
                self.drain_retiring().await?;
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
        if self.conn.as_ref().is_some_and(|c| c.adapter == Adapter::Codex) {
            renamed = codex_names::avoid_clashes(&mut servers, &self.cfg.cwd, self.saved_env("CODEX_HOME")).await;
        }
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
                    conn.loaded.insert(sid.clone(), Instant::now());
                    conn.renamed.insert(sid.clone(), renamed);
                    conn.pending_models.insert(sid.clone(), res);
                    if let Some(s) = snapshot {
                        // Its system prompt now names the bot and holds memory as rendered.
                        context::mark_announced(&self.hub.store, &self.cfg.id, &s, s.system_identity.clone())?;
                        memory::opened(&self.hub.store, &self.cfg.id, &sid, &s.memory)?;
                        self.applied_system.insert(sid.clone(), s.system);
                    }
                    self.finish_model_setup(&sid).await?;
                    self.turn_session = Some(sid);
                    return Ok(());
                }
                Err(e) => {
                    self.retire_agent("load_failed").await?;
                    return Err(e).context("couldn't resume the saved conversation; retry to resume it");
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
        conn.renamed.insert(sid.clone(), renamed);
        conn.pending_models.insert(sid.clone(), res);
        self.set_session(slot, Some(&sid));
        self.session_fresh = true;
        self.turn_session = Some(sid.clone());
        context::adopt(&self.hub.store, &self.cfg, &sid, system.clone(), revision)?;
        if instructions == Instructions::ClaudeMeta {
            self.applied_system.insert(sid.clone(), system);
        }
        self.finish_model_setup(&sid).await?;
        Ok(())
    }

    /// Selection may fail after the adapter already allocated tools. Retry that same session.
    async fn finish_model_setup(&mut self, sid: &str) -> Result<()> {
        let conn = self.conn.as_ref().ok_or_else(|| anyhow!("agent not running"))?;
        let Some(res) = conn.pending_models.get(sid).cloned() else { return Ok(()) };
        let options = select_model(&conn.acp, &res, sid, &self.cfg).await?;
        self.note_options(sid, &options, Listing::Session);
        if let Some(conn) = self.conn.as_mut() {
            conn.pending_models.remove(sid);
        }
        Ok(())
    }

    /// A session request the agent doesn't answer in time stops the agent: a hung one
    /// would otherwise hold the bot, and its Stop, forever.
    pub(super) async fn session_request(&mut self, method: &str, params: Value) -> Result<Value> {
        let conn = self.conn.as_mut().ok_or_else(|| anyhow!("agent not running"))?;
        conn.opening = matches!(method, "session/new" | "session/load");
        if conn.opening {
            conn.allocations += 1;
            conn.untracked_allocations += 1;
        }
        let timeout = if method == "session/close" { Duration::from_secs(10) } else { SESSION_TIMEOUT };
        let loading_saved = method == "session/load" && params["sessionId"].is_string();
        let answered = tokio::time::timeout(timeout, conn.acp.request(method, params)).await;
        conn.opening = false;
        if let Ok(result) = answered {
            if matches!(method, "session/new" | "session/load")
                && result
                    .as_ref()
                    .is_ok_and(|r| loading_saved || r["sessionId"].as_str().is_some_and(|s| !s.is_empty()))
            {
                conn.untracked_allocations -= 1;
            }
            return result;
        }
        self.retire_agent("session_timeout").await?;
        let message = format!("the agent didn't answer {method} within {}s", timeout.as_secs());
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
        if let Err(error) = self.retire_agent("setup_cancelled").await {
            tracing::warn!(%error, "couldn't retire interrupted session setup");
        }
    }

    /// Drops the main ACP session so the next turn starts a fresh one.
    pub(super) fn forget_session(&mut self) {
        self.set_session(None, None);
    }
}

/// How long `session/new` and `session/load` may take.
const SESSION_TIMEOUT: Duration = Duration::from_mins(3);
