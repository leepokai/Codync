//! iMate agents live on imate.woa.com, not on this computer. The host fetches the
//! signed-in user's list to fill the model picker and passes the personal token to
//! the stdio bridge as `IMATE_API_TOKEN`.

use super::Status;
use crate::agent::login_profiles;
use crate::analytics::{Event, capture};
use crate::http;
use crate::hub::Hub;
use crate::store::Store;
use anyhow::{Context, Result, bail};
use codync_agent::backends;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde_json::{Map, Value, json};
use std::sync::Arc;

const TOKEN: &str = "IMATE_API_TOKEN";
const BASE: &str = "https://imate.woa.com/server/web-api";

/// What the phone needs to show iMate's token field and sign-in state.
pub async fn status(hub: Arc<Hub>) -> Result<Value> {
    crate::market::vault::unlock(hub.clone()).await?;
    Ok(json!({
        "signedIn": saved_token(&hub.store)?.is_some(),
        "loginKeys": [TOKEN],
        "installHint": "Add an iMate personal API token in Login profiles.",
    }))
}

/// Saves the personal token in the host's vault and refreshes the agent catalog.
pub async fn set_token(hub: Arc<Hub>, token: &str) -> Result<Value> {
    let token = token.trim();
    if token.is_empty() {
        bail!("token is empty");
    }
    crate::market::vault::unlock(hub.clone()).await?;
    super::save_env(&hub.store, "agent-env:imate", &[(TOKEN.to_owned(), Some(token.to_owned()))])?;
    backends::set_signed_in("imate", Some(true));
    hub.model_lists.clear("imate");
    status(hub).await
}

/// Whether an iMate API key has been entered, for the agent's setup card.
pub fn check(store: &Store) -> Result<Status> {
    let signed_in = saved_token(store)?.is_some();
    Ok(Status { signed_in: Some(signed_in), detail: None, methods: Vec::new() })
}

/// The token a bridge process should start with: the agent's own, or one saved on a login profile.
pub(crate) fn launch_token(store: &Store) -> Result<Option<String>> {
    saved_token(store)
}

fn saved_token(store: &Store) -> Result<Option<String>> {
    if let Some(raw) = crate::market::vault::read(store, "agent-env:imate")? {
        let vars: Map<String, Value> = serde_json::from_str(&raw)?;
        if let Some(token) = vars.get(TOKEN).and_then(Value::as_str) {
            let token = token.trim();
            if !token.is_empty() {
                return Ok(Some(token.to_owned()));
            }
        }
    }
    for profile in login_profiles::signed_in_for(store, "imate") {
        let slot = format!("login-profile:{}", profile.account.id);
        let Some(raw) = crate::market::vault::read(store, &slot)? else { continue };
        let vars: Map<String, Value> = serde_json::from_str(&raw)?;
        if let Some(token) = vars.get(TOKEN).and_then(Value::as_str) {
            let token = token.trim();
            if !token.is_empty() {
                return Ok(Some(token.to_owned()));
            }
        }
    }
    Ok(None)
}

/// The signed-in user's `iMate` bots, in the shape the shared model picker reads.
pub async fn shared_models(hub: &Hub) -> Result<Value> {
    // An API key profile's value is in the vault; a model request has no other
    // reason to unlock it. A token saved for the agent itself is in the same store.
    let fetch = async {
        let token = saved_token(&hub.store)?.context("add an iMate API token in Extensions first")?;
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).context("invalid iMate API token")?,
        );
        let response = http()
            .get(format!("{BASE}/api/v1/open/imates"))
            .headers(headers)
            .send()
            .await
            .context("listing iMate agents")?;
        let status = response.status();
        let body = response.text().await.context("reading the iMate agent list")?;
        if !status.is_success() {
            bail!("iMate agent list failed ({status}): {}", first_line(&body));
        }
        let value: Value = serde_json::from_str(first_line(&body)).context("decoding the iMate agent list")?;
        if value["code"] != 0 {
            bail!("iMate agent list failed: {}", value["message"].as_str().unwrap_or("unknown error"));
        }
        let mut models: Vec<Value> = value["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|imate| {
                let id = imate["clientUuid"].as_str()?;
                Some(json!({"id": id, "name": imate["botName"].as_str().unwrap_or(id), "description": null, "options": []}))
            })
            .collect();
        models.sort_by(|a, b| a["name"].as_str().unwrap_or_default().cmp(b["name"].as_str().unwrap_or_default()));
        Ok(json!({
            "models": models,
            "currentModelId": models.first().and_then(|m| m["id"].as_str()).unwrap_or_default(),
            "defaultModelOptions": false,
            "fastMode": false,
            "modes": [],
        }))
    };
    let result = fetch.await;
    let mut properties = Map::new();
    properties.insert("outcome".into(), json!(if result.is_ok() { "success" } else { "failed" }));
    if let Err(error) = &result {
        properties.insert("category".into(), json!(failure_category(error)));
    }
    capture(hub, Event::ImateAgentsListed, properties);
    hub.model_lists.get("imate", None, async { result }).await
}

fn first_line(body: &str) -> &str {
    body.split('\n').next().unwrap_or(body)
}

fn failure_category(error: &anyhow::Error) -> &'static str {
    let text = format!("{error:#}").to_lowercase();
    if text.contains("token") || text.contains("401") || text.contains("403") {
        "auth"
    } else if text.contains("timed out") || text.contains("timeout") {
        "timeout"
    } else if text.contains("decode") || text.contains("parse") {
        "protocol"
    } else {
        "network"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_line_ignores_the_framework_suffix() {
        assert_eq!(first_line("{\"error\":\"x\"}\n{\"code\":0}"), "{\"error\":\"x\"}");
    }

    #[test]
    fn failure_categories_never_include_the_error_text() {
        assert_eq!(failure_category(&anyhow::anyhow!("iMate token rejected: secret")), "auth");
        assert_eq!(failure_category(&anyhow::anyhow!("request timed out")), "timeout");
        assert_eq!(failure_category(&anyhow::anyhow!("connection refused")), "network");
    }
}
