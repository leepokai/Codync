//! Grok Build's plan window, from the CLI's own `_x.ai/billing` ACP method.
//!
//! The CLI talks to xAI's billing service for the current period. Its answer is
//! a proto3 JSON object, so a period with no usage yet has no
//! `creditUsagePercent` at all: absent means 0 %, not "unknown".

use serde_json::{Value, json};
use std::process::Stdio;
use std::time::Duration;

use super::Window;

pub(super) async fn collect() -> Option<Vec<Window>> {
    let bin = super::command::program("grok")?;
    // No saved login: starting the agent would only wait for a sign-in.
    if !dirs::home_dir()?.join(".grok/auth.json").is_file() {
        return None;
    }
    let mut cmd = tokio::process::Command::new(bin);
    cmd.args(["agent", "stdio"]).kill_on_drop(true).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    if let Some(home) = dirs::home_dir() {
        cmd.current_dir(home);
    }
    let mut child = cmd.spawn().ok()?;
    let answer = tokio::time::timeout(Duration::from_secs(20), handshake(&mut child)).await.ok().flatten();
    let _ = child.kill().await;
    answer
}

async fn handshake(child: &mut tokio::process::Child) -> Option<Vec<Window>> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let mut stdin = child.stdin.take()?;
    let stdout = child.stdout.take()?;
    let mut lines = BufReader::new(stdout).lines();
    let init = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientCapabilities": {"fs": {"readTextFile": false, "writeTextFile": false}},
            "clientInfo": {"name": "codync", "version": env!("CARGO_PKG_VERSION")},
        },
    });
    stdin.write_all(format!("{init}\n").as_bytes()).await.ok()?;
    wait_for(&mut lines, 1).await?;
    let billing = json!({"jsonrpc": "2.0", "id": 2, "method": "_x.ai/billing", "params": {}});
    stdin.write_all(format!("{billing}\n").as_bytes()).await.ok()?;
    let result = wait_for(&mut lines, 2).await?;
    let windows = parse_billing(&result);
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

/// The billing answer: the current period's share used, and when it ends.
pub fn parse_billing(result: &Value) -> Vec<Window> {
    let config = &result["config"];
    let period = &config["currentPeriod"];
    if !config.is_object() || !period.is_object() {
        return Vec::new();
    }
    let Some(percent) = credit_usage_percent(config) else { return Vec::new() };
    let start = time(&period["start"]);
    let end = time(&period["end"]);
    let duration = match (start, end) {
        (Some(start), Some(end)) => Some((end - start) / 60_000),
        _ => None,
    };
    let period_type = period["type"].as_str().or_else(|| period["periodType"].as_str()).unwrap_or_default();
    let label = if period_type.to_ascii_lowercase().contains("weekly") {
        "Weekly"
    } else if period_type.to_ascii_lowercase().contains("monthly") {
        "Monthly"
    } else {
        match duration {
            Some(mins) if mins >= 28 * 1440 => "Monthly",
            Some(mins) if mins >= 7 * 1440 => "Weekly",
            _ => "Current period",
        }
    };
    vec![Window::new("grok", label, percent, end)]
}

fn time(value: &Value) -> Option<i64> {
    let text = value.as_str()?;
    chrono::DateTime::parse_from_rfc3339(text).ok().map(|t| t.timestamp_millis())
}

/// proto3 JSON omits a zero scalar: no `creditUsagePercent` and no deprecated
/// `monthlyLimit` above zero means the period has used nothing yet.
fn credit_usage_percent(config: &Value) -> Option<f64> {
    if let Some(value) = config.get("creditUsagePercent") {
        return value.as_f64().filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 100.0));
    }
    match cent(config, "monthlyLimit") {
        Cent::Absent | Cent::Zero => Some(0.0),
        Cent::Value(limit) if limit > 0.0 => {
            let used = match cent(config, "used") {
                Cent::Absent | Cent::Zero => 0.0,
                Cent::Value(used) if used >= 0.0 => used,
                Cent::Value(_) | Cent::Bad => return None,
            };
            Some((used / limit * 100.0).clamp(0.0, 100.0))
        }
        Cent::Value(_) | Cent::Bad => None,
    }
}

/// A `$0` Cent arrives as `{}`; a malformed one as something other than `val`.
enum Cent {
    Absent,
    Zero,
    Value(f64),
    Bad,
}

fn cent(config: &Value, key: &str) -> Cent {
    let Some(value) = config.get(key) else { return Cent::Absent };
    let Some(object) = value.as_object() else { return Cent::Bad };
    match object.get("val") {
        None => Cent::Zero,
        Some(value) => value.as_f64().filter(|v| v.is_finite()).map_or(Cent::Bad, Cent::Value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_weekly_period_with_no_usage_is_zero_percent() {
        let w = parse_billing(&json!({"config": {"currentPeriod": {
            "type": "USAGE_PERIOD_TYPE_WEEKLY",
            "start": "2026-11-01T00:00:00+00:00",
            "end": "2026-11-08T00:00:00+00:00"
        }}}));
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].label, "Weekly");
        assert_eq!(w[0].percent, 0.0);
        assert_eq!(w[0].resets_at, Some(1_794_096_000_000));
    }

    #[test]
    fn a_reported_percentage_is_the_share_used() {
        let w = parse_billing(&json!({"config": {
            "creditUsagePercent": 42.5,
            "currentPeriod": {"type": "USAGE_PERIOD_TYPE_MONTHLY", "end": "2026-12-01T00:00:00Z"}
        }}));
        assert_eq!(w[0].label, "Monthly");
        assert_eq!(w[0].percent, 42.5);
    }

    #[test]
    fn the_deprecated_dollar_fields_still_read() {
        let w = parse_billing(&json!({"config": {
            "monthlyLimit": {"val": 2000},
            "used": {"val": 500},
            "currentPeriod": {"start": "2026-11-01T00:00:00Z", "end": "2026-11-08T00:00:00Z"}
        }}));
        assert_eq!(w[0].percent, 25.0);
        assert_eq!(w[0].label, "Weekly");
    }

    #[test]
    fn a_malformed_percentage_is_silence_not_zero() {
        assert_eq!(
            parse_billing(&json!({"config": {
                "creditUsagePercent": "lots",
                "currentPeriod": {"end": "2026-11-08T00:00:00Z"}
            }}))
            .len(),
            0
        );
        assert_eq!(parse_billing(&json!({})).len(), 0);
    }
}
