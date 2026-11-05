//! Bound live tool allocations independently of whether a busy bot ever becomes idle.

use super::background_work::background_work;
use super::{Actor, Instructions};
use anyhow::{Result, ensure};

const MAX_SESSIONS: usize = 2;
/// Some adapters keep tools outside the sessions they say they closed (e.g. title generation).
const MAX_ALLOCATIONS: usize = 8;

impl Actor {
    /// Called only in serialized session setup, before issuing a prompt. Existing work in the queue
    /// does not block retirement; running prompts, permissions and detached background work do.
    pub(super) async fn prepare_allocation(&mut self, slot: Option<&str>) -> Result<()> {
        self.drain_retiring().await?;
        let current = self.session_for(slot);
        let Some(conn) = &self.conn else { return Ok(()) };
        let refresh = if let Some(sid) = current.as_deref().filter(|s| conn.loaded.contains_key(*s)) {
            if conn.pending_models.contains_key(sid) || conn.instructions() != Instructions::ClaudeMeta {
                return Ok(());
            }
            let snapshot = self.snapshot(sid).await?;
            if self.applied_system.get(sid).is_some_and(|system| *system == snapshot.system) {
                return Ok(());
            }
            true
        } else {
            false
        };
        let conn = self.conn.as_ref().expect("snapshot keeps the connection");
        let full = conn.loaded.len() + conn.untracked_allocations >= MAX_SESSIONS;
        let exhausted = conn.allocations >= MAX_ALLOCATIONS && conn.sessions.reopen.is_some();
        if !refresh && !full && !exhausted {
            return Ok(());
        }
        if !exhausted && conn.sessions.close {
            // Never close a main session we cannot load again. A refresh always names a reloadable session.
            let victim = if refresh {
                current.clone()
            } else if conn.sessions.reopen.is_some() {
                conn.loaded.iter().min_by_key(|(_, at)| *at).map(|(sid, _)| sid.clone())
            } else {
                None
            };
            if let Some(sid) = victim {
                let closed = self.close_session(&sid).await;
                if closed.is_ok() {
                    return Ok(());
                }
            }
        }
        self.recycle_agent(if refresh { "instructions_changed" } else { "session_budget" }).await
    }

    pub(super) async fn recycle_agent(&mut self, reason: &'static str) -> Result<()> {
        let Some(conn) = &self.conn else { return Ok(()) };
        ensure!(
            conn.sessions.reopen.is_some(),
            "The agent cannot resume conversations; choose a resumable agent before opening more sessions"
        );
        ensure!(
            !background_work(&conn.acp).await,
            "The agent has background work; stop it before opening more sessions"
        );
        self.retire_agent(reason).await
    }

    /// A failed cleanup keeps ownership, so a retry cannot start a replacement over a surviving tree.
    pub(super) async fn retire_agent(&mut self, reason: &'static str) -> Result<()> {
        let Some(conn) = &self.conn else { return Ok(()) };
        let pid = conn.acp.pid();
        let sessions = conn.loaded.len();
        tracing::info!(bot = %self.cfg.id, ?pid, sessions, reason, "retiring agent allocation");
        let started = std::time::Instant::now();
        self.stop_agent().await;
        let result = self.drain_retiring().await;
        let outcome = if result.is_ok() { "done" } else { "failed" };
        crate::analytics::capture(
            &self.hub,
            crate::analytics::Event::AgentRetired,
            serde_json::Map::from_iter([
                ("reason".into(), reason.into()),
                ("outcome".into(), outcome.into()),
                ("sessions".into(), sessions.into()),
                ("duration_ms".into(), u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX).into()),
            ]),
        );
        result?;
        self.applied_system.clear();
        Ok(())
    }
    /// Stop requests keep their process owner until a later admission has verified cleanup.
    pub(super) async fn stop_agent(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.retiring.push(conn.acp);
        }
        if let Some(acp) = self.starting.take() {
            self.retiring.push(acp);
        }
        for acp in &self.retiring {
            acp.kill().await;
        }
    }

    pub(super) async fn drain_retiring(&mut self) -> Result<()> {
        if self.starting.is_some() {
            self.stop_agent().await;
        }
        while let Some(acp) = self.retiring.last() {
            acp.stop_and_wait().await?;
            self.retiring.pop();
        }
        Ok(())
    }
}
