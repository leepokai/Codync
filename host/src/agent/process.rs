//! Own a program's descendants (an agent's, a dev server's) even after its leader exits or its owner unwinds.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Child;
use tokio::sync::Mutex;
use tokio::time::Instant;

const REAP_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(100);

/// One process in an agent's tree: on Windows its job, on Unix what `ps` lists under it.
#[derive(Clone)]
pub(crate) struct JobProcess {
    pub(crate) pid: u32,
    pub(crate) parent: u32,
    pub(crate) name: String,
}

/// Every process on this computer.
#[cfg(unix)]
pub(crate) async fn list_processes() -> Result<Vec<JobProcess>> {
    let listed = tokio::process::Command::new("ps").args(["-A", "-o", "pid=,ppid=,comm="]).output().await?;
    anyhow::ensure!(listed.status.success(), "ps failed");
    Ok(parse_ps(&String::from_utf8_lossy(&listed.stdout)))
}

/// Rows of `ps -o pid=,ppid=,comm=`; a name may hold spaces.
#[cfg(any(unix, test))]
pub(crate) fn parse_ps(listed: &str) -> Vec<JobProcess> {
    listed
        .lines()
        .filter_map(|line| {
            let mut fields = line.trim_start().splitn(2, char::is_whitespace);
            let pid = fields.next()?.parse().ok()?;
            let mut fields = fields.next()?.trim_start().splitn(2, char::is_whitespace);
            let parent = fields.next()?.parse().ok()?;
            let name = fields.next().unwrap_or_default().trim().to_owned();
            Some(JobProcess { pid, parent, name })
        })
        .collect()
}

/// Everything under `leader` in `rows`, without it.
#[cfg(any(unix, test))]
pub(crate) fn descendants(rows: &[JobProcess], leader: u32) -> Vec<JobProcess> {
    let mut found: Vec<JobProcess> = Vec::new();
    let mut parents = vec![leader];
    while let Some(parent) = parents.pop() {
        for row in rows.iter().filter(|row| row.parent == parent && row.pid != parent) {
            if !found.iter().any(|known| known.pid == row.pid) {
                parents.push(row.pid);
                found.push(row.clone());
            }
        }
    }
    found
}

/// A process's program name without its folder.
pub(crate) fn program_name(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

/// Tells whether a process in `tree` is `program`, or something it started. Every step stays inside `tree`, so `take`
/// reaches every ancestor and ends pid links that loop (reused pids).
pub(crate) fn under_program<'a>(tree: &'a [JobProcess], program: &'a str) -> impl Fn(&JobProcess) -> bool + 'a {
    let by_pid: HashMap<u32, &JobProcess> = tree.iter().map(|p| (p.pid, p)).collect();
    move |process| {
        std::iter::successors(Some(process), |c| by_pid.get(&c.parent).copied())
            .take(tree.len())
            .any(|c| program_name(&c.name).eq_ignore_ascii_case(program))
    }
}

/// Sets `command` up so [`Process::new`] can own everything it starts: a process group of its own on Unix,
/// suspended until its job holds it on Windows.
pub(crate) fn own_tree(command: &mut tokio::process::Command) {
    #[cfg(unix)]
    command.process_group(0);
    #[cfg(windows)]
    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_SUSPENDED);
}

pub(crate) struct Process {
    // Drop the tree before Child can reap the leader and make its PID reusable.
    tree: Tree,
    child: Child,
    stopping: bool,
}

impl Process {
    pub(crate) fn new(child: Child) -> Result<Self> {
        let tree = Tree::new(&child)?;
        let owned = Self { tree, child, stopping: false };
        #[cfg(windows)]
        super::process_windows::resume(&owned.child)?;
        Ok(owned)
    }

