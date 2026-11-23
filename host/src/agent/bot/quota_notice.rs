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

/// Only the start of a reply can be a quota notice. Once confirmed, keep the rest for the reset time.
pub(super) enum QuotaNotice {
    Checking(String),
    Confirmed(String),
    Reply,
}

impl Default for QuotaNotice {
    fn default() -> Self {
        Self::Checking(String::new())
    }
}

impl QuotaNotice {
    /// Returns ordinary reply text as soon as the start rules out a notice.
    pub(super) fn push(&mut self, backend: &str, text: String) -> Option<String> {
        if backend != BACKEND {
            return Some(text);
        }
        match self {
            Self::Reply => Some(text),
            Self::Confirmed(held) => {
                held.push_str(&text);
                None
            }
            Self::Checking(held) => {
                if held.is_empty() {
                    *held = text;
                } else {
                    held.push_str(&text);
                }
                let start = held.trim_start();
                if STARTS.iter().any(|s| start.starts_with(s)) {
                    *self = Self::Confirmed(std::mem::take(held));
                } else if !STARTS.iter().any(|s| s.starts_with(start)) {
                    let reply = std::mem::take(held);
                    *self = Self::Reply;
                    return Some(reply);
                }
                None
            }
        }
    }

    /// A confirmed notice only becomes a usage limit if the prompt ended with a refusal.
    pub(super) fn finish(self, done: super::Done) -> (super::Done, String) {
        match self {
            Self::Confirmed(text) if done.as_ref().is_ok_and(|v| StopReason::of(v) == StopReason::Refusal) => {
                let error = RpcError {
                    code: INTERNAL_ERROR,
                    message: text.trim().to_owned(),
                    kind: Some("usage_limit_exceeded".into()),
                };
                (Err(error.into()), String::new())
            }
            Self::Checking(text) | Self::Confirmed(text) => (done, text),
            Self::Reply => (done, String::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const START: &str = "Usage Limit Reached";
    const RESET: &str = "\nYour limit will reset in 1 hour, 25 minutes.";

    fn refusal() -> Value {
        json!({"stopReason": "refusal"})
    }

    #[test]
    fn a_prefix_is_held_until_it_deviates() {
        let mut notice = QuotaNotice::default();
        assert_eq!(notice.push(BACKEND, " \n".into()), None);
        assert_eq!(notice.push(BACKEND, "Usage Li".into()), None);
        assert_eq!(notice.push(BACKEND, "brary".into()).as_deref(), Some(" \nUsage Library"));
        assert_eq!(notice.push(BACKEND, START.into()).as_deref(), Some(START));
        let (done, text) = notice.finish(Ok(refusal()));
        assert_eq!(done.expect("a plain refusal"), refusal());
        assert_eq!(text, "");
    }

    #[test]
    fn only_antigravitys_notice_counts() {
        for other in ["claude", "codex", "custom"] {
            let mut notice = QuotaNotice::default();
            assert_eq!(notice.push(other, START.into()).as_deref(), Some(START), "{other}");
        }
    }

    #[test]
    fn a_refusal_with_the_accumulated_notice_fails_as_a_usage_limit() {
        let mut notice = QuotaNotice::default();
        for chunk in ["  Usage Li", "mit Reached", RESET, "\nMore text.  "] {
            assert_eq!(notice.push(BACKEND, chunk.into()), None);
        }
        let (done, text) = notice.finish(Ok(refusal()));
        assert_eq!(text, "");
        let error = done.expect_err("a usage limit");
        let rpc = error.downcast_ref::<RpcError>().expect("an RpcError");
        assert_eq!(rpc.kind.as_deref(), Some("usage_limit_exceeded"));
        assert_eq!(rpc.message, format!("{START}{RESET}\nMore text."));
        assert_eq!(crate::agent::fallback::reason::of(&error), Some(crate::agent::fallback::Reason::UsageLimit));
    }

    #[test]
    fn a_confirmed_notice_on_end_turn_is_released_verbatim() {
        let mut notice = QuotaNotice::default();
        assert_eq!(notice.push(BACKEND, "  You have reached your current quota".into()), None);
        assert_eq!(notice.push(BACKEND, " in this example.\n".into()), None);
        let ended = json!({"stopReason": "end_turn"});
        let (done, text) = notice.finish(Ok(ended.clone()));
        assert_eq!(done.expect("an answer"), ended);
        assert_eq!(text, "  You have reached your current quota in this example.\n");
    }

    #[test]
    fn an_incomplete_prefix_is_released_even_on_refusal() {
        let mut notice = QuotaNotice::default();
        assert_eq!(notice.push(BACKEND, " Usage Li".into()), None);
        let (done, text) = notice.finish(Ok(refusal()));
        assert_eq!(done.expect("a plain refusal"), refusal());
        assert_eq!(text, " Usage Li");
    }
}
