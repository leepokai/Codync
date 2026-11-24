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
    // A bridge already running still has the token it started with.
    hub.restart_agents("imate");
    status(hub).await
}

/// Whether an iMate API key has been entered, for the agent's setup card.
pub fn check(store: &Store) -> Result<Status> {
    let signed_in = saved_token(store)?.is_some();
    Ok(Status { signed_in: Some(signed_in), detail: None, methods: Vec::new() })
}

/// The token a bridge starts with for `login_profile` (none: the agent's own, then a signed-in profile).
/// The model picker uses the same choice, so the colleagues it lists are the ones that bot can reach.
pub(crate) fn token_for(store: &Store, login_profile: Option<&str>) -> Result<Option<String>> {
    if let Some(id) = login_profile.filter(|id| !id.is_empty())
        && let Some(token) = profile_token(store, id)?
    {
        return Ok(Some(token));
    }
    saved_token(store)
}

/// Puts the chosen colleague and, when the login didn't, its token on `env`. A chat and a background
/// job (naming, a call title) both go through here, so they open the same tunnel.
pub(crate) fn apply_launch(
    store: &Store,
    model: Option<&str>,
    login_profile: Option<&str>,
    env: &mut Vec<(String, String)>,
) -> Result<()> {
    if let Some(model) = model.map(str::trim).filter(|model| !model.is_empty()) {
        env.retain(|(name, _)| name != "IMATE_CLIENT_UUID");
        env.push(("IMATE_CLIENT_UUID".into(), model.to_owned()));
    }
    let has_token = env.iter().any(|(name, value)| name == "IMATE_API_TOKEN" && !value.is_empty());
    if !has_token && let Some(token) = token_for(store, login_profile)? {
        env.retain(|(name, _)| name != "IMATE_API_TOKEN");
        env.push(("IMATE_API_TOKEN".into(), token));
    }
    Ok(())
}

fn profile_token(store: &Store, id: &str) -> Result<Option<String>> {
    let Some(raw) = crate::market::vault::read(store, &format!("login-profile:{id}"))? else {
        return Ok(None);
    };
    let vars: Map<String, Value> = serde_json::from_str(&raw)?;
    Ok(token_text(vars.get(TOKEN)))
}

fn saved_token(store: &Store) -> Result<Option<String>> {
    if let Some(raw) = crate::market::vault::read(store, "agent-env:imate")? {
        let vars: Map<String, Value> = serde_json::from_str(&raw)?;
        if let Some(token) = token_text(vars.get(TOKEN)) {
            return Ok(Some(token));
        }
    }
    for profile in login_profiles::signed_in_for(store, "imate") {
        let slot = format!("login-profile:{}", profile.account.id);
        let Some(raw) = crate::market::vault::read(store, &slot)? else { continue };
        let vars: Map<String, Value> = serde_json::from_str(&raw)?;
        if let Some(token) = token_text(vars.get(TOKEN)) {
            return Ok(Some(token));
        }
    }
    Ok(None)
}

fn token_text(value: Option<&Value>) -> Option<String> {
    let token = value.and_then(Value::as_str)?.trim();
    if token.is_empty() { None } else { Some(token.to_owned()) }
}

/// The signed-in user's iMate colleagues, in the shape the shared model picker reads.
/// `login_profile` is the bot's login, so the list is the one that token can see.
pub async fn shared_models(hub: &Arc<Hub>, login_profile: Option<&str>) -> Result<Value> {
    // The vault stays locked across a host restart until something opens it. Listing colleagues is that read.
    crate::market::vault::unlock(hub.clone()).await?;
    let login_profile = login_profile.filter(|id| !id.is_empty());
    hub.model_lists
        .get("imate", login_profile, async {
            let result = list_colleagues(&hub.store, login_profile).await;
            let mut properties = Map::new();
            properties.insert("outcome".into(), json!(if result.is_ok() { "success" } else { "failed" }));
            if let Err(error) = &result {
                properties.insert("category".into(), json!(failure_category(error)));
            }
            capture(hub, Event::ImateAgentsListed, properties);
            result
        })
        .await
}

async fn list_colleagues(store: &Store, login_profile: Option<&str>) -> Result<Value> {
    let token = token_for(store, login_profile)?.context("add an iMate API token in Extensions first")?;
    let mut headers = HeaderMap::new();
    headers
        .insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {token}")).context("invalid iMate API token")?);
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
    // The same order as the bridge's default colleague: first by name, so "Default" is that person.
    let mut models: Vec<Value> = value["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|imate| {
            let id = imate["clientUuid"].as_str()?;
            Some(json!({"id": id, "name": colleague_name(imate), "description": null, "options": []}))
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
}

/// The name the picker sorts by. Empty names sort as the colleague's id, the same as the bridge.
pub(crate) fn colleague_name(imate: &Value) -> &str {
    imate["botName"]
        .as_str()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| imate["clientUuid"].as_str().unwrap_or_default())
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
