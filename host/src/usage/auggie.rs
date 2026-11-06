//! `auggie account status --json`: the plan's credits left this cycle, with no session. Signed out
//! or on a plan without it, the CLI exits non-zero with nothing on stdout.
//! Read from the 0.36.0 CLI's source; no signed-in computer has run it yet.

use crate::hub::Hub;
use serde_json::Value;
use std::time::Duration;

use super::{Window, iso_ms};

pub(super) async fn collect(hub: &Hub) -> Option<Vec<Window>> {
    let args = ["account", "status", "--json"];
    super::command::json_windows(hub, "auggie", &args, Duration::from_secs(20), parse).await
}

/// `None`: a status with no amount left, which the CLI doesn't print for a plan without credits.
fn parse(status: &Value) -> Option<Vec<Window>> {
    let left = amount(&status["amountRemaining"])?;
    let resets = status["billingCycleEndDate"].as_str().and_then(iso_ms);
    Some(match amount(&status["amountIncludedPerCycle"]).filter(|included| *included > 0.0) {
        Some(included) => {
            vec![Window::new("cycle", "Credits", ((included - left) / included * 100.0).clamp(0.0, 100.0), resets)]
        }
        None => vec![Window::note("balance", "Balance", format!("{left} left"))],
    })
}

/// The CLI passes the server's amount on as it came: a number or a string.
fn amount(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str()?.trim().parse().ok()).filter(|amount: &f64| amount.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn credits_left_read_as_the_share_used() {
        let w = parse(&json!({"planName": "Pro", "usageUnit": "credits", "amountRemaining": "25",
            "amountIncludedPerCycle": 100, "billingCycleEndDate": "2026-12-01T00:00:00Z",
            "daysRemainingInCycle": 25, "banner": null}))
        .unwrap();
        assert!((w[0].percent - 75.0).abs() < 1e-9);
        assert!(w[0].resets_at.is_some());
    }

    #[test]
    fn no_cycle_allowance_is_a_balance() {
        let w = parse(&json!({"amountRemaining": 12.5, "amountIncludedPerCycle": 0})).unwrap();
        assert_eq!(w[0].text.as_deref(), Some("12.5 left"));
    }

    #[test]
    fn a_status_with_no_amount_is_a_changed_format() {
        assert_eq!(parse(&json!({"planName": "Pro"})), None);
    }
}
