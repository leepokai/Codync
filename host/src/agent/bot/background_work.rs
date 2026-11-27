//! Whether an agent still runs something that stopping it would end, which keeps it running when it's idle, whether
//! it still runs Codync's MCP servers, and what a thread's turn left running that its task waits on (#634).

use super::{Actor, Done};
use codync_agent::acp::{Acp, StopReason};
use codync_agent::process::{self, JobProcess};

/// Whether one of Codync's MCP servers (`codync-host mcp`) still runs under the agent. OpenCode 2.x drops them from a
/// live session once it has been idle a while (#508). Unsure counts as running.
#[cfg(unix)]
pub(super) async fn codync_servers_run(acp: &Acp) -> bool {
    let Some(leader) = acp.pid() else { return true };
    match process::list_processes().await {
        Ok(processes) => runs_host(&process::descendants(&processes, leader), host_program().as_deref()),
        Err(error) => {
            tracing::warn!(error = format!("{error:#}"), "couldn't list processes");
            true
        }
    }
}

#[cfg(windows)]
pub(super) async fn codync_servers_run(acp: &Acp) -> bool {
    match acp.job_processes().await {
        Ok(processes) => runs_host(&processes, host_program().as_deref()),
        Err(error) => {
            tracing::warn!(error = format!("{error:#}"), "couldn't list the agent's processes");
            true
        }
    }
}

/// Whether `processes` hold one running the host's own program (`host`; unknown counts as running).
fn runs_host(processes: &[JobProcess], host: Option<&str>) -> bool {
    host.is_none_or(|host| processes.iter().any(|p| process::program_name(&p.name).eq_ignore_ascii_case(host)))
}

fn host_program() -> Option<String> {
    std::env::current_exe().ok()?.file_name()?.to_str().map(str::to_owned)
}

/// Whether stopping an agent that dropped Codync's MCP servers (#508) would end work it runs detached, so it stays.
/// Windows can't tell: once the servers are gone, the agent's own engine or an MCP server from the user's config looks
/// like a dev server there ([`runs_something_else`]), so the agent starts again regardless.
#[cfg(unix)]
pub(super) async fn holds_restart(acp: &Acp) -> bool {
    background_work(acp).await
}

#[cfg(windows)]
#[allow(clippy::unused_async, reason = "the Unix one awaits")]
pub(super) async fn holds_restart(_acp: &Acp) -> bool {
    false
}

/// Whether something the agent started runs detached, in a session of its own, like a dev server
/// from its shell tool. Stopping the agent would orphan it and lose track of it, so the agent
/// stays. MCP servers stay in the agent's session and don't count, even in a process group of
/// their own (Codex starts each one in its own group), and neither does a session the agent's
/// own program or one of Codync's MCP servers leads (OpenCode 2.x, #515). Unsure counts as busy.
// ponytail: a dev server keeps its agent (and memory) indefinitely; add a cap if that matters.
#[cfg(unix)]
pub(super) async fn background_work(acp: &Acp) -> bool {
    detached_work(acp.pid()).await
}

#[cfg(unix)]
async fn detached_work(leader: Option<u32>) -> bool {
    detached_sessions_of(leader).await.is_none_or(|sessions| !sessions.is_empty())
}

/// The sessions of what the agent led by `leader` runs detached ([`detached_sessions`]); None when unsure.
#[cfg(unix)]
async fn detached_sessions_of(leader: Option<u32>) -> Option<Vec<i32>> {
    let (leader, host) = (leader?, host_program()?);
    let listed = process::list_processes().await;
    let processes = listed.inspect_err(|e| tracing::warn!(error = format!("{e:#}"), "couldn't list processes")).ok()?;
    detached_sessions(&processes, leader, &host, running_session)
}

/// `pid`'s session while it runs. A zombie is work that ended and its parent hasn't reaped yet: macOS gives no session
/// for it, Linux still does.
// ponytail: the crate's `process::session` belongs to answer None for a Linux zombie too; this stays here so the fix
// costs no codync-agent release, and moves there with its next change.
#[cfg(unix)]
fn running_session(pid: u32) -> Option<i32> {
    #[cfg(target_os = "linux")]
    if std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .is_ok_and(|stat| stat.rsplit_once(") ").is_some_and(|(_, state)| state.starts_with('Z')))
    {
        return None;
    }
    process::session(pid)
}

/// The sessions `acp` runs detached in now ([`detached_sessions`]), which aren't what a thread's task leaves running
/// later ([`Actor::left_running`]). None when unsure, and on Windows.
#[cfg(unix)]
async fn detached_now(acp: &Acp) -> Option<Vec<i32>> {
    detached_sessions_of(acp.pid()).await
}

#[cfg(windows)]
#[allow(clippy::unused_async, reason = "the Unix one awaits")]
async fn detached_now(_acp: &Acp) -> Option<Vec<i32>> {
    None
}

