//! Clearing the failed runs a routine keeps: the ones the user already saw, can't retry any more and isn't waiting
//! on. Healthy history and anything still to report or retry stays.
//!
//! Idea from mercury-agent's dead-letter queue (`queue.ts`, MIT); no code copied.

use super::{Routines, State, Status};
use crate::store::Store;
use anyhow::Result;

/// Drops the finished failures of `bot` (every bot when `None`) that nothing is waiting on. Returns how many.
fn purge(state: &mut State, bot: Option<&str>) -> usize {
    let before = state.runs.len();
    state.runs.retain(|run| {
        let failed = matches!(run.status, Status::Failed | Status::Interrupted);
        let mine = bot.is_none_or(|b| run.bot_id == b);
        !(failed && mine && !run.report_pending && !run.retryable && run.retry_at.is_none())
    });
    before - state.runs.len()
}

impl Routines {
    /// Clears a bot's (or every bot's) settled failed runs and says how many it cleared.
    pub fn clear_failed(&self, store: &Store, bot: Option<&str>) -> Result<usize> {
        self.change(store, |s| Ok(purge(s, bot)))
    }
}

#[cfg(test)]
mod tests {
    use super::super::{new_run, runner::failure_code, tests::routine};
    use super::*;
    use serde_json::Value;

    fn run(bot: &str, status: Status) -> super::super::Run {
        let mut r = routine();
        r.bot_id = bot.into();
        let mut run = new_run(&r, Value::Null, None);
        run.status = status;
        run
    }

    #[test]
    fn a_failure_keeps_a_stable_reason() {
        assert_eq!(failure_code(Status::Interrupted, false, None), "stopped");
        assert_eq!(failure_code(Status::Failed, true, Some("anything")), "timeout");
        assert_eq!(failure_code(Status::Failed, false, Some("this computer's memory is full")), "memory");
        assert_eq!(failure_code(Status::Failed, false, Some("usage limit reached")), "usage_limit");
        assert_eq!(failure_code(Status::Failed, false, Some("Permission denied by the agent")), "denied");
        assert_eq!(failure_code(Status::Failed, false, Some("the bot is busy")), "busy");
        assert_eq!(failure_code(Status::Failed, false, Some("file not found")), "missing");
        assert_eq!(failure_code(Status::Failed, false, Some("it broke")), "error");
        assert_eq!(failure_code(Status::Failed, false, None), "error");
    }

    #[test]
    fn only_settled_failures_of_the_bot_are_cleared() {
        let mut settled = run("a", Status::Failed);
        settled.retryable = false;
        let mut waiting = run("a", Status::Failed);
        waiting.retryable = true;
        let mut state = State {
            pending_flows: vec![],
            routines: vec![],
            runs: vec![
                settled,
                waiting,
                run("a", Status::Succeeded),
                run("b", Status::Failed),
                run("a", Status::Interrupted),
            ],
        };
        assert_eq!(purge(&mut state, Some("a")), 2);
        assert_eq!(state.runs.len(), 3);
        assert_eq!(purge(&mut state, None), 1, "the other bot's failure goes when no bot is named");
    }
}
