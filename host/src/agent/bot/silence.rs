//! A turn whose agent goes silent ends instead of waiting forever. ACP has no keepalive: an agent that hangs on its
//! provider (OpenCode on an exhausted OpenAI-compatible endpoint, anomalyco/opencode#47804) answers neither
//! `session/prompt` nor its own `session/cancel`, so the bot stayed on "Thinking…" until the user pressed Stop. When
//! the agent owes the turn its next word and sends nothing for [`limit`], its process stops and the turn fails with
//! `FailureKind::Timeout`, which moves the bot on to its fallback like any agent that stopped answering.
//! Time spent waiting on the user (an approval card) or on a running tool doesn't count: only the agent's own silence.

use super::{Actor, Done};
use crate::agent::acp::{Failure, FailureKind};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

/// Minutes of silence the watchdog allows; `0` turns it off.
const LIMIT_VAR: &str = "CODYNC_AGENT_SILENCE_MINUTES";
/// Long enough for a model that reasons a while before it streams anything.
const DEFAULT_LIMIT: Duration = Duration::from_mins(10);

static LIMIT: LazyLock<Option<Duration>> = LazyLock::new(|| limit_from(std::env::var(LIMIT_VAR).ok().as_deref()));

/// How long an agent may stay silent mid-turn; `None` when the user turned the watchdog off.
fn limit_from(value: Option<&str>) -> Option<Duration> {
    let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else { return Some(DEFAULT_LIMIT) };
    match value.parse::<u64>() {
        Ok(0) => None,
        Ok(minutes) => Some(Duration::from_secs(minutes.saturating_mul(60))),
        Err(error) => {
            tracing::warn!(var = LIMIT_VAR, value, %error, "not a whole number of minutes; using the default");
            Some(DEFAULT_LIMIT)
        }
    }
}

/// Who the running turn waits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Waiting {
    Agent,
    /// An approval card: the agent may be quiet as long as the user takes.
    User,
}

/// The watchdog's view of the running turn.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) enum Watch {
    /// No turn waits on the agent.
    #[default]
    Off,
    /// The turn waits on the agent, which last spoke at this instant.
    Listening(Instant),
    /// The agent said nothing for this long and its process was stopped; the turn ends with that failure.
    Stopped(Duration),
}

impl Watch {
    /// The agent sent something.
    pub(super) fn heard(&mut self, now: Instant) {
        if let Self::Listening(at) = self {
            *at = now;
        }
    }

    /// One tick: how long the agent has been silent, once that reaches `limit`. `waiting` is `None` with no turn.
    fn tick(&mut self, waiting: Option<Waiting>, limit: Duration, now: Instant) -> Option<Duration> {
        match (waiting, &*self) {
            (None, _) => *self = Self::Off,
            (Some(_), Self::Stopped(_)) => {}
            (Some(Waiting::User), _) | (Some(Waiting::Agent), Self::Off) => *self = Self::Listening(now),
            (Some(Waiting::Agent), Self::Listening(at)) => {
                let silent = now.saturating_duration_since(*at);
                return (silent >= limit).then_some(silent);
            }
        }
        None
    }

    /// The turn's prompt answered with `done`: a failure after the watchdog stopped the agent says why it stopped.
    pub(super) fn end(&mut self, done: Done) -> Done {
        match (std::mem::take(self), done) {
            (Self::Stopped(silent), Err(_)) => Err(Failure::error(
                FailureKind::Timeout,
                format!("it went silent for {} min, so Codync stopped it", silent.as_secs() / 60),
            )),
            (_, done) => done,
        }
    }
}

impl Actor {
    /// Runs on the actor's tick: stops an agent that has been silent past the limit while its turn waits on it.
    pub(super) async fn watch_silence(&mut self) {
        let Some(limit) = *LIMIT else { return };
        let waiting = match self.turn {
            None => None,
            // A Stop or a routine's time limit already ends this turn its own way.
            Some(_) if self.stop_requested || self.routine_timed_out => return,
            Some(_) if !self.perms.is_empty() => Some(Waiting::User),
            Some(_) => Some(Waiting::Agent),
        };
        let now = Instant::now();
        let Some(silent) = self.silence.tick(waiting, limit, now) else { return };
        // An agent that already exited ends the turn by itself.
        if self.conn.is_none() {
            return;
        }
        // A long build or test run reports nothing until it's done: that silence is the tool's, not the agent's.
        if self.tool_running() {
            self.silence.heard(now);
            return;
        }
        let Some(conn) = self.conn.take() else { return };
        tracing::warn!(
            bot = %self.cfg.id,
            agent = %self.cfg.backend,
            silent_secs = silent.as_secs(),
            "the agent went silent mid-turn; stopping it"
        );
        self.silence = Watch::Stopped(silent);
        conn.acp.kill().await;
    }

