//! Subscription usage limits, read locally on the host — no extra login.
//!
//! Claude: asks the installed `claude` binary (`claude -p /usage`, a local
//! command that costs nothing), plus the `rate_limits` Claude Code hands its
//! status line and the Claude ACP adapter's rate-limit events. Codex: the
//! newest `rate_limits` event in `~/.codex/sessions` rollout files.

use crate::hub::Hub;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

pub fn window(id: &str, label: &str, percent: f64, resets_at_ms: Option<i64>) -> Value {
    json!({"id": id, "label": label, "percent": percent, "resetsAt": resets_at_ms})
}

fn window_text(id: &str, label: &str, percent: f64, resets_text: &str) -> Value {
    json!({"id": id, "label": label, "percent": percent, "resetsAt": null, "resetsText": resets_text})
}

fn provider(id: &str, name: &str, windows: Vec<Value>, source: &str) -> Value {
    json!({"id": id, "name": name, "windows": windows, "source": source, "updatedAt": crate::store::now_ms()})
}

fn merge(hub: &Hub, p: Value) {
    let mut usage = hub.usage.lock().unwrap().clone();
    let list = usage["providers"].as_array_mut().unwrap();
    list.retain(|x| x["id"] != p["id"]);
    list.push(p);
    list.sort_by_key(|x| x["id"].as_str().unwrap_or_default().to_owned());
    hub.set_usage(usage);
}

/// Replaces or adds windows for one provider, keeping the others it already had.
fn merge_windows(hub: &Hub, id: &str, name: &str, source: &str, new: Vec<Value>) {
    let existing = hub.usage.lock().unwrap()["providers"]
        .as_array()
        .and_then(|l| l.iter().find(|p| p["id"] == id))
        .and_then(|p| p["windows"].as_array().cloned())
        .unwrap_or_default();
    let mut windows: Vec<Value> = existing.into_iter().filter(|w| !new.iter().any(|n| n["id"] == w["id"])).collect();
    windows.extend(new);
    windows.sort_by_key(|w| w["id"].as_str().unwrap_or_default().to_owned());
    merge(hub, provider(id, name, windows, source));
}

pub async fn poll(hub: Arc<Hub>) {
    loop {
        refresh(&hub).await;
        tokio::time::sleep(Duration::from_secs(300)).await;
    }
}

pub async fn refresh(hub: &Hub) {
    if let Some(p) = tokio::task::spawn_blocking(codex_usage).await.ok().flatten() {
        merge(hub, p);
    }
    if let Some(windows) = claude_cli_usage().await {
        merge_windows(hub, "claude", "Claude", "claude", windows);
    }
}

