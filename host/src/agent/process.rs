//! Own the agent's descendants even after its leader exits or the actor unwinds.

use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Child;
use tokio::sync::Mutex;
use tokio::time::Instant;

const REAP_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(100);

/// One process in an agent's job on Windows.
#[cfg(any(windows, test))]
#[derive(Clone)]
pub(crate) struct JobProcess {
    pub(crate) pid: u32,
    pub(crate) parent: u32,
    pub(crate) name: String,
    /// Kernel creation time disambiguates reused process IDs.
    pub(crate) created: u64,
}

pub(super) struct Process {
    tree: Tree,
    // Unix Drop transfers the child with its tree, pinning the session ID through fallback cleanup.
    child: Option<Child>,
    stopping: bool,
    #[cfg(all(test, unix))]
    fail_stop: bool,
}

impl Process {
    pub(super) fn new(child: Child) -> Result<Self> {
        let tree = Tree::new(&child)?;
        #[cfg(windows)]
        super::process_windows::resume(&child)?;
        Ok(Self {
            tree,
            child: Some(child),
            stopping: false,
            #[cfg(all(test, unix))]
            fail_stop: false,
        })
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
                        if let Err(error) = process.kill().await {
                            tracing::warn!(%error, "couldn't clean up exited agent");
                        }
                        break;
                    }
                    Ok(false) => {}
                    Err(error) => {
                        tracing::warn!(%error, "couldn't observe agent process exit");
                        if let Err(error) = process.kill().await {
                            tracing::warn!(%error, "couldn't clean up exited agent");
                        }
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
    fn exited(&self) -> std::io::Result<bool> {
        let Some(pid) = self.child.as_ref().and_then(Child::id) else { return Ok(true) };
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
    fn exited(&self) -> std::io::Result<bool> {
        use windows_sys::Win32::Foundation::{WAIT_FAILED, WAIT_OBJECT_0};
        let Some(handle) = self.child.as_ref().and_then(Child::raw_handle) else { return Ok(true) };
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

    /// The graceful-shutdown task keeps ownership until its deadline.
    pub(super) fn begin_stop(&mut self) -> bool {
        !std::mem::replace(&mut self.stopping, true)
    }

    #[cfg(all(test, unix))]
    pub(super) fn fail_next_stop(&mut self) {
        self.fail_stop = true;
    }

    pub(super) fn retry_stop(&mut self) {
        self.stopping = false;
    }

    /// Gives the leader and everything it started until `grace` runs out to finish by themselves,
    /// then kills what is left. Returns as soon as nothing is left.
    pub(super) async fn stop_within(&mut self, grace: Duration) -> Result<()> {
        #[cfg(all(test, unix))]
        if std::mem::take(&mut self.fail_stop) {
            anyhow::bail!("injected cleanup failure");
        }
        let deadline = Instant::now() + grace;
        while !self.exited().unwrap_or(true) && Instant::now() < deadline {
            tokio::time::sleep(POLL).await;
        }
        #[cfg(unix)]
        self.tree.grace(deadline).await?;
        // Keep the Unix leader unreaped until every owned group has been signalled.
        // Its session ID cannot then be reused for an unrelated process.
        self.kill().await
    }

    pub(super) async fn kill(&mut self) -> Result<()> {
        let terminated = self.tree.terminate_within(Instant::now() + REAP_TIMEOUT).await;
        terminated.context("terminating agent descendants")?;
        // On Unix keep the unreaped leader pinning the session ID until all owned groups drain.
        #[cfg(unix)]
        self.tree.drained(Instant::now() + REAP_TIMEOUT).await?;
        let child = self.child.as_mut().context("agent process already released")?;
        let result = tokio::time::timeout(REAP_TIMEOUT, child.kill()).await?;
        match result {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => {}
            Err(error) => return Err(error).context("reaping agent process"),
        }
        #[cfg(windows)]
        self.tree.drained(Instant::now() + REAP_TIMEOUT).await?;
        Ok(())
    }
}

#[cfg(unix)]
impl Drop for Process {
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            self.tree.release_on_drop(child);
        }
    }
}

#[cfg(unix)]
use super::process_unix::Tree;

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
            return Err(std::io::Error::last_os_error()).context("creating agent process job");
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
            return Err(std::io::Error::last_os_error()).context("configuring agent process job");
        }
        let process = child.raw_handle().context("agent exited before process ownership was established")?;
        // SAFETY: the child and job handles remain owned and valid during assignment.
        if unsafe { AssignProcessToJobObject(handle.as_raw_handle(), process) } == 0 {
            return Err(std::io::Error::last_os_error()).context("owning agent process descendants");
        }
        Ok(Self { handle })
    }

    fn terminate_within(&mut self, _deadline: Instant) -> std::future::Ready<Result<()>> {
        std::future::ready(self.terminate().context("terminating agent job"))
    }

    async fn drained(&mut self, deadline: Instant) -> Result<()> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::{
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectBasicAccountingInformation, QueryInformationJobObject,
        };
        loop {
            let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
            // SAFETY: the job is owned; info is writable with the exact structure size.
            let ok = unsafe {
                QueryInformationJobObject(
                    self.handle.as_raw_handle(),
                    JobObjectBasicAccountingInformation,
                    std::ptr::from_mut(&mut info).cast(),
                    u32::try_from(size_of_val(&info))?,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                return Err(std::io::Error::last_os_error()).context("reading agent job");
            }
            if info.ActiveProcesses == 0 {
                return Ok(());
            }
            anyhow::ensure!(Instant::now() < deadline, "agent job did not drain before cleanup deadline");
            tokio::time::sleep(POLL).await;
        }
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
