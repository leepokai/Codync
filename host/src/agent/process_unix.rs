//! A program's tree on Unix: the session it leads (`own_tree`) with every process group in it, and what `ps` lists.

use super::process::{JobProcess, parse_ps};
use anyhow::{Context, Result};
use tokio::process::Child;

/// Lists every process on this computer, for [`listed`].
fn ps() -> std::process::Command {
    let mut ps = std::process::Command::new("ps");
    ps.args(["-A", "-o", "pid=,ppid=,comm="]);
    ps
}

/// Every process on this computer.
pub(crate) async fn list_processes() -> Result<Vec<JobProcess>> {
    listed(&tokio::process::Command::from(ps()).output().await?)
}

fn listed(output: &std::process::Output) -> Result<Vec<JobProcess>> {
    anyhow::ensure!(output.status.success(), "ps failed");
    Ok(parse_ps(&String::from_utf8_lossy(&output.stdout)))
}

/// `pid`'s session, or `None` once it's gone (a zombie's too, on macOS).
pub(crate) fn session(pid: u32) -> Option<libc::pid_t> {
    let pid = libc::pid_t::try_from(pid).ok()?;
    // SAFETY: getsid only reads the process table.
    let sid = unsafe { libc::getsid(pid) };
    (sid >= 0).then_some(sid)
}

/// Whether `pid` is in session `sid`. A session's id can't go to another process while anything is left in it.
pub(super) fn in_session(sid: libc::pid_t, pid: u32) -> bool {
    session(pid) == Some(sid)
}

/// Kills every process group `listed` has in `session`, like the one an MCP server starts for itself (Codex does). Each
/// group is read from a process then checked to be in the session: a group never spans two, so one that ended since the
/// listing isn't signalled under an id another process took.
pub(super) fn end_groups(session: libc::pid_t, listed: &[JobProcess]) {
    let mut ended = Vec::new();
    for process in listed {
        let Ok(pid) = libc::pid_t::try_from(process.pid) else { continue };
        // SAFETY: getpgid only reads the process table.
        let group = unsafe { libc::getpgid(pid) };
        if group <= 0 || ended.contains(&group) || !in_session(session, process.pid) {
            continue;
        }
        // SAFETY: kill only sends a signal, to a group of the owned session.
        if unsafe { libc::kill(-group, libc::SIGKILL) } == -1 {
            tracing::debug!(group, error = %std::io::Error::last_os_error(), "couldn't end a program's process group");
        }
        ended.push(group);
    }
}

/// The program leads its process group and its session (`own_tree`), under the same id.
pub(super) struct Tree {
    pub(super) group: Option<libc::pid_t>,
    /// What it started in groups of their own is still to be ended through this ([`end_groups`]).
    pub(super) session: Option<libc::pid_t>,
}

impl Tree {
    pub(super) fn new(child: &Child) -> Result<Self> {
        let pid = child.id().context("the program exited before process ownership was established")?;
        let leader = libc::pid_t::try_from(pid)?;
        Ok(Self { group: Some(leader), session: Some(leader) })
    }

    pub(super) fn interrupt(&self) {
        let Some(group) = self.group else { return };
        // SAFETY: as in `terminate`; the group is still owned and only receives a signal.
        unsafe { libc::kill(-group, libc::SIGTERM) };
    }

    pub(super) fn terminate(&mut self) -> std::io::Result<()> {
        let Some(group) = self.group.take() else { return Ok(()) };
        // SAFETY: spawn put this still-owned, unreaped child in its own process group.
        // A negative PID signals that group. No pointer or shared memory is involved.
        if unsafe { libc::kill(-group, libc::SIGKILL) } == -1 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error);
            }
        }
        Ok(())
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        if let Err(error) = self.terminate() {
            tracing::warn!(%error, "couldn't release a program's process group");
        }
        // Dropped without `Process::kill`: the rest of its session takes a listing, off this (maybe the runtime's) thread.
        let Some(session) = self.session.take() else { return };
        let sweep = move || match ps().output().map_err(anyhow::Error::from).and_then(|output| listed(&output)) {
            Ok(listed) => end_groups(session, &listed),
            Err(error) => tracing::warn!(error = format!("{error:#}"), "couldn't list a program's processes"),
        };
        // Not `thread::spawn`, which panics when no thread can start: a panic here, maybe while unwinding, would abort.
        if let Err(error) = std::thread::Builder::new().spawn(sweep) {
            tracing::warn!(%error, "couldn't end a program's process groups");
        }
    }
}
