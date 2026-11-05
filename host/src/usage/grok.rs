//! Grok's plan: the `x.ai/billing` extension on its ACP server, which asks xAI itself, with no
//! session and no prompt. Signed out or on an API key, it answers with an error.
//! Field names are from the 1.0.75 binary; no signed-in computer has run it yet.

use crate::agent::auth::Started;
use crate::hub::Hub;
use crate::store::Store;
use serde_json::{Value, json};
use std::time::Duration;

use super::reading::UsageReadStage;
use super::{Window, acp_read, iso_ms};

const READ_TIMEOUT: Duration = Duration::from_secs(20);

pub(super) async fn collect(hub: &Hub) -> Option<Vec<Window>> {
    acp_read::guarded(hub, "grok", read(&hub.store)).await
}

async fn read(store: &Store) -> Result<Vec<Window>, UsageReadStage> {
    let Started { acp, inbox: _inbox, .. } = acp_read::start(store, "grok").await?;
    // ACP's extension methods take a leading underscore, which not every build expects.
    let ask = async {
        match acp.request("x.ai/billing", json!({})).await {
            Ok(reply) => Ok(reply),
            Err(_) => acp.request("_x.ai/billing", json!({})).await,
        }
    };
    let reply = tokio::time::timeout(READ_TIMEOUT, ask).await;
    acp.kill().await;
    let reply = reply.map_err(|_| UsageReadStage::Timeout)?.map_err(|_| UsageReadStage::Prompt)?;
    let windows = parse(&reply);
    if windows.is_empty() { Err(UsageReadStage::Parse) } else { Ok(windows) }
}

fn parse(reply: &Value) -> Vec<Window> {
    let config = &reply["config"];
    let mut windows = Vec::new();
    if let Some(used) = config["creditUsagePercent"].as_f64().filter(|used| used.is_finite()) {
        let period = &config["currentPeriod"];
        let resets = period["end"].as_str().and_then(iso_ms);
        let kind = period["type"].as_str().unwrap_or_default();
        let label = if kind.ends_with("WEEKLY") {
            "Weekly"
        } else if kind.ends_with("MONTHLY") {
            "Monthly"
        } else {
            "Plan"
        };
        windows.push(Window::new("plan", label, used.clamp(0.0, 100.0), resets));
    }
    // In cents, as `{"val": N}`.
    if let Some(cents) = config["prepaidBalance"]["val"].as_f64().filter(|cents| cents.is_finite() && *cents > 0.0) {
        windows.push(Window::note("prepaid", "Prepaid", format!("${:.2}", cents / 100.0)));
    }
    windows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plan_reads_as_the_share_used_with_its_prepaid_balance() {
        let w = parse(&json!({"config": {
            "creditUsagePercent": 42.5,
            "currentPeriod": {"type": "USAGE_PERIOD_TYPE_WEEKLY", "start": "2026-11-01T00:00:00Z", "end": "2026-11-08T00:00:00Z"},
            "prepaidBalance": {"val": 1250}
        }}));
        assert_eq!(w[0].label, "Weekly");
        assert!((w[0].percent - 42.5).abs() < 1e-9);
        assert!(w[0].resets_at.is_some());
        assert_eq!(w[1].text.as_deref(), Some("$12.50"));
    }

    #[test]
    fn nothing_recognizable_is_no_reading() {
        assert_eq!(parse(&json!({})), []);
        assert_eq!(parse(&json!({"config": {"prepaidBalance": {"val": 0}}})), []);
    }
}
