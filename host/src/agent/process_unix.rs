//! An ACP allocation owns a Unix session, including MCP children with their own process groups.
//! Deliberately detached work starts another session and is protected by the actor's retirement guard.

use crate::LockExt;
use anyhow::{Context, Result, ensure};
use std::time::Duration;
use tokio::process::Child;
use tokio::time::Instant;

use super::process_table::{Groups, session_groups};

pub(super) struct Tree {
    session: Option<libc::pid_t>,
}

impl Tree {
    pub(super) fn new(child: &Child) -> Result<Self> {
        let pid = child.id().context("agent exited before process ownership was established")?;
        Ok(Self { session: Some(libc::pid_t::try_from(pid)?) })
    }

    async fn groups(&self, deadline: Instant) -> Result<Groups> {
        let Some(session) = self.session else { return Ok(Groups::new()) };
        let listing = tokio::task::spawn_blocking(move || session_groups(session));
        let listed = tokio::time::timeout_at(deadline, listing).await??;
        listed.context("listing agent session")
    }

    fn signal(&self, groups: Groups) -> std::io::Result<()> {
        let Some(session) = self.session else { return Ok(()) };
        for (group, pid) in groups {
            // Recheck membership just before signalling: a PID from ps may already have exited.
            // SAFETY: session/group lookup sends no signal; kill targets only the still-owned group.
            if unsafe { libc::getsid(pid) } != session || unsafe { libc::getpgid(pid) } != group {
                continue;
            }
            // SAFETY: membership was just checked; the agent's session is still owned.
            if unsafe { libc::kill(-group, libc::SIGKILL) } == -1 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    pub(super) async fn terminate_within(&mut self, deadline: Instant) -> Result<()> {
        let groups = self.groups(deadline).await?;
        self.signal(groups)?;
        Ok(())
    }

    pub(super) async fn grace(&mut self, deadline: Instant) -> Result<()> {
        while Instant::now() < deadline {
            let listed = self.groups(deadline).await;
            if Instant::now() >= deadline {
                break;
            }
            let groups = listed?;
            if groups.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(())
    }

    pub(super) async fn drained(&mut self, deadline: Instant) -> Result<()> {
        loop {
            let groups = self.groups(deadline).await?;
            if groups.is_empty() {
                break;
            }
            ensure!(Instant::now() < deadline, "agent session did not drain before cleanup deadline");
            self.signal(groups)?;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        self.session = None;
        Ok(())
    }
}

/// Drop cleanup is independent of Tokio's blocking queue, which may discard queued work at shutdown.
static FALLBACKS: std::sync::Mutex<Vec<std::thread::JoinHandle<()>>> = std::sync::Mutex::new(Vec::new());

struct Fallback {
    tree: Tree,
    // Keep the leader unreaped until Drop has signalled every group in its session.
    _child: Child,
}

impl Drop for Fallback {
    fn drop(&mut self) {
        let Some(session) = self.tree.session else { return };
        let cleanup = session_groups(session).and_then(|groups| self.tree.signal(groups));
        if let Err(error) = cleanup {
            tracing::warn!(%error, "couldn't release agent session");
        }
    }
}

impl Tree {
    pub(super) fn release_on_drop(&mut self, child: Child) {
        let Some(session) = self.session.take() else { return };
        let fallback = Fallback { tree: Self { session: Some(session) }, _child: child };
        if tokio::runtime::Handle::try_current().is_err() {
            drop(fallback);
            return;
        }
        let mut workers = FALLBACKS.locked();
        workers.retain(|worker| !worker.is_finished());
        // If the OS cannot start a worker, dropping the closure still runs Fallback's cleanup.
        match std::thread::Builder::new().name("agent-cleanup".into()).spawn(move || drop(fallback)) {
            Ok(worker) => workers.push(worker),
            Err(error) => tracing::warn!(%error, "couldn't start agent cleanup worker; cleaned up synchronously"),
        }
    }
}

/// Deliberate host shutdown must join independent fallback workers before `process::exit`.
pub(super) async fn stopped() {
    loop {
        let workers = std::mem::take(&mut *FALLBACKS.locked());
        if workers.is_empty() {
            return;
        }
        let joined = tokio::task::spawn_blocking(move || {
            for worker in workers {
                if let Err(error) = worker.join() {
                    tracing::warn!(?error, "agent cleanup worker panicked");
                }
            }
        })
        .await;
        if let Err(error) = joined {
            tracing::warn!(%error, "couldn't join agent cleanup workers");
        }
    }
}
