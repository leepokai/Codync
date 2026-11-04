//! Telegram as a way to talk to a bot: named Telegram bots, each assigned to one of the host's bots.
//!
//! The Telegram bot tokens live in the host's vault. A person becomes allowed by sending a one-time code (made by
//! `pairTelegram`) to the Telegram bot; only allowed people reach the assigned bot. Messages are received by
//! long polling (`getUpdates`), so nothing has to be reachable from the internet. The reply is streamed into one
//! message with `editMessageText`, approvals are inline buttons, and `/estado`, `/rutinas` and `/detener` cost no
//! model turn. Every call goes out through `egress`.

mod client;
mod commands;
mod pairing;
mod worker;

#[cfg(test)]
mod tests;

use crate::{hub::Hub, store::Store};
use anyhow::{Context, Result, bail};
use client::Endpoint;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

const SLOT: &str = "telegram.channels";
pub const METHODS: &[&str] =
    &["telegramStatus", "addTelegramBot", "removeTelegramBot", "pairTelegram", "setTelegramAllowed"];
/// Telegram bots a host talks through; more is a mistake.
const MAX_BOTS: usize = 10;
const MAX_ALLOWED: usize = 20;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TgBot {
    /// What the person calls it: `home`, `work`.
    name: String,
    token: String,
    /// `@username` Telegram gave it, without the `@`.
    #[serde(default)]
    username: String,
    /// The host bot its messages go to.
    bot_id: String,
    /// Telegram user ids allowed to talk to it.
    #[serde(default)]
    allowed: Vec<i64>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Config {
    #[serde(default)]
    bots: Vec<TgBot>,
}

fn load(store: &Store) -> Result<Config> {
    Ok(crate::market::vault::read(store, SLOT)?
        .filter(|s| !s.is_empty())
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or_default())
}

fn save(store: &Store, config: &Config) -> Result<()> {
    crate::market::vault::write(store, SLOT, &serde_json::to_string(config)?)
}

pub(crate) fn configuration_state(store: &Store) -> &'static str {
    match load(store) {
        Ok(c) if !c.bots.is_empty() => "configured",
        Ok(_) => "notConfigured",
        Err(_) => "unavailable",
    }
}

/// Runs the receivers of every configured Telegram bot, for as long as the host runs.
pub(crate) async fn run(hub: Arc<Hub>) {
    worker::run(hub, Endpoint::telegram()).await;
}

