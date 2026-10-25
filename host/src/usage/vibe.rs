//! Tokens and cost from Mistral Vibe's session logs. There is no plan
//! percent here — that lives behind a browser cookie — so these are notes,
//! not limit bars.
//!
//! A `messages.jsonl` line with a time and a token or cost count contributes
//! only when that time is today, including a session opened yesterday. A
//! session with no such lines counts only when its folder started today.

use serde_json::Value;
use std::path::Path;

use super::Window;

pub(super) fn collect() -> Option<Vec<Window>> {
    let root = dirs::home_dir()?.join(".vibe/logs/session");
    if !root.is_dir() {
        return None;
    }
    let now = crate::store::now_ms();
    let start = local_midnight(now)?;
    Some(windows_in(&root, start, now))
}

fn windows_in(root: &Path, start: i64, now: i64) -> Vec<Window> {
    let mut tokens = 0.0;
    let mut cost = 0.0;
    let mut from_stamp = false;
    let mut from_dated = false;
    let Some(entries) = std::fs::read_dir(root).ok() else { return Vec::new() };
    let end = now.saturating_add(86_400_000);
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(at) = session_ms(name) else { continue };
        let log = entry.path().join("messages.jsonl");
        // Untouched since midnight, and opened before today: it cannot add to today's total.
        if at < start && !written_since(&log, start) {
            continue;
        }
        if let Some((session_tokens, session_cost)) = dated_usage(&log, start, end) {
            if session_tokens > 0.0 || session_cost > 0.0 {
                from_dated = true;
                tokens += session_tokens;
                cost += session_cost;
            }
            continue;
        }
        if !(start..=end).contains(&at) {
            continue;
        }
        let Some((session_tokens, session_cost)) = read_meta(&entry.path().join("meta.json")) else { continue };
        if session_tokens > 0.0 || session_cost > 0.0 {
            from_stamp = true;
            tokens += session_tokens;
            cost += session_cost;
        }
    }
    notes(tokens, cost, from_stamp && !from_dated)
}

/// Whether `path` was last written at or after `start` (ms). Missing counts as not written.
fn written_since(path: &Path, start: i64) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false };
    let Ok(modified) = meta.modified() else { return true };
    let Ok(elapsed) = modified.duration_since(std::time::UNIX_EPOCH) else { return true };
    i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX) >= start
}

/// Today's totals when the file has dated usage lines. `None` when it does not,
/// so the session total can be used for a folder started today.
fn dated_usage(path: &Path, start: i64, end: i64) -> Option<(f64, f64)> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut saw = false;
    let mut tokens = 0.0;
    let mut cost = 0.0;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else { continue };
        let Some(record) = usage_record(&value) else { continue };
        // An undated line can't be placed in a day: it leaves the session total to `meta.json`.
        let Some(at) = record.at else { continue };
        saw = true;
        if !(start..=end).contains(&at) {
            continue;
        }
        tokens += record.tokens;
        cost += record.cost;
    }
    saw.then_some((tokens, cost))
}

struct Dated {
    at: Option<i64>,
    tokens: f64,
    cost: f64,
}

fn usage_record(value: &Value) -> Option<Dated> {
    let tokens = token_count(value);
    let cost = json_f64(&value["cost"]).or_else(|| json_f64(&value["usage"]["cost"]));
    if tokens.is_none() && cost.is_none() {
        return None;
    }
    Some(Dated { at: message_time(value), tokens: tokens.unwrap_or(0.0), cost: cost.unwrap_or(0.0) })
}

fn token_count(value: &Value) -> Option<f64> {
    let usage = value.get("usage").filter(|v| v.is_object()).unwrap_or(value);
    if let Some(total) = json_f64(&usage["total_tokens"]) {
        return Some(total);
    }
    let prompt = json_f64(&usage["prompt_tokens"]);
    let completion = json_f64(&usage["completion_tokens"]);
    if prompt.is_none() && completion.is_none() {
        return json_f64(&usage["tokens"]);
    }
    Some(prompt.unwrap_or(0.0) + completion.unwrap_or(0.0))
}