/// `claude -p /usage`: Claude Code's own local usage report, no model call.
async fn claude_cli_usage() -> Option<Vec<Value>> {
    let bin = crate::backends::which("claude")?;
    let out = tokio::time::timeout(
        Duration::from_secs(60),
        tokio::process::Command::new(bin)
            .args(["-p", "/usage", "--output-format", "json", "--no-session-persistence"])
            .current_dir(dirs::home_dir()?)
            .stdin(std::process::Stdio::null())
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    let v: Value = serde_json::from_slice(&out.stdout).ok()?;
    let windows = parse_claude_usage(v["result"].as_str()?);
    (!windows.is_empty()).then_some(windows)
}

/// Lines like `Current week (all models): 53% used · resets Sep 26 at 12pm (Asia/Taipei)`.
pub fn parse_claude_usage(text: &str) -> Vec<Value> {
    text.lines()
        .filter_map(|line| {
            let (name, rest) = line.trim().split_once(": ")?;
            let pct: f64 = rest.split_once("% used")?.0.trim().parse().ok()?;
            let resets = rest.split_once("resets ").map(|(_, r)| r.trim()).unwrap_or_default();
            let (id, label) = match name {
                "Current session" => ("five_hour".to_owned(), "5-hour".to_owned()),
                "Current week (all models)" => ("seven_day".to_owned(), "Weekly".to_owned()),
                n => {
                    let model = n.strip_prefix("Current week (")?.strip_suffix(')')?;
                    (format!("seven_day_{}", model.to_lowercase()), format!("Weekly · {model}"))
                }
            };
            Some(window_text(&id, &label, pct, resets))
        })
        .collect()
}

/// `POST /ingest/statusline` body: Claude Code's statusline JSON.
pub fn ingest_statusline(hub: &Hub, body: &Value) {
    let rl = &body["rate_limits"];
    let mut windows = vec![];
    for (key, label) in [("five_hour", "5-hour"), ("seven_day", "Weekly")] {
        if let Some(p) = rl[key]["used_percentage"].as_f64() {
            windows.push(window(key, label, p, rl[key]["resets_at"].as_i64().map(|s| s * 1000)));
        }
    }
    if !windows.is_empty() {
        merge_windows(hub, "claude", "Claude", "statusline", windows);
    }
}

/// Claude ACP adapter: `usage_update._meta["_claude/rateLimit"]` (the SDK's SDKRateLimitInfo).
pub fn ingest_claude_rate_limit(hub: &Hub, info: &Value) {
    if let Some(w) = claude_window(info) {
        merge_windows(hub, "claude", "Claude", "agent", vec![w]);
    }
}

fn claude_window(info: &Value) -> Option<Value> {
    let kind = info["rateLimitType"].as_str()?;
    let label = match kind {
        "five_hour" => "5-hour",
        "seven_day" => "Weekly",
        "seven_day_opus" => "Weekly · Opus",
        "seven_day_sonnet" => "Weekly · Sonnet",
        _ => return None,
    };
    let u = info["utilization"].as_f64()?;
    // The SDK reports a 0–1 fraction (from the unified rate-limit headers).
    let pct = if u <= 1.0 { u * 100.0 } else { u };
    let resets = info["resetsAt"].as_i64().map(|t| if t > 100_000_000_000 { t } else { t * 1000 });
    Some(window(kind, label, pct, resets))
}

fn newest_rollout(root: &Path) -> Option<PathBuf> {
    // sessions/YYYY/MM/DD/rollout-*.jsonl — descend into the lexicographically largest dir each level.
    let mut dir = root.to_path_buf();
    for _ in 0..3 {
        let mut subs: Vec<PathBuf> = std::fs::read_dir(&dir).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect();
        subs.sort();
        dir = subs.pop()?;
    }
    std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

fn codex_usage() -> Option<Value> {
    let path = newest_rollout(&dirs::home_dir()?.join(".codex/sessions"))?;
    let text = std::fs::read_to_string(&path).ok()?;
    let line = text.lines().rev().find(|l| l.contains("\"rate_limits\""))?;
    let v: Value = serde_json::from_str(line).ok()?;
    let rl = if v["payload"]["rate_limits"].is_object() { &v["payload"]["rate_limits"] } else { &v["rate_limits"] };
    let modified_ms = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let windows = parse_codex(rl, modified_ms);
    (!windows.is_empty()).then(|| provider("codex", "Codex", windows, "sessions"))
}

pub fn parse_codex(rl: &Value, observed_ms: i64) -> Vec<Value> {
    ["primary", "secondary"]
        .iter()
        .filter_map(|k| {
            let w = &rl[*k];
            let pct = w["used_percent"].as_f64()?;
            let minutes = w["window_minutes"].as_i64().unwrap_or(0);
            let label = match minutes {
                300 => "5-hour".to_owned(),
                10080 => "Weekly".to_owned(),
                m if m > 0 && m % 1440 == 0 => format!("{}-day", m / 1440),
                m if m > 0 => format!("{}-hour", m / 60),
                _ => (*k).to_owned(),
            };
            let resets = w["resets_at"]
                .as_i64()
                .map(|s| s * 1000)
                .or_else(|| w["resets_in_seconds"].as_i64().map(|s| observed_ms + s * 1000));
            Some(window(k, &label, pct, resets))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_codex_rollout_limits() {
        let x = parse_codex(
            &json!({"primary": {"used_percent": 3.0, "window_minutes": 300, "resets_in_seconds": 10},
                    "secondary": {"used_percent": 40.0, "window_minutes": 10080, "resets_at": 100}}),
            1000,
        );
        assert_eq!(x[0]["label"], "5-hour");
        assert_eq!(x[0]["resetsAt"], 11_000);
        assert_eq!(x[1]["resetsAt"], 100_000);
    }

    #[test]
    fn parses_claude_usage_report() {
        let w = parse_claude_usage(
            "You are currently using your subscription\n\nCurrent session: 11% used · resets Sep 25 at 5am (Asia/Taipei)\nCurrent week (all models): 53% used · resets Sep 26 at 12pm (Asia/Taipei)\nCurrent week (Fable): 83% used · resets Sep 26 at 12pm (Asia/Taipei)\n\nLast 24h · 794 requests",
        );
        assert_eq!(w.len(), 3);
        assert_eq!(w[0]["id"], "five_hour");
        assert_eq!(w[0]["percent"], 11.0);
        assert_eq!(w[0]["resetsText"], "Sep 25 at 5am (Asia/Taipei)");
        assert_eq!(w[2]["label"], "Weekly · Fable");
    }

    #[test]
    fn claude_rate_limit_fraction_becomes_percent() {
        let w = claude_window(&json!({"rateLimitType": "five_hour", "utilization": 0.42, "resetsAt": 1_790_000_000}));
        let w = w.unwrap();
        assert_eq!(w["percent"], 42.0);
        assert_eq!(w["resetsAt"], 1_790_000_000_000i64);
        assert!(claude_window(&json!({"rateLimitType": "overage", "utilization": 0.1})).is_none());
    }
}
