//! HTTP API: `POST /api/<method>` commands + `GET /events` SSE for this computer's own
//! apps and helpers (loopback + `Authorization: Bearer <token>`), and `GET /channel`, the
//! direct end-to-end encrypted channel phones use (see `channel`).

mod bots;
pub mod devices;
mod events;
mod files;
mod folders;
mod host;
mod http;
mod marketplace;
mod network;
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
    let observed = crate::analytics::observe_call(hub, caller, method, &b);
    let result = route(hub, caller, method, b).await;
    let report = observed.and_then(|(event, properties)| match &result {
        Ok(answer) => crate::analytics::answered(event, properties, answer).map(|p| (event, p)),
        Err(error) => crate::analytics::failed(event, properties, error),
    });
    if let Some((event, properties)) = report {
        crate::analytics::capture(hub, event, properties);
    }
    result
}

async fn route(hub: &Arc<Hub>, caller: &Caller, method: &str, b: Value) -> Result<Value> {
    if method == "setIcon" {
        return crate::icons::set(hub, &b).await;
    }
    if method == "icons" {
        return Ok(serde_json::to_value(crate::icons::snapshot(&hub.store)?)?);
    }
    if method.contains("onnector")
        || method.starts_with("composio")
        || method == "setComposioKey"
        || method.starts_with("credential")
        // Only signing in reads the vault; other computer tools work without a keyring.
        || (method == "computerCall" && b["name"] == "type_login")
        // Dictation on the user's own speech server (`localDictation`, `setSpeechServer`, local `voiceTranscribe`) has no key.
        || (method.starts_with("voice") && !(method == "voiceTranscribe" && b["provider"] == "local"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LockExt;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use serde_json::json;
    use std::time::Duration;

    /// A host whose keyring read is still waiting (an unanswered unlock prompt).
    fn hub_unlocking() -> (Arc<Hub>, tokio::sync::OwnedMutexGuard<()>) {
        let dir = std::env::temp_dir().join(format!("codync-api-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).unwrap();
        *store.secret_key.locked() = None;
        let identity = crate::remote::identity::Identity::load_or_create(&dir).unwrap();
        let hub = Hub::new(store, "h".into(), identity, "t".into(), 0);
        let reading = Arc::clone(&hub.store.vault_unlock).try_lock_owned().unwrap();
        (hub, reading)
    }

    async fn call(hub: &Arc<Hub>, method: &str, b: Value) -> Option<Result<Value>> {
        tokio::time::timeout(Duration::from_secs(2), dispatch(hub, &Caller::Local, method, b)).await.ok()
    }

    #[tokio::test]
    async fn dictation_on_this_computer_never_waits_for_the_keyring() {
        let (hub, _reading) = hub_unlocking();
        let local = call(&hub, "localDictation", json!({})).await.expect("answered").unwrap();
        assert!(call(&hub, "setSpeechServer", json!({"url": ""})).await.is_some(), "linking waited for the keyring");
        assert!(local["ready"].is_boolean(), "{local}");
        let audio = STANDARD.encode(crate::voice::wav(&[0; 3200], 16_000));
        let said = call(&hub, "voiceTranscribe", json!({"provider": "local", "model": "", "audio": audio})).await;
        assert!(said.is_some(), "local dictation waited for the keyring");
        // Provider keys still come from the vault, so their checks wait for it.
        assert!(call(&hub, "voiceStatus", json!({})).await.is_none());
    }
}