fn message_time(value: &Value) -> Option<i64> {
    let stamp = ["created_at", "timestamp"].iter().find_map(|key| value.get(*key).filter(|v| !v.is_null()))?;
    if let Some(text) = stamp.as_str() {
        return chrono::DateTime::parse_from_rfc3339(text).ok().map(|t| t.timestamp_millis());
    }
    let n = stamp.as_i64().or_else(|| stamp.as_f64().and_then(super::finite_i64))?;
    Some(if n > 100_000_000_000 { n } else { n.saturating_mul(1000) })
}

#[allow(clippy::cast_precision_loss)]
fn json_f64(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_i64().map(|n| n as f64))
}

fn notes(tokens: f64, cost: f64, stamp_only: bool) -> Vec<Window> {
    // Parentheses, not " · ": the apps read the part after " · " as Claude's model name.
    let (token_label, cost_label) =
        if stamp_only { ("Tokens (started today)", "Cost (started today)") } else { ("Tokens today", "Cost today") };
    let mut windows = Vec::new();
    if tokens > 0.0 {
        windows.push(Window::note("tokens", token_label, token_text(tokens)));
    }
    if cost > 0.0 {
        windows.push(Window::note("cost", cost_label, format!("${cost:.2}")));
    }
    windows
}

fn read_meta(path: &Path) -> Option<(f64, f64)> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let stats = &value["stats"];
    let tokens = json_f64(&stats["session_total_llm_tokens"]).unwrap_or(0.0);
    let cost = json_f64(&stats["session_cost"]).unwrap_or(0.0);
    Some((tokens, cost))
}

/// `session_YYYYMMDD_HHMMSS`, the stamp Vibe puts on the folder, in UTC.
fn session_ms(name: &str) -> Option<i64> {
    let stamp = name.strip_prefix("session_")?;
    let (date, time) = stamp.split_once('_')?;
    if date.len() != 8 || time.len() < 6 {
        return None;
    }
    let year: i32 = date[0..4].parse().ok()?;
    let month: u32 = date[4..6].parse().ok()?;
    let day: u32 = date[6..8].parse().ok()?;
    let hour: u32 = time[0..2].parse().ok()?;
    let minute: u32 = time[2..4].parse().ok()?;
    let second: u32 = time[4..6].parse().ok()?;
    let at = chrono::NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, second)?;
    Some(at.and_utc().timestamp_millis())
}

fn local_midnight(now_ms: i64) -> Option<i64> {
    let now = chrono::DateTime::from_timestamp_millis(now_ms)?.with_timezone(&chrono::Local);
    let day = now.date_naive();
    Some(day.and_hms_opt(0, 0, 0)?.and_local_timezone(chrono::Local).single()?.timestamp_millis())
}

