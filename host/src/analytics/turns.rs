//! `turn_finished`: how each agent turn ended, never its text or the error's.

use super::{Event, capture};
use crate::hub::Hub;
use serde::Serialize;
use serde_json::json;

/// How an agent's turn ended (`turn_finished`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnOutcome {
    /// `end_turn`: the agent finished.
    Done,
    /// The user stopped it.
    Stopped,
    /// The agent process or connection failed.
    Failed,
    /// It ended short of `end_turn` (output or step limit, refusal).
    Incomplete,
}

/// What started the turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnSource {
    User,
    Routine,
    Group,
    Bot,
    /// The host's own turn, with no message of the user's (the lead opening a call).
    Host,
}

/// `turn_finished`: the agent, how the turn ended, why it failed and how long it ran;
/// never its text or the error's.
pub fn turn_finished(
    hub: &Hub,
    agent: &str,
    outcome: TurnOutcome,
    stop_reason: &str,
    failure: Option<&anyhow::Error>,
    source: TurnSource,
    ms: i64,
) {
    let failure = failure.map(failure_kind);
    let outcome = outcome_name(outcome, failure.map(|(kind, _)| kind));
    // ACP's stop reasons plus ours; anything else an adapter invents is just `other`.
    let stop_reason = match stop_reason {
        "end_turn" | "cancelled" | "max_tokens" | "max_turn_requests" | "refusal" | "error" | "start_failed" => {
            stop_reason
        }
        _ => "other",
    };
    let source = match source {
        TurnSource::User => "user",
        TurnSource::Routine => "routine",
        TurnSource::Group => "group",
        TurnSource::Bot => "bot",
        TurnSource::Host => "host",
    };
    let mut p = json!({"agent": agent, "outcome": outcome, "stop_reason": stop_reason, "source": source, "seconds": ms.max(0) / 1000});
    if let Some((kind, rpc)) = failure {
        p["error"] = kind.into();
        if let Some(rpc) = rpc {
            p["rpc_code"] = rpc.code.into();
            p["agent_reason"] = rpc.kind.as_deref().unwrap_or_else(|| reason_of(&rpc.message)).into();
        }
    }
    capture(hub, Event::TurnFinished, p.as_object().cloned().unwrap_or_default());
}

/// A turn that failed only because the agent isn't installed or signed in on this computer is
/// `needs_setup`, not `failed`: the user has a step to take, nothing broke.
fn outcome_name(outcome: TurnOutcome, error: Option<&str>) -> &'static str {
    match outcome {
        TurnOutcome::Done => "done",
        TurnOutcome::Stopped => "stopped",
        TurnOutcome::Failed if matches!(error, Some("sign_in" | "not_installed")) => "needs_setup",
        TurnOutcome::Failed => "failed",
        TurnOutcome::Incomplete => "incomplete",
    }
}

/// `agent_fell_back`: a turn moved on from `agent` to the bot's next one, and why (never the error's text).
pub fn agent_fell_back(hub: &Hub, agent: &str, reason: crate::agent::fallback::Reason) {
    let p = json!({"agent": agent, "reason": reason});
    capture(hub, Event::AgentFellBack, p.as_object().cloned().unwrap_or_default());
}

/// What an agent taking over the main chat was told first (`handoff_carried`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Carried {
    /// A new session: the conversation so far.
    Recap,
    /// A session it had before: what happened since it left.
    CatchUp,
}

/// `handoff_carried`: `agent` took over a bot's chat and was told `bytes` of it first (never the text).
/// `window_known`: a recap sized to the agent's window; a catch-up never is.
pub fn handoff_carried(hub: &Hub, agent: &str, kind: Carried, bytes: usize, window_known: bool) {
    let p = json!({"agent": agent, "kind": kind, "bytes": bytes, "window_known": window_known});
    capture(hub, Event::HandoffCarried, p.as_object().cloned().unwrap_or_default());
}

/// Why a bot's agent stopped between turns (`agent_retired`); the next message starts it again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Retired {
    /// Every session sat unused for 30 minutes.
    Idle,
    /// A routine run ended with no other session open.
    RoutineFinished,
    /// A routine run ended and the agent couldn't close its session, which holds MCP servers (Cursor can't close one).
    SessionNotClosed,
}

