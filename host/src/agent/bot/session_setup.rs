//! Allocate and reload sessions without retaining superseded tool trees.

use super::lifecycle::{LiveSession, Retirement};
use super::session::system_meta;
use super::{Actor, launch_commands, select_model, start_agent};
use crate::agent::acp::Incoming;
use crate::chat::context;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::time::Duration;

const SESSION_TIMEOUT: Duration = Duration::from_secs(180);

impl Actor {
    pub(super) async fn ensure_session(&mut self, slot: Option<&str>) -> Result<()> {
        self.lifecycle.clear_idle();
        crate::chat::memory::prepare(&self.cfg.id).await?;
        if !self.cfg.connectors.is_empty()
            || self.hub.store.kv_read(&format!("agent-env:{}", self.cfg.backend))?.is_some()
        {
            crate::market::vault::unlock(self.hub.clone()).await?;
        }
        if self.tools_changed {
            if self.conn.as_ref().is_some_and(|c| !c.sessions.load && !c.loaded.is_empty()) {
                bail!(
                    "This agent can't reload conversations after refreshing tools. Existing conversations have been preserved; start a new session before refreshing tools."
                );
            }
            self.retire_agent(Retirement::Refresh).await;
            self.tools_changed = false;
        }
        self.ensure_agent().await?;
        if let Some(sid) = self.session_for(slot) {
            return self.load_saved_session(&sid, slot).await;
        }
        let fork = slot.is_some()
            && self.session_id.is_some()
            && self.conn.as_ref().is_some_and(|c| c.sessions.fork && c.sessions.load);
        if fork && self.fork_thread(slot).await? {
            return Ok(());
        }
        self.new_live_session(slot).await
    }

    async fn ensure_agent(&mut self) -> Result<()> {
        if self.conn.is_some() {
            return Ok(());
        }
        let (hub, id) = (self.hub.clone(), self.cfg.id.clone());
        let candidates = launch_commands(&self.cfg, move |msg: &str| {
            let msg = msg.to_owned();
            hub.set_runtime(&id, |r| r.activity = msg);
        })
        .await?;
        self.hub.set_runtime(&self.id(), |r| r.activity = "Starting agent…".into());
        let env = crate::agent::auth::env(&self.hub.store, &self.cfg.backend)?;
        let mut last_err = None;
        for (i, command) in candidates.iter().enumerate() {
            let budget = Duration::from_secs(if i + 1 < candidates.len() { 20 } else { 180 });
            let result = start_agent(command, &self.cfg.cwd, &env, budget).await;
            match result {
                Ok(conn) => {
                    self.lifecycle.generation += 1;
                    self.conn = Some(conn);
                    return Ok(());
                }
                Err(error) => {
                    tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "agent failed to start");
                    last_err = Some(error);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("agent failed to start")))
    }

    async fn session_request(&mut self, method: &str, params: Value) -> Result<Value> {
        let conn = self.conn.as_mut().ok_or_else(|| anyhow!("agent not running"))?;
        // An outer routine deadline can drop this future during any allocation.
        conn.allocation_uncertain = true;
        let response = tokio::time::timeout(SESSION_TIMEOUT, conn.acp.request(method, params)).await;
        // Session loading replays history before its response. Never publish it again.
        while let Ok(inc) = conn.rx.try_recv() {
            match inc {
                Incoming::Request { id, .. } => conn.acp.respond_error(id, -32601, "not supported").await?,
                Incoming::Closed { .. } => bail!("agent exited during session setup"),
                Incoming::Notification { .. } => {}
            }
        }
        response.context("agent session setup timed out")?
    }

    async fn load_saved_session(&mut self, sid: &str, slot: Option<&str>) -> Result<()> {
        let claude = self.conn.as_ref().is_some_and(|c| c.claude);
        let system = if claude {
            let snapshot = self.snapshot(sid).await?;
            Some(snapshot.system)
        } else {
            None
        };
        let existing = self.conn.as_ref().and_then(|c| c.loaded.get(sid));
        if matches!(existing, Some(LiveSession::Ready { system: applied }) if *applied == system) {
            self.turn_session = Some(sid.to_owned());
            return Ok(());
        }
        if matches!(existing, Some(LiveSession::PendingModel { system: applied, .. }) if *applied == system) {
            self.finish_session_model(sid).await?;
            self.turn_session = Some(sid.to_owned());
            return Ok(());
        }
        if existing.is_some() {
            self.release_session(sid, Retirement::Refresh).await?;
        }
        self.make_session_room(None).await?;
        self.ensure_agent().await?;
        if !self.conn.as_ref().is_some_and(|c| c.sessions.load) {
            bail!(
                "This agent can't reload the saved conversation. Start a new session to continue; the saved conversation has been preserved."
            );
        }
        self.hub.set_runtime(&self.cfg.id, |r| r.activity = "Resuming session…".into());
        let servers = self.mcp_servers(slot)?;
        let mut params = json!({"sessionId":sid,"cwd":self.cfg.cwd,"mcpServers":servers});
        if let Some(system) = &system {
            params["_meta"] = system_meta(system);
        }
        let response = self.session_request("session/load", params).await;
        let res = match response {
            Ok(res) => res,
            Err(error) => {
                self.retire_agent(Retirement::Refresh).await;
                return Err(error).context("Couldn't reload the saved conversation; retry to resume it. Its session identity has been preserved");
            }
        };
        self.record_session(sid, LiveSession::PendingModel { system, response: res });
        self.finish_session_model(sid).await?;
        self.turn_session = Some(sid.to_owned());
        Ok(())
    }

