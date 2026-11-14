//! How long an agent rests after it couldn't answer. Usage belongs to the agent's login, not to one bot, so a
//! rest is the computer's: once one bot finds Claude Code out of usage, every bot skips it until it resets. A usage
//! limit rests until the reset time the agent reported (`usage`); other failures rest longer each time in a row.
//! Kept in `kv` (`rest.<login>`, see `login`), so a host restart remembers them.

use super::{Reason, login};
use crate::store::Store;
use crate::usage::Usage;
use serde::{Deserialize, Serialize};

const MINUTE: i64 = 60_000;
/// Transient failures in a row rest 1, 5, 25, then 60 minutes.
const BACKOFF: [i64; 4] = [MINUTE, 5 * MINUTE, 25 * MINUTE, 60 * MINUTE];
/// A usage limit whose reset time isn't known: try again in an hour.
const UNKNOWN_RESET: i64 = 60 * MINUTE;
/// No credits: the user has to add some, so don't look again for a while.
const OUT_OF_CREDITS: i64 = 6 * 60 * MINUTE;
/// Signed out of an agent Codync signs in to: signing in ends it sooner (`fallback::signed_in`), so look again only
/// now and then (a sign-in outside Codync).
const SIGNED_OUT: i64 = 60 * MINUTE;
/// Signed out of one it can't sign in to (not in `backends::HARNESSES`): only a sign-in outside Codync helps, so look
/// again soon.
const SIGNED_OUT_ELSEWHERE: i64 = 10 * MINUTE;
const NOT_STARTING: i64 = 5 * MINUTE;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Rest {
    pub until: i64,
    pub reason: Reason,
    /// Failures in a row, for the backoff.
    pub failures: u32,
}

fn key(login: &str) -> String {
    format!("rest.{login}")
}

/// `login`'s last rest, over or not.
fn saved(store: &Store, login: &str) -> Option<Rest> {
    serde_json::from_str(&store.kv_get(&key(login))?).ok()
}

/// `login`'s rest at `now`, while it lasts.
pub fn get(store: &Store, login: &str, now: i64) -> Option<Rest> {
    saved(store, login).filter(|rest| rest.until > now)
}

/// Records that `login` couldn't answer for `reason` at `now`. `resets_at`: when its usage limit resets, if it
/// said. A model the agent lacks is the bot's setup, not the agent's, so it doesn't rest the agent.
pub fn record(store: &Store, login: &str, reason: Reason, now: i64, resets_at: Option<i64>) -> Option<Rest> {
    let before = saved(store, login);
    // An hour after its last rest ended, a failure starts the backoff over.
    let failures = before.filter(|r| now - r.until < BACKOFF[3]).map_or(0, |r| r.failures) + 1;
    let wait = match reason {
        Reason::ModelMissing | Reason::ModeMissing => return None,
        Reason::UsageLimit => resets_at.filter(|t| *t > now).map_or(UNKNOWN_RESET, |t| t - now),
        Reason::OutOfCredits => OUT_OF_CREDITS,
        Reason::SignedOut if crate::agent::backends::harness(login::backend_of(login)).is_some() => SIGNED_OUT,
        Reason::SignedOut => SIGNED_OUT_ELSEWHERE,
        Reason::NotStarting => NOT_STARTING,
        Reason::RateLimited | Reason::Overloaded => BACKOFF[(failures as usize - 1).min(BACKOFF.len() - 1)],
    };
    let rest = Rest { until: now + wait, reason, failures };
    let saved = serde_json::to_string(&rest).map_err(anyhow::Error::from).and_then(|v| store.kv_set(&key(login), &v));
    if let Err(error) = saved {
        tracing::warn!(login, error = format!("{error:#}"), "couldn't record the agent's rest");
    }
    Some(rest)
}

/// When the usage limit of `backend`'s login (`login_profile`'s, or the default one's) resets, from the windows it
/// last reported (`usage`): the latest reset of a window that is used up. `None` when none is, or none said when.
pub fn reset_time(usage: &Usage, backend: &str, login_profile: Option<&str>, now: i64) -> Option<i64> {
    let windows = usage.windows(backend, login_profile)?;
    windows.iter().filter(|w| w.percent >= 99.5).filter_map(|w| w.resets_at).filter(|t| *t > now).max()
}

