//! Idle agents give their resources back. A session unused for [`IDLE`] is closed (a routine run's
//! as soon as the run ends); once every session is idle, or a routine run ends with none other open,
//! the agent stops with its process group.
//! The next message starts the agent again and loads the session, so the conversation carries on.
//! Agents that can't load a session keep everything: stopping them would lose the conversation.

use super::Actor;
use serde_json::json;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Coming back costs a relaunch plus `session/load` (seconds), so ordinary pauses in a chat
/// stay warm.
const IDLE: Duration = Duration::from_mins(30);
/// How often an idle bot looks for something to give back.
const CHECK_EVERY: Duration = Duration::from_mins(1);

#[derive(Debug, PartialEq)]
enum Release {
    Nothing,
    /// These sessions; the agent keeps running the rest.
    Sessions(Vec<String>),
    /// The agent process, with everything in its process group.
    Agent,
}

/// `used`: each live session's last use. Only an agent that can close sessions releases them one
/// by one; any agent that can load them is stopped once all are idle.
fn plan(used: &HashMap<String, Instant>, started: Instant, can_close: bool, now: Instant) -> Release {
    let mut idle: Vec<String> =
        used.iter().filter(|(_, at)| now.duration_since(**at) >= IDLE).map(|(sid, _)| sid.clone()).collect();
    if idle.len() == used.len() && now.duration_since(started) >= IDLE {
        Release::Agent
    } else if can_close && !idle.is_empty() {
        idle.sort();
        Release::Sessions(idle)
    } else {
        Release::Nothing
    }
}

impl Actor {
    /// Runs on the actor's tick: at most every [`CHECK_EVERY`], and only while the bot has nothing
    /// running, queued or waiting on the user. A compaction due while idle goes first.
    pub(super) async fn release_idle(&mut self) {
        if self.idle_checked.elapsed() < CHECK_EVERY || !self.at_rest() {
            return;
        }
        self.idle_checked = Instant::now();
        self.tend_idle();
        if !self.queue.is_empty() {
            return;
        }
        let Some(conn) = &self.conn else { return };
        if !conn.sessions.load {
            return;
        }
        let release = plan(&conn.loaded, conn.started, conn.sessions.close, Instant::now());
        let leader = conn.acp.pid();
        if release == Release::Nothing || background_work(leader).await {
            return;
        }
        match release {
            Release::Agent => {
                tracing::info!(bot = %self.cfg.id, "stopping an idle agent");
                if let Some(conn) = self.conn.take() {
                    conn.acp.kill().await;
                }
            }
            Release::Sessions(sessions) => {
                for sid in sessions {
                    self.close_session(&sid).await;
                }
            }
            Release::Nothing => {}
        }
    }

    fn at_rest(&self) -> bool {
        self.turn.is_none()
            && self.queue.is_empty()
            && self.perms.is_empty()
            && self.active_request.is_none()
            && self.active_group.is_none()
            && self.active_routine.is_none()
            && self.routine_completion.is_none()
            && self.cancel_deadline.is_none()
    }

    /// A routine run's session is spent once the run is. When it was the only session open and nothing waits, the
    /// agent ran just for the run and stops with it, with everything it started; otherwise the session alone closes,
    /// so a bot on a short schedule doesn't hold a set of MCP servers per run until [`IDLE`].
    pub(super) async fn release_spent(&mut self, sid: &str) {
        let Some(conn) = &self.conn else { return };
        if !conn.sessions.load {
            return;
        }
        let alone = conn.loaded.keys().all(|s| s == sid) && self.queue.is_empty();
        if alone && !background_work(conn.acp.pid()).await {
            tracing::info!(bot = %self.cfg.id, "stopping the agent after its routine run");
            if let Some(conn) = self.conn.take() {
                conn.acp.kill().await;
            }
        } else {
            self.close_reloadable(sid).await;
        }
    }

