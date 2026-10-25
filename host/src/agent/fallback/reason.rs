//! Which failures move a bot on to its next agent, read from what the agent answered over ACP: the JSON-RPC code,
//! the adapter's category (`errorKind`, codex-acp's `codexErrorInfo`), and the host's own `Failure`. The agents
//! retry what is worth retrying before they answer with an error, so Codync never asks the same agent again.

use crate::agent::acp::{AUTH_REQUIRED, Failure, FailureKind, RpcError};
use serde::{Deserialize, Serialize};

/// Why an agent couldn't answer, and how long it rests for it (see `rest`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    /// The plan's usage limit: it comes back when the limit resets.
    UsageLimit,
    /// No credits left, or the account is on hold.
    OutOfCredits,
    RateLimited,
    /// The provider is overloaded or failing.
    Overloaded,
    SignedOut,
    /// The agent didn't start, stopped answering, or exited.
    NotStarting,
    /// The agent doesn't have the step's model.
    ModelMissing,
}

impl Reason {
    /// How the chat says it: "Claude Code reached its usage limit".
    pub fn phrase(self) -> &'static str {
        match self {
            Self::UsageLimit => "reached its usage limit",
            Self::OutOfCredits => "is out of credits",
            Self::RateLimited => "is rate limited",
            Self::Overloaded => "is overloaded",
            Self::SignedOut => "is signed out",
            Self::NotStarting => "couldn't start",
            Self::ModelMissing => "doesn't have this bot's model",
        }
    }
}

/// How Claude's adapter words a usage limit (the Agent SDK's `USAGE_LIMIT_ERROR_PREFIXES`); it reports it as a
/// `rate_limit` or `billing_error` with this text in the message.
const USAGE_LIMIT_TEXT: [&str; 12] = [
    "You've hit your",
    "You've reached your",
    "You're out of usage credits",
    "Your org is out of usage",
    "Your seat type doesn't include usage credits",
    "Your seat type doesn't include usage",
    "Your usage allocation has been disabled by your admin",
    "Your group's usage limit is set to $0",
    "requires usage credits",
    "You're out of extra usage",
    "Your seat type doesn't include extra usage",
    "usage limit",
];

/// Why `error` moves the bot on, or `None` when it shouldn't (a stop, a refusal, a bug in the request).
pub fn of(error: &anyhow::Error) -> Option<Reason> {
    for cause in error.chain() {
        if let Some(f) = cause.downcast_ref::<Failure>() {
            return Some(match f.kind {
                FailureKind::NotSetUp | FailureKind::Timeout | FailureKind::Exited => Reason::NotStarting,
                FailureKind::NoModel => Reason::ModelMissing,
            });
        }
        if let Some(r) = cause.downcast_ref::<RpcError>() {
            return of_rpc(r);
        }
    }
    None
}

/// Why an agent's error answer moves the bot on (see [`of`]).
pub fn of_rpc(r: &RpcError) -> Option<Reason> {
    if r.code == AUTH_REQUIRED {
        return Some(Reason::SignedOut);
    }
    let usage_text = USAGE_LIMIT_TEXT.iter().any(|t| r.message.contains(t));
    // Claude's adapter sends the Agent SDK's assistant error (`rate_limit`…); codex-acp its `codexErrorInfo`
    // (`usage_limit_exceeded`…); CodeBuddy its `category` (`auth`…).
    Some(match r.kind.as_deref() {
        Some("usage_limit_exceeded") => Reason::UsageLimit,
        Some("rate_limit" | "billing_error" | "account_on_hold") if usage_text => Reason::UsageLimit,
        Some("billing_error" | "account_on_hold") => Reason::OutOfCredits,
        Some("rate_limit" | "rate_limit_exceeded") => Reason::RateLimited,
        Some(
            "overloaded"
            | "server_error"
            | "server_overloaded"
            | "internal_server_error"
            | "response_too_many_failed_attempts",
        ) => Reason::Overloaded,
        Some("authentication_failed" | "oauth_org_not_allowed" | "unauthorized" | "auth") => Reason::SignedOut,
        Some("model_not_found") => Reason::ModelMissing,
        _ if usage_text => Reason::UsageLimit,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rpc(code: i64, message: &str, kind: Option<&str>) -> anyhow::Error {
        RpcError { code, message: message.into(), kind: kind.map(Into::into) }.into()
    }

    #[test]
    fn claude_and_codex_usage_limits_move_on() {
        let claude =
            rpc(-32603, "Internal error: You've hit your limit · resets 3pm (Asia/Taipei)", Some("rate_limit"));
        assert_eq!(of(&claude), Some(Reason::UsageLimit));
        assert_eq!(of(&rpc(-32603, "Internal error", Some("usage_limit_exceeded"))), Some(Reason::UsageLimit));
        assert_eq!(
            of(&rpc(-32603, "Internal error: Your org is out of usage · add funds to continue", Some("billing_error"))),
            Some(Reason::UsageLimit)
        );
        assert_eq!(
            of(&rpc(-32603, "Internal error: credit balance too low", Some("billing_error"))),
            Some(Reason::OutOfCredits)
        );
    }

    #[test]
    fn transient_and_access_failures_move_on() {
        assert_eq!(of(&rpc(-32603, "Internal error: 429", Some("rate_limit"))), Some(Reason::RateLimited));
        assert_eq!(of(&rpc(-32603, "Internal error", Some("overloaded"))), Some(Reason::Overloaded));
        assert_eq!(of(&rpc(-32603, "Internal error", Some("server_overloaded"))), Some(Reason::Overloaded));
        assert_eq!(of(&rpc(AUTH_REQUIRED, "Authentication required", None)), Some(Reason::SignedOut));
        assert_eq!(of(&rpc(-32603, "Internal error", Some("model_not_found"))), Some(Reason::ModelMissing));
        let gone = Failure::error(FailureKind::Exited, "agent process exited");
        assert_eq!(of(&gone.context("prompting")), Some(Reason::NotStarting));
        let no_model = Failure::error(FailureKind::NoModel, "Couldn't select model gpt-5.5: not supported");
        assert_eq!(of(&no_model), Some(Reason::ModelMissing));
    }

    #[test]
    fn a_bad_request_or_an_uncategorized_error_stays() {
        assert_eq!(of(&rpc(-32603, "Internal error", None)), None);
        assert_eq!(of(&rpc(-32603, "Internal error", Some("invalid_request"))), None);
        assert_eq!(of(&rpc(-32602, "Invalid params", None)), None);
        assert_eq!(of(&anyhow::anyhow!("no session")), None);
    }
}
