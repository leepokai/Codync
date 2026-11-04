//! An ACP allocation owns a Unix session, including MCP children with their own process groups.
//! Deliberately detached work starts another session and is protected by the actor's retirement guard.

use anyhow::{Context, Result, ensure};
use std::collections::HashMap;
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::time::Instant;

type Groups = HashMap<libc::pid_t, libc::pid_t>;

pub(super) struct Tree {
    session: Option<libc::pid_t>,
}

impl Tree {
    pub(super) fn new(child: &Child) -> Result<Self> {
        let pid = child.id().context("agent exited before process ownership was established")?;
        Ok(Self { session: Some(libc::pid_t::try_from(pid)?) })
    }

    async fn groups(&self, deadline: Instant) -> Result<Groups> {
        if self.session.is_none() {
            return Ok(Groups::new());
        }
        let listing = Command::new("ps").args(["-A", "-o", "pid=,stat="]).kill_on_drop(true).output();
        let listed = tokio::time::timeout_at(deadline, listing).await??;
        ensure!(listed.status.success(), "couldn't list agent session");
        Ok(self.parse_groups(&String::from_utf8_lossy(&listed.stdout)))
    }

    /// Ignore zombies that hold no resources and cannot be signalled.
    fn parse_groups(&self, listed: &str) -> Groups {
        let Some(session) = self.session else { return Groups::new() };
        let mut groups = Groups::new();
        for line in listed.lines() {
            let mut fields = line.split_whitespace();
            let Some(pid) = fields.next().and_then(|s| s.parse::<libc::pid_t>().ok()) else { continue };
            if fields.next().is_none_or(|state| state.starts_with('Z')) {
                continue;
            }
            // SAFETY: these calls only read the process table; an exited PID returns -1.
            if unsafe { libc::getsid(pid) } == session {
                // SAFETY: getpgid only reads the process table.
                let group = unsafe { libc::getpgid(pid) };
                if group > 0 {
                    groups.insert(group, pid);
                }
            }
        }
        groups
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

impl Drop for Tree {
    fn drop(&mut self) {
        if self.session.is_none() {
            return;
        }
        // Drop cannot await. Normal retirement uses the bounded async path above.
        let result = std::process::Command::new("ps").args(["-A", "-o", "pid=,stat="]).output();
        let cleanup = result.and_then(|listed| {
            if !listed.status.success() {
                return Err(std::io::Error::other("couldn't list agent session"));
            }
            self.signal(self.parse_groups(&String::from_utf8_lossy(&listed.stdout)))
        });
        if let Err(error) = cleanup {
            tracing::warn!(%error, "couldn't release agent session");
        }
    }
}