    /// Closes a session the agent can load again when it's next needed, like one a turn failed in (see
    /// `acp::may_end_session`): the next turn loads it afresh instead of finding it dead.
    pub(super) async fn close_reloadable(&mut self, sid: &str) {
        if self.conn.as_ref().is_some_and(|c| c.sessions.close && c.sessions.load && c.loaded.contains_key(sid)) {
            self.close_session(sid).await;
        }
    }

    async fn close_session(&mut self, sid: &str) {
        let closed = self.session_request("session/close", json!({ "sessionId": sid })).await;
        let Some(conn) = self.conn.as_mut() else { return };
        match closed {
            Ok(_) => {
                tracing::info!(bot = %self.cfg.id, session = sid, "closed a session");
                conn.loaded.remove(sid);
                conn.fast_options.remove(sid);
                conn.modes.remove(sid);
                conn.mode_set.remove(sid);
                conn.usage.remove(sid);
                conn.renamed.remove(sid);
                self.applied_system.remove(sid);
            }
            Err(error) => {
                tracing::warn!(bot = %self.cfg.id, session = sid, error = format!("{error:#}"), "couldn't close a session");
                // Try again after another idle period rather than every check.
                conn.loaded.insert(sid.to_owned(), Instant::now());
            }
        }
    }
}

/// Whether something the agent started runs detached, in a session of its own, like a dev server
/// from its shell tool. Stopping the agent would orphan it and lose track of it, so the agent
/// stays. MCP servers stay in the agent's session and don't count, even in a process group of
/// their own (Codex starts each one in its own group). Unsure counts as busy.
// ponytail: a dev server keeps its agent (and memory) indefinitely; add a cap if that matters.
#[cfg(unix)]
async fn background_work(leader: Option<u32>) -> bool {
    let Some(leader) = leader else { return true };
    let listed = tokio::process::Command::new("ps").args(["-A", "-o", "pid=,ppid="]).output().await;
    let Ok(listed) = listed.inspect_err(|e| tracing::warn!(error = format!("{e:#}"), "couldn't list processes")) else {
        return true;
    };
    if !listed.status.success() {
        return true;
    }
    let processes: Vec<(u32, u32)> = String::from_utf8_lossy(&listed.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace().map(str::parse);
            Some((fields.next()?.ok()?, fields.next()?.ok()?))
        })
        .collect();
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

/// Windows has no process groups to tell a dev server from an MCP server, and stopping the agent
/// ends its whole process tree there, so its agents are never released.
#[cfg(windows)]
#[allow(clippy::unused_async)] // the Unix check's signature
async fn background_work(_leader: Option<u32>) -> bool {
    true
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
    fn idle_sessions_close_one_by_one_and_an_idle_agent_stops() {
        let started = Instant::now();
        let now = started + IDLE * 4;
        let (stale, fresh) = (started + IDLE * 2, now);
        let used =
            |rows: &[(&str, Instant)]| rows.iter().map(|(s, at)| ((*s).to_owned(), *at)).collect::<HashMap<_, _>>();
        assert_eq!(
            plan(&used(&[("main", fresh), ("t1", stale)]), started, true, now),
            Release::Sessions(vec!["t1".into()])
        );
        assert_eq!(plan(&used(&[("main", fresh), ("t1", stale)]), started, false, now), Release::Nothing);
        assert_eq!(plan(&used(&[("main", stale), ("t1", stale)]), started, false, now), Release::Agent);
        assert_eq!(plan(&used(&[]), started, true, now), Release::Agent);
        // A just-started agent isn't stopped before it had a chance to be used.
        assert_eq!(plan(&used(&[]), now, true, now), Release::Nothing);
        assert_eq!(plan(&used(&[("main", fresh)]), started, true, now), Release::Nothing);
    }

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
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while !background_work(detached.id()).await {
            assert!(tokio::time::Instant::now() < deadline, "the detached child never showed up");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(!background_work(plain.id()).await);
        assert!(!background_work(grouped.id()).await);
        for child in [plain, grouped, detached] {
            if let Some(pid) = child.id() {
                let _ = std::process::Command::new("/bin/kill").args(["-KILL", "--", &format!("-{pid}")]).status();
            }
        }
    }
}
