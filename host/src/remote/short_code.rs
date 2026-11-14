//! Short pairing codes (docs/plans/require-sign-in.md, phase 3): the QR's link registered with the Codync cloud
//! under a code people type where scanning a terminal is awkward, like `QRTX-4821`.

use super::cloud::{call, codync_url};
use crate::hub::Hub;
use anyhow::{Context, Result};
use reqwest::Method;
use serde_json::json;
use std::time::Duration;

/// The code for `link`, or an error with the cloud off, on a private relay, or unreachable within a few seconds.
pub async fn register(hub: &Hub, link: &str, expires_at: i64) -> Result<String> {
    let base = codync_url(hub)?;
    let body = json!({"link": link, "expiresAt": expires_at});
    let request = call(hub, &base, Method::POST, "/v1/host/pairing-codes", Some(&body));
    let answer =
        tokio::time::timeout(Duration::from_secs(5), request).await.context("the Codync cloud didn't answer")??;
    Ok(answer["code"].as_str().context("the cloud gave no code")?.to_owned())
}