    /// One of this turn's tool calls is still pending or in progress.
    fn tool_running(&self) -> bool {
        self.tools
            .values()
            .filter_map(|id| self.hub.store.entry(id))
            .any(|e| matches!(e.data["status"].as_str(), Some("pending" | "in_progress")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::fallback::{Reason, reason};
    use anyhow::anyhow;
    use serde_json::json;

    const LIMIT: Duration = Duration::from_mins(10);

    #[test]
    fn a_silent_agent_is_caught_once_the_limit_passes() {
        let start = Instant::now();
        let mut watch = Watch::default();
        assert_eq!(watch.tick(Some(Waiting::Agent), LIMIT, start), None, "the first tick of a turn arms it");
        assert_eq!(watch.tick(Some(Waiting::Agent), LIMIT, start + LIMIT / 2), None);
        let late = start + LIMIT + Duration::from_secs(1);
        assert_eq!(watch.tick(Some(Waiting::Agent), LIMIT, late), Some(LIMIT + Duration::from_secs(1)));
    }

    #[test]
    fn anything_the_agent_sends_restarts_the_clock() {
        let start = Instant::now();
        let mut watch = Watch::default();
        watch.tick(Some(Waiting::Agent), LIMIT, start);
        watch.heard(start + LIMIT / 2);
        assert_eq!(watch.tick(Some(Waiting::Agent), LIMIT, start + LIMIT), None);
        assert!(watch.tick(Some(Waiting::Agent), LIMIT, start + LIMIT / 2 + LIMIT).is_some());
    }

    #[test]
    fn waiting_on_the_user_never_counts_against_the_agent() {
        let start = Instant::now();
        let mut watch = Watch::default();
        watch.tick(Some(Waiting::Agent), LIMIT, start);
        // An approval card sat unanswered for an hour.
        assert_eq!(watch.tick(Some(Waiting::User), LIMIT, start + LIMIT * 6), None);
        // Answered: the agent gets the whole limit from there.
        assert_eq!(watch.tick(Some(Waiting::Agent), LIMIT, start + LIMIT * 6 + LIMIT / 2), None);
    }

    #[test]
    fn a_new_turn_starts_with_a_fresh_clock() {
        let start = Instant::now();
        let mut watch = Watch::Listening(start);
        assert_eq!(watch.end(Ok(json!({"stopReason": "end_turn"}))).ok(), Some(json!({"stopReason": "end_turn"})));
        assert_eq!(watch, Watch::Off);
        assert_eq!(watch.tick(Some(Waiting::Agent), LIMIT, start + LIMIT * 2), None);
        assert_eq!(watch.tick(None, LIMIT, start + LIMIT * 3), None);
        assert_eq!(watch, Watch::Off);
    }

    #[test]
    fn a_stopped_agent_fails_its_turn_so_the_fallback_takes_over() {
        let mut watch = Watch::Stopped(LIMIT);
        assert_eq!(watch.tick(Some(Waiting::Agent), LIMIT, Instant::now() + LIMIT * 2), None, "stopped once");
        let exited = Err(Failure::error(FailureKind::Exited, "agent process exited"));
        let error = watch.end(exited).expect_err("the turn fails");
        assert_eq!(error.to_string(), "it went silent for 10 min, so Codync stopped it");
        assert_eq!(reason::of(&error), Some(Reason::NotStarting));
        assert_eq!(watch, Watch::Off);
        // An answer that beat the stop stands.
        assert!(Watch::Stopped(LIMIT).end(Ok(json!({}))).is_ok());
        // Without the watchdog, the error stays the agent's own.
        let own = Watch::Listening(Instant::now()).end(Err(anyhow!("boom"))).expect_err("still fails");
        assert_eq!(own.to_string(), "boom");
    }

    #[test]
    fn the_limit_comes_from_the_environment() {
        assert_eq!(limit_from(None), Some(DEFAULT_LIMIT));
        assert_eq!(limit_from(Some(" ")), Some(DEFAULT_LIMIT));
        assert_eq!(limit_from(Some("25")), Some(Duration::from_mins(25)));
        assert_eq!(limit_from(Some("0")), None, "0 turns it off");
        assert_eq!(limit_from(Some("ten")), Some(DEFAULT_LIMIT));
    }
}
