//! A GitHub token the host keeps in its vault, so a workflow can comment, label or open an issue on
//! the user's behalf. A fine-grained token with the repository permissions the outputs need is enough;
//! no GitHub App install is involved.

#[cfg(test)]
mod tests;

use crate::{LockExt, hub::Hub, store::Store};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

const SLOT: &str = "github.connection";
pub const METHODS: &[&str] = &["githubStatus", "setGitHubToken", "disconnectGitHub"];

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Config {
    pub token: String,
    pub login: String,
    /// What the token may do, as GitHub reports it.
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub generation: String,
}

fn load(store: &Store) -> Result<Option<Config>> {
    crate::market::vault::read(store, SLOT)?
        .filter(|s| !s.is_empty())
        .map(|s| serde_json::from_str(&s).map_err(Into::into))
        .transpose()
}

fn save(store: &Store, config: &Config) -> Result<()> {
    crate::market::vault::write(store, SLOT, &serde_json::to_string(config)?)
}

pub(crate) fn configuration_state(store: &Store) -> &'static str {
    match load(store) {
        Ok(Some(_)) => "configured",
        Ok(None) => "notConfigured",
        Err(_) => "unavailable",
    }
}

fn token_ok(token: &str) -> bool {
    let token = token.trim();
    (20..=255).contains(&token.len()) && token.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Asks GitHub who the token belongs to and what it may do; nothing is saved on a refusal.
async fn verify(token: &str) -> Result<(String, Vec<String>)> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let response = client
        .get("https://api.github.com/user")
        .header("authorization", format!("Bearer {token}"))
        .header("accept", "application/vnd.github+json")
        .header("x-github-api-version", "2022-11-28")
        .header("user-agent", "codync")
        .send()
        .await?;
    if !response.status().is_success() {
        bail!("GitHub refused the token ({}).", response.status());
    }
    let scopes: Vec<String> = response
        .headers()
        .get("x-oauth-scopes")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(',').map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    let body: Value = response.json().await?;
    let login = body["login"].as_str().filter(|l| !l.is_empty()).context("GitHub sent no login for that token")?;
    Ok((login.to_owned(), scopes))
}

pub(crate) async fn call(hub: &Arc<Hub>, method: &str, body: Value) -> Result<Value> {
    crate::market::vault::unlock(hub.clone()).await?;
    let token = body["token"].as_str().unwrap_or_default().trim().to_owned();
    let prepared = if method == "setGitHubToken" {
        if !token_ok(&token) {
            bail!("Paste a GitHub token (20–255 letters, digits, - or _).");
        }
        Some(verify(&token).await?)
    } else {
        None
    };
    let _guard = hub.store.connector_lock.locked();
    match method {
        "setGitHubToken" => {
            let (login, scopes) = prepared.context("Missing GitHub token.")?;
            save(&hub.store, &Config { token, login, scopes, generation: uuid::Uuid::new_v4().to_string() })?;
        }
        "disconnectGitHub" => {
            save(
                &hub.store,
                &Config { token: String::new(), login: String::new(), scopes: Vec::new(), generation: String::new() },
            )?;
        }
        "githubStatus" => {}
        _ => bail!("Unknown GitHub method."),
    }
    Ok(status(load(&hub.store)?.as_ref()))
}

fn status(c: Option<&Config>) -> Value {
    let live = c.filter(|c| !c.token.is_empty());
    json!({
        "configured": live.is_some(),
        "login": live.map(|c| &c.login),
        "scopes": live.map_or(&[][..], |c| c.scopes.as_slice()),
    })
}

/// The token and the account it belongs to, for a caller that needs to make a request.
pub(crate) fn connection(store: &Store) -> Result<Option<Config>> {
    Ok(load(store)?.filter(|c| !c.token.is_empty()))
}
