//! How long an agent rests after it couldn't answer. Usage belongs to the agent's login, not to one bot, so a
//! rest is the computer's: once one bot finds Claude Code out of usage, every bot skips it until it resets. A usage
//! limit rests until the reset time the agent reported (`usage`); other failures rest longer each time in a row.
//! Kept in `kv` (`rest.<login>`, see `login`), so a host restart remembers them.

use super::{Reason, login};
use crate::store::Store;
use crate::usage::Usage;
use serde::{Deserialize, Serialize};

const MINUTE: i64 = 60_000;
/// Longest real quota window (7 days), so an odd or extreme `"reset in …"` message never benches a login for months.
const MAX_RESET_IN: i64 = 7 * 24 * 60 * MINUTE;
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
        Reason::SignedOut if codync_agent::backends::harness(login::backend_of(login)).is_some() => SIGNED_OUT,
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

/// The later of a cached exhausted usage window's reset (`reset_time`) and a relative duration stated in the failure
/// message (`parse_reset_in`), so a shorter cached window (such as 5h) never cuts off a longer refusal (such as 7d).
pub fn reset_until(
    usage: &Usage,
    backend: &str,
    login_profile: Option<&str>,
    error_text: &str,
    now: i64,
) -> Option<i64> {
    [reset_time(usage, backend, login_profile, now), parse_reset_in(error_text, now)].into_iter().flatten().max()
}

fn duration_unit_ms(unit: &str) -> Option<i64> {
    Some(match unit {
        "w" | "wk" | "wks" | "week" | "weeks" => 7 * 24 * 60 * MINUTE,
        "d" | "day" | "days" => 24 * 60 * MINUTE,
        "h" | "hr" | "hrs" | "hour" | "hours" => 60 * MINUTE,
        "m" | "min" | "mins" | "minute" | "minutes" => MINUTE,
        "s" | "sec" | "secs" | "second" | "seconds" => 1_000,
        _ => return None,
    })
}

