//! Current Codex plan windows, read through the CLI's account API without a model turn.

use crate::agent::acp::Acp;
use serde_json::{Value, json};
use std::time::Duration;

use super::Window;

const READ_TIMEOUT: Duration = Duration::from_secs(20);

pub(super) async fn collect() -> Option<Vec<Window>> {
    let Some(bin) = super::command::program("codex") else { return Some(Vec::new()) };
    let home = dirs::home_dir()?;
    let command = format!("{} app-server", crate::shell::quote(&bin.to_string_lossy()));
    let reading = read_live(&command, &home.to_string_lossy(), READ_TIMEOUT).await;
    // A failed sign-in or an unavailable CLI must not leave an expired snapshot on screen.
    Some(reading.unwrap_or_default())
}

async fn read_live(command: &str, cwd: &str, timeout: Duration) -> Option<Vec<Window>> {
    // Reuse the host's JSON-RPC transport and whole-process-tree cleanup.
    let (client, _events) = Acp::spawn(command, cwd, &[]).ok()?;
    let result = tokio::time::timeout(timeout, read_limits(&client)).await;
    client.kill().await;
    match result {
        Ok(Ok(response)) => Some(parse(&response, crate::store::now_ms())),
        Ok(Err(_)) => {
            tracing::debug!("codex account usage was unavailable");
            None
        }
        Err(_) => {
            tracing::debug!("codex account usage timed out");
            None
        }
    }
}

async fn read_limits(client: &Acp) -> anyhow::Result<Value> {
    let initialized = client
        .request(
            "initialize",
            json!({"clientInfo": {"name": "codync", "title": "Codync", "version": env!("CARGO_PKG_VERSION")}}),
        )
        .await;
    initialized?;
    let notified = client.notify("initialized", json!({})).await;
    notified?;
    client.request("account/rateLimits/read", json!({})).await
}

fn parse(response: &Value, now_ms: i64) -> Vec<Window> {
    let limits = response["rateLimitsByLimitId"]
        .get("codex")
        .filter(|v| v.is_object())
        .or_else(|| response.get("rateLimits").filter(|v| v.is_object()));
    let Some(limits) = limits else { return Vec::new() };
    ["primary", "secondary"].into_iter().filter_map(|id| window(id, &limits[id], now_ms)).collect()
}

fn window(id: &str, value: &Value, now_ms: i64) -> Option<Window> {
    let percent = value["usedPercent"].as_f64().filter(|p| p.is_finite() && *p >= 0.0)?;
    let resets = value["resetsAt"].as_i64().map(|s| s.saturating_mul(1000));
    if resets.is_some_and(|at| at <= now_ms) {
        return None;
    }
    let label = match value["windowDurationMins"].as_i64().unwrap_or(0) {
        300 => "5-hour".to_owned(),
        10080 => "Weekly".to_owned(),
        m if m > 0 && m % 1440 == 0 => format!("{}-day", m / 1440),
        m if m > 0 && m % 60 == 0 => format!("{}-hour", m / 60),
        m if m > 0 => format!("{m}-minute"),
        _ => id.to_owned(),
    };
    Some(Window::new(id, label, percent.min(100.0), resets))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_codex_bucket_wins_over_the_compatibility_view() {
        let now = 1_790_000_000_000;
        let reset = now / 1000 + 6 * 86_400 + 13 * 3600;
        let response = json!({
            "rateLimitsByLimitId": {"codex": {
                "primary": {"usedPercent": 12, "windowDurationMins": 300, "resetsAt": reset},
                "secondary": {"usedPercent": 21, "windowDurationMins": 10080, "resetsAt": reset}
            }},
            "rateLimits": {"primary": {"usedPercent": 99, "windowDurationMins": 10080, "resetsAt": 100}}
        });
        let windows = parse(&response, now);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].label, "5-hour");
        assert!((windows[0].percent - 12.0).abs() < f64::EPSILON);
        assert_eq!(windows[1].label, "Weekly");
        assert_eq!(windows[1].resets_at, Some(now + (6 * 24 + 13) * 3_600_000));
    }

    #[test]
    fn the_single_bucket_response_is_still_supported() {
        let windows = parse(
            &json!({"rateLimits": {"primary": {"usedPercent": 25, "windowDurationMins": 15, "resetsAt": 30}}}),
            1000,
        );
        assert_eq!(windows[0].label, "15-minute");
        assert_eq!(windows[0].resets_at, Some(30_000));
    }

    #[test]
    fn expired_or_invalid_readings_are_not_presented_as_current_usage() {
        let response = json!({"rateLimits": {
            "primary": {"usedPercent": 17, "windowDurationMins": 10080, "resetsAt": 100},
            "secondary": {"usedPercent": -1, "windowDurationMins": 300, "resetsAt": 300}
        }});
        assert_eq!(parse(&response, 200_000), Vec::new());
        assert_eq!(parse(&json!({"rateLimits": {"primary": {"resetsAt": 300}}}), 200_000), Vec::new());
    }

    async fn fixture(mode: &str, timeout: Duration) -> Option<Vec<Window>> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let script = root.join("tests/fixtures/codex_usage.py");
        let command = format!("{} {}", crate::shell::python(&script), crate::shell::quote(mode));
        let result = read_live(&command, &root.to_string_lossy(), timeout).await;
        crate::agent::acp::stopped().await;
        result
    }

    #[tokio::test]
    async fn the_probe_initializes_and_reads_limits_without_starting_a_turn() {
        let result = fixture("ready", Duration::from_secs(10)).await;
        let windows = result.unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].label, "Weekly");
        assert!((windows[0].percent - 23.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn authentication_failure_does_not_become_zero_usage() {
        let result = fixture("signed-out", Duration::from_secs(10)).await;
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn an_unresponsive_probe_is_bounded_and_stopped() {
        let result = fixture("hang", Duration::from_millis(100)).await;
        assert_eq!(result, None);
    }
}
