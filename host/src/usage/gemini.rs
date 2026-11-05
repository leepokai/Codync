//! Gemini CLI's plan, from the CLI's own `/usage` report (an alias of `/stats`).
//! The command runs locally inside the CLI — no model call — so polling it costs
//! nothing. When the account can't report (Code Assist for individuals retired,
//! a missing auth method), the CLI answers with an `error` and the probe stays
//! silent, like every other collector.

use std::time::Duration;

use serde_json::Value;

use super::Window;

pub(super) async fn collect() -> Option<Vec<Window>> {
    let bin = super::command::program("gemini")?;
    let text =
        super::command::output(&bin, &["-p", "/usage", "--skip-trust", "-o", "json"], None, Duration::from_secs(60))
            .await?;
    parse_gemini_report(&text)
}

/// The headless report: `{"session_id": …, "result": …}` on success,
/// `{"session_id": …, "error": {…}}` when the CLI can't answer. A leading
/// banner is allowed; parsing starts at the first `{`.
pub(super) fn parse_gemini_report(text: &str) -> Option<Vec<Window>> {
    let start = text.find('{').unwrap_or(0);
    let value: Value = serde_json::from_str(&text[start..]).ok()?;
    if value["error"].is_object() {
        return None;
    }
    let result = &value["result"];
    let windows = if result.is_object() {
        windows_from_stats(result)
    } else {
        let report = result.as_str().unwrap_or_default();
        if let Ok(object) = serde_json::from_str::<Value>(report)
            && object.is_object()
        {
            windows_from_stats(&object)
        } else {
            windows_from_text(report)
        }
    };
    (!windows.is_empty()).then_some(windows)
}

/// A `/stats` result object: a pooled window, one window per quota bucket, and
/// the credit balance as a note.
fn windows_from_stats(stats: &Value) -> Vec<Window> {
    let mut windows = Vec::new();
    if let Some(w) = pooled(stats) {
        windows.push(w);
    }
    if let Some(buckets) = stats["quotas"]["buckets"].as_array() {
        for bucket in buckets {
            if let Some(w) = bucket_window(bucket) {
                windows.push(w);
            }
        }
    }
    if let Some(credits) = stats["creditBalance"].as_f64()
        && credits > 0.0
    {
        windows.push(Window::note("credits", "Credit balance", credits_note(credits)));
    }
    windows
}

/// `pooledRemaining` / `pooledLimit`: the plan's combined share used.
fn pooled(stats: &Value) -> Option<Window> {
    let remaining = stats["pooledRemaining"].as_f64()?;
    let limit = stats["pooledLimit"].as_f64()?;
    if limit <= 0.0 {
        return None;
    }
    let percent = ((limit - remaining) / limit * 100.0).clamp(0.0, 100.0);
    Some(Window::new("pooled", "Pooled", percent, reset(&stats["pooledResetTime"])))
}

/// One quota bucket: `remainingFraction` is the share still left, 0–1.
fn bucket_window(bucket: &Value) -> Option<Window> {
    let model = bucket["modelId"].as_str()?;
    let fraction = bucket["remainingFraction"].as_f64()?;
    if !(0.0..=1.0).contains(&fraction) {
        return None;
    }
    let percent = ((1.0 - fraction) * 100.0).clamp(0.0, 100.0);
    Some(Window::new(format!("model_{model}"), model.to_owned(), percent, reset(&bucket["resetTime"])))
}

/// A reset time: epoch seconds or milliseconds, or an ISO 8601 string.
fn reset(v: &Value) -> Option<i64> {
    let epoch = |n: i64| if n > 100_000_000_000 { n } else { n.saturating_mul(1000) };
    if let Some(n) = v.as_i64() {
        return Some(epoch(n));
    }
    let s = v.as_str()?;
    if let Ok(n) = s.parse::<i64>() {
        return Some(epoch(n));
    }
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp_millis())
}

fn credits_note(credits: f64) -> String {
    if credits.fract() == 0.0 { format!("{credits:.0} credits") } else { format!("{credits} credits") }
}

/// A human-readable report: `Pooled: 53% used · resets Sep 26 at 12pm`.
fn windows_from_text(text: &str) -> Vec<Window> {
    text.lines()
        .filter_map(|line| {
            let (name, rest) = line.trim().split_once(": ").unwrap_or((line.trim(), ""));
            let percent: f64 = rest.split_once("% used")?.0.trim().parse().ok()?;
            let resets = rest.split_once("resets ").map(|(_, r)| r.trim().to_owned());
            let (id, label) = if name.to_lowercase().contains("pooled") {
                ("pooled".to_owned(), "Pooled".to_owned())
            } else {
                let id: String =
                    name.to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                (id, name.to_owned())
            };
            Some(Window { resets_text: resets, ..Window::new(id, label, percent, None) })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stats_report_yields_pooled_buckets_and_credits() {
        let text = r#"{
            "session_id": "abc",
            "result": {
                "type": "stats",
                "tier": "Free",
                "creditBalance": 250,
                "pooledRemaining": 900,
                "pooledLimit": 1000,
                "pooledResetTime": 1792830000,
                "quotas": { "buckets": [
                    { "modelId": "gemini-2.5-pro", "remainingAmount": "300", "remainingFraction": 0.3, "resetTime": 1792830000 }
                ] }
            }
        }"#;
        let w = parse_gemini_report(text).unwrap();
        let pooled = w.iter().find(|x| x.id == "pooled").unwrap();
        assert!((pooled.percent - 10.0).abs() < 1e-9);
        assert_eq!(pooled.resets_at, Some(1_792_830_000_000));
        let pro = w.iter().find(|x| x.id == "model_gemini-2.5-pro").unwrap();
        assert!((pro.percent - 70.0).abs() < 1e-9);
        let credits = w.iter().find(|x| x.id == "credits").unwrap();
        assert_eq!(credits.text.as_deref(), Some("250 credits"));
    }

    #[test]
    fn an_error_report_is_silence() {
        let text = r#"{"session_id": "abc", "error": { "type": "Error", "message": "Please set an Auth method", "code": 41 }}"#;
        assert!(parse_gemini_report(text).is_none());
    }

    #[test]
    fn a_text_report_parses_percent_and_reset() {
        let text = r#"{"session_id": "abc", "result": "Pooled: 53% used · resets Nov 5 at 12pm\nPro model: 20% used"}"#;
        let w = parse_gemini_report(text).unwrap();
        let pooled = w.iter().find(|x| x.id == "pooled").unwrap();
        assert!((pooled.percent - 53.0).abs() < 1e-9);
        assert_eq!(pooled.resets_text.as_deref(), Some("Nov 5 at 12pm"));
        assert_eq!(w.len(), 2);
    }

    #[test]
    fn a_result_without_quota_is_silence() {
        let text = r#"{"session_id": "abc", "result": "ok"}"#;
        assert!(parse_gemini_report(text).is_none());
    }

    #[test]
    fn iso_reset_times_are_accepted() {
        let stats: Value = serde_json::from_str(
            r#"{"pooledRemaining": 50, "pooledLimit": 100, "pooledResetTime": "2026-11-05T12:00:00Z"}"#,
        )
        .unwrap();
        let w = pooled(&stats).unwrap();
        assert!((w.percent - 50.0).abs() < 1e-9);
        assert_eq!(
            w.resets_at,
            Some(chrono::DateTime::parse_from_rfc3339("2026-11-05T12:00:00Z").unwrap().timestamp_millis())
        );
    }
}