/// Whether a thread's agent left something running detached that its task may wait on: a session of its own
/// ([`background_work`]) that isn't one of `before` (already there when its task's round began), where nothing
/// listens. A server left listening is meant to stay, and shows among the bot's dev servers. Unsure counts as
/// nothing, so no task waits on a guess.
#[cfg(unix)]
async fn left_detached(acp: &Acp, before: &[i32]) -> bool {
    left_detached_under(acp.pid(), before).await
}

#[cfg(unix)]
async fn left_detached_under(leader: Option<u32>, before: &[i32]) -> bool {
    let Some(sessions) = detached_sessions_of(leader).await else { return false };
    let sessions: Vec<i32> = sessions.into_iter().filter(|sid| !before.contains(sid)).collect();
    if sessions.is_empty() {
        return false;
    }
    let listening = tokio::task::spawn_blocking(crate::dev_servers::ports::listening).await;
    let Ok(Ok(listening)) = listening else { return false };
    let serving: Vec<i32> = listening.iter().filter_map(|l| process::session(l.pid)).collect();
    sessions.iter().any(|sid| !serving.contains(sid))
}

/// Windows has no sessions to tell what the agent left running from its own servers ([`runs_something_else`]): unsure,
/// so nothing.
#[cfg(windows)]
#[allow(clippy::unused_async, reason = "the Unix one awaits")]
async fn left_detached(_acp: &Acp, _before: &[i32]) -> bool {
    false
}

impl Actor {
    /// A thread's turn begins: what its agent already runs detached isn't what its task leaves running, unless the
    /// task's round began earlier and goes on with what a turn left running ([`Self::turn_over`]).
    pub(super) async fn note_detached(&mut self) {
        if self.cfg.is_thread()
            && self.detached_before.is_none()
            && let Some(conn) = &self.conn
        {
            self.detached_before = Some(detached_now(&conn.acp).await.unwrap_or_default());
        }
    }

    /// What a thread's turn that just ended normally (`done`) left running in the background, which its task waits on,
    /// in words that fit "while … still runs": a subagent Claude's adapter launched this turn to run in the
    /// background (`isAsync`), or something its agent runs detached since its task's round began
    /// ([`left_detached`]). None for any other turn.
    pub(super) async fn left_running(&self, done: &Done) -> Option<String> {
        let ended = done.as_ref().is_ok_and(|v| StopReason::of(v) == StopReason::EndTurn);
        if !self.cfg.is_thread() || self.stop_requested || !ended {
            return None;
        }
        let subagents = self
            .tools
            .values()
            .filter_map(|id| self.hub.store.entry(id))
            .filter(|e| e.data["subagent"]["background"] == true)
            .count();
        let mut left = Vec::new();
        match subagents {
            0 => {}
            1 => left.push("a subagent launched in the background".to_owned()),
            n => left.push(format!("{n} subagents launched in the background")),
        }
        if let Some(conn) = &self.conn
            && left_detached(&conn.acp, self.detached_before.as_deref().unwrap_or_default()).await
        {
            left.push("a command started in the background".to_owned());
        }
        (!left.is_empty()).then(|| left.join(" and "))
    }
}

