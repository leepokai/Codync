//! Whether an agent still runs something that stopping it would end, which keeps it running when it's idle.

use crate::agent::acp::Acp;
#[cfg(any(windows, test))]
use crate::agent::process::JobProcess;
use std::collections::HashMap;
#[cfg(any(windows, test))]
use std::collections::HashSet;

/// Whether something the agent started runs detached, in a session of its own, like a dev server
/// from its shell tool. Stopping the agent would orphan it and lose track of it, so the agent
/// stays. MCP servers stay in the agent's session and don't count, even in a process group of
/// their own (Codex starts each one in its own group). Unsure counts as busy.
// ponytail: a dev server keeps its agent (and memory) indefinitely; add a cap if that matters.
#[cfg(unix)]
pub(super) async fn background_work(acp: &Acp) -> bool {
    detached_work(acp.pid()).await
}

#[cfg(unix)]
async fn detached_work(leader: Option<u32>) -> bool {
    let Some(leader) = leader else { return true };
    let listing = tokio::task::spawn_blocking(crate::agent::process_table::parent_links);
    let listed = listing.await;
    let processes = match listed {
        Ok(Ok(processes)) => processes,
        error => {
            tracing::warn!(?error, "couldn't list processes");
            return true;
        }
    };
    detached_descendant(&processes, leader, session)
}

/// `pid`'s session, or `None` once it's gone.
#[cfg(unix)]
fn session(pid: u32) -> Option<i32> {
    let pid = libc::pid_t::try_from(pid).ok()?;
    // SAFETY: getsid only reads the process table.
    let sid = unsafe { libc::getsid(pid) };
    (sid >= 0).then_some(sid)
}

/// Windows has no sessions to tell a dev server from an MCP server, and stopping the agent ends everything in its
/// job there, so the job's processes are judged by what they are ([`runs_something_else`]). Unsure counts as busy.
#[cfg(windows)]
pub(super) async fn background_work(acp: &Acp) -> bool {
    let Ok(exe) = std::env::current_exe() else { return true };
    let Some(own) = exe.file_name().and_then(|n| n.to_str()) else { return true };
    match acp.job_processes().await {
        Ok(processes) => runs_something_else(&processes, own, acp.pid(), acp.startup_processes(), acp.agent_program()),
        Err(error) => {
            tracing::warn!(error = format!("{error:#}"), "couldn't list the agent's processes");
            true
        }
    }
}

/// The console window Windows gives a process started without one: no work of its own.
#[cfg(any(windows, test))]
const CONSOLE_HOST: &str = "conhost.exe";

/// Whether an agent's job holds a process that isn't the agent or Codync's MCP servers (`own`: the host's own
/// program name, and whatever those servers start). The agent is what leads to them, so anything else that starts
/// nothing further counts, except the exact adapter startup chain captured before session creation. This
/// recognizes a lazy adapter with no MCP children without authorizing later dev servers beneath it.
// ponytail: user-configured MCP servers and lazily started engines remain conservative holds without evidence
// that they belong to the adapter; identify them from its configuration if that matters.
#[cfg(any(windows, test))]
fn runs_something_else(
    processes: &[JobProcess],
    own: &str,
    leader: Option<u32>,
    startup: &[JobProcess],
    program: Option<&str>,
) -> bool {
    let by_pid: HashMap<u32, &JobProcess> = processes.iter().map(|p| (p.pid, p)).collect();
    let is_console = |p: &JobProcess| p.name.eq_ignore_ascii_case(CONSOLE_HOST);
    // Every step stays inside `processes`, so `take` reaches every ancestor and ends pid links that loop (reused pids).
    let ours = |p: &JobProcess| {
        std::iter::successors(Some(p), |c| by_pid.get(&c.parent).copied().filter(|parent| parent.created <= c.created))
            .take(processes.len())
            .any(|c| c.name.eq_ignore_ascii_case(own))
    };
    let starts_nothing = |p: &JobProcess| {
        !processes.iter().any(|c| c.parent == p.pid && c.pid != p.pid && c.created >= p.created && !is_console(c))
    };
    let known = startup_chain(leader, startup, program);
    processes.iter().filter(|p| !is_console(p) && !known.contains(&(p.pid, p.created)) && !ours(p)).any(starts_nothing)
}