fn token_text(n: f64) -> String {
    if n >= 1_000_000.0 {
        format!("{:.1}M tokens", n / 1_000_000.0)
    } else if n >= 1_000.0 {
        format!("{:.1}k tokens", n / 1_000.0)
    } else {
        format!("{n:.0} tokens")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_stamp_is_utc_and_today_sums() {
        let at = session_ms("session_20261025_153045").unwrap();
        assert_eq!(
            at,
            chrono::NaiveDate::from_ymd_opt(2026, 10, 25)
                .unwrap()
                .and_hms_opt(15, 30, 45)
                .unwrap()
                .and_utc()
                .timestamp_millis()
        );
        let rows = notes(1_500_000.0, 1.5, false);
        assert_eq!(rows[0].label, "Tokens today");
        assert_eq!(rows[0].text.as_deref(), Some("1.5M tokens"));
        assert_eq!(rows[1].text.as_deref(), Some("$1.50"));
        assert_eq!(notes(1.0, 0.0, true)[0].label, "Tokens (started today)");
    }

    #[test]
    fn a_session_opened_yesterday_counts_only_todays_lines() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 10, 25)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        let start = local_midnight(now).unwrap();
        let today = chrono::DateTime::from_timestamp_millis(start + 3_600_000).unwrap().to_rfc3339();
        let yesterday = chrono::DateTime::from_timestamp_millis(start - 3_600_000).unwrap().to_rfc3339();
        let root = std::env::temp_dir().join(format!("codync-vibe-dated-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("session_20200101_000000");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("meta.json"), r#"{"stats":{"session_total_llm_tokens":9999,"session_cost":99}}"#)
            .unwrap();
        std::fs::write(
            dir.join("messages.jsonl"),
            format!(
                "{{\"role\":\"assistant\",\"content\":\"old\"}}\n{{\"created_at\":{yesterday:?},\"prompt_tokens\":100,\"completion_tokens\":0,\"cost\":1}}\n{{\"created_at\":{today:?},\"prompt_tokens\":10,\"completion_tokens\":5,\"cost\":0.25}}\n"
            ),
        )
        .unwrap();
        let windows = windows_in(&root, start, now);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(windows[0].label, "Tokens today");
        assert_eq!(windows[0].text.as_deref(), Some("15 tokens"));
        assert_eq!(windows[1].text.as_deref(), Some("$0.25"));
    }

    #[test]
    fn undated_usage_lines_leave_the_session_total() {
        let now = chrono::Utc::now().timestamp_millis();
        let start = local_midnight(now).unwrap();
        let root = std::env::temp_dir().join(format!("codync-vibe-undated-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root
            .join(chrono::DateTime::from_timestamp_millis(now).unwrap().format("session_%Y%m%d_%H%M%S").to_string());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("meta.json"), r#"{"stats":{"session_total_llm_tokens":40,"session_cost":2}}"#).unwrap();
        std::fs::write(dir.join("messages.jsonl"), "{\"prompt_tokens\":10,\"completion_tokens\":5,\"cost\":0.25}\n")
            .unwrap();
        let windows = windows_in(&root, start, now);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(windows[0].label, "Tokens (started today)");
        assert_eq!(windows[0].text.as_deref(), Some("40 tokens"));
        assert_eq!(windows[1].text.as_deref(), Some("$2.00"));
    }

    #[test]
    fn a_stamp_only_session_from_yesterday_adds_nothing() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 10, 25)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        let start = local_midnight(now).unwrap();
        let root = std::env::temp_dir().join(format!("codync-vibe-stamp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("session_20200101_000000");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("meta.json"), r#"{"stats":{"session_total_llm_tokens":50,"session_cost":1}}"#).unwrap();
        let windows = windows_in(&root, start, now);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(windows, []);
    }

    #[test]
    fn an_old_log_untouched_since_midnight_is_not_reread() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 10, 25)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        let start = local_midnight(now).unwrap();
        let today = chrono::DateTime::from_timestamp_millis(start + 3_600_000).unwrap().to_rfc3339();
        let root = std::env::temp_dir().join(format!("codync-vibe-stale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let stale = root.join("session_20200101_000000");
        let fresh = root.join("session_20200102_000000");
        let line = format!("{{\"created_at\":{today:?},\"prompt_tokens\":100,\"completion_tokens\":0,\"cost\":1}}\n");
        for dir in [&stale, &fresh] {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("messages.jsonl"), &line).unwrap();
        }
        let earlier = std::time::UNIX_EPOCH + std::time::Duration::from_millis(u64::try_from(start - 60_000).unwrap());
        let later = std::time::UNIX_EPOCH + std::time::Duration::from_millis(u64::try_from(start + 60_000).unwrap());
        // Windows refuses to change the time of a read-only handle.
        std::fs::File::options().write(true).open(stale.join("messages.jsonl")).unwrap().set_modified(earlier).unwrap();
        std::fs::File::options().write(true).open(fresh.join("messages.jsonl")).unwrap().set_modified(later).unwrap();
        let windows = windows_in(&root, start, now);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(windows[0].text.as_deref(), Some("100 tokens"));
        assert_eq!(windows[1].text.as_deref(), Some("$1.00"));
    }
}