    async fn fork_thread(&mut self, slot: Option<&str>) -> Result<bool> {
        let main = self.session_id.clone().ok_or_else(|| anyhow!("no parent session"))?;
        self.load_saved_session(&main, None).await?;
        self.make_session_room(Some(&main)).await?;
        self.ensure_agent().await?;
        self.load_saved_session(&main, None).await?;
        self.hub.set_runtime(&self.cfg.id, |r| r.activity = "Opening the thread…".into());
        let servers = self.mcp_servers(slot)?;
        let params = json!({"sessionId":main,"cwd":self.cfg.cwd,"mcpServers":servers});
        let response = self.session_request("session/fork", params).await;
        let sid = match response {
            Ok(res) => res["sessionId"].as_str().filter(|sid| !sid.is_empty() && *sid != main).map(str::to_owned),
            Err(error) => {
                tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "session fork failed");
                None
            }
        };
        let Some(sid) = sid else {
            // A failed allocation can still have started tools inside the adapter.
            self.retire_agent(Retirement::Refresh).await;
            return Ok(false);
        };
        self.record_session(&sid, LiveSession::Preparing);
        let parent = self.snapshot(&main).await?;
        context::fork(&self.hub.store, &self.cfg, &parent, &sid)?;
        self.set_session(slot, Some(&sid));
        self.load_saved_session(&sid, slot).await?;
        Ok(true)
    }

    async fn new_live_session(&mut self, slot: Option<&str>) -> Result<()> {
        self.make_session_room(None).await?;
        self.ensure_agent().await?;
        let (hub, cfg) = (self.hub.clone(), self.cfg.clone());
        let system = tokio::task::spawn_blocking(move || context::render(&hub.store, &cfg)).await?;
        let claude = self.conn.as_ref().is_some_and(|c| c.claude);
        let servers = self.mcp_servers(slot)?;
        let mut params = json!({"cwd":self.cfg.cwd,"mcpServers":servers});
        if claude {
            params["_meta"] = system_meta(&system);
        }
        if let Some(conn) = &mut self.conn {
            conn.allocation_uncertain = true;
        }
        let response = self.session_request("session/new", params).await;
        let res = match response {
            Ok(res) => res,
            Err(error) => {
                if self.conn.as_ref().is_some_and(|c| c.sessions.load) {
                    self.retire_agent(Retirement::Refresh).await;
                }
                return Err(error);
            }
        };
        let Some(sid) = res["sessionId"].as_str().filter(|s| !s.is_empty()).map(str::to_owned) else {
            if self.conn.as_ref().is_some_and(|c| c.sessions.load) {
                self.retire_agent(Retirement::Refresh).await;
            }
            bail!("agent returned no sessionId");
        };
        self.record_session(&sid, LiveSession::PendingModel { system: claude.then(|| system.clone()), response: res });
        context::adopt_new(&self.hub.store, &self.cfg, &sid, system)?;
        self.set_session(slot, Some(&sid));
        self.finish_session_model(&sid).await?;
        self.turn_session = Some(sid);
        Ok(())
    }

    async fn finish_session_model(&mut self, sid: &str) -> Result<()> {
        let conn = self.conn.as_ref().ok_or_else(|| anyhow!("agent not running"))?;
        let Some(LiveSession::PendingModel { system, response }) = conn.loaded.get(sid) else {
            bail!("session is not awaiting model selection");
        };
        let system = system.clone();
        let selected = tokio::time::timeout(SESSION_TIMEOUT, select_model(&conn.acp, response, sid, &self.cfg)).await;
        selected.context("agent model selection timed out")??;
        self.record_session(sid, LiveSession::Ready { system });
        Ok(())
    }

    fn record_session(&mut self, sid: &str, state: LiveSession) {
        if let Some(conn) = &mut self.conn {
            conn.allocation_uncertain = false;
            conn.loaded.insert(sid.to_owned(), state);
            tracing::debug!(bot = %self.cfg.id, generation = self.lifecycle.generation, sessions = conn.loaded.len(), "agent session allocated");
        }
    }
}
