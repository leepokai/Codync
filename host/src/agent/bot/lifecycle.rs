//! Idle agents give their resources back. A session unused for [`IDLE`] is closed (a routine run's
//! as soon as the run ends); once every session is idle, or a routine run ends with none other open
//! or with its session left open (an agent that can't close one), the agent stops with everything it started.
//! The next message starts the agent again and reopens the session, so the conversation carries on.
//! Agents that can't reopen a session keep everything: stopping them would lose the conversation.
//! An agent whose keeper grants are gone (a cloud computer's keeper started again) starts again too.

use super::Actor;
use super::background_work::background_work;
use crate::analytics::Retired;
use serde_json::json;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Coming back costs a relaunch plus reopening the session (seconds), so ordinary pauses in a chat
/// stay warm.
const IDLE: Duration = Duration::from_mins(30);
/// How often an idle bot looks for something to give back.
const CHECK_EVERY: Duration = Duration::from_mins(1);

/// What a keeper restart does to an agent, whose grants went with the keeper that gave them.
#[derive(Debug, PartialEq)]
enum AfterKeeper {
    /// It holds none.
    Nothing,
    /// It starts again before its next turn, resuming its session.
    BeforeNextTurn,
    /// It can't resume a session, so it starts fresh (its keys would fail anyway) once its turn ends,
    AfterTurn,
    /// or now, between turns.
    Now,
}

fn after_keeper(holds_grants: bool, can_reopen: bool, in_turn: bool) -> AfterKeeper {
    match (holds_grants, can_reopen, in_turn) {
        (false, ..) => AfterKeeper::Nothing,
        (true, true, _) => AfterKeeper::BeforeNextTurn,
        (true, false, true) => AfterKeeper::AfterTurn,
        (true, false, false) => AfterKeeper::Now,
    }
}

#[derive(Debug, PartialEq)]
enum Release {
    Nothing,
    /// These sessions; the agent keeps running the rest.
    Sessions(Vec<String>),
    /// The agent process, with everything it started (every process group in its session), and why.
    Agent(Retired),
}

/// `used`: each live session's last use. Only an agent that can close sessions releases them one
/// by one; any agent that can reopen them is stopped once all are idle, or while it holds a `spent`
/// session ([`Actor::stop_spent`] stops it at a turn's end unless something runs detached, then this tries again).
fn plan(used: &HashMap<String, Instant>, started: Instant, can_close: bool, spent: bool, now: Instant) -> Release {
    if spent {
        return Release::Agent(Retired::SessionNotClosed);
    }
    let mut idle: Vec<String> =
        used.iter().filter(|(_, at)| now.duration_since(**at) >= IDLE).map(|(sid, _)| sid.clone()).collect();
    if idle.len() == used.len() && now.duration_since(started) >= IDLE {
        Release::Agent(Retired::Idle)
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
        if conn.sessions.reopen.is_none() {
            return;
        }
        let release = plan(&conn.loaded, conn.started, conn.sessions.close, conn.spent, Instant::now());
        if release == Release::Nothing || background_work(&conn.acp).await {
            return;
        }
        match release {
            Release::Agent(reason) => self.retire(reason).await,
            Release::Sessions(sessions) => {
                for sid in sessions {
                    self.close_session(&sid).await;
                }
            }
            Release::Nothing => {}
        }
    }

    /// A cloud computer's keeper started again (`Cmd::KeeperRestarted`), so the grants the agent started with are gone.
    pub(super) async fn keeper_restarted(&mut self) {
        let Some(conn) = &self.conn else { return };
        match after_keeper(conn.acp.holds_grants(), conn.sessions.reopen.is_some(), self.turn.is_some()) {
            AfterKeeper::Nothing => {}
            AfterKeeper::BeforeNextTurn => self.tools_changed = true,
            AfterKeeper::AfterTurn => self.restart_after_turn = true,
            AfterKeeper::Now => self.restart_agent().await,
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
    /// so a bot on a short schedule doesn't hold a set of MCP servers per run until [`IDLE`]. One the agent couldn't
    /// close (Cursor can't) goes with the agent instead, once nothing runs or waits ([`Self::stop_spent`]): the main
    /// chat reopens its own session on its next turn.
    pub(super) async fn release_spent(&mut self, sid: &str) {
        let Some(conn) = &self.conn else { return };
        if conn.sessions.reopen.is_none() {
            return;
        }
        let alone = conn.loaded.keys().all(|s| s == sid) && self.queue.is_empty();
        if alone && !background_work(&conn.acp).await {
            self.retire(Retired::RoutineFinished).await;
            return;
        }
        self.close_reopenable(sid).await;
        if let Some(conn) = self.conn.as_mut()
            && conn.loaded.contains_key(sid)
        {
            conn.spent = true;
        }
    }

    /// Stops an agent holding a spent session (`release_spent`) once nothing runs or waits, unless something runs
    /// detached under it: then the idle check tries again.
    pub(super) async fn stop_spent(&mut self) {
        let Some(conn) = &self.conn else { return };
        if conn.spent && self.turn.is_none() && self.queue.is_empty() && !background_work(&conn.acp).await {
            self.retire(Retired::SessionNotClosed).await;
        }
    }

    /// Stops the agent with everything it started, to give that back; the next turn starts it again and reopens its
    /// session.
    async fn retire(&mut self, reason: Retired) {
        let Some(conn) = self.conn.take() else { return };
        tracing::info!(bot = %self.cfg.id, ?reason, "stopping the agent");
        crate::analytics::agent_retired(&self.hub, &self.cfg.backend, reason);
        conn.acp.kill().await;
    }

    /// Closes a session the agent can reopen when it's next needed, like one a turn failed in (see
    /// `acp::may_end_session`): the next turn reopens it instead of finding it dead.
    pub(super) async fn close_reopenable(&mut self, sid: &str) {
        if self
            .conn
            .as_ref()
            .is_some_and(|c| c.sessions.close && c.sessions.reopen.is_some() && c.loaded.contains_key(sid))
        {
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
            plan(&used(&[("main", fresh), ("t1", stale)]), started, true, false, now),
            Release::Sessions(vec!["t1".into()])
        );
        assert_eq!(plan(&used(&[("main", fresh), ("t1", stale)]), started, false, false, now), Release::Nothing);
        let idle = Release::Agent(Retired::Idle);
        assert_eq!(plan(&used(&[("main", stale), ("t1", stale)]), started, false, false, now), idle);
        assert_eq!(plan(&used(&[]), started, true, false, now), idle);
        // A just-started agent isn't stopped before it had a chance to be used.
        assert_eq!(plan(&used(&[]), now, true, false, now), Release::Nothing);
        assert_eq!(plan(&used(&[("main", fresh)]), started, true, false, now), Release::Nothing);
    }

    #[test]
    fn a_spent_session_stops_the_agent_however_fresh_its_chat() {
        let now = Instant::now();
        let used = HashMap::from([("main".to_owned(), now), ("run".to_owned(), now)]);
        assert_eq!(plan(&used, now, false, true, now), Release::Agent(Retired::SessionNotClosed));
    }

    #[test]
    fn a_keeper_restart_restarts_only_agents_with_grants() {
        assert_eq!(after_keeper(false, false, false), AfterKeeper::Nothing);
        assert_eq!(after_keeper(true, true, true), AfterKeeper::BeforeNextTurn, "resumes its session");
        assert_eq!(after_keeper(true, false, true), AfterKeeper::AfterTurn, "the running turn ends first");
        assert_eq!(after_keeper(true, false, false), AfterKeeper::Now);
    }
}