    pub(super) fn monitor(child: &Arc<Mutex<Self>>) {
        let weak = Arc::downgrade(child);
        tokio::spawn(async move {
            loop {
                let Some(child) = weak.upgrade() else { break };
                let mut process = child.lock().await;
                if process.stopping {
                    break;
                }
                match process.exited() {
                    Ok(true) => {
                        process.kill().await;
                        break;
                    }
                    Ok(false) => {}
                    Err(error) => {
                        tracing::warn!(%error, "couldn't observe agent process exit");
                        process.kill().await;
                        break;
                    }
                }
                drop(process);
                drop(child);
                tokio::time::sleep(POLL).await;
            }
        });
    }

    #[cfg(unix)]
    pub(crate) fn exited(&self) -> std::io::Result<bool> {
        let Some(pid) = self.child.id() else { return Ok(true) };
        // SAFETY: zero is a valid siginfo value; waitid writes only into this object.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // Keep the leader unreaped until its group is killed, preventing PID reuse.
        // SAFETY: pid identifies our owned child and the output pointer is writable.
        let result =
            unsafe { libc::waitid(libc::P_PID, pid, &raw mut info, libc::WEXITED | libc::WNOHANG | libc::WNOWAIT) };
        if result == -1 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                return Ok(false);
            }
            return Err(error);
        }
        #[cfg(target_os = "macos")]
        let exited = info.si_pid != 0;
        #[cfg(not(target_os = "macos"))]
        // SAFETY: waitid initialized the child-status variant of siginfo.
        let exited = unsafe { info.si_pid() } != 0;
        Ok(exited)
    }

    #[cfg(windows)]
    pub(crate) fn exited(&self) -> std::io::Result<bool> {
        use windows_sys::Win32::Foundation::{WAIT_FAILED, WAIT_OBJECT_0};
        let Some(handle) = self.child.raw_handle() else { return Ok(true) };
        // SAFETY: the child owns this valid process handle; a zero timeout cannot block.
        let status = unsafe { windows_sys::Win32::System::Threading::WaitForSingleObject(handle, 0) };
        if status == WAIT_FAILED {
            return Err(std::io::Error::last_os_error());
        }
        Ok(status == WAIT_OBJECT_0)
    }

    /// The ids of what is running in the agent's job.
    #[cfg(windows)]
    pub(super) fn job_members(&self) -> Result<Vec<u32>> {
        super::process_windows::job_members(&self.tree.handle)
    }

    /// Which of `candidates` belong to this program's tree. A descendant that left the group for a session of its
    /// own isn't counted on Unix: stopping the program wouldn't reach it either.
    #[cfg(unix)]
    pub(crate) fn members(&self, candidates: &[u32]) -> Vec<u32> {
        let Some(group) = self.tree.group else { return Vec::new() };
        let in_group = |pid: u32| {
            // SAFETY: getpgid only reads the process table.
            libc::pid_t::try_from(pid).is_ok_and(|pid| unsafe { libc::getpgid(pid) } == group)
        };
        candidates.iter().copied().filter(|pid| in_group(*pid)).collect()
    }

    /// Which of `candidates` belong to this program's job.
    #[cfg(windows)]
    pub(crate) fn members(&self, candidates: &[u32]) -> Vec<u32> {
        let members = self.job_members().unwrap_or_default();
        candidates.iter().copied().filter(|pid| members.contains(pid)).collect()
    }

    /// How the leader ended, once it was reaped; `None` for a signal.
    pub(crate) fn exit_code(&mut self) -> Option<i32> {
        self.child.try_wait().ok().flatten().and_then(|status| status.code())
    }

    /// Ends a program that takes no input: asks it to stop and gives it `grace` to (Windows has no such request for a
    /// program without a console of ours, so its job ends at once).
    pub(crate) async fn stop_gracefully(&mut self, grace: Duration) {
        #[cfg(unix)]
        {
            self.tree.interrupt();
            self.stop_within(grace).await;
        }
        #[cfg(windows)]
        {
            let _ = grace;
            self.kill().await;
        }
    }

    /// The graceful-shutdown task keeps ownership until its deadline.
    pub(super) fn begin_stop(&mut self) -> bool {
        !std::mem::replace(&mut self.stopping, true)
    }

    /// Gives the leader and everything it started until `grace` runs out to finish by themselves,
    /// then kills what is left. Returns as soon as nothing is left.
    pub(super) async fn stop_within(&mut self, grace: Duration) {
        let deadline = Instant::now() + grace;
        while !self.exited().unwrap_or(true) && Instant::now() < deadline {
            tokio::time::sleep(POLL).await;
        }
        #[cfg(unix)]
        if self.exited().unwrap_or(false) {
            // Reaped, the leader no longer pins its PID: only a live group member does, and
            // that is what `drained` waits out.
            let _ = self.child.wait().await;
            self.tree.drained(deadline).await;
        }
        self.kill().await;
    }

    pub(crate) async fn kill(&mut self) {
        if let Err(error) = self.tree.terminate() {
            tracing::warn!(%error, "couldn't terminate a program's descendants");
        }
        let result = tokio::time::timeout(REAP_TIMEOUT, self.child.kill()).await;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::InvalidInput => {}
            Ok(Err(error)) => tracing::warn!(%error, "couldn't reap a program's process"),
            Err(error) => tracing::warn!(%error, "a program's process did not exit before the cleanup deadline"),
        }
    }
}

