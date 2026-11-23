//! Antigravity (antigravity-acp) answers a prompt its Google quota can't pay for as `stopReason: "refusal"` with the
//! limit notice streamed as the reply ("Usage Limit Reached\n\nYou have reached your current quota for this period.
//! Your limit will reset in 1 hour, 25 minutes."), not as a JSON-RPC error. So the notice is held back from the chat,
//! and a refusal that came with it fails the turn as a usage limit: the bot moves on to its next agent and the
//! login rests until the reset the notice names. Any other refusal is still the agent declining.

use codync_agent::acp::{INTERNAL_ERROR, RpcError, StopReason};

/// The `backends` id of Google Antigravity.
const BACKEND: &str = "antigravity";

/// How the notice starts.
const STARTS: [&str; 2] = ["Usage Limit Reached", "You have reached your current quota"];

/// `text` when it is Antigravity's quota notice (a reply chunk of an `antigravity` bot).
pub(super) fn notice_text<'a>(backend: &str, text: &'a str) -> Option<&'a str> {
    let text = text.trim();
    (backend == BACKEND && STARTS.iter().any(|s| text.starts_with(s))).then_some(text)
}

/// The prompt's answer `done`, failed as a usage limit with `notice` when it was a refusal that came with one.
pub(super) fn turn_result(done: super::Done, notice: Option<String>) -> super::Done {
    match notice {
        Some(message) if done.as_ref().is_ok_and(|v| StopReason::of(v) == StopReason::Refusal) => {
            Err(RpcError { code: INTERNAL_ERROR, message, kind: Some("usage_limit_exceeded".into()) }.into())
        }
        _ => done,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const NOTICE: &str = "Usage Limit Reached\n\nYou have reached your current quota for this period. Your limit will reset in 1 hour, 25 minutes.";

    fn refusal() -> Value {
        json!({"stopReason": "refusal"})
    }

    #[test]
    fn only_antigravitys_notice_counts() {
        assert_eq!(notice_text("antigravity", &format!("  {NOTICE}\n")), Some(NOTICE));
        assert!(notice_text("antigravity", "I can't help with that.").is_none());
        assert!(notice_text("antigravity", "I can't help bypass a Usage Limit Reached banner.").is_none());
        for other in ["claude", "codex", "custom"] {
            assert!(notice_text(other, NOTICE).is_none(), "{other}");
        }
    }

    #[test]
    fn a_refusal_with_the_notice_fails_as_a_usage_limit() {
        let error = turn_result(Ok(refusal()), Some(NOTICE.into())).expect_err("a usage limit");
        let rpc = error.downcast_ref::<RpcError>().expect("an RpcError");
        assert_eq!((rpc.kind.as_deref(), rpc.message.as_str()), (Some("usage_limit_exceeded"), NOTICE));
        assert_eq!(crate::agent::fallback::reason::of(&error), Some(crate::agent::fallback::Reason::UsageLimit));
    }

    #[test]
    fn other_answers_stay() {
        assert!(turn_result(Ok(refusal()), None).is_ok(), "a plain refusal");
        let ended = json!({"stopReason": "end_turn"});
        assert_eq!(turn_result(Ok(ended.clone()), Some(NOTICE.into())).expect("an answer"), ended);
    }
}