/// Recognize the shell and its selected executable, never that executable's unknown children.
/// Complex commands and ambiguous branches remain conservative holds.
#[cfg(any(windows, test))]
fn startup_chain(leader: Option<u32>, processes: &[JobProcess], program: Option<&str>) -> HashSet<(u32, u64)> {
    let mut known = HashSet::new();
    let Some(root) = leader.and_then(|pid| processes.iter().find(|p| p.pid == pid)) else { return known };
    known.insert((root.pid, root.created));
    let Some(program) = program else { return known };
    let mut children = processes
        .iter()
        .filter(|c| c.parent == root.pid && c.created >= root.created && !c.name.eq_ignore_ascii_case(CONSOLE_HOST));
    if let Some(child) = children.next()
        && children.next().is_none()
        && child.name.eq_ignore_ascii_case(program)
    {
        known.insert((child.pid, child.created));
    }
    known
}

/// `processes`: (pid, parent pid) rows; `session` finds a live process's session.
#[cfg(any(unix, test))]
fn detached_descendant(processes: &[(u32, u32)], leader: u32, session: impl Fn(u32) -> Option<i32>) -> bool {
    let Some(own) = session(leader) else { return true };
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for &(pid, parent) in processes {
        children.entry(parent).or_default().push(pid);
    }
    let mut stack = vec![leader];
    while let Some(parent) = stack.pop() {
        for &pid in children.get(&parent).into_iter().flatten() {
            // One that exited since the listing holds nothing.
            if session(pid).is_some_and(|sid| sid != own) {
                return true;
            }
            stack.push(pid);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_work_in_another_session_holds_the_agent() {
        // 10 agent -> 11 codex -> 12 MCP server in a group of its own; all in the host's session 5.
        let sessions: HashMap<u32, i32> = [(10, 5), (11, 5), (12, 5), (20, 20), (21, 20), (30, 30)].into();
        let session = |pid| sessions.get(&pid).copied();
        let agent = [(10, 1), (11, 10), (12, 11), (40, 1)];
        assert!(!detached_descendant(&agent, 10, session));
        // The shell tool runs `npm run dev` in a session of its own.
        let mut busy = agent.to_vec();
        busy.extend([(20, 11), (21, 20)]);
        assert!(detached_descendant(&busy, 10, session));
        // Someone else's process in another session isn't the agent's.
        assert!(!detached_descendant(&[(10, 1), (30, 1)], 10, session));
        // A child that exited since the listing holds nothing; an agent that can't be looked up is unsure.
        assert!(!detached_descendant(&[(10, 1), (50, 10)], 10, session));
        assert!(detached_descendant(&agent, 99, session));
    }

    #[test]
    fn on_windows_only_what_isnt_the_agent_or_codyncs_servers_holds_the_agent() {
        let own = "codync-host.exe";
        let process = |pid, parent, name: &str| JobProcess { pid, parent, name: name.to_owned(), created: 0 };
        let busy = |processes: &[JobProcess]| runs_something_else(processes, own, Some(1), &[], None);
        // cmd -> node (adapter) -> codex (engine) -> Codync's MCP servers, with a console window.
        let agent = vec![
            process(1, 900, "cmd.exe"),
            process(2, 1, "conhost.exe"),
            process(3, 1, "node.exe"),
            process(4, 3, "codex.exe"),
            process(5, 4, "CODYNC-HOST.EXE"),
            process(6, 4, "codync-host.exe"),
        ];
        assert!(!busy(&agent));
        assert!(!busy(&[]), "nothing left in the job");

        // A dev server from the shell tool, alive or with the shell that started it gone.
        let mut server = agent.clone();
        server.extend([process(7, 4, "powershell.exe"), process(8, 7, "node.exe")]);
        assert!(busy(&server));
        let mut orphan = agent.clone();
        orphan.push(process(8, 777, "node.exe"));
        assert!(busy(&orphan));
        // A console window of its own doesn't hide it.
        let mut windowed = agent.clone();
        windowed.extend([process(8, 4, "node.exe"), process(9, 8, "conhost.exe")]);
        assert!(busy(&windowed));

        // What Codync's own servers start is theirs, like a connector they launch.
        let mut connector = agent.clone();
        connector.extend([process(10, 5, "cmd.exe"), process(11, 10, "node.exe")]);
        assert!(!busy(&connector));

        // Pid links that loop (a reused pid) end the walk, and an agent with no server left is unsure.
        assert!(busy(&[process(1, 2, "a.exe"), process(2, 1, "b.exe"), process(3, 1, "c.exe")]));
        assert!(busy(&[process(1, 900, "cmd.exe"), process(3, 1, "node.exe")]));
    }

    #[test]
    fn an_initialized_windows_agent_stays_recyclable_after_its_helpers_exit() {
        let process = |pid, parent, name: &str, created| JobProcess { pid, parent, name: name.to_owned(), created };
        let startup =
            vec![process(1, 900, "cmd.exe", 10), process(2, 1, "conhost.exe", 11), process(3, 1, "python.exe", 12)];
        let busy = |processes: &[JobProcess]| {
            runs_something_else(processes, "codync-host.exe", Some(1), &startup, Some("python.exe"))
        };
        assert!(!busy(&startup), "an adapter need not start an MCP server for a plain reply");
        let mut with_helper = startup.clone();
        with_helper.push(process(5, 3, "codync-host.exe", 20));
        assert!(!busy(&with_helper));
        assert!(!busy(&startup), "the agent remains known once its helper exits");
        with_helper.push(process(6, 3, "node.exe", 21));
        assert!(busy(&with_helper), "a later child of the known adapter remains protected");
    }

    #[test]
    fn an_adapters_sole_startup_child_remains_unknown_work() {
        let process =
            |pid, parent, name: &str| JobProcess { pid, parent, name: name.to_owned(), created: u64::from(pid) };
        let startup = vec![process(1, 900, "cmd.exe"), process(3, 1, "python.exe"), process(5, 3, "node.exe")];
        assert!(runs_something_else(&startup, "codync-host.exe", Some(1), &startup, Some("python.exe")));
        assert_eq!(startup_chain(Some(1), &startup, Some("python.exe")), HashSet::from([(1, 1), (3, 3)]));
    }

    #[test]
    fn startup_branches_and_missing_identity_do_not_authorize_unknown_work() {
        let process = |pid, parent, created| JobProcess { pid, parent, name: "node.exe".into(), created };
        let startup = vec![process(1, 900, 10), process(3, 1, 11), process(4, 1, 12)];
        let known = startup_chain(Some(1), &startup, Some("node.exe"));
        assert_eq!(known, HashSet::from([(1, 10)]), "neither startup branch is proven to be the adapter");
        assert!(runs_something_else(&startup, "codync-host.exe", Some(1), &startup, Some("node.exe")));
        let linear = vec![process(1, 900, 10), process(3, 1, 11)];
        assert!(runs_something_else(&linear, "codync-host.exe", None, &linear, Some("node.exe")));
        assert!(runs_something_else(&linear, "codync-host.exe", Some(1), &[], Some("node.exe")));
    }

    #[test]
    fn reused_windows_pids_cannot_inherit_an_agents_identity_or_parent_links() {
        let process = |pid, parent, name: &str, created| JobProcess { pid, parent, name: name.to_owned(), created };
        let startup = vec![process(1, 900, "cmd.exe", 10), process(3, 1, "python.exe", 11)];
        let reused = vec![startup[0].clone(), process(3, 1, "python.exe", 30)];
        assert!(runs_something_else(&reused, "codync-host.exe", Some(1), &startup, Some("python.exe")));
        // An old helper still names its original parent's PID; a newer process reusing it is unrelated.
        let stale_parent =
            vec![startup[0].clone(), process(3, 1, "node.exe", 30), process(5, 3, "codync-host.exe", 20)];
        assert!(runs_something_else(&stale_parent, "codync-host.exe", Some(1), &startup, Some("python.exe")));
        assert_eq!(startup_chain(Some(3), &stale_parent, Some("node.exe")), HashSet::from([(3, 30)]));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_process_started_in_its_own_session_is_background_work() {
        use std::process::Stdio;
        let spawn = |script: &str| {
            let mut command = tokio::process::Command::new("perl");
            command.args(["-e", script]).stdin(Stdio::null()).kill_on_drop(true).process_group(0);
            command.spawn().expect("perl runs")
        };
        let plain = spawn("my $pid = fork; if (!$pid) { sleep 30; exit } sleep 30");
        // How Codex starts an MCP server.
        let grouped = spawn("my $pid = fork; if (!$pid) { setpgrp; sleep 30; exit } sleep 30");
        let detached = spawn("use POSIX (); my $pid = fork; if (!$pid) { POSIX::setsid(); sleep 30; exit } sleep 30");
        // The children exist once perl has forked them: wait for that instead of a fixed pause,
        // which a busy CI runner can outlast.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        while !detached_work(detached.id()).await {
            assert!(tokio::time::Instant::now() < deadline, "the detached child never showed up");
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(!detached_work(plain.id()).await);
        assert!(!detached_work(grouped.id()).await);
        for child in [plain, grouped, detached] {
            if let Some(pid) = child.id() {
                let _ = std::process::Command::new("/bin/kill").args(["-KILL", "--", &format!("-{pid}")]).status();
            }
        }
    }
}
