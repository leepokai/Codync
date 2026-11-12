//! Whether an agent still runs something that stopping it would end, which keeps it running when it's idle.

use crate::agent::acp::Acp;
use crate::agent::process::{self, JobProcess};

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
    let listed = process::list_processes().await;
    let Ok(processes) = listed.inspect_err(|e| tracing::warn!(error = format!("{e:#}"), "couldn't list processes"))
    else {
        return true;
    };
    detached_descendant(&processes, leader, process::session)
}

/// Windows has no sessions to tell a dev server from an MCP server, and stopping the agent ends everything in its
/// job there, so the job's processes are judged by what they are ([`runs_something_else`]). Unsure counts as busy.
#[cfg(windows)]
pub(super) async fn background_work(acp: &Acp) -> bool {
    let Ok(exe) = std::env::current_exe() else { return true };
    let Some(own) = exe.file_name().and_then(|n| n.to_str()) else { return true };
    match acp.job_processes().await {
        Ok(processes) => runs_something_else(&processes, own),
        Err(error) => {
            tracing::warn!(error = format!("{error:#}"), "couldn't list the agent's processes");
            true
        }
    }
}

/// Whether an agent's job holds a process that isn't the agent or Codync's MCP servers (`own`: the host's own
/// program name, and whatever those servers start). The agent is what leads to them, so anything else that starts
/// nothing further counts: a dev server from its shell tool, but also an MCP server from the user's own config, a
/// shell the agent keeps open, and the agent's own last process once its servers have all gone.
// ponytail: those last three keep their agent as before; tell them from a dev server by the agent's config, or by
// remembering which process started the servers, if it matters.
#[cfg(any(windows, test))]
fn runs_something_else(processes: &[JobProcess], own: &str) -> bool {
    let ours = process::under_program(processes, own);
    processes.iter().filter(|p| !process::is_console(p) && !ours(p)).any(|p| process::starts_nothing(processes, p))
}

/// `processes`: what's on the computer; `session` finds a live process's session. One that exited since the listing
/// holds nothing.
#[cfg(any(unix, test))]
fn detached_descendant(processes: &[JobProcess], leader: u32, session: impl Fn(u32) -> Option<i32>) -> bool {
    let Some(own) = session(leader) else { return true };
    process::descendants(processes, leader).iter().any(|p| session(p.pid).is_some_and(|sid| sid != own))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// (pid, parent) rows.
    fn rows(pairs: &[(u32, u32)]) -> Vec<JobProcess> {
        pairs.iter().map(|&(pid, parent)| JobProcess { pid, parent, name: "x".into() }).collect()
    }

    #[test]
    fn only_work_in_another_session_holds_the_agent() {
        // 10 agent -> 11 codex -> 12 MCP server in a group of its own; all in the host's session 5.
        let sessions: HashMap<u32, i32> = [(10, 5), (11, 5), (12, 5), (20, 20), (21, 20), (30, 30)].into();
        let session = |pid| sessions.get(&pid).copied();
        let agent = [(10, 1), (11, 10), (12, 11), (40, 1)];
        assert!(!detached_descendant(&rows(&agent), 10, session));
        // The shell tool runs `npm run dev` in a session of its own.
        let mut busy = agent.to_vec();
        busy.extend([(20, 11), (21, 20)]);
        assert!(detached_descendant(&rows(&busy), 10, session));
        // Someone else's process in another session isn't the agent's.
        assert!(!detached_descendant(&rows(&[(10, 1), (30, 1)]), 10, session));
        // A child that exited since the listing holds nothing; an agent that can't be looked up is unsure.
        assert!(!detached_descendant(&rows(&[(10, 1), (50, 10)]), 10, session));
        assert!(detached_descendant(&rows(&agent), 99, session));
    }

    #[test]
    fn on_windows_only_what_isnt_the_agent_or_codyncs_servers_holds_the_agent() {
        let own = "codync-host.exe";
        let process = |pid, parent, name: &str| JobProcess { pid, parent, name: name.to_owned() };
        // cmd -> node (adapter) -> codex (engine) -> Codync's MCP servers, with a console window.
        let agent = vec![
            process(1, 900, "cmd.exe"),
            process(2, 1, "conhost.exe"),
            process(3, 1, "node.exe"),
            process(4, 3, "codex.exe"),
            process(5, 4, "CODYNC-HOST.EXE"),
            process(6, 4, "codync-host.exe"),
        ];
        assert!(!runs_something_else(&agent, own));
        assert!(!runs_something_else(&[], own), "nothing left in the job");

        // A dev server from the shell tool, alive or with the shell that started it gone.
        let mut server = agent.clone();
        server.extend([process(7, 4, "powershell.exe"), process(8, 7, "node.exe")]);
        assert!(runs_something_else(&server, own));
        let mut orphan = agent.clone();
        orphan.push(process(8, 777, "node.exe"));
        assert!(runs_something_else(&orphan, own));
        // A console window of its own doesn't hide it.
        let mut windowed = agent.clone();
        windowed.extend([process(8, 4, "node.exe"), process(9, 8, "conhost.exe")]);
        assert!(runs_something_else(&windowed, own));

        // What Codync's own servers start is theirs, like a connector they launch.
        let mut connector = agent.clone();
        connector.extend([process(10, 5, "cmd.exe"), process(11, 10, "node.exe")]);
        assert!(!runs_something_else(&connector, own));

        // Pid links that loop (a reused pid) end the walk, and an agent with no server left is unsure.
        assert!(runs_something_else(&[process(1, 2, "a.exe"), process(2, 1, "b.exe"), process(3, 1, "c.exe")], own));
        assert!(runs_something_else(&[process(1, 900, "cmd.exe"), process(3, 1, "node.exe")], own));
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
