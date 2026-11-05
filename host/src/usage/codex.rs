//! Codex plan windows.
//!
//! The Codex app-server answers `account/rateLimits/read` with the plan's own
//! windows, including when the newest local rollout has no `rate_limits` (a new
//! session, or a plan the CLI hasn't recorded yet). When the app-server can't
//! answer, the newest rollout's `rate_limits` is the fallback.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use super::Window;

/// Plan windows from Codex, preferring the app-server's live answer.
pub(super) async fn collect() -> Option<Vec<Window>> {
    if let Some(windows) = app_server().await {
        return Some(windows);
    }
    tokio::task::spawn_blocking(rollout).await.ok().flatten()
}

async fn app_server() -> Option<Vec<Window>> {
    let bin = super::command::program("codex")?;
    let mut cmd = tokio::process::Command::new(bin);
    cmd.arg("app-server").kill_on_drop(true).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    if let Some(home) = dirs::home_dir() {
        cmd.current_dir(home);
    }
    let mut child = cmd.spawn().ok()?;
    let answer = tokio::time::timeout(Duration::from_secs(20), handshake(&mut child)).await.ok().flatten();
    let _ = child.kill().await;
    answer
}

/// `initialize`, then `account/rateLimits/read`, over the app-server's
/// newline-delimited JSON-RPC. Notifications in between are skipped.
async fn handshake(child: &mut tokio::process::Child) -> Option<Vec<Window>> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let mut stdin = child.stdin.take()?;
    let stdout = child.stdout.take()?;
    let mut lines = BufReader::new(stdout).lines();
    let init = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {"clientInfo": {"name": "codync", "version": env!("CARGO_PKG_VERSION")}},
    });
    stdin.write_all(format!("{init}\n").as_bytes()).await.ok()?;
    wait_for(&mut lines, 1).await?;
    let read = json!({"jsonrpc": "2.0", "id": 2, "method": "account/rateLimits/read", "params": {}});
    stdin.write_all(format!("{read}\n").as_bytes()).await.ok()?;
    let result = wait_for(&mut lines, 2).await?;
    let windows = parse_app_server(&result);
    (!windows.is_empty()).then_some(windows)
}

async fn wait_for<R: tokio::io::AsyncBufRead + Unpin>(lines: &mut tokio::io::Lines<R>, id: i64) -> Option<Value> {
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(value) = serde_json::from_str::<Value>(&line) else { continue };
        if value["id"].as_i64() == Some(id) {
            return value.get("result").cloned();
        }
    }
    None
}

/// The `account/rateLimits/read` result: the codex limit, else the first limit
/// that has a window.
pub fn parse_app_server(result: &Value) -> Vec<Window> {
    let limits = result["rateLimitsByLimitId"].as_object().and_then(|by_id| {
        by_id
            .get("codex")
            .or_else(|| by_id.values().find(|limit| limit["primary"].is_object() || limit["secondary"].is_object()))
    });
    let limit = limits.unwrap_or_else(|| &result["rateLimits"]);
    windows_from_limits(limit)
}

fn windows_from_limits(limit: &Value) -> Vec<Window> {
    ["primary", "secondary"]
        .into_iter()
        .filter_map(|key| {
            let window = &limit[key];
            let percent = window["usedPercent"].as_f64()?;
            let minutes = window["windowDurationMins"].as_i64().unwrap_or(0);
            let resets = window["resetsAt"].as_i64().map(|s| s.saturating_mul(1000));
            Some(Window::new(key, window_label(minutes, key), percent.clamp(0.0, 100.0), resets))
        })
        .collect()
}

fn window_label(minutes: i64, key: &str) -> String {
    match minutes {
        300 => "5-hour".to_owned(),
        10_080 => "Weekly".to_owned(),
        43_200 => "Monthly".to_owned(),
        m if m > 0 && m % 1440 == 0 => format!("{}-day", m / 1440),
        m if m > 0 => format!("{}-hour", m / 60),
        _ => key.to_owned(),
    }
}

/// Blocking: reads the newest Codex rollout file.
fn rollout() -> Option<Vec<Window>> {
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

pub fn parse_codex(rl: &Value, observed_ms: i64) -> Vec<Window> {
    ["primary", "secondary"]
        .into_iter()
        .filter_map(|k| {
            let w = &rl[k];
            let percent = w["used_percent"].as_f64()?;
            let minutes = w["window_minutes"].as_i64().unwrap_or(0);
            let label = window_label(minutes, k);
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
    fn reads_the_app_server_answer_with_camel_case_windows() {
        let w = parse_app_server(&json!({
            "rateLimitsByLimitId": {"codex": {
                "primary": {"usedPercent": 0, "windowDurationMins": 43200, "resetsAt": 1_796_445_049},
                "secondary": null
            }}
        }));
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].label, "Monthly");
        assert_eq!(w[0].percent, 0.0);
        assert_eq!(w[0].resets_at, Some(1_796_445_049_000));
    }

    #[test]
    fn a_plan_without_windows_is_not_zero_usage() {
        assert_eq!(parse_app_server(&json!({"rateLimitsByLimitId": null, "rateLimits": null})).len(), 0);
        assert_eq!(parse_app_server(&json!({"rateLimits": {"primary": null}})).len(), 0);
    }
}
