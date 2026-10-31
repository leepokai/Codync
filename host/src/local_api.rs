//! The running host's local API, as the `codync-host` commands call it (`~/.codync/token` over loopback).
use crate::{http, service};
use anyhow::{Context, Result};
use std::time::Duration;

/// What a local command says when it can't reach the running host.
pub(crate) const NOT_RUNNING: &str =
    "codync-host isn't running here. Start it with `codync-host install` (or `serve`).";

/// Calls `method` on the running host.
pub(crate) async fn local_call(port: u16, method: &str, body: serde_json::Value) -> Result<serde_json::Value> {
    local_call_for(port, method, body, Duration::from_secs(20)).await
}

/// `local_call` for a method that may take longer than 20 seconds.
pub(crate) async fn local_call_for(
    port: u16,
    method: &str,
    body: serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value> {
    let not_running = || anyhow::anyhow!(NOT_RUNNING);
    let token = std::fs::read_to_string(service::data_dir().join("token")).map_err(|_| not_running())?;
    let res = http()
        .post(format!("http://127.0.0.1:{port}/api/{method}"))
        .bearer_auth(token.trim())
        .json(&body)
        .timeout(timeout)
        .send()
        .await
        .map_err(|_| not_running())?;
    let status = res.status();
    let v: serde_json::Value = res.json().await.context("reading the host's answer")?;
    if !status.is_success() {
        anyhow::bail!("{}", v["error"].as_str().unwrap_or("the host refused"));
    }
    Ok(v)
}
