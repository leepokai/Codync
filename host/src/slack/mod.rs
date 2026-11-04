//! Owner-configured Slack channel. Secrets stay in the host vault.
use crate::{LockExt, hub::Hub, store::Store};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
pub(crate) mod ingress;
mod socket;
#[cfg(test)]
#[path = "socket_tests.rs"]
mod socket_tests;
mod worker;
pub(crate) use worker::claim_prompt;

/// Delivers replies and, when the app has an app-level token, listens over Socket Mode.
pub(crate) async fn run(hub: Arc<Hub>) {
    tokio::join!(worker::run(hub.clone()), socket::run(hub, socket::Endpoint::slack()));
}
const SLOT: &str = "slack.channel";
pub const METHODS: &[&str] = &["slackStatus", "setSlackChannel", "disconnectSlack"];

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Binding {
    channel_id: String,
    bot_id: String,
    allowed_users: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Config {
    workspace_id: String,
    bot_user_id: String,
    bot_token: String,
    /// Empty when the app uses Socket Mode only: the HTTP endpoint then refuses every request.
    signing_secret: String,
    /// The app-level `xapp-` token that opens a Socket Mode connection; empty to receive events over HTTP.
    #[serde(default)]
    app_token: String,
    bindings: Vec<Binding>,
    #[serde(default)]
    generation: String,
}

fn load(store: &Store) -> Result<Option<Config>> {
    crate::market::vault::read(store, SLOT)?
        .filter(|s| !s.is_empty())
        .map(|s| serde_json::from_str(&s).map_err(Into::into))
        .transpose()
}
pub(crate) fn configuration_state(store: &Store) -> &'static str {
    match load(store) {
        Ok(Some(_)) => "configured",
        Ok(None) => "notConfigured",
        Err(_) => "unavailable",
    }
}
fn slack_id(value: &str, prefix: &[char]) -> bool {
    value.len() >= 2
        && value.len() <= 80
        && value.starts_with(prefix)
        && value.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
/// A signing secret is 32 hex digits; it may be left out only when Socket Mode brings the events.
fn signing_secret_ok(c: &Config) -> bool {
    if c.signing_secret.is_empty() {
        return !c.app_token.is_empty();
    }
    c.signing_secret.len() == 32 && c.signing_secret.bytes().all(|b| b.is_ascii_hexdigit())
}
fn validate(hub: &Hub, c: &Config) -> Result<()> {
    if !slack_id(&c.workspace_id, &['T'])
        || !slack_id(&c.bot_user_id, &['U', 'W'])
        || !c.bot_token.starts_with("xoxb-")
        || c.bot_token.len() > 8192
        || !signing_secret_ok(c)
        || (!c.app_token.is_empty() && (!c.app_token.starts_with("xapp-") || c.app_token.len() > 8192))
    {
        bail!("Workspace, bot user, bot token, app token or signing secret is invalid.");
    }
    if c.bindings.is_empty() || c.bindings.len() > 50 {
        bail!("Configure 1–50 channel bindings.");
    }
    let mut seen = std::collections::HashSet::new();
    for binding in &c.bindings {
        if !slack_id(&binding.channel_id, &['C', 'G', 'D'])
            || !seen.insert(&binding.channel_id)
            || binding.allowed_users.is_empty()
            || binding.allowed_users.len() > 100
            || binding.allowed_users.iter().any(|user| !slack_id(user, &['U', 'W']))
        {
            bail!("Each unique channel needs an explicit Slack user allowlist.");
        }
        hub.store
            .bot(&binding.bot_id)?
            .filter(|bot| !bot.deleted && !bot.config.is_group())
            .context("A binding bot is unavailable.")?;
    }
    Ok(())
}
pub(crate) async fn call(hub: &Arc<Hub>, method: &str, body: Value) -> Result<Value> {
    crate::market::vault::unlock(hub.clone()).await?;
    let prepared = if method == "setSlackChannel" {
        let config: Config = serde_json::from_value(body)?;
        validate(hub, &config)?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(20))
            .build()?;
        let response = client.post("https://slack.com/api/auth.test").bearer_auth(&config.bot_token).send().await?;
        if !response.status().is_success() {
            bail!("Slack could not verify the bot token.");
        }
        let identity: Value = response.json().await?;
        if identity["ok"] != true
            || identity["team_id"] != config.workspace_id
            || identity["user_id"] != config.bot_user_id
        {
            bail!("The bot token does not match the configured workspace and bot user.");
        }
        Some(config)
    } else {
        None
    };
    let _guard = hub.store.connector_lock.locked();
    match method {
        "setSlackChannel" => {
            let mut config = prepared.context("Missing Slack configuration.")?;
            validate(hub, &config)?;
            config.generation = uuid::Uuid::new_v4().to_string();
            crate::market::vault::write(&hub.store, SLOT, &serde_json::to_string(&config)?)?;
        }
        "disconnectSlack" => {
            crate::market::vault::write(&hub.store, SLOT, "")?;
        }
        "slackStatus" => {}
        _ => bail!("Unknown Slack method."),
    }
    let c = load(&hub.store)?;
    if method != "slackStatus" {
        for id in hub.store.reject_stale_slack(c.as_ref().map(|c| c.generation.as_str()))? {
            if let Some(mut entry) = hub.store.entry(&id).filter(|e| e.data["status"] == "queued") {
                entry.data["status"] = "cancelled".into();
                hub.set_entry(&id, &entry.data).context("Could not update cancelled Slack prompt.")?;
            }
        }
    }
    Ok(json!({"configured":c.is_some(),"socketMode":c.as_ref().is_some_and(|c|!c.app_token.is_empty()),
        "workspaceId":c.as_ref().map(|c|&c.workspace_id),
        "bindings":c.as_ref().map(|c|&c.bindings),"deliveries":hub.store.slack_deliveries()?,"webhookPath":"/hooks/slack"}))
}
