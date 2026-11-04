//! Why a turn ended, and whether that ending may be called "done". An agent can stop for five reasons (ACP's stop
//! reasons plus ours): only `end_turn` is a finished turn. Running out of steps or output leaves the work half done,
//! and calling that "Finished" sends the person away from a task that still needs them.
//!
//! Idea from Mercury Agent's `src/core/completion-verdict.ts` (MIT; `THIRD-PARTY-NOTICES.md`): a step budget that runs
//! out while the agent is still working is a pause, not a completion. The code is Codync's own.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TurnVerdict {
    /// The agent ended its turn with an answer (`end_turn`).
    Completed,
    /// The agent used up its steps for the turn while still working (`max_turn_requests`): the work can go on.
    Paused,
    /// The agent's answer was cut at its output limit (`max_tokens`): the rest can be asked for.
    Truncated,
    /// The person stopped it.
    Cancelled,
    /// It failed, refused, did not pass its check, or ended for a reason nobody knows.
    Failed,
}

impl TurnVerdict {
    /// The verdict of a turn that ended with `stop_reason`.
    pub fn of(stop_reason: &str) -> Self {
        match stop_reason {
            "end_turn" => Self::Completed,
            "max_turn_requests" => Self::Paused,
            "max_tokens" => Self::Truncated,
            "cancelled" => Self::Cancelled,
            _ => Self::Failed,
        }
    }

    /// Stopped short of its end, with work that can be picked up again.
    pub fn unfinished(self) -> bool {
        matches!(self, Self::Paused | Self::Truncated)
    }

    /// The `state` a user message's `turnResult` carries. A refusal or an unknown stop reason that did not error stays
    /// `incomplete`, as before.
    pub fn state(self, errored: bool) -> &'static str {
        match self {
            Self::Completed => "done",
            Self::Paused => "paused",
            Self::Truncated => "truncated",
            Self::Cancelled => "stopped",
            Self::Failed if errored => "failed",
            Self::Failed => "incomplete",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stop_reason_has_one_verdict() {
        for (reason, verdict) in [
            ("end_turn", TurnVerdict::Completed),
            ("max_turn_requests", TurnVerdict::Paused),
            ("max_tokens", TurnVerdict::Truncated),
            ("cancelled", TurnVerdict::Cancelled),
            ("refusal", TurnVerdict::Failed),
            ("error", TurnVerdict::Failed),
            (crate::agent::bot::verify::FAILED, TurnVerdict::Failed),
            ("host_restart", TurnVerdict::Failed),
            ("something an adapter invented", TurnVerdict::Failed),
        ] {
            assert_eq!(TurnVerdict::of(reason), verdict, "{reason}");
        }
    }

    #[test]
    fn only_a_pause_or_a_cut_answer_can_be_picked_up_again() {
        assert!(TurnVerdict::Paused.unfinished() && TurnVerdict::Truncated.unfinished());
        for verdict in [TurnVerdict::Completed, TurnVerdict::Cancelled, TurnVerdict::Failed] {
            assert!(!verdict.unfinished(), "{verdict:?}");
        }
    }

    #[test]
    fn the_result_state_keeps_its_old_words() {
        assert_eq!(TurnVerdict::Completed.state(false), "done");
        assert_eq!(TurnVerdict::Cancelled.state(false), "stopped");
        assert_eq!(TurnVerdict::Failed.state(true), "failed");
        assert_eq!(TurnVerdict::Failed.state(false), "incomplete", "a refusal is not an error");
        assert_eq!((TurnVerdict::Paused.state(false), TurnVerdict::Truncated.state(false)), ("paused", "truncated"));
    }
}