/// Windows has no sessions to tell a dev server from an MCP server, and stopping the agent ends everything in its
/// job there, so the job's processes are judged by what they are ([`runs_something_else`]). Unsure counts as busy.
#[cfg(windows)]
pub(super) async fn background_work(acp: &Acp) -> bool {
    let Some(own) = host_program() else { return true };
    match acp.job_processes().await {
        Ok(processes) => runs_something_else(&processes, &own),
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

/// The sessions what the agent led by `leader` runs detached in, each once; None when the agent's own can't be looked
/// up. `processes`: what's on the computer; `session` finds a live process's session. One that exited since the listing
/// holds nothing. In a session of its own, these are still the agent's: what runs the agent's own program and what it
/// starts in that session, Codync's MCP servers and what they start (`host`: the host's program name), and a process
/// the agent's program starts to lead a session of its own, without what that one starts. OpenCode 2.x starts its
/// engine (`opencode serve --stdio`), each MCP server and its terminal daemon (`opencode-pty daemon`) that way (#515),
/// and a dev server from its shell tool runs under that daemon, so it still counts.
// ponytail: matched by program name, so an agent behind a launcher of another name (an npm shim) still keeps its
// engine, a generic runtime (node, python) never exempts its own name, and what an MCP server from the user's own agent
// config starts still holds the agent; match the launched program or the agent's config if one such shows up.
#[cfg(any(unix, test))]
fn detached_sessions(
    processes: &[JobProcess],
    leader: u32,
    host: &str,
    session: impl Fn(u32) -> Option<i32>,
) -> Option<Vec<i32>> {
    let own = session(leader)?;
    let agent = processes
        .iter()
        .find(|p| p.pid == leader)
        .map(|p| process::program_name(&p.name))
        .filter(|name| !is_runtime(name));
    let runs_agent = |p: &JobProcess| agent.is_some_and(|agent| process::program_name(&p.name) == agent);
    let descendants = process::descendants(processes, leader);
    let codync = process::under_program(&descendants, host);
    let find = |pid: u32| descendants.iter().find(|p| p.pid == pid);
    let agents_own = |p: &JobProcess, sid: i32| {
        let leads = u32::try_from(sid) == Ok(p.pid);
        let started_by_agent = find(p.parent).is_some_and(runs_agent);
        if leads {
            runs_agent(p) || started_by_agent
        } else {
            u32::try_from(sid).ok().and_then(find).is_some_and(runs_agent)
        }
    };
    let mut sessions: Vec<i32> = descendants
        .iter()
        .filter_map(|p| session(p.pid).filter(|&sid| sid != own && !codync(p) && !agents_own(p, sid)))
        .collect();
    sessions.sort_unstable();
    sessions.dedup();
    Some(sessions)
}

/// A shell or an interpreter (`python3.12` too, and `Python`, as macOS names a framework build's process): as an
/// agent's program it says nothing about what else running it is the agent's.
#[cfg(any(unix, test))]
fn is_runtime(program: &str) -> bool {
    let program = program.to_ascii_lowercase();
    let program = program.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    ["node", "bun", "deno", "python", "ruby", "perl", "sh", "bash", "zsh", "dash", "fish"].contains(&program)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const HOST: &str = "codync-host";

    fn detached_descendant(
        processes: &[JobProcess],
        leader: u32,
        host: &str,
        session: impl Fn(u32) -> Option<i32>,
    ) -> bool {
        detached_sessions(processes, leader, host, session).is_none_or(|sessions| !sessions.is_empty())
    }

    fn process(pid: u32, parent: u32, name: &str) -> JobProcess {
        JobProcess { pid, parent, name: name.to_owned() }
    }

    /// (pid, parent) rows, each its own program.
    fn rows(pairs: &[(u32, u32)]) -> Vec<JobProcess> {
        pairs.iter().map(|&(pid, parent)| process(pid, parent, &format!("p{pid}"))).collect()
    }

    #[test]
    fn only_work_in_another_session_holds_the_agent() {
        // 10 agent -> 11 codex -> 12 MCP server in a group of its own; all in the host's session 5.
        let sessions: HashMap<u32, i32> = [(10, 5), (11, 5), (12, 5), (20, 20), (21, 20), (30, 30)].into();
        let session = |pid| sessions.get(&pid).copied();
        let agent = [(10, 1), (11, 10), (12, 11), (40, 1)];
        assert!(!detached_descendant(&rows(&agent), 10, HOST, session));
        // The shell tool runs `npm run dev` in a session of its own.
        let mut busy = agent.to_vec();
        busy.extend([(20, 11), (21, 20)]);
        assert!(detached_descendant(&rows(&busy), 10, HOST, session));
        // Someone else's process in another session isn't the agent's.
        assert!(!detached_descendant(&rows(&[(10, 1), (30, 1)]), 10, HOST, session));
        // A child that exited since the listing holds nothing; an agent that can't be looked up is unsure.
        assert!(!detached_descendant(&rows(&[(10, 1), (50, 10)]), 10, HOST, session));
        assert!(detached_descendant(&rows(&agent), 99, HOST, session));
    }

    /// #515: OpenCode 2.x (`opencode acp`) starts its engine, `opencode serve --stdio`, in a session of its own right
    /// after `initialize`, and the engine starts each MCP server in a session of its own (seen with the registry build
    /// 2.3.2 on macOS).
    #[test]
    fn opencodes_own_engine_isnt_background_work() {
        let oc = "/Users/me/.codync/agents/opencode/opencode";
        let host = "/Applications/Codync.app/Contents/Resources/codync-host";
        // 10 `opencode acp` -> 11 `opencode serve --stdio` -> 12 `codync-host mcp chat` -> 13 a connector it starts.
        let sessions: HashMap<u32, i32> = [(10, 10), (11, 11), (12, 12), (13, 12), (20, 20), (21, 20)].into();
        let session = |pid| sessions.get(&pid).copied();
        let idle = vec![process(10, 1, oc), process(11, 10, oc), process(12, 11, host), process(13, 12, "node")];
        assert!(!detached_descendant(&idle, 10, HOST, session), "an idle OpenCode bot has no background work");
        // A dev server its shell tool starts in a session of its own still holds it.
        let mut busy = idle.clone();
        busy.extend([process(20, 11, "/bin/zsh"), process(21, 20, "node")]);
        assert!(detached_descendant(&busy, 10, HOST, session));
    }

    /// #515, seen end to end: once a bot has run a shell command, OpenCode 2.x keeps a terminal daemon
    /// (`opencode-pty daemon`) in a session of its own, and an MCP server from the user's OpenCode config leads one too.
    /// Neither holds the bot; a background command the shell tool starts runs under the daemon and does.
    #[test]
    fn opencodes_terminal_daemon_and_config_servers_arent_background_work() {
        let oc = "/Users/me/.codync/agents/opencode/opencode";
        let pty = "/Users/me/.cache/opencode/bin/opencode-pty/0.6.0/opencode-pty";
        // 10 acp -> 11 engine -> 30 pty daemon, 40 the user's `cua-driver mcp`; 31 a server under the daemon.
        let sessions: HashMap<u32, i32> = [(10, 10), (11, 11), (30, 30), (31, 30), (40, 40)].into();
        let session = |pid| sessions.get(&pid).copied();
        let idle = vec![process(10, 1, oc), process(11, 10, oc), process(30, 11, pty), process(40, 11, "cua-driver")];
        assert!(!detached_descendant(&idle, 10, HOST, session), "an idle OpenCode bot that ran a shell command");
        let mut busy = idle.clone();
        busy.push(process(31, 30, "python3"));
        assert!(detached_descendant(&busy, 10, HOST, session), "a background server from the shell tool");
    }

    /// A runtime's name tells nothing: a dev server `node` leads in a session of its own holds a `node` agent.
    #[test]
    fn a_runtime_agent_doesnt_exempt_its_runtime() {
        let sessions: HashMap<u32, i32> = [(10, 10), (11, 11)].into();
        let session = |pid| sessions.get(&pid).copied();
        let agent = [process(10, 1, "/usr/local/bin/node"), process(11, 10, "node")];
        assert!(detached_descendant(&agent, 10, HOST, session));
        assert!(is_runtime("python3.12") && !is_runtime("opencode"));
        // A framework Python on macOS runs as `…/Python.app/Contents/MacOS/Python`.
        let framework =
            "/opt/homebrew/Frameworks/Python.framework/Versions/3.13/Resources/Python.app/Contents/MacOS/Python";
        let agent = [process(10, 1, framework), process(11, 10, framework)];
        assert!(detached_descendant(&agent, 10, HOST, session));
    }

    /// #508: OpenCode 2.x evicts a directory's services after `experimental.location_idle_timeout` and takes the MCP
    /// servers a session was opened with along; the engine and the user's own servers come back without them.
    #[test]
    fn opencode_that_dropped_codyncs_servers_is_seen() {
        let oc = "/Users/me/.codync/agents/opencode/opencode";
        let host = "/Applications/Codync.app/Contents/Resources/codync-host";
        // 11 `opencode serve --stdio` under 10 `opencode acp`, with Codync's servers and the user's own.
        let mut tree = vec![process(11, 10, oc), process(40, 11, "cua-driver")];
        let evicted = tree.clone();
        tree.extend([process(12, 11, host), process(13, 11, host)]);
        assert!(runs_host(&tree, Some(HOST)));
        assert!(!runs_host(&evicted, Some(HOST)), "after the eviction only the user's own server runs");
        assert!(runs_host(&[process(5, 4, "CODYNC-HOST.EXE")], Some("codync-host.exe")), "Windows names");
        assert!(runs_host(&evicted, None), "unsure counts as running");
    }

    #[test]
    fn on_windows_only_what_isnt_the_agent_or_codyncs_servers_holds_the_agent() {
        let own = "codync-host.exe";
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
        // #634: what a thread's task waits on, unless it serves (a dev server left listening is meant to stay).
        assert!(left_detached_under(detached.id(), &[]).await);
        let before = detached_sessions_of(detached.id()).await.expect("its sessions");
        assert!(!left_detached_under(detached.id(), &before).await, "there before its task's round began");
        let serving = spawn(
            "use POSIX (); use IO::Socket::INET; my $pid = fork; if (!$pid) { POSIX::setsid(); \
             my $s = IO::Socket::INET->new(LocalAddr => '127.0.0.1', LocalPort => 0, Listen => 1) or die; sleep 30; \
             exit } sleep 30",
        );
        while !detached_work(serving.id()).await {
            assert!(tokio::time::Instant::now() < deadline, "the serving child never showed up");
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        while left_detached_under(serving.id(), &[]).await {
            assert!(tokio::time::Instant::now() < deadline, "a detached server still reads as work its task waits on");
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(!left_detached_under(None, &[]).await, "unsure is nothing");
        for child in [plain, grouped, detached, serving] {
            if let Some(pid) = child.id() {
                let _ = std::process::Command::new("/bin/kill").args(["-KILL", "--", &format!("-{pid}")]).status();
            }
        }
    }
}
