//! Subscription usage limits, read locally on the host — no extra login.
//!
//! Claude: asks the installed `claude` binary (`claude -p /usage`, a local
//! command that costs nothing), plus the `rate_limits` Claude Code hands its
//! status line and the Claude ACP adapter's rate-limit events. Codex: the
//! newest `rate_limits` event in `~/.codex/sessions` rollout files.

use crate::LockExt;
use crate::hub::Hub;
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const POLL_EVERY: Duration = Duration::from_secs(5 * 60);

/// Everything the apps show about limits (wire shape of the `usage` event).
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Usage {
    pub providers: Vec<Provider>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: &'static str,
    pub name: &'static str,
    pub windows: Vec<Window>,
    /// Where the numbers came from: `claude` (CLI), `statusline`, `agent`, `sessions`.
    pub source: &'static str,
    pub updated_at: i64,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub id: String,
    pub label: String,
    pub percent: f64,
    /// Epoch milliseconds, when known.
    pub resets_at: Option<i64>,
    /// Human text when only that is known ("Sep 26 at 12pm (Asia/Taipei)").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_text: Option<String>,
}

impl Window {
    fn new(id: impl Into<String>, label: impl Into<String>, percent: f64, resets_at: Option<i64>) -> Self {
        Self { id: id.into(), label: label.into(), percent, resets_at, resets_text: None }
    }
}

impl Usage {
    /// Replaces `new` windows (by id) in provider `id`, keeping the provider's other windows.
    fn merge(&mut self, id: &'static str, name: &'static str, source: &'static str, new: Vec<Window>) {
        let mut windows = self.providers.iter().find(|p| p.id == id).map(|p| p.windows.clone()).unwrap_or_default();
        windows.retain(|w| !new.iter().any(|n| n.id == w.id));
        windows.extend(new);
        windows.sort_by(|a, b| a.id.cmp(&b.id));
        self.providers.retain(|p| p.id != id);
        self.providers.push(Provider { id, name, windows, source, updated_at: crate::store::now_ms() });
        self.providers.sort_by_key(|p| p.id);
    }
}

fn merge(hub: &Hub, id: &'static str, name: &'static str, source: &'static str, windows: Vec<Window>) {
    let mut usage = hub.usage.locked().clone();
    usage.merge(id, name, source, windows);
    hub.set_usage(usage);
}

pub async fn poll(hub: Arc<Hub>) {
    loop {
        refresh(&hub).await;
        tokio::time::sleep(POLL_EVERY).await;
    }
}

pub async fn refresh(hub: &Hub) {
    if let Some(windows) = tokio::task::spawn_blocking(codex_usage).await.ok().flatten() {
        merge(hub, "codex", "Codex", "sessions", windows);
    }
    if let Some(windows) = claude_cli_usage().await {
        merge(hub, "claude", "Claude", "claude", windows);
    }
}