fn name_ok(name: &str) -> bool {
    (1..=32).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// `123456:ABC-def…`, the shape `BotFather` gives.
fn token_ok(token: &str) -> bool {
    let Some((id, secret)) = token.split_once(':') else { return false };
    token.len() <= 128
        && !id.is_empty()
        && id.bytes().all(|b| b.is_ascii_digit())
        && secret.len() >= 30
        && secret.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn assigned_bot_ok(hub: &Hub, bot_id: &str) -> Result<()> {
    hub.store
        .bot(bot_id)?
        .filter(|b| !b.deleted && !b.config.is_group() && !crate::external::is_external(&b.config))
        .context("Pick one of your bots (not a group or a terminal session).")?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Add {
    name: String,
    token: String,
    bot_id: String,
}

pub(crate) async fn call(hub: &Arc<Hub>, method: &str, body: Value) -> Result<Value> {
    call_with(hub, &Endpoint::telegram(), method, body).await
}

async fn call_with(hub: &Arc<Hub>, ep: &Endpoint, method: &str, body: Value) -> Result<Value> {
    crate::market::vault::unlock(hub.clone()).await?;
    let added = if method == "addTelegramBot" {
        let add: Add = serde_json::from_value(body.clone())?;
        if !name_ok(&add.name) || !token_ok(&add.token) {
            bail!("Use a short name (a-z, 0-9, -) and the token BotFather gave you.");
        }
        assigned_bot_ok(hub, &add.bot_id)?;
        let me = client::call(ep, &add.token, "getMe", &json!({})).await?;
        Some((add, me["username"].as_str().unwrap_or_default().to_owned()))
    } else {
        None
    };
    let _guard = crate::LockExt::locked(&hub.store.connector_lock);
    let mut config = load(&hub.store)?;
    let name = body["name"].as_str().unwrap_or_default();
    let mut result = json!({});
    match method {
        "addTelegramBot" => {
            let (add, username) = added.context("missing bot")?;
            let allowed =
                config.bots.iter().find(|b| b.name == add.name).map(|b| b.allowed.clone()).unwrap_or_default();
            config.bots.retain(|b| b.name != add.name);
            if config.bots.len() >= MAX_BOTS {
                bail!("That is as many Telegram bots as one host talks through ({MAX_BOTS}).");
            }
            config.bots.push(TgBot { name: add.name, token: add.token, username, bot_id: add.bot_id, allowed });
            save(&hub.store, &config)?;
        }
        "removeTelegramBot" => {
            config.bots.retain(|b| b.name != name);
            save(&hub.store, &config)?;
            for key in ["pair", "offset", "seen", "state"] {
                hub.store.kv_delete(&format!("telegram.{key}.{name}"))?;
            }
        }
        "pairTelegram" => {
            config.bots.iter().find(|b| b.name == name).context("Unknown Telegram bot.")?;
            let (code, expires) = pairing::issue(&hub.store, name, crate::store::now_ms())?;
            result = json!({"code": code, "expiresAt": expires});
        }
        "setTelegramAllowed" => {
            let user = body["userId"].as_i64().context("userId must be a Telegram user id.")?;
            let bot = config.bots.iter_mut().find(|b| b.name == name).context("Unknown Telegram bot.")?;
            bot.allowed.retain(|u| *u != user);
            if body["allowed"].as_bool().unwrap_or(true) {
                if bot.allowed.len() >= MAX_ALLOWED {
                    bail!("That is as many people as one Telegram bot lets in ({MAX_ALLOWED}).");
                }
                bot.allowed.push(user);
            }
            save(&hub.store, &config)?;
        }
        "telegramStatus" => {}
        _ => bail!("Unknown Telegram method."),
    }
    let mut status = status(hub, &config);
    status["result"] = result;
    Ok(status)
}

/// What a client shows: never a token, never a live pairing code.
fn status(hub: &Hub, config: &Config) -> Value {
    let kv = |key: &str, name: &str| hub.store.kv_read(&format!("telegram.{key}.{name}")).ok().flatten();
    let bots: Vec<Value> = config
        .bots
        .iter()
        .map(|b| {
            json!({"name": b.name, "username": b.username, "botId": b.bot_id, "botName": hub.bot_name(&b.bot_id),
                "allowed": b.allowed,
                "state": kv("state", &b.name).unwrap_or_else(|| "starting".into()),
                "lastReceivedAt": kv("seen", &b.name).and_then(|s| s.parse::<i64>().ok()),
                "pairingExpiresAt": kv("pair", &b.name).and_then(|s| s.split('|').nth(1).and_then(|e| e.parse::<i64>().ok()))
                    .filter(|e| *e > crate::store::now_ms())})
        })
        .collect();
    json!({"configured": !bots.is_empty(), "bots": bots})
}

#[cfg(test)]
mod shape_tests {
    use super::*;

    #[test]
    fn names_and_tokens_have_the_shape_telegram_and_we_expect() {
        assert!(name_ok("home") && name_ok("work-2"));
        assert!(!name_ok("") && !name_ok("Home") && !name_ok("a b") && !name_ok(&"x".repeat(33)));
        assert!(token_ok("123456789:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw_"));
        for bad in
            ["", "123", "abc:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw_", "1:short", "1:has space AAHdqTcvCH1vGWJxfSeofSAs0K5"]
        {
            assert!(!token_ok(bad), "{bad}");
        }
    }
}