/// `agent_retired`: a bot's `agent` stopped to give back its processes, and why.
pub fn agent_retired(hub: &Hub, agent: &str, reason: Retired) {
    let p = json!({"agent": agent, "reason": reason});
    capture(hub, Event::AgentRetired, p.as_object().cloned().unwrap_or_default());
}

/// `context_compacted`: Codync started summarizing a bot's chat, at the end of a turn or while it rested, and at
/// which of the user's thresholds.
pub fn context_compacted(hub: &Hub, agent: &str, idle: bool, threshold: codync_mem::compaction::Threshold) {
    let when = if idle { "idle" } else { "turn_end" };
    let p = json!({"agent": agent, "when": when, "threshold": threshold.kind()});
    capture(hub, Event::ContextCompacted, p.as_object().cloned().unwrap_or_default());
}

/// What became of a queued message the user had handled now (`message_steered`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Steer {
    /// The agent took it into its running turn.
    Joined,
    /// The turn ended first: it is the next one.
    Next,
    /// The agent can't steer: the user's turn stopped for it.
    Interrupted,
    /// Other work runs (a routine, a room, another bot): it is next in line.
    MovedUp,
    /// The agent refused it or didn't answer: it waits for its turn.
    Failed,
}

/// `message_steered`: the agent and what became of the message.
pub fn message_steered(hub: &Hub, agent: &str, how: Steer) {
    let p = json!({"agent": agent, "how": how});
    capture(hub, Event::MessageSteered, p.as_object().cloned().unwrap_or_default());
}

/// Why a turn failed, as a category (plus the agent's JSON-RPC error, for its code and own
/// category): never the error's text, which can hold paths and agent output.
fn failure_kind(e: &anyhow::Error) -> (&'static str, Option<&crate::agent::acp::RpcError>) {
    use crate::agent::acp::{Failure, FailureKind, RpcError};
    use crate::agent::fallback::{Reason, reason};
    if let Some(f) = e.chain().find_map(|c| c.downcast_ref::<Failure>()) {
        let kind = match f.kind {
            FailureKind::NotSetUp => "not_installed",
            FailureKind::Timeout => "timeout",
            FailureKind::Exited => "exited",
            FailureKind::NoModel => "no_model",
            FailureKind::ModeMissing => "mode_missing",
        };
        return (kind, None);
    }
    // The same two tests that put a sign-in card in the chat (`invite_sign_in`, `fall_back`).
    if crate::agent::acp::needs_sign_in(e) || reason::of(e) == Some(Reason::SignedOut) {
        return ("sign_in", None);
    }
    for cause in e.chain() {
        if let Some(r) = cause.downcast_ref::<RpcError>() {
            return ("agent_error", Some(r));
        }
        // Never `not_installed`: agents start through the shell, so a missing one exits instead,
        // and a NotFound here is the host's own (a bot's missing folder).
        if cause.downcast_ref::<std::io::Error>().is_some() {
            return ("io", None);
        }
    }
    ("other", None)
}