/// `claude -p /usage`: Claude Code's own local usage report, no model call.
async fn claude_cli_usage() -> Option<Vec<Window>> {
    let bin = crate::backends::which("claude")?;
    let out = tokio::time::timeout(
        Duration::from_secs(60),
        tokio::process::Command::new(bin)
            .args(["-p", "/usage", "--output-format", "json", "--no-session-persistence"])
            .current_dir(dirs::home_dir()?)
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true)
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
pub fn parse_claude_usage(text: &str) -> Vec<Window> {
    text.lines()
        .filter_map(|line| {
            let (name, rest) = line.trim().split_once(": ")?;
            let percent: f64 = rest.split_once("% used")?.0.trim().parse().ok()?;
            let resets = rest.split_once("resets ").map(|(_, r)| r.trim().to_owned());
            let (id, label) = match name {
                "Current session" => ("five_hour".to_owned(), "5-hour".to_owned()),
                "Current week (all models)" => ("seven_day".to_owned(), "Weekly".to_owned()),
                n => {
                    let model = n.strip_prefix("Current week (")?.strip_suffix(')')?;
                    (format!("seven_day_{}", model.to_lowercase()), format!("Weekly · {model}"))
                }
            };
            Some(Window { resets_text: resets, ..Window::new(id, label, percent, None) })
        })
        .collect()
}

/// `POST /ingest/statusline` body: Claude Code's statusline JSON.
pub fn ingest_statusline(hub: &Hub, body: &Value) {
    let rl = &body["rate_limits"];
    let windows: Vec<Window> = [("five_hour", "5-hour"), ("seven_day", "Weekly")]
        .into_iter()
        .filter_map(|(key, label)| {
            let percent = rl[key]["used_percentage"].as_f64()?;
            Some(Window::new(key, label, percent, rl[key]["resets_at"].as_i64().map(|s| s.saturating_mul(1000))))
        })
        .collect();
    if !windows.is_empty() {
        merge(hub, "claude", "Claude", "statusline", windows);
    }
}

/// Claude ACP adapter: `usage_update._meta["_claude/rateLimit"]` (the SDK's `SDKRateLimitInfo`).
pub fn ingest_claude_rate_limit(hub: &Hub, info: &Value) {
    if let Some(w) = claude_window(info) {
        merge(hub, "claude", "Claude", "agent", vec![w]);
    }
}

fn claude_window(info: &Value) -> Option<Window> {
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
    let percent = if u <= 1.0 { u * 100.0 } else { u };
    // Seconds, but accept milliseconds too.
    let resets = info["resetsAt"].as_i64().map(|t| if t > 100_000_000_000 { t } else { t.saturating_mul(1000) });
    Some(Window::new(kind, label, percent, resets))
}

/// Blocking. `sessions/YYYY/MM/DD/rollout-*.jsonl`: descend into the largest dir each level.
fn newest_rollout(root: &Path) -> Option<PathBuf> {
    let mut dir = root.to_path_buf();
    for _ in 0..3 {
        let mut subs: Vec<PathBuf> =
            std::fs::read_dir(&dir).ok()?.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir()).collect();
        subs.sort();
        dir = subs.pop()?;
    }
    std::fs::read_dir(&dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

/// Blocking: reads the newest Codex rollout file.
fn codex_usage() -> Option<Vec<Window>> {
    let path = newest_rollout(&dirs::home_dir()?.join(".codex/sessions"))?;
    let text = std::fs::read_to_string(&path).ok()?;
    let line = text.lines().rev().find(|l| l.contains("\"rate_limits\""))?;
    let v: Value = serde_json::from_str(line).ok()?;
    let rl = if v["payload"]["rate_limits"].is_object() { &v["payload"]["rate_limits"] } else { &v["rate_limits"] };
    let modified_ms = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
    let windows = parse_codex(rl, modified_ms);
    (!windows.is_empty()).then_some(windows)
}

pub fn parse_codex(rl: &Value, observed_ms: i64) -> Vec<Window> {
    ["primary", "secondary"]
        .into_iter()
        .filter_map(|k| {
            let w = &rl[k];
            let percent = w["used_percent"].as_f64()?;
            let minutes = w["window_minutes"].as_i64().unwrap_or(0);
            let label = match minutes {
                300 => "5-hour".to_owned(),
                10080 => "Weekly".to_owned(),
                m if m > 0 && m % 1440 == 0 => format!("{}-day", m / 1440),
                m if m > 0 => format!("{}-hour", m / 60),
                _ => k.to_owned(),
            };
            let resets = w["resets_at"].as_i64().map(|s| s.saturating_mul(1000)).or_else(|| {
                w["resets_in_seconds"].as_i64().map(|s| observed_ms.saturating_add(s.saturating_mul(1000)))
            });
            Some(Window::new(k, label, percent, resets))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_codex_rollout_limits() {
        let x = parse_codex(
            &json!({"primary": {"used_percent": 3.0, "window_minutes": 300, "resets_in_seconds": 10},
                    "secondary": {"used_percent": 40.0, "window_minutes": 10080, "resets_at": 100}}),
            1000,
        );
        assert_eq!(x[0].label, "5-hour");
        assert_eq!(x[0].resets_at, Some(11_000));
        assert_eq!(x[1].resets_at, Some(100_000));
    }

    #[test]
    fn parses_claude_usage_report() {
        let w = parse_claude_usage(
            "You are currently using your subscription\n\nCurrent session: 11% used · resets Sep 25 at 5am (Asia/Taipei)\nCurrent week (all models): 53% used · resets Sep 26 at 12pm (Asia/Taipei)\nCurrent week (Fable): 83% used · resets Sep 26 at 12pm (Asia/Taipei)\n\nLast 24h · 794 requests",
        );
        assert_eq!(w.len(), 3);
        assert_eq!(w[0].id, "five_hour");
        assert!((w[0].percent - 11.0).abs() < f64::EPSILON);
        assert_eq!(w[0].resets_text.as_deref(), Some("Sep 25 at 5am (Asia/Taipei)"));
        assert_eq!(w[2].label, "Weekly · Fable");
    }

    #[test]
    fn claude_rate_limit_fraction_becomes_percent() {
        let w = claude_window(&json!({"rateLimitType": "five_hour", "utilization": 0.42, "resetsAt": 1_790_000_000}))
            .unwrap();
        assert!((w.percent - 42.0).abs() < 1e-9);
        assert_eq!(w.resets_at, Some(1_790_000_000_000));
        assert!(claude_window(&json!({"rateLimitType": "overage", "utilization": 0.1})).is_none());
    }

    #[test]
    fn merge_replaces_only_matching_windows() {
        let mut u = Usage::default();
        u.merge(
            "claude",
            "Claude",
            "claude",
            vec![Window::new("five_hour", "5-hour", 10.0, None), Window::new("seven_day", "Weekly", 50.0, None)],
        );
        u.merge("claude", "Claude", "statusline", vec![Window::new("five_hour", "5-hour", 12.0, None)]);
        let c = &u.providers[0];
        assert_eq!((c.windows.len(), c.source), (2, "statusline"));
        assert!((c.windows[0].percent - 12.0).abs() < f64::EPSILON, "five_hour updated");
        assert_eq!(serde_json::to_value(&u).unwrap()["providers"][0]["windows"][1]["resetsAt"], Value::Null);
    }
}
