//! Own a program's descendants (an agent's, a dev server's) even after its leader exits or its owner unwinds.

#[cfg(unix)]
use super::process_unix::{Tree, end_groups, in_session};
#[cfg(unix)]
pub(crate) use super::process_unix::{list_processes, session};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Child;
use tokio::sync::Mutex;
use tokio::time::Instant;

const REAP_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(100);
/// How long a process whose children have all ended gets to finish by itself before it is ended too: a `PyInstaller`
/// onefile launcher (Antigravity's Windows server) removes its ~131 MB unpacked folder only after its program ends (#399).
const PARENT_GRACE: Duration = Duration::from_secs(3);
/// The longest a tree unwinds that way before what is left is killed at once: [`PARENT_GRACE`] for each of a few levels
/// of parents that outlive their children, like an agent under its launcher and an MCP server under the agent.
pub(crate) const UNWIND: Duration = Duration::from_secs(15);
/// The console window Windows gives a process started without one: no work of its own, and no child that keeps its
/// parent going.
const CONSOLE_HOST: &str = "conhost.exe";

/// One process in an agent's tree: on Windows its job, on Unix what `ps` lists under it.
#[derive(Clone)]
pub(crate) struct JobProcess {
    pub(crate) pid: u32,
    pub(crate) parent: u32,
    pub(crate) name: String,
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

/// Whether `process` is a console window ([`CONSOLE_HOST`]).
pub(crate) fn is_console(process: &JobProcess) -> bool {
    process.name.eq_ignore_ascii_case(CONSOLE_HOST)
}

/// Whether nothing in `tree` runs under `process` but a console window. A pid listed as its own parent (a reused pid)
/// isn't its child.
pub(crate) fn starts_nothing(tree: &[JobProcess], process: &JobProcess) -> bool {
    !tree.iter().any(|child| child.parent == process.pid && child.pid != process.pid && !is_console(child))
}

/// Which of `tree` to end at `now`, from its leaves up: at the first listing whatever starts nothing, at once; later a
/// process left with nothing under it, [`PARENT_GRACE`] after it is first seen so, to finish by itself first. `ends_at`
/// keeps those times from one listing to the next. A console window is left to the job's end: what it serves needs it.
fn due(tree: &[JobProcess], ends_at: &mut HashMap<u32, Instant>, now: Instant) -> Vec<u32> {
    let grace = if ends_at.is_empty() { Duration::ZERO } else { PARENT_GRACE };
    tree.iter()
        .filter(|process| !is_console(process) && starts_nothing(tree, process))
        .filter(|process| now >= *ends_at.entry(process.pid).or_insert(now + grace))
        .map(|process| process.pid)
        .collect()
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

/// Sets `command` up so [`Process::new`] can own everything it starts: a session of its own on Unix, which what it
/// starts stays in even in a process group of its own (Codex's MCP servers), suspended until its job holds it on
/// Windows.
pub(crate) fn own_tree(command: &mut tokio::process::Command) {
    #[cfg(unix)]
    // SAFETY: the hook runs in the forked child before exec and only calls setsid, which is async-signal-safe.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
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
    fn job_members(&self) -> Result<Vec<u32>> {
        super::process_windows::job_members(&self.tree.handle)
    }

    /// Everything still running in the program's tree.
    #[cfg(windows)]
    pub(super) async fn processes(&self) -> Result<Vec<JobProcess>> {
        let members = self.job_members()?;
        tokio::task::spawn_blocking(move || super::process_windows::named(&members)).await?
    }

    /// Everything still running in the program's session.
    #[cfg(unix)]
    async fn processes(&self) -> Result<Vec<JobProcess>> {
        Ok(list_processes().await?.into_iter().filter(|process| self.in_session(process.pid)).collect())
    }

    /// Kills `pid` if it is still in the program's session.
    #[cfg(unix)]
    fn end(&self, pid: u32) -> std::io::Result<()> {
        if !self.in_session(pid) {
            return Ok(());
        }
        let pid = libc::pid_t::try_from(pid).expect("a pid in the session fits pid_t");
        // SAFETY: kill only sends a signal, to a process just checked to be in the owned session.
        if unsafe { libc::kill(pid, libc::SIGKILL) } == -1 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    /// Ends `pid` if it is still in the program's job.
    #[cfg(windows)]
    fn end(&self, pid: u32) -> Result<()> {
        super::process_windows::end(&self.tree.handle, pid)
    }

    /// Ends the tree from its leaves up ([`due`]) while its leader runs, for at most [`UNWIND`].
    async fn unwind(&self) {
        let deadline = Instant::now() + UNWIND;
        let mut ends_at = HashMap::new();
        while !self.exited().unwrap_or(true) && Instant::now() < deadline {
            let Ok(tree) = self.processes().await else { return };
            for pid in due(&tree, &mut ends_at, Instant::now()) {
                if let Err(error) = self.end(pid) {
                    tracing::debug!(pid, error = format!("{error:#}"), "couldn't end a program's process");
                }
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// Whether `pid` is in this program's session, whatever its process group. A descendant that left it for a
    /// session of its own isn't: stopping the program wouldn't reach it either.
    #[cfg(unix)]
    fn in_session(&self, pid: u32) -> bool {
        self.tree.session.is_some_and(|session| in_session(session, pid))
    }

    /// Which of `candidates` belong to this program's tree ([`Self::in_session`]).
    #[cfg(unix)]
    pub(crate) fn members(&self, candidates: &[u32]) -> Vec<u32> {
        candidates.iter().copied().filter(|pid| self.in_session(*pid)).collect()
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
    /// program without a console of ours, so there it unwinds at once).
    pub(crate) async fn stop_gracefully(&mut self, grace: Duration) {
        #[cfg(unix)]
        self.tree.interrupt();
        self.stop_within(if cfg!(windows) { Duration::ZERO } else { grace }).await;
    }

    /// The graceful-shutdown task keeps ownership until its deadline.
    pub(super) fn begin_stop(&mut self) -> bool {
        !std::mem::replace(&mut self.stopping, true)
    }

    /// Gives the leader and everything it started until `grace` runs out to finish by themselves,
    /// then ends what is left from its leaves up ([`Self::unwind`]). Returns as soon as nothing is left.
    pub(super) async fn stop_within(&mut self, grace: Duration) {
        let deadline = Instant::now() + grace;
        while !self.exited().unwrap_or(true) && Instant::now() < deadline {
            tokio::time::sleep(POLL).await;
        }
        self.unwind().await;
        #[cfg(unix)]
        if self.exited().unwrap_or(false) {
            // Reaped, the leader no longer pins its PID: only a live member of its session does, and
            // that is what `drained` waits out. A group of zombies alone isn't signalled (macOS refuses it).
            let _ = self.child.wait().await;
            self.drained(deadline).await;
        }
        self.kill().await;
    }

    /// Waits until nothing is left in the program's session, or `deadline`, so everything it started has the leader's
    /// grace to end by itself, an MCP server in a process group of its own too. An empty session is forgotten with its
    /// group, so their (reusable) id is never signalled.
    #[cfg(unix)]
    async fn drained(&mut self, deadline: Instant) {
        loop {
            if self.processes().await.is_ok_and(|left| left.is_empty()) {
                self.tree.group = None;
                self.tree.session = None;
                return;
            }
            if Instant::now() >= deadline {
                return;
            }
            tokio::time::sleep(POLL).await;
        }
    }

    pub(crate) async fn kill(&mut self) {
        #[cfg(unix)]
        if let Some(session) = self.tree.session.take() {
            match list_processes().await {
                Ok(listed) => end_groups(session, &listed),
                Err(error) => tracing::warn!(error = format!("{error:#}"), "couldn't list a program's processes"),
            }
        }
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

#[cfg(windows)]
struct Tree {
    handle: std::os::windows::io::OwnedHandle,
}

#[cfg(windows)]
impl Tree {
    fn new(child: &Child) -> Result<Self> {
        use anyhow::Context;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tree_ends_from_its_leaves_up() {
        let process = |pid, parent, name: &str| JobProcess { pid, parent, name: name.to_owned() };
        // Antigravity on Windows: cmd -> console window, launcher -> agent -> an MCP server.
        let mut tree = vec![
            process(1, 900, "cmd.exe"),
            process(2, 1, "conhost.exe"),
            process(3, 1, "agy_acp_server.exe"),
            process(4, 3, "agy_acp_server.exe"),
            process(5, 4, "codync-host.exe"),
        ];
        let mut ends_at = HashMap::new();
        let mut now = Instant::now();
        let mut ended = 5;
        assert_eq!(due(&tree, &mut ends_at, now), [ended], "what starts nothing ends at once, but the console window");
        // Each parent left alone gets its grace, the launcher too, to remove its folder; the console window keeps
        // nothing going.
        for parent in [4, 3, 1] {
            tree.retain(|p| p.pid != ended);
            assert_eq!(due(&tree, &mut ends_at, now), Vec::<u32>::new());
            now += PARENT_GRACE;
            assert_eq!(due(&tree, &mut ends_at, now), [parent]);
            ended = parent;
        }

        // A pid listed as its own parent is still a leaf.
        assert_eq!(due(&[process(7, 7, "x")], &mut HashMap::new(), Instant::now()), [7]);
    }

    /// The leader exits at once; what it started in a process group of its own still gets the grace to finish.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_stop_gives_a_server_in_a_group_of_its_own_the_leaders_grace() {
        let done = std::env::temp_dir().join(format!("codync-process-{}", uuid::Uuid::new_v4()));
        let mut command = tokio::process::Command::new("perl");
        let server = r#"if (!fork) { setpgrp; select undef, undef, undef, 0.5; open my $f, ">", $ARGV[0]; exit }"#;
        command.args(["-e", server]).arg(&done).stdin(std::process::Stdio::null());
        own_tree(&mut command);
        let mut process = Process::new(command.spawn().expect("perl runs")).expect("owns its tree");
        process.stop_within(Duration::from_secs(5)).await;
        assert!(done.exists(), "the server was killed before its grace ran out");
        let _ = std::fs::remove_file(done);
    }
}
