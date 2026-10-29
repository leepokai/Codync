//! Current Codex plan windows, read through the CLI's account API without a model turn.

use crate::agent::acp::Acp;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

use super::Window;

const READ_TIMEOUT: Duration = Duration::from_secs(20);

pub(super) async fn collect() -> Option<Vec<Window>> {
    let reading = read_live(start_codex, READ_TIMEOUT).await;
    // A failed sign-in or an unavailable CLI must not leave an expired snapshot on screen.
    match reading {
        Ok(windows) => Some(windows.unwrap_or_default()),
        Err(error) => {
            tracing::debug!(error = format!("{error:#}"), "codex account usage was unavailable");
            Some(Vec::new())
        }
    }
}

fn start_codex() -> Result<Option<Arc<Acp>>> {
    let Some(bin) = super::command::program("codex") else { return Ok(None) };
    let home = dirs::home_dir().context("locating the host user's home directory")?;
    let command = format!("{} app-server", crate::shell::quote(&bin.to_string_lossy()));
    let (client, _events) = Acp::spawn(&command, &home.to_string_lossy(), &[])?;
    Ok(Some(client))
}

async fn read_live(
    start: impl FnOnce() -> Result<Option<Arc<Acp>>> + Send + 'static,
    timeout: Duration,
) -> Result<Option<Vec<Window>>> {
    // Lookup and process creation can block; the transport's async reads stay on the runtime.
    let started = tokio::task::spawn_blocking(start).await;
    let client = started.context("joining Codex usage startup")?.context("starting Codex usage probe")?;
    let Some(client) = client else { return Ok(None) };
    let result = tokio::time::timeout(timeout, read_limits(&client)).await;
    client.kill().await;
    let response = result.with_context(|| format!("Codex usage read exceeded {}ms", timeout.as_millis()))??;
    Ok(Some(parse(&response, crate::store::now_ms())))
}

async fn read_limits(client: &Acp) -> Result<Value> {
    let initialized = client
        .request(
            "initialize",
            json!({"clientInfo": {"name": "codync", "title": "Codync", "version": env!("CARGO_PKG_VERSION")}}),
        )
        .await;
    initialized.context("initializing Codex usage probe")?;
    let notified = client.notify("initialized", json!({})).await;
    notified.context("announcing Codex usage initialization")?;
    let limits = client.request("account/rateLimits/read", json!({})).await;
    limits.context("reading Codex account rate limits")
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

    fn fixture_start(mode: &str) -> impl FnOnce() -> Result<Option<Arc<Acp>>> + Send + 'static {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let script = root.join("tests/fixtures/codex_usage.py");
        let command = format!("{} {}", crate::shell::python(&script), crate::shell::quote(mode));
        move || {
            let (client, _events) = Acp::spawn(&command, &root.to_string_lossy(), &[])?;
            Ok(Some(client))
        }
    }

    async fn fixture(mode: &str, timeout: Duration) -> Result<Option<Vec<Window>>> {
        let result = read_live(fixture_start(mode), timeout).await;
        crate::agent::acp::stopped().await;
        result
    }

    #[tokio::test]
    async fn the_probe_initializes_and_reads_limits_without_starting_a_turn() {
        let result = fixture("ready", Duration::from_secs(10)).await;
        let windows = result.unwrap().unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].label, "Weekly");
        assert!((windows[0].percent - 23.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn authentication_failure_does_not_become_zero_usage() {
        let result = fixture("signed-out", Duration::from_secs(10)).await;
        let error = result.unwrap_err();
        assert!(format!("{error:#}").contains("reading Codex account rate limits: Sign in required"));
    }

    #[tokio::test]
    async fn an_unresponsive_probe_is_bounded_and_stopped() {
        let result = fixture("hang", Duration::from_millis(100)).await;
        let error = result.unwrap_err();
        assert!(error.downcast_ref::<tokio::time::error::Elapsed>().is_some());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn slow_startup_does_not_stall_other_host_work() {
        let (release, waiting) = std::sync::mpsc::channel();
        let start = fixture_start("ready");
        let probe = read_live(
            move || {
                waiting.recv_timeout(Duration::from_secs(5)).context("async work could not release startup")?;
                start()
            },
            Duration::from_secs(10),
        );
        let resume = async {
            tokio::task::yield_now().await;
            release.send(()).unwrap();
        };
        let (result, ()) = tokio::join!(probe, resume);
        crate::agent::acp::stopped().await;
        assert!(result.unwrap().is_some());
    }

    #[tokio::test]
    async fn startup_errors_retain_their_cause_and_context() {
        let result = read_live(
            || Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "probe denied").into()),
            Duration::from_secs(10),
        )
        .await;
        let error = result.unwrap_err();
        assert!(format!("{error:#}").contains("starting Codex usage probe: probe denied"));
    }

    #[tokio::test]
    async fn protocol_errors_identify_the_failed_handshake() {
        let result = fixture("bad-handshake", Duration::from_secs(10)).await;
        let error = result.unwrap_err();
        assert!(format!("{error:#}").contains("initializing Codex usage probe: initialize rejected"));
    }
}
