//! antigravity-acp answers a prompt its account has no quota left for with `stopReason: "refusal"` and the limit
//! notice streamed as the reply (`"Usage Limit Reached\n\nYou have reached your current quota for this period. Your
//! limit will reset in 6 days, 12 hours."`), not with a JSON-RPC error.

use super::{INTERNAL_ERROR, RpcError, StopReason, USAGE_LIMIT_EXCEEDED};
use anyhow::Result;
use serde_json::Value;

/// The `backends` id of Google Antigravity.
const BACKEND: &str = "antigravity";

/// How the quota notice starts.
const QUOTA_NOTICE: [&str; 2] = ["Usage Limit Reached", "You have reached your current quota"];

/// A `session/prompt` answer of `backend`'s agent, with Antigravity's quota refusal (`reply`: the turn's streamed
/// text) returned as a `usage_limit_exceeded` error carrying the notice, so a fallback follows from it as from any
/// other agent's usage limit.
pub fn prompt_result(backend: &str, result: Result<Value>, reply: Option<&str>) -> Result<Value> {
    let notice = reply.map(str::trim).filter(|text| QUOTA_NOTICE.iter().any(|n| text.starts_with(n)));
    match (notice, &result) {
        (Some(notice), Ok(answer)) if backend == BACKEND && StopReason::of(answer) == StopReason::Refusal => {
            let kind = Some(USAGE_LIMIT_EXCEEDED.into());
            Err(RpcError { code: INTERNAL_ERROR, message: notice.to_owned(), kind }.into())
        }
        _ => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOTICE: &str = "Usage Limit Reached\n\nYou have reached your current quota for this period. Your limit will reset in 6 days, 12 hours.";

    fn refusal() -> Value {
        json!({"stopReason": "refusal"})
    }

    #[test]
    fn a_quota_refusal_is_a_usage_limit_error() {
        for notice in [NOTICE, "  You have reached your current quota for this period."] {
            let error = prompt_result(BACKEND, Ok(refusal()), Some(notice)).expect_err("a usage limit");
            let rpc = error.downcast_ref::<RpcError>().expect("an RpcError");
            assert_eq!((rpc.kind.as_deref(), rpc.message.as_str()), (Some(USAGE_LIMIT_EXCEEDED), notice.trim()));
        }
    }

    #[test]
    fn other_refusals_answers_and_agents_stay() {
        for words in ["I cannot help with that request.", "I can't help bypass a usage limit."] {
            assert!(prompt_result(BACKEND, Ok(refusal()), Some(words)).is_ok(), "{words}");
        }
        assert!(prompt_result(BACKEND, Ok(refusal()), None).is_ok());
        assert!(prompt_result(BACKEND, Ok(json!({"stopReason": "end_turn"})), Some(NOTICE)).is_ok());
        for other in ["claude", "codex", "custom"] {
            assert!(prompt_result(other, Ok(refusal()), Some(NOTICE)).is_ok(), "{other}");
        }
    }
}
