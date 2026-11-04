//! HTTP API: `POST /api/<method>` commands + `GET /events` SSE for this computer's own
//! apps and helpers (loopback + `Authorization: Bearer <token>`), and `GET /channel`, the
//! direct end-to-end encrypted channel phones use (see `channel`).

mod apps_bridge;
pub mod apps_runtime;
mod bots;
mod catalog;
mod computer_activity;
pub mod devices;
mod documents;
mod events;
mod files;
mod host;
mod http;
mod marketplace;
mod network;
mod origin;
mod rebinding;
mod remote;
mod roster;
mod saved_searches;

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
    let observed = crate::analytics::observe_call(hub, caller, method, &b);
    let result = route(hub, caller, method, b).await;
    if method == "createBot" && result.as_ref().is_ok_and(|value| value["bot"]["kind"] != "group") {
        crate::analytics::setup::ready(hub);
    }
    if method == "setSlackChannel" && result.is_err() {
        crate::analytics::setup::failed(hub, true);
    }
    if let (Ok(answer), Some((event, properties))) = (&result, observed)
        && let Some(properties) = crate::analytics::answered(event, properties, answer)
    {
        crate::analytics::capture(hub, event, properties);
    }
    result
}

async fn route(hub: &Arc<Hub>, caller: &Caller, method: &str, b: Value) -> Result<Value> {
    if matches!(method, "savedSearches" | "saveSearch" | "deleteSavedSearch") {
        return saved_searches::call(hub, caller, method, &b);
    }
    if crate::research::METHODS.contains(&method) {
        if method == "setResearchKey" && !matches!(caller, Caller::Local) {
            return Err(Forbidden.into());
        }
        return crate::research::call(hub, method, &b).await;
    }
    if crate::slack::METHODS.contains(&method) {
        if !matches!(caller, Caller::Local) && method != "slackStatus" {
            return Err(Forbidden.into());
        }
        return crate::slack::call(hub, method, b).await;
    }
    if crate::telegram::METHODS.contains(&method) {
        // A paired phone may see the channels and let someone in or out; a token is only ever given on the computer.
        if !matches!(caller, Caller::Local)
            && !matches!(method, "telegramStatus" | "pairTelegram" | "setTelegramAllowed")
        {
            return Err(Forbidden.into());
        }
        return crate::telegram::call(hub, method, b).await;
    }
    if matches!(method, "beginProtectedEntry" | "finishProtectedEntry") {
        return crate::screen::protected_entry::call(hub, method, b).await;
    }
    if matches!(method, "voiceDecisions" | "confirmVoiceDecisions") {
        return crate::voice::decisions::call(hub, caller, method, b);
    }
    #[cfg(windows)]
    if matches!(method, "openProtectedBrowser" | "fillProtectedLogin" | "fillProtectedCode") {
        return crate::screen::protected_browser::call(hub, method, b).await;
    }
    if crate::computer::panel::METHODS.contains(&method) {
        let (kind, id) = match caller {
            Caller::Device { key, .. } => ("device", key.as_str()),
            _ => ("owner", "local"),
        };
        return crate::computer::panel::call(hub, method, &b, kind, id).await;
    }
    if crate::mcp::policy_management::METHODS.contains(&method) {
        market::vault::unlock(hub.clone()).await?;
        return crate::mcp::policy_management::call(hub, method, &b);
    }
    if crate::mcp::policy::METHODS.contains(&method) {
        return crate::mcp::policy::call(hub, caller, method, b).await;
    }
    if documents::METHODS.contains(&method) {
        return documents::call(hub, method, b);
    }
    if matches!(method, "listComputerActivity" | "exportComputerActivity") {
        return computer_activity::read(hub, method, &b);
    }
    if method == "searchMessages" {
        return roster::search(hub, &b).await;
    }
    if matches!(method, "rosterPreview" | "messageContext") {
        return roster::read(hub, method, &b).await;
    }
    if method.contains("onnector")
        || method.starts_with("composio")
        || method == "setComposioKey"
        || method.starts_with("credential")
        // Only signing in reads the vault; other computer tools work without a keyring.
        || (method == "computerCall" && b["name"] == "type_login")
        || method.starts_with("voice")
        || method == "setVoiceKey"
        || matches!(method, "agentAuth" | "agentAuthenticate" | "setAgentEnv" | "accountAuthenticate" | "agentModels")
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
    let b = match catalog::call(hub, method, b).await? {
        ControlFlow::Break(v) => return Ok(v),
        ControlFlow::Continue(b) => b,
    };
    let b = match network::call(hub, method, b).await? {
        ControlFlow::Break(v) => return Ok(v),
        ControlFlow::Continue(b) => b,
    };
    let b = match bots::call(hub, caller, method, b).await? {
        ControlFlow::Break(v) => return Ok(v),
        ControlFlow::Continue(b) => b,
    };
    match remote::call(hub, caller, method, b).await? {
        ControlFlow::Break(v) => Ok(v),
        ControlFlow::Continue(_) => Err(UnknownMethod.into()),
    }
}