/// Parses a relative reset duration from an agent's usage-limit message (`"Your limit will reset in 6 days, 12 hours."`
/// or `"resets in 2h 30m"`), returning `now + duration` capped at [`MAX_RESET_IN`] (7 days) when one is found.
pub fn parse_reset_in(text: &str, now: i64) -> Option<i64> {
    let lower = text.to_ascii_lowercase();
    let tail = lower.split_once("reset in ").or_else(|| lower.split_once("resets in "))?.1;
    let clause = tail.split(['.', '\n', ';', '(', ')']).next().unwrap_or(tail);
    let mut total_ms: i64 = 0;
    let mut pending_num: Option<i64> = None;
    for raw in clause.split(|c: char| c.is_ascii_whitespace() || c == ',') {
        let token = raw.trim();
        if token.is_empty() || token == "and" {
            continue;
        }
        if token.strip_prefix('-').is_some_and(|rest| rest.as_bytes().first().is_some_and(u8::is_ascii_digit)) {
            return None;
        }
        if token.bytes().all(|b| b.is_ascii_digit()) {
            pending_num = Some(token.parse::<i64>().unwrap_or(i64::MAX));
            continue;
        }
        if token.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            pending_num = None;
            let mut rest = token;
            while let Some(digits_end) = rest.find(|c: char| !c.is_ascii_digit()) {
                if digits_end == 0 {
                    break;
                }
                let n = rest[..digits_end].parse::<i64>().unwrap_or(i64::MAX);
                let after_digits = &rest[digits_end..];
                let unit_end = after_digits.find(|c: char| c.is_ascii_digit()).unwrap_or(after_digits.len());
                if let Some(unit_ms) = duration_unit_ms(&after_digits[..unit_end]) {
                    total_ms = total_ms.saturating_add(n.saturating_mul(unit_ms));
                }
                rest = &after_digits[unit_end..];
            }
            continue;
        }
        if let Some(n) = pending_num.take()
            && let Some(unit_ms) = duration_unit_ms(token)
        {
            total_ms = total_ms.saturating_add(n.saturating_mul(unit_ms));
        }
    }
    let capped_ms = total_ms.min(MAX_RESET_IN);
    (capped_ms > 0).then(|| now.saturating_add(capped_ms))
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

    #[test]
    fn parses_relative_reset_duration_from_usage_limit_message() {
        let now = 1_000_000;
        let msg = "Usage Limit Reached\n\nYou have reached your current quota for this period. Your limit will reset in 6 days, 12 hours.";
        assert_eq!(parse_reset_in(msg, now), Some(now + (6 * 24 + 12) * 60 * MINUTE));
        assert_eq!(parse_reset_in("Limit resets in 2h 30m.", now), Some(now + 150 * MINUTE));
        assert_eq!(parse_reset_in("Your limit will reset in 45 minutes", now), Some(now + 45 * MINUTE));
        assert_eq!(parse_reset_in("No reset time mentioned", now), None);
    }

    #[test]
    fn parse_reset_in_caps_at_seven_days_and_handles_edge_cases() {
        let now = 1_000_000;
        // Exactly 7 days: kept intact.
        assert_eq!(parse_reset_in("Your limit will reset in 7 days.", now), Some(now + MAX_RESET_IN));
        assert_eq!(parse_reset_in("Limit resets in 168 hours.", now), Some(now + MAX_RESET_IN));
        assert_eq!(parse_reset_in("Limit resets in 7d.", now), Some(now + MAX_RESET_IN));

        // Over 7 days (or overflow): capped at 7 days.
        assert_eq!(parse_reset_in("Your limit will reset in 7 days, 1 second.", now), Some(now + MAX_RESET_IN));
        assert_eq!(parse_reset_in("Your limit will reset in 8 days, 12 hours.", now), Some(now + MAX_RESET_IN));
        assert_eq!(parse_reset_in("Your limit will reset in 30 days.", now), Some(now + MAX_RESET_IN));
        assert_eq!(parse_reset_in("Your limit will reset in 2 weeks.", now), Some(now + MAX_RESET_IN));
        assert_eq!(parse_reset_in("Limit resets in 169 hours.", now), Some(now + MAX_RESET_IN));
        assert_eq!(
            parse_reset_in("Your limit will reset in 99999999999999999999 days.", now),
            Some(now + MAX_RESET_IN)
        );

        // Zero and negative durations: ignored (`None`).
        assert_eq!(parse_reset_in("Your limit will reset in 0 minutes.", now), None);
        assert_eq!(parse_reset_in("Limit resets in 0s.", now), None);
        assert_eq!(parse_reset_in("Your limit will reset in -5 minutes.", now), None);
        assert_eq!(parse_reset_in("Limit resets in -2h 30m.", now), None);
        assert_eq!(parse_reset_in("Limit resets in 10m, -5m.", now), None);

        // Unusual unit spellings and separators.
        assert_eq!(parse_reset_in("Limit resets in 2h30m.", now), Some(now + 150 * MINUTE));
        assert_eq!(parse_reset_in("Limit resets in 1d12h.", now), Some(now + 36 * 60 * MINUTE));
        assert_eq!(parse_reset_in("Limit resets in 2hr 30mins and 15secs.", now), Some(now + 150 * MINUTE + 15_000));
        assert_eq!(parse_reset_in("Limit resets in 1wk.", now), Some(now + MAX_RESET_IN));
        assert_eq!(parse_reset_in("Limit resets in 2 hours (UTC).", now), Some(now + 120 * MINUTE));
        assert_eq!(parse_reset_in("Limit resets in 3 fortnights.", now), None);
        assert_eq!(parse_reset_in("Limit resets in foo bar.", now), None);
    }

    #[test]
    fn longer_refusal_reset_is_not_cut_off_by_shorter_cached_window() {
        let now = 1_000_000;
        let short_window = now + 20 * MINUTE;
        let long_refusal = now + (6 * 24 + 12) * 60 * MINUTE;
        let usage = Usage {
            providers: vec![crate::usage::Provider {
                id: "antigravity",
                name: "Google Antigravity",
                registry: None,
                windows: vec![
                    crate::usage::Window {
                        id: "5h".into(),
                        label: "5-hour".into(),
                        percent: 100.0,
                        resets_at: Some(short_window),
                        resets_text: None,
                        text: None,
                    },
                    crate::usage::Window {
                        id: "7d".into(),
                        label: "7-day".into(),
                        percent: 35.0,
                        resets_at: Some(now + 7 * 24 * 60 * MINUTE),
                        resets_text: None,
                        text: None,
                    },
                ],
                source: "agent",
                checked: crate::usage::Checked::default(),
                default_login_profile: None,
                login_profiles: Vec::new(),
            }],
            services: Vec::new(),
        };
        let msg = "Usage Limit Reached\n\nYou have reached your current quota for this period. Your limit will reset in 6 days, 12 hours.";
        assert_eq!(reset_until(&usage, "antigravity", None, msg, now), Some(long_refusal));
        assert_eq!(
            reset_until(&usage, "antigravity", None, "Your limit will reset in 5 minutes.", now),
            Some(short_window),
            "still keeps the later cached reset when the refusal says a shorter duration"
        );
    }
}