#[cfg(unix)]
struct Tree {
    group: Option<libc::pid_t>,
}

#[cfg(unix)]
impl Tree {
    fn new(child: &Child) -> Result<Self> {
        let pid = child.id().context("the program exited before process ownership was established")?;
        Ok(Self { group: Some(libc::pid_t::try_from(pid)?) })
    }

    /// Waits until the group has no process left, or `deadline`; an empty group is forgotten so
    /// its (reusable) ID is never signalled.
    async fn drained(&mut self, deadline: Instant) {
        let Some(group) = self.group else { return };
        // SAFETY: signal 0 sends nothing; it only checks that the group still has a process.
        let running = || unsafe { libc::killpg(group, 0) } == 0;
        while running() && Instant::now() < deadline {
            tokio::time::sleep(POLL).await;
        }
        if !running() {
            self.group = None;
        }
    }

    fn interrupt(&self) {
        let Some(group) = self.group else { return };
        // SAFETY: as in `terminate`; the group is still owned and only receives a signal.
        unsafe { libc::kill(-group, libc::SIGTERM) };
    }

    fn terminate(&mut self) -> std::io::Result<()> {
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

#[cfg(unix)]
impl Drop for Tree {
    fn drop(&mut self) {
        if let Err(error) = self.terminate() {
            tracing::warn!(%error, "couldn't release a program's process group");
        }
    }
}

#[cfg(windows)]
struct Tree {
    handle: std::os::windows::io::OwnedHandle,
}

#[cfg(windows)]
impl Tree {
    fn new(child: &Child) -> Result<Self> {
        use std::os::windows::io::{AsRawHandle, FromRawHandle};
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, SetInformationJobObject,
        };
        // SAFETY: null uses default security and an unnamed, newly owned job.
        let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if raw.is_null() {
            return Err(std::io::Error::last_os_error()).context("creating a program's job");
        }
        // SAFETY: CreateJobObjectW transferred a unique valid handle to this owner.
        let handle = unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(raw) };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let size = u32::try_from(size_of_val(&limits))?;
        // SAFETY: limits is initialized and its exact size is supplied for this class.
        if unsafe {
            SetInformationJobObject(
                handle.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&limits).cast(),
                size,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error()).context("configuring a program's job");
        }
        let process = child.raw_handle().context("the program exited before process ownership was established")?;
        // SAFETY: the child and job handles remain owned and valid during assignment.
        if unsafe { AssignProcessToJobObject(handle.as_raw_handle(), process) } == 0 {
            return Err(std::io::Error::last_os_error()).context("owning a program's descendants");
        }
        Ok(Self { handle })
    }

    fn terminate(&mut self) -> std::io::Result<()> {
        use std::os::windows::io::AsRawHandle;
        // SAFETY: this object owns the job handle; termination does not close it.
        if unsafe { windows_sys::Win32::System::JobObjects::TerminateJobObject(self.handle.as_raw_handle(), 1) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}
