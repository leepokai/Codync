//! Idle agents give their resources back. A session unused for [`IDLE`] is closed (a routine run's
//! as soon as the run ends); once every session is idle, or a routine run ends with none other open,
//! the agent stops with its process group.
//! The next message starts the agent again and reopens the session, so the conversation carries on.
//! Agents that can't reopen a session keep everything: stopping them would lose the conversation.
//! An agent whose keeper grants are gone (a cloud computer's keeper started again) starts again too.

use super::Actor;
use super::background_work::background_work;
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
    /// The agent process, with everything in its process group.
    Agent,
}

/// `used`: each live session's last use. Only an agent that can close sessions releases them one
/// by one; any agent that can reopen them is stopped once all are idle.
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
        if conn.sessions.reopen.is_none() {
            return;
        }
        let release = plan(&conn.loaded, conn.started, conn.sessions.close, Instant::now());
        if release == Release::Nothing || background_work(&conn.acp).await {
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
    /// so a bot on a short schedule doesn't hold a set of MCP servers per run until [`IDLE`].
    pub(super) async fn release_spent(&mut self, sid: &str) {
        let Some(conn) = &self.conn else { return };
        if conn.sessions.reopen.is_none() {
            return;
        }
        let alone = conn.loaded.keys().all(|s| s == sid) && self.queue.is_empty();
        if alone && !background_work(&conn.acp).await {
            tracing::info!(bot = %self.cfg.id, "stopping the agent after its routine run");
            if let Some(conn) = self.conn.take() {
                conn.acp.kill().await;
            }
        } else {
            self.close_reopenable(sid).await;
        }
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
    fn a_keeper_restart_restarts_only_agents_with_grants() {
        assert_eq!(after_keeper(false, false, false), AfterKeeper::Nothing);
        assert_eq!(after_keeper(true, true, true), AfterKeeper::BeforeNextTurn, "resumes its session");
        assert_eq!(after_keeper(true, false, true), AfterKeeper::AfterTurn, "the running turn ends first");
        assert_eq!(after_keeper(true, false, false), AfterKeeper::Now);
    }
}
