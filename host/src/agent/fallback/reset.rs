//! When a usage limit resets, as an agent's error words it: Google's `quotaResetTimeStamp: <RFC 3339>`, or a
//! duration after "resets in" ("Resets in 1h25m37s", "Your limit will reset in 1 hour, 25 minutes."). Anything
//! else, or a time already past, is `None` and the rest falls back to its default (`rest::UNKNOWN_RESET`).

use chrono::DateTime;
use codync_agent::acp::RpcError;

/// A reset further off than this is read as a misparse; the login looks again before then.
const MAX_WAIT: i64 = 7 * 86_400_000;

/// The epoch milliseconds `text` says the limit resets at, as of `now`.
pub fn of_text(text: &str, now: i64) -> Option<i64> {
    let at = timestamp(text).or_else(|| in_duration(text).map(|wait| now.saturating_add(wait)))?;
    Some(at).filter(|at| *at > now).map(|at| at.min(now.saturating_add(MAX_WAIT)))
}

/// When the limit resets, from the message of the first agent error in `error`'s chain that names it.
pub fn of_error(error: &anyhow::Error, now: i64) -> Option<i64> {
    error.chain().filter_map(|cause| cause.downcast_ref::<RpcError>()).find_map(|r| of_text(&r.message, now))
}

fn timestamp(text: &str) -> Option<i64> {
    let rest = text.split_once("quotaResetTimeStamp")?.1;
    let value = rest.trim_start_matches([':', '=', ' ', '"']);
    let end = value.find(|c: char| c.is_whitespace() || matches!(c, ',' | '"' | '}')).unwrap_or(value.len());
    DateTime::parse_from_rfc3339(&value[..end]).ok().map(|at| at.timestamp_millis())
}

/// The wait after the first "reset in" / "resets in", up to the end of its sentence.
fn in_duration(text: &str) -> Option<i64> {
    let lower = text.to_ascii_lowercase();
    let after = lower.split_once("resets in ").or_else(|| lower.split_once("reset in "))?.1;
    let clause = after.split(['\n', ';', '(', ')']).next().unwrap_or(after);
    let (mut total, mut any, mut number) = (0_i64, false, None::<i64>);
    for token in clause.split(|c: char| c.is_whitespace() || c == ',') {
        // "1h25m37s" is digits and letters in turn; a sentence's last period ends the duration.
        let mut rest = token.trim_end_matches('.');
        while !rest.is_empty() {
            let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            if digits > 0 {
                number = rest[..digits].parse().ok();
                rest = &rest[digits..];
                continue;
            }
            let letters = rest.find(|c: char| c.is_ascii_digit()).unwrap_or(rest.len());
            if let (Some(n), Some(unit)) = (number.take(), unit_ms(&rest[..letters])) {
                total = total.saturating_add(n.saturating_mul(unit));
                any = true;
            }
            rest = &rest[letters..];
        }
        if token.ends_with('.') {
            break;
        }
    }
    any.then_some(total)
}

fn unit_ms(unit: &str) -> Option<i64> {
    Some(match unit {
        "d" | "day" | "days" => 86_400_000,
        "h" | "hr" | "hrs" | "hour" | "hours" => 3_600_000,
        "m" | "min" | "mins" | "minute" | "minutes" => 60_000,
        "s" | "sec" | "secs" | "second" | "seconds" => 1_000,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_795_269_600_000; // 2026-11-21T14:00:00Z

    #[test]
    fn a_timestamp_or_a_duration_gives_the_reset() {
        let log = "Error 429, Message: Individual quota reached. Resets in 1h25m37s., Status: RESOURCE_EXHAUSTED, \
                   reason: QUOTA_EXHAUSTED, model: gemini-4-neon-medium, quotaResetTimeStamp: 2026-11-21T15:00:04Z";
        assert_eq!(of_text(log, NOW), Some(NOW + 3_604_000), "the timestamp is exact and wins");
        let hms = "Individual quota reached. Resets in 1h25m37s., Status: RESOURCE_EXHAUSTED";
        assert_eq!(of_text(hms, NOW), Some(NOW + (3_600 + 25 * 60 + 37) * 1000));
        let panel = "Usage Limit Reached\n\nYou have reached your current quota for this period. Your limit will reset in 1 hour, 25 minutes.";
        assert_eq!(of_text(panel, NOW), Some(NOW + (3_600 + 25 * 60) * 1000));
    }

    #[test]
    fn nothing_usable_is_none_and_never_panics() {
        for text in [
            "",
            "Usage Limit Reached",
            "quotaResetTimeStamp: tomorrow",
            "quotaResetTimeStamp:",
            "resets in soon",
            "resets in 99999999999999999999999999 hours",
            "resets in 3 fortnights",
            "quotaResetTimeStamp: 2020-01-01T00:00:00Z",
        ] {
            let at = of_text(text, NOW);
            assert!(at.is_none_or(|t| t > NOW && t <= NOW + MAX_WAIT), "{text}: {at:?}");
        }
        assert_eq!(of_text("quotaResetTimeStamp: 2020-01-01T00:00:00Z", NOW), None, "a past time is no reset");
        assert_eq!(of_text("resets in 3 fortnights", NOW), None);
    }

    #[test]
    fn a_far_reset_is_capped() {
        assert_eq!(of_text("resets in 40 days", NOW), Some(NOW + MAX_WAIT));
    }
}