/// What an agent error without the adapter's own category is about, read from its message, which
/// itself never leaves the computer. Checked in order: the first match wins. A status code counts
/// only as a word of its own, not inside a token count or an id.
fn reason_of(message: &str) -> &'static str {
    const REASONS: &[(&str, &[&str])] = &[
        ("session_ended", &["session has ended", "session not found", "no session"]),
        ("rate_limit", &["rate limit", "rate_limit", "too many requests", "429"]),
        ("quota", &["quota", "usage limit", "credit balance", "billing", "insufficient", "402"]),
        ("overloaded", &["overloaded", "529", "503", "unavailable", "capacity"]),
        ("context", &["context length", "context window", "maximum context", "too long", "token limit"]),
        (
            "network",
            &[
                "econn",
                "enotfound",
                "etimedout",
                "fetch failed",
                "network",
                "socket",
                "getaddrinfo",
                "connection error",
                "timed out",
                "timeout",
            ],
        ),
        ("provider", &["provider", "api key", "api_key", "base url"]),
        // Last: many messages name a model in passing.
        ("model", &["model"]),
    ];
    let message = message.to_lowercase();
    let words: Vec<&str> = message.split(|c: char| !c.is_ascii_alphanumeric()).collect();
    let says = |w: &str| if w.bytes().all(|b| b.is_ascii_digit()) { words.contains(&w) } else { message.contains(w) };
    REASONS.iter().find(|(_, patterns)| patterns.iter().any(|w| says(w))).map_or("unknown", |(reason, _)| reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_turn_names_its_kind_not_its_text() {
        use crate::agent::acp::{Failure, FailureKind, RpcError};
        use anyhow::Context as _;
        let code = |e: &anyhow::Error| {
            let (kind, rpc) = failure_kind(e);
            (kind, rpc.map(|r| r.code))
        };
        let setup = Failure::error(FailureKind::NotSetUp, "Codex isn't set up on this computer.");
        assert_eq!(code(&setup), ("not_installed", None));
        let hung = Err::<(), _>(Failure::error(FailureKind::Timeout, "no answer")).context("starting");
        assert_eq!(code(&hung.expect_err("failed")), ("timeout", None));
        let missing = std::io::Error::new(std::io::ErrorKind::NotFound, "/Users/me/bin/agent");
        let spawn = Err::<(), _>(missing).context("starting `/Users/me/bin/agent`");
        assert_eq!(code(&spawn.expect_err("failed")), ("io", None));
        let rpc_error = |code, message: &str, kind: Option<&str>| {
            anyhow::Error::new(RpcError { code, message: message.into(), kind: kind.map(Into::into) })
        };
        assert_eq!(code(&rpc_error(-32000, "Authentication required", None)), ("sign_in", None));
        assert_eq!(code(&rpc_error(-32603, "Internal error", Some("authentication_failed"))), ("sign_in", None));
        assert_eq!(code(&rpc_error(-32603, "OAuth token has expired", None)), ("sign_in", None));
        assert_eq!(code(&rpc_error(-32603, "Internal error: /Users/me", None)), ("agent_error", Some(-32603)));
        let limited = rpc_error(-32603, "x", Some("rate_limit"));
        assert_eq!(failure_kind(&limited).1.and_then(|r| r.kind.as_deref()), Some("rate_limit"));
        assert_eq!(code(&anyhow::anyhow!("something else")), ("other", None));
    }

    #[test]
    fn a_turn_the_user_must_set_up_is_not_a_failure() {
        use crate::agent::acp::{AUTH_REQUIRED, Failure, FailureKind, RpcError};
        let failed = |e: anyhow::Error| outcome_name(TurnOutcome::Failed, Some(failure_kind(&e).0));
        assert_eq!(failed(Failure::error(FailureKind::NotSetUp, "Codex isn't set up")), "needs_setup");
        assert_eq!(failed(RpcError { code: AUTH_REQUIRED, message: "x".into(), kind: None }.into()), "needs_setup");
        assert_eq!(failed(RpcError { code: -32603, message: "x".into(), kind: None }.into()), "failed");
        assert_eq!(outcome_name(TurnOutcome::Done, None), "done");
    }

    #[test]
    fn an_uncategorized_agent_error_gets_a_reason_from_its_message() {
        assert_eq!(reason_of("The Claude Agent session has ended"), "session_ended");
        assert_eq!(reason_of("429 Too Many Requests: rate limit reached for model x"), "rate_limit");
        assert_eq!(reason_of("Your credit balance is too low"), "quota");
        assert_eq!(reason_of("Overloaded"), "overloaded");
        assert_eq!(reason_of("prompt is too long: 210000 tokens"), "context");
        assert_eq!(reason_of("prompt is too long: 205290 tokens > 200000 maximum"), "context", "529 inside a count");
        assert_eq!(reason_of("Error 503 Service Unavailable"), "overloaded");
        assert_eq!(reason_of("Model not found: openai/gpt-x"), "model");
        assert_eq!(reason_of("fetch failed (ECONNRESET)"), "network");
        assert_eq!(reason_of("API Error: Connection error."), "network");
        assert_eq!(reason_of("Request timed out"), "network");
        assert_eq!(reason_of("fetch failed while loading the model list"), "network");
        assert_eq!(reason_of("No provider configured"), "provider");
        assert_eq!(reason_of("Internal error: /Users/me/project"), "unknown");
    }
}
