//! Mini-apps for phones: a device on the E2E channel can't reach this computer's loopback
//! `/apps/...` server, so `appRequest` runs one of those requests here and returns the answer.

use super::apps_runtime::handle_request;
use super::str_arg;
use crate::hub::Hub;
use anyhow::{Context, Result};
use axum::body::{Bytes, to_bytes};
use axum::http::{Method, header};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use std::sync::Arc;

/// Largest answer sent over the channel (files and data pages of a mini-app).
const MAX_BODY: usize = 8 * 1024 * 1024;

/// `appRequest`: `{botId, slug, token, method, path, body?}` -> `{status, contentType, csp, body}`
/// (`body` is base64). The link token is checked exactly as for a browser request.
pub(super) async fn request(hub: &Arc<Hub>, b: &Value) -> Result<Value> {
    let bot_id = str_arg(b, "botId")?.to_owned();
    let slug = str_arg(b, "slug")?.to_owned();
    let token = str_arg(b, "token")?.to_owned();
    let method = Method::from_bytes(b["method"].as_str().unwrap_or("GET").to_ascii_uppercase().as_bytes())
        .context("invalid `method`")?;
    let path = b["path"].as_str().unwrap_or_default();
    let rel = path.split(['?', '#']).next().unwrap_or_default().trim_start_matches('/').to_owned();
    if rel.contains('\\') || rel.split('/').any(|part| part == "..") {
        return Ok(json!({
            "status":400, "contentType":"text/plain; charset=utf-8", "csp":"",
            "body":STANDARD.encode("invalid `path`"),
        }));
    }
    let body = Bytes::from(b["body"].as_str().unwrap_or_default().to_owned());
    let hub = Arc::clone(hub);
    let response =
        tokio::task::spawn_blocking(move || handle_request(&hub, &bot_id, &slug, &token, &rel, &method, &body))
            .await
            .context("the app request stopped")?;
    let (parts, body) = response.into_parts();
    let bytes = to_bytes(body, MAX_BODY).await.context("the app answer is too large")?;
    let text = |name| parts.headers.get(name).and_then(|v| v.to_str().ok()).unwrap_or_default().to_owned();
    Ok(json!({
        "status": parts.status.as_u16(),
        "contentType": text(header::CONTENT_TYPE),
        "csp": text(header::CONTENT_SECURITY_POLICY),
        "body": STANDARD.encode(&bytes),
    }))
}

#[cfg(test)]
#[path = "apps_bridge_tests.rs"]
mod tests;
