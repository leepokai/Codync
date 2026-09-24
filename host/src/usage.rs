//! Subscription usage limits, read on the host only (tokens never leave it).
//!
//! Claude: the statusline `rate_limits` payload (documented) when Claude Code
//! runs, plus the undocumented `/api/oauth/usage` endpoint as a poller using the
//! token Claude Code already stored. Codex: the newest `rate_limits` event in
//! `~/.codex/sessions` rollout files.

use crate::hub::Hub;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

pub fn window(id: &str, label: &str, percent: f64, resets_at_ms: Option<i64>) -> Value {
    json!({"id": id, "label": label, "percent": percent, "resetsAt": resets_at_ms})
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

pub async fn poll(hub: Arc<Hub>) {
    loop {
        refresh(&hub).await;
        tokio::time::sleep(Duration::from_secs(300)).await;
    }
}

static CLAUDE_BACKOFF_UNTIL: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

pub async fn refresh(hub: &Hub) {
    // Live statusline data beats polling an undocumented endpoint.
    let fresh_statusline = hub.usage.lock().unwrap()["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|p| p["id"] == "claude" && p["source"] == "statusline" && p["updatedAt"].as_i64().is_some_and(|t| crate::store::now_ms() - t < 600_000));
    let backing_off = CLAUDE_BACKOFF_UNTIL.lock().unwrap().is_some_and(|t| std::time::Instant::now() < t);
    if !fresh_statusline && !backing_off {
        match claude_oauth_usage().await {
            Ok(Some(p)) => merge(hub, p),
            Ok(None) => {}
            Err(e) => {
                tracing::debug!("claude usage: {e}");
                *CLAUDE_BACKOFF_UNTIL.lock().unwrap() = Some(std::time::Instant::now() + Duration::from_secs(3600));
            }
        }
    }
    if let Some(p) = tokio::task::spawn_blocking(codex_usage).await.ok().flatten() {
        merge(hub, p);
    }
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
        merge(hub, provider("claude", "Claude", windows, "statusline"));
    }
}

fn claude_token() -> Option<String> {
    let raw = if cfg!(target_os = "macos") {
        let out = std::process::Command::new("security")
            .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
            .stdin(std::process::Stdio::null())
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        String::from_utf8(out.stdout).ok()?
    } else {
        std::fs::read_to_string(dirs::home_dir()?.join(".claude/.credentials.json")).ok()?
    };
    let v: Value = serde_json::from_str(raw.trim()).ok()?;
    let oauth = &v["claudeAiOauth"];
    if oauth["expiresAt"].as_i64().is_some_and(|exp| exp < crate::store::now_ms()) {
        return None; // Expired: Claude Code refreshes it next time it runs; we never touch credentials.
    }
    oauth["accessToken"].as_str().map(str::to_owned)
}

fn claude_version() -> String {
    std::process::Command::new("claude")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.split_whitespace().next().map(str::to_owned))
        .unwrap_or_else(|| "2.1.0".into())
}

async fn claude_oauth_usage() -> anyhow::Result<Option<Value>> {
    let Some(token) = tokio::task::spawn_blocking(claude_token).await? else { return Ok(None) };
    let version = tokio::task::spawn_blocking(claude_version).await?;
    let res: Value = reqwest::Client::new()
        .get("https://api.anthropic.com/api/oauth/usage")
        .bearer_auth(token)
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("User-Agent", format!("claude-code/{version}"))
        .timeout(Duration::from_secs(15))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(Some(provider("claude", "Claude", parse_claude(&res), "oauth")))
}

pub fn parse_claude(res: &Value) -> Vec<Value> {
    let labels = [
        ("five_hour", "5-hour"),
        ("seven_day", "Weekly"),
        ("seven_day_opus", "Weekly · Opus"),
        ("seven_day_sonnet", "Weekly · Sonnet"),
    ];
    labels
        .iter()
        .filter_map(|(k, label)| {
            let w = &res[*k];
            let pct = w["utilization"].as_f64()?;
            let resets = w["resets_at"].as_str().and_then(parse_rfc3339_ms);
            Some(window(k, label, pct, resets))
        })
        .collect()
}

/// Minimal RFC 3339 → epoch ms (UTC offsets supported), avoiding a date crate.
pub fn parse_rfc3339_ms(s: &str) -> Option<i64> {
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-').map(|x| x.parse::<i64>());
    let (y, m, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let (clock, offset_min) = if let Some(c) = time.strip_suffix('Z') {
        (c, 0)
    } else {
        let idx = time.rfind(['+', '-'])?;
        let (c, off) = time.split_at(idx);
        let sign = if off.starts_with('-') { -1 } else { 1 };
        let mut o = off[1..].split(':');
        let oh: i64 = o.next()?.parse().ok()?;
        let om: i64 = o.next().unwrap_or("0").parse().ok()?;
        (c, sign * (oh * 60 + om))
    };
    let mut c = clock.split(':');
    let (h, mi) = (c.next()?.parse::<i64>().ok()?, c.next()?.parse::<i64>().ok()?);
    let sec: f64 = c.next().unwrap_or("0").parse().ok()?;
    // Days from civil (Howard Hinnant).
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + h * 3600 + mi * 60 - offset_min * 60;
    Some(secs * 1000 + (sec * 1000.0) as i64)
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
    fn parses_claude_and_codex_shapes() {
        let c = parse_claude(&json!({
            "five_hour": {"utilization": 12.0, "resets_at": "2026-09-25T10:00:00+00:00"},
            "seven_day": {"utilization": 55.5, "resets_at": "2026-09-25T10:00:00.500Z"},
            "seven_day_opus": null
        }));
        assert_eq!(c.len(), 2);
        assert_eq!(c[0]["resetsAt"], 1_790_330_400_000i64);
        assert_eq!(c[1]["resetsAt"], 1_790_330_400_500i64);
        let x = parse_codex(
            &json!({"primary": {"used_percent": 3.0, "window_minutes": 300, "resets_in_seconds": 10},
                    "secondary": {"used_percent": 40.0, "window_minutes": 10080, "resets_at": 100}}),
            1000,
        );
        assert_eq!(x[0]["label"], "5-hour");
        assert_eq!(x[0]["resetsAt"], 11_000);
        assert_eq!(x[1]["resetsAt"], 100_000);
        assert_eq!(parse_rfc3339_ms("1970-01-01T01:00:00+01:00"), Some(0));
    }
}
