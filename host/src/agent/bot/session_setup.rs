//! Session allocation and resumption, retaining first-prompt instructions across setup failures.

use super::fast_mode::Listing;
use super::session::system_meta;
use super::{Actor, NoticeStyle, launch_commands, select_model, start_agent};
use crate::agent::acp::{self, Incoming};
use crate::chat::context;
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const SESSION_TIMEOUT: Duration = Duration::from_secs(180);

impl Actor {
    /// Makes sure the agent runs and `slot`'s session (main, or a thread's) is live;
    /// sets `turn_session`. A thread without one forks the main session when it can.
    pub(super) async fn ensure_session(&mut self, slot: Option<&str>) -> Result<()> {
        // A cancelled allocation may have started tools without returning their session ID.
        // Resumable agents can be replaced; nonresumable agents retain confirmed conversations.
        if self.conn.as_ref().is_some_and(|conn| conn.allocation_uncertain && conn.sessions.load)
            && let Some(conn) = self.conn.take()
        {
            conn.acp.kill().await;
        }
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
                let started = start_agent(command, &self.cfg.cwd, &env, budget).await;
                match started {
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
        if slot.is_some()
            && self.session_for(slot).is_none()
            && let Some(main) = self.session_id.clone()
            && self.conn.as_ref().is_some_and(|c| c.sessions.fork)
        {
            self.hub.set_runtime(&self.cfg.id, |r| r.activity = "Opening the thread…".into());
            let response = self
                .session_request("session/fork", json!({"sessionId": main, "cwd": self.cfg.cwd, "mcpServers": servers}))
                .await;
            let res = response.ok();
            if let Some(res) = res
                && let Some(sid) =
                    res["sessionId"].as_str().filter(|sid| !sid.is_empty() && **sid != main).map(str::to_owned)
                && let Some(conn) = self.conn.as_mut()
            {
                conn.loaded.insert(sid.clone(), Instant::now());
                conn.allocation_uncertain = false;
                conn.pending_models.insert(sid.clone(), res.clone());
                let parent = self.snapshot(&main).await?;
                context::fork(&self.hub.store, &self.cfg, &parent, &sid)?;
                self.set_session(slot, Some(&sid));
                self.note_options(&sid, &res, Listing::Session);
            } else {
                tracing::warn!(bot = %self.cfg.id, "session/fork failed; the thread starts fresh");
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
            if conn.loaded.contains_key(&sid) && prompt_current {
                self.finish_session_model(&sid).await?;
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
                Ok(res) => {
                    conn.loaded.insert(sid.clone(), Instant::now());
                    conn.allocation_uncertain = false;
                    if res.get("configOptions").is_some() || res.get("models").is_some() {
                        conn.pending_models.insert(sid.clone(), res.clone());
                    }
                    self.note_options(&sid, &res, Listing::Session);
                    if let Some(s) = snapshot {
                        // Its system prompt now names the bot as rendered.
                        context::mark_announced(&self.hub.store, &self.cfg.id, &s, s.system_identity.clone())?;
                        self.applied_system.insert(sid.clone(), s.system);
                    }
                    self.finish_session_model(&sid).await?;
                    self.turn_session = Some(sid);
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!(error = format!("{e:#}"), "session/load failed; starting fresh");
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
        conn.loaded.insert(sid.clone(), Instant::now());
        conn.allocation_uncertain = false;
        conn.pending_models.insert(sid.clone(), res);
        self.set_session(slot, Some(&sid));
        context::adopt_new(&self.hub.store, &self.cfg, &sid, system.clone())?;
        if claude {
            self.applied_system.insert(sid.clone(), system);
        }
        self.finish_session_model(&sid).await?;
        self.turn_session = Some(sid);
        Ok(())
    }

    /// Only allocations can leave resources with an unknown session identity when cancelled.
    pub(super) async fn session_request(&mut self, method: &str, params: Value) -> Result<Value> {
        let conn = self.conn.as_mut().ok_or_else(|| anyhow!("agent not running"))?;
        if conn.allocation_uncertain {
            bail!(
                "The agent did not confirm its previous allocation. Existing conversations are preserved; reset the affected session before opening another."
            );
        }
        conn.allocation_uncertain = true;
        let result = self.request_with_timeout(method, params).await;
        if result.is_err()
            && let Some(conn) = self.conn.as_mut()
        {
            conn.allocation_uncertain = false;
        }
        result
    }

    /// Mode changes and idle closes do not allocate sessions.
    pub(super) async fn request_with_timeout(&mut self, method: &str, params: Value) -> Result<Value> {
        let acp = self.conn.as_ref().ok_or_else(|| anyhow!("agent not running"))?.acp.clone();
        let response = tokio::time::timeout(SESSION_TIMEOUT, acp.request(method, params)).await;
        if let Ok(result) = response {
            return result;
        }
        if let Some(conn) = self.conn.take() {
            conn.acp.kill().await;
        }
        let message = format!("the agent didn't answer {method} within {}s", SESSION_TIMEOUT.as_secs());
        Err(acp::Failure::error(acp::FailureKind::Timeout, message))
    }

    async fn finish_session_model(&mut self, sid: &str) -> Result<()> {
        let Some(conn) = self.conn.as_ref() else { bail!("agent not running") };
        let Some(response) = conn.pending_models.get(sid) else { return Ok(()) };
        let options = select_model(&conn.acp, response, sid, &self.cfg).await?;
        self.note_options(sid, &options, Listing::Session);
        if let Some(conn) = self.conn.as_mut() {
            conn.pending_models.remove(sid);
        }
        Ok(())
    }
}
