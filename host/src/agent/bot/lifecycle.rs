//! Live resources are bounded independently of saved conversation identities.

use super::{Actor, Cmd};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;

const IDLE_GRACE: Duration = Duration::from_secs(120);
const LIVE_SESSION_LIMIT: usize = 2;
const CLOSE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Default)]
pub(super) struct Lifecycle {
    idle_since: Option<Instant>,
    pub(super) generation: u64,
}

impl Lifecycle {
    pub(super) fn clear_idle(&mut self) {
        self.idle_since = None;
    }
}

pub(super) enum LiveSession {
    /// A fork or partially prepared session may still own adapter resources.
    Preparing,
    /// Allocation succeeded; model selection can retry without replacing the session.
    PendingModel {
        system: Option<String>,
        response: Value,
    },
    Ready {
        system: Option<String>,
    },
}

#[derive(Debug)]
pub(super) enum Retirement {
    Idle,
    Capacity,
    Refresh,
    Reset,
    Reconfigure,
    Timeout,
    Shutdown,
    Exited,
}

impl Actor {
    pub(super) async fn retire_agent(&mut self, reason: Retirement) {
        self.lifecycle.idle_since = None;
        let Some(conn) = self.conn.take() else { return };
        tracing::info!(bot = %self.cfg.id, generation = self.lifecycle.generation, sessions = conn.loaded.len(), ?reason, "retiring agent resources");
        conn.acp.kill().await;
        // These IDs are durable in the store; only the in-memory lookup cache goes.
        self.thread_sessions.clear();
    }

    pub(super) async fn retire_if_idle(&mut self, commands: &mpsc::UnboundedReceiver<Cmd>) {
        let eligible = self.conn.as_ref().is_some_and(|c| c.sessions.load)
            && self.turn.is_none()
            && self.queue.is_empty()
            && commands.is_empty()
            && self.perms.is_empty()
            && self.active_request.is_none()
            && self.active_group.is_none()
            && self.active_routine.is_none()
            && self.routine_completion.is_none();
        if !eligible {
            self.lifecycle.idle_since = None;
            return;
        }
        let since = self.lifecycle.idle_since.get_or_insert_with(Instant::now);
        if since.elapsed() >= IDLE_GRACE {
            self.retire_agent(Retirement::Idle).await;
        }
    }

    /// Reserve one allocation, retaining a live parent when preparing a fork.
    pub(super) async fn make_session_room(&mut self, protected: Option<&str>) -> Result<()> {
        let Some(conn) = &self.conn else { return Ok(()) };
        if conn.allocation_uncertain {
            if conn.sessions.load {
                self.retire_agent(Retirement::Refresh).await;
                return Ok(());
            }
            bail!(
                "The agent did not confirm its last session allocation. Existing conversations have been preserved; reset the affected session before opening another."
            );
        }
        if conn.loaded.len() < LIVE_SESSION_LIMIT {
            return Ok(());
        }
        if !conn.sessions.load {
            bail!(
                "This agent can't reload saved conversations and has reached its live session limit. Existing conversations have been preserved."
            );
        }
        let victim = conn.loaded.keys().find(|sid| Some(sid.as_str()) != protected).cloned();
        if let Some(sid) = victim {
            self.release_session(&sid, Retirement::Capacity).await?;
        } else {
            self.retire_agent(Retirement::Capacity).await;
        }
        Ok(())
    }

    pub(super) async fn release_main_session(&mut self) -> Result<()> {
        if let Some(sid) = self.session_id.clone() {
            return self.release_session(&sid, Retirement::Reset).await;
        }
        if let Some(conn) = &self.conn
            && conn.allocation_uncertain
        {
            if !conn.sessions.load && !conn.loaded.is_empty() {
                bail!(
                    "This agent has an unconfirmed allocation and other nonresumable conversations. Those conversations have been preserved."
                );
            }
            self.retire_agent(Retirement::Reset).await;
        }
        Ok(())
    }

    pub(super) async fn release_session(&mut self, sid: &str, reason: Retirement) -> Result<()> {
        let Some(conn) = &self.conn else { return Ok(()) };
        if !conn.loaded.contains_key(sid) {
            return Ok(());
        }
        let can_reload = conn.sessions.load;
        let only_session = conn.loaded.len() == 1;
        if !can_reload && !matches!(reason, Retirement::Reset) {
            bail!(
                "This agent can't reload its conversation after releasing resources. Existing conversations have been preserved."
            );
        }
        if conn.allocation_uncertain && only_session && matches!(reason, Retirement::Reset) {
            self.retire_agent(reason).await;
            return Ok(());
        }
        if conn.sessions.close {
            let result =
                tokio::time::timeout(CLOSE_TIMEOUT, conn.acp.request("session/close", json!({"sessionId":sid}))).await;
            match result {
                Ok(Ok(_)) => {
                    if let Some(conn) = &mut self.conn {
                        conn.loaded.remove(sid);
                    }
                    tracing::info!(bot = %self.cfg.id, generation = self.lifecycle.generation, ?reason, "released agent session");
                    return Ok(());
                }
                error => {
                    tracing::warn!(bot = %self.cfg.id, ?error, "session close failed; checking safe process retirement");
                }
            }
        }
        if can_reload || (matches!(reason, Retirement::Reset) && only_session) {
            self.retire_agent(reason).await;
            return Ok(());
        }
        bail!(
            "This agent can't release this session safely while other nonresumable conversations remain open. Existing conversations have been preserved."
        );
    }
}