/// Agent `backend`'s accounts on one login (`login_profile`'s, or the computer's own, which a command of its own
/// runs on too) whose last rest was for being signed out.
pub fn signed_out(store: &Store, backend: &str, login_profile: Option<&str>) -> Vec<String> {
    // The prefix also finds agents whose name starts with `backend`'s.
    store
        .kv_prefix(&key(backend))
        .iter()
        .filter_map(|k| k.strip_prefix("rest."))
        .filter(|a| login::backend_of(a) == backend && login::login_profile(a) == login_profile)
        .filter(|a| saved(store, a).is_some_and(|r| r.reason == Reason::SignedOut))
        .map(str::to_owned)
        .collect()
}

/// The agent answered (or the user asked for it again): its rest and backoff are over. Whether it was resting.
pub fn clear(store: &Store, login: &str) -> bool {
    if store.kv_get(&key(login)).is_none() {
        return false;
    }
    if let Err(error) = store.kv_delete(&key(login)) {
        tracing::warn!(login, error = format!("{error:#}"), "couldn't clear the agent's rest");
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_in_ends_only_a_signed_out_rest() {
        let store = Store::open(std::path::Path::new(":memory:")).expect("a store");
        let wrapped = login::key("claude", Some("claude-wrapper"), None);
        for login in ["claude", wrapped.as_str(), "claudette"] {
            assert_eq!(record(&store, login, Reason::SignedOut, 0, None).map(|r| r.reason), Some(Reason::SignedOut));
        }
        record(&store, "codex", Reason::UsageLimit, 0, None);
        let mut on_profile: crate::store::BotConfig = serde_json::from_value(
            serde_json::json!({"id": "b", "name": "B", "backend": "claude", "loginProfile": "a1"}),
        )
        .expect("a bot");
        let a1 = login::of(&on_profile);
        record(&store, &a1, Reason::SignedOut, 0, None);
        let mut ended = signed_out(&store, "claude", None);
        ended.sort();
        assert_eq!(ended, ["claude".to_owned(), wrapped], "a command of its own signs in with the agent");
        assert_eq!(
            signed_out(&store, "claude", Some("a1")),
            std::slice::from_ref(&a1),
            "a login profile signs in by itself"
        );
        assert_eq!(
            get(&store, &a1, 0).map(|r| r.until),
            Some(SIGNED_OUT),
            "a login profile of an agent Codync signs in"
        );
        on_profile.login_profile = None;
        assert_eq!(login::of(&on_profile), "claude");
        assert!(signed_out(&store, "codex", None).is_empty(), "signing in doesn't give usage back");
        // One Codync can't sign in to looks again sooner.
        assert_eq!(get(&store, "claudette", 0).map(|r| r.until), Some(SIGNED_OUT_ELSEWHERE));
        assert_eq!(record(&store, "claude", Reason::SignedOut, 0, None).map(|r| r.until), Some(SIGNED_OUT));
    }

    #[test]
    fn a_usage_limit_rests_until_its_reset_and_transient_failures_back_off() {
        let store = Store::open(std::path::Path::new(":memory:")).expect("a store");
        let rest = record(&store, "claude", Reason::UsageLimit, 1_000, Some(7_201_000)).expect("rests");
        assert_eq!(rest.until, 7_201_000);
        assert_eq!(get(&store, "claude", 7_200_999).map(|r| r.reason), Some(Reason::UsageLimit));
        assert_eq!(get(&store, "claude", 7_201_000), None, "over at the reset");
        assert_eq!(record(&store, "codex", Reason::UsageLimit, 0, None).map(|r| r.until), Some(UNKNOWN_RESET));

        let waits: Vec<i64> =
            (0..5).filter_map(|_| record(&store, "gemini", Reason::Overloaded, 0, None)).map(|r| r.until).collect();
        assert_eq!(waits, [MINUTE, 5 * MINUTE, 25 * MINUTE, 60 * MINUTE, 60 * MINUTE]);
        assert!(clear(&store, "gemini"));
        assert_eq!(record(&store, "gemini", Reason::Overloaded, 0, None).map(|r| r.until), Some(MINUTE));
        assert!(!clear(&store, "cursor"));
        assert_eq!(record(&store, "cursor", Reason::ModelMissing, 0, None), None);
        assert_eq!(record(&store, "opencode", Reason::ModeMissing, 0, None), None);

        let mut custom: crate::store::BotConfig =
            serde_json::from_value(serde_json::json!({"id": "b", "name": "B", "backend": "custom", "command": "a"}))
                .expect("a bot");
        let a = login::of(&custom);
        custom.command = Some("b".into());
        assert!(a.starts_with("custom#") && a != login::of(&custom), "two custom commands are two agents");
        custom.backend = "codex".into();
        custom.command = None;
        assert_eq!(login::of(&custom), "codex");
    }
}
