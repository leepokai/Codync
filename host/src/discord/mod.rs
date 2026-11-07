//! Discord as a way to talk to a bot: one bot token, bound guild channels, and the people allowed in
//! each. The token lives in the host's vault and the host opens the connection to Discord (Gateway),
//! so nothing has to be reachable from the internet. Every call goes out through `egress`.

mod client;
mod gateway;
mod ingress;
mod worker;

pub use gateway::Endpoint;
pub(crate) use worker::claim_prompt;
#[cfg(test)]
mod tests;

use crate::{LockExt, hub::Hub, store::Store};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

const SLOT: &str = "discord.channel";
pub const METHODS: &[&str] = &["discordStatus", "setDiscordChannel", "disconnectDiscord"];
const MAX_BINDINGS: usize = 50;
const MAX_ALLOWED: usize = 100;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Binding {
    pub channel_id: String,
    pub bot_id: String,
    pub allowed_users: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Config {
    pub guild_id: String,
    pub bot_user_id: String,
    pub bot_token: String,
    pub bindings: Vec<Binding>,
    /// Changes whenever the connection is replaced, so an old one's prompts are dropped.
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

/// Runs the receiver and the sender while the host runs.
pub(crate) async fn run(hub: Arc<Hub>) {
    tokio::join!(gateway::run(hub.clone(), Endpoint::discord()), worker::run(hub));
}

fn id_ok(id: &str) -> bool {
    (1..=32).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit())
}

fn token_ok(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    token.len() <= 200
        && parts.len() == 3
        && parts[0].len() >= 20
        && !parts[1].is_empty()
        && parts[2].len() >= 20
        && parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
}

fn binding_ok(hub: &Hub, binding: &Binding) -> Result<()> {
    if !id_ok(&binding.channel_id) || !id_ok(&binding.bot_id) {
        bail!("Each binding needs a channel id and one of your bots.");
    }
    if binding.allowed_users.is_empty() || binding.allowed_users.len() > MAX_ALLOWED {
        bail!("Each channel needs 1–{MAX_ALLOWED} allowed Discord users.");
    }
    if binding.allowed_users.iter().any(|u| !id_ok(u)) {
        bail!("Allowed Discord users are user ids.");
    }
    hub.store
        .bot(&binding.bot_id)?
        .filter(|bot| !bot.deleted && !bot.config.is_group() && !crate::external::is_external(&bot.config))
        .context("A bound bot is unavailable.")?;
    Ok(())
}

fn validate(hub: &Hub, c: &Config) -> Result<()> {
    if !id_ok(&c.guild_id) || !id_ok(&c.bot_user_id) || !token_ok(&c.bot_token) {
        bail!("The server id, bot user id or bot token is invalid.");
    }
    if c.bindings.is_empty() || c.bindings.len() > MAX_BINDINGS {
        bail!("Bind 1–{MAX_BINDINGS} channels.");
    }
    let mut seen = std::collections::HashSet::new();
    for binding in &c.bindings {
        if !seen.insert(&binding.channel_id) {
            bail!("A channel is bound twice.");
        }
        binding_ok(hub, binding)?;
    }
    Ok(())
}

/// Confirms the token is this bot and every channel belongs to the guild, before anything is saved.
async fn verify(c: &Config) -> Result<()> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let me: Value = client
        .get("https://discord.com/api/v10/users/@me")
        .header("authorization", format!("Bot {}", c.bot_token))
        .send()
        .await?
        .error_for_status()
        .context("Discord could not verify the bot token.")?
        .json()
        .await?;
    if me["bot"] != true || me["id"] != c.bot_user_id.as_str() {
        bail!("The token does not belong to that bot user.");
    }
    for binding in &c.bindings {
        let channel: Value = client
            .get(format!("https://discord.com/api/v10/channels/{}", binding.channel_id))
            .header("authorization", format!("Bot {}", c.bot_token))
            .send()
            .await?
            .error_for_status()
            .with_context(|| format!("Discord does not know channel {}.", binding.channel_id))?
            .json()
            .await?;
        let text_channel = matches!(channel["type"].as_i64(), Some(0 | 5 | 10 | 11 | 12));
        if !text_channel || channel["guild_id"] != c.guild_id.as_str() {
            bail!("Channel {} is not a text channel of this server.", binding.channel_id);
        }
    }
    Ok(())
}

pub(crate) async fn call(hub: &Arc<Hub>, method: &str, body: Value) -> Result<Value> {
    crate::market::vault::unlock(hub.clone()).await?;
    let prepared = if method == "setDiscordChannel" {
        let config: Config = serde_json::from_value(body)?;
        validate(hub, &config)?;
        verify(&config).await?;
        Some(config)
    } else {
        None
    };
    let _guard = hub.store.connector_lock.locked();
    match method {
        "setDiscordChannel" => {
            let mut config = prepared.context("Missing Discord configuration.")?;
            validate(hub, &config)?;
            config.generation = uuid::Uuid::new_v4().to_string();
            save(&hub.store, &config)?;
        }
        "disconnectDiscord" => save(
            &hub.store,
            &Config {
                guild_id: String::new(),
                bot_user_id: String::new(),
                bot_token: String::new(),
                bindings: Vec::new(),
                generation: String::new(),
            },
        )?,
        "discordStatus" => {}
        _ => bail!("Unknown Discord method."),
    }
    let c = load(&hub.store)?;
    if method != "discordStatus" {
        for id in hub.store.reject_stale_discord(c.as_ref().map(|c| c.generation.as_str()))? {
            if let Some(mut entry) = hub.store.entry(&id).filter(|e| e.data["status"] == "queued") {
                entry.data["status"] = "cancelled".into();
                hub.set_entry(&id, &entry.data).context("Could not update cancelled Discord prompt.")?;
            }
        }
    }
    let status = status(hub, c.as_ref());
    Ok(status)
}

/// What a client shows: never the token.
fn status(hub: &Hub, c: Option<&Config>) -> Value {
    let state = hub.store.kv_read("discord.state").ok().flatten();
    json!({
        "configured": c.is_some_and(|c| !c.bot_token.is_empty()),
        "guildId": c.map(|c| &c.guild_id).filter(|g| !g.is_empty()),
        "botUserId": c.map(|c| &c.bot_user_id).filter(|u| !u.is_empty()),
        "bindings": c.map(|c| &c.bindings),
        "state": state.unwrap_or_else(|| "starting".into()),
        "deliveries": hub.store.discord_deliveries().unwrap_or_default(),
    })
}
