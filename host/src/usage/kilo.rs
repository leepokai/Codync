//! `kilo profile --json`: the account's dollar balance, the only plan number the CLI prints. A note,
//! as it has no cap or reset. Signed out, it exits non-zero with nothing on stdout.
//! Read from the 7.8.18 CLI's source; no signed-in computer has run it yet.

use crate::hub::Hub;
use serde_json::Value;
use std::time::Duration;

use super::Window;

pub(super) async fn collect(hub: &Hub) -> Option<Vec<Window>> {
    let parse = |profile: &Value| Some(parse(profile));
    super::command::json_windows(hub, "kilo", &["profile", "--json"], Duration::from_secs(20), parse).await
}

/// The CLI prints a balance of 0 when its own balance request fails, so a zero is an empty reading:
/// the row goes until the next poll, rather than claim the account is empty or keep an old balance.
fn parse(profile: &Value) -> Vec<Window> {
    let balance = profile["balance"].as_f64().filter(|balance| balance.is_finite() && *balance > 0.0);
    balance.map(|balance| Window::note("balance", "Balance", format!("${balance:.2}"))).into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_balance_is_a_note() {
        let w = parse(&json!({"name": "n", "team": null, "balance": 4.2}));
        assert_eq!(w[0].text.as_deref(), Some("$4.20"));
    }

    #[test]
    fn a_zero_or_a_missing_balance_is_an_empty_reading() {
        assert_eq!(parse(&json!({"balance": 0})), []);
        assert_eq!(parse(&json!({"name": "n"})), []);
    }
}
