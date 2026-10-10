//! HTTP API: `POST /api/<method>` commands + `GET /events` SSE for this computer's own
//! apps and helpers (loopback + `Authorization: Bearer <token>`), and `GET /channel`, the
//! direct end-to-end encrypted channel phones use (see `channel`).

mod bots;
pub mod devices;
mod events;
mod files;
mod host;
mod http;
mod marketplace;
mod remote;

pub use events::{events_stream, term_events};
pub use http::router;

use crate::api::devices::{Caller, Forbidden};
use crate::hub::Hub;
use crate::market;
use anyhow::{Result, anyhow};
use axum::http::StatusCode;
use serde_json::Value;
use std::ops::ControlFlow;
use std::sync::Arc;

/// `POST /api/<method>` with a method this host doesn't know (404, not 400).
#[derive(Debug)]
struct UnknownMethod;

impl std::fmt::Display for UnknownMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unknown method")
    }
}

impl std::error::Error for UnknownMethod {}

/// HTTP status and message for a failed call; channels send the same pair in `err`.
pub fn error_status(e: &anyhow::Error) -> (StatusCode, String) {
    if e.is::<UnknownMethod>() {
        (StatusCode::NOT_FOUND, e.to_string())
    } else if e.is::<Forbidden>() {
        (StatusCode::FORBIDDEN, e.to_string())
    } else if e.is::<crate::remote::cloud::Conflict>() {
        (StatusCode::CONFLICT, e.to_string())
    } else {
        // `{:#}` keeps the context chain ("starting `npx …`: No such file or directory").
        (StatusCode::BAD_REQUEST, format!("{e:#}"))
    }
}

fn str_arg<'a>(b: &'a Value, k: &str) -> Result<&'a str> {
    b[k].as_str().ok_or_else(|| anyhow!("`{k}` is required"))
}

/// Runs one API method for `caller` (permissions per spec §6.6).
pub async fn dispatch(hub: &Arc<Hub>, caller: &Caller, method: &str, b: Value) -> Result<Value> {
    devices::permit(caller, method)?;
    let observed = crate::analytics::observe(caller, method, &b);
    let result = route(hub, caller, method, b).await;
    if result.is_ok()
        && let Some((event, properties)) = observed
    {
        crate::analytics::capture(hub, event, properties);
    }
    result
}

async fn route(hub: &Arc<Hub>, caller: &Caller, method: &str, b: Value) -> Result<Value> {
    if method.contains("onnector")
        || method.starts_with("composio")
        || method == "setComposioKey"
        || method.starts_with("credential")
        // Only signing in reads the vault; other computer tools work without a keyring.
        || (method == "computerCall" && b["name"] == "type_login")
        || method.starts_with("voice")
        || method == "setVoiceKey"
        || matches!(method, "agentAuth" | "agentAuthenticate" | "setAgentEnv")
    {
        market::vault::unlock(hub.clone()).await?;
    }
    if method == "readFile" {
        return files::read(hub, &b).await;
    }
    // Each method group answers its own methods and hands the body back otherwise.
    let b = match host::call(hub, caller, method, b).await? {
        ControlFlow::Break(v) => return Ok(v),
        ControlFlow::Continue(b) => b,
    };
    let b = match marketplace::call(hub, caller, method, b).await? {
        ControlFlow::Break(v) => return Ok(v),
        ControlFlow::Continue(b) => b,
    };
    let b = match bots::call(hub, method, b).await? {
        ControlFlow::Break(v) => return Ok(v),
        ControlFlow::Continue(b) => b,
    };
    match remote::call(hub, caller, method, b).await? {
        ControlFlow::Break(v) => Ok(v),
        ControlFlow::Continue(_) => Err(UnknownMethod.into()),
    }
}
