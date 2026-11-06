//! The receivers: one poller and one streamer per configured Telegram bot, kept in step with the configuration.
//!
//! The poller reads updates (`getUpdates`), pairs people, answers commands and hands a message to the assigned bot as
//! if it had been typed in a client. The streamer watches the host's events and shows the reply, and the approvals
//! the bot asks, in the Telegram chat the message came from.

use super::client::{self, Endpoint};
use super::{Arc, Hub, SLOT, TgBot, Value, commands, json, load, pairing};
use crate::api::devices::Caller;
use sha2::{Digest as _, Sha256};
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::{Mutex, broadcast::error::RecvError};
use tokio::task::JoinHandle;

/// Aborts its tasks when dropped, so a stopped host loop leaves nothing polling.
struct Tasks(Vec<JoinHandle<()>>);

impl Drop for Tasks {
    fn drop(&mut self) {
        self.0.iter().for_each(JoinHandle::abort);
    }
}

/// Where the reply to the last message of a Telegram chat goes.
struct Route {
    chat: i64,
    /// Entries of this turn and later belong to it.
    turn: i64,
    live: Option<Live>,
    /// Approval cards shown in the chat, by entry id → Telegram message id.
    cards: HashMap<String, i64>,
}

/// The Telegram message the current reply is being written into.
struct Live {
    entry: String,
    message: i64,
    sent: String,
    at: Instant,
}

type Shared = Arc<Mutex<Option<Route>>>;

pub(super) async fn run(hub: Arc<Hub>, ep: Endpoint) {
    let mut running: HashMap<String, (String, Tasks)> = HashMap::new();
    loop {
        if let Err(error) = reconcile(&hub, &ep, &mut running).await {
            tracing::warn!(error = format!("{error:#}"), "Telegram channels need attention");
        }
        tokio::time::sleep(ep.reconcile).await;
    }
}

fn fingerprint(bot: &TgBot) -> String {
    format!("{}|{}|{}", bot.token, bot.bot_id, bot.username)
}

async fn reconcile(
    hub: &Arc<Hub>,
    ep: &Endpoint,
    running: &mut HashMap<String, (String, Tasks)>,
) -> anyhow::Result<()> {
    let wanted = if hub.store.kv_read(SLOT)?.is_none() {
        Vec::new()
    } else {
        crate::market::vault::unlock(hub.clone()).await?;
        load(&hub.store)?.bots
    };
    running.retain(|name, (print, _)| wanted.iter().any(|b| &b.name == name && &fingerprint(b) == print));
    for bot in wanted {
        if running.contains_key(&bot.name) {
            continue;
        }
        let shared = Shared::default();
        let tasks = Tasks(vec![
            tokio::spawn(poll(hub.clone(), ep.clone(), bot.name.clone(), shared.clone())),
            tokio::spawn(stream(hub.clone(), ep.clone(), bot.name.clone(), shared)),
        ]);
        running.insert(bot.name.clone(), (fingerprint(&bot), tasks));
    }
    Ok(())
}

fn current(hub: &Hub, name: &str) -> Option<TgBot> {
    load(&hub.store).ok()?.bots.into_iter().find(|b| b.name == name)
}

fn set(hub: &Hub, key: &str, name: &str, value: &str) {
    if let Err(error) = hub.store.kv_set(&format!("telegram.{key}.{name}"), value) {
        tracing::warn!(error = format!("{error:#}"), "Telegram state was not saved");
    }
}

// MARK: receiving

async fn poll(hub: Arc<Hub>, ep: Endpoint, name: String, shared: Shared) {
    loop {
        let Some(bot) = current(&hub, &name) else { return };
        let offset = hub
            .store
            .kv_read(&format!("telegram.offset.{name}"))
            .ok()
            .flatten()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        match client::updates(&ep, &bot.token, offset).await {
            Ok(list) => {
                set(&hub, "state", &name, "ok");
                for update in list {
                    handle(&hub, &ep, &name, &shared, &update).await;
                    if let Some(id) = update["update_id"].as_i64() {
                        set(&hub, "offset", &name, &(id + 1).to_string());
                    }
                }
            }
            Err(error) => {
                let text = format!("{error:#}");
                tracing::warn!(error = text, telegram = name, "Telegram could not be reached");
                set(&hub, "state", &name, &format!("error: {}", text.chars().take(160).collect::<String>()));
                tokio::time::sleep(ep.retry).await;
            }
        }
    }
}

async fn say(ep: &Endpoint, bot: &TgBot, chat: i64, text: &str) {
    if let Err(error) = client::message(ep, &bot.token, chat, text, None).await {
        tracing::warn!(error = format!("{error:#}"), telegram = bot.name, "Telegram message was not sent");
    }
}

async fn handle(hub: &Arc<Hub>, ep: &Endpoint, name: &str, shared: &Shared, update: &Value) {
    let Some(bot) = current(hub, name) else { return };
    if let Some(query) = update.get("callback_query") {
        return answer_button(hub, ep, &bot, query).await;
    }
    let message = &update["message"];
    // Only a private chat: a group would let everyone in it speak to the bot.
    if message["chat"]["type"] != "private" {
        return;
    }
    let (Some(chat), Some(user)) = (message["chat"]["id"].as_i64(), message["from"]["id"].as_i64()) else { return };
    let text = message["text"].as_str().unwrap_or_default().trim();
    if text.is_empty() {
        return;
    }
    set(hub, "seen", name, &crate::store::now_ms().to_string());
    if !bot.allowed.contains(&user) {
        return pair(hub, ep, &bot, chat, user, text).await;
    }
    if let Some(command) = commands::parse(text, Some(&bot.username)) {
        return say(ep, &bot, chat, &commands::run(hub, &bot.bot_id, &command)).await;
    }
    if text.starts_with("/start") {
        let help = "Write here to talk to your bot. /estado shows what it is doing, /rutinas lists its routines, /detener stops it.";
        return say(ep, &bot, chat, help).await;
    }
    // The route exists before the message is sent: a quick reply must not beat it. Entries of earlier turns are not ours.
    let turn = hub.store.max_turn(&bot.bot_id) + 1;
    *shared.lock().await = Some(Route { chat, turn, live: None, cards: HashMap::new() });
    let nonce = format!("tg-{name}-{}", update["update_id"]);
    let body = json!({"botId": bot.bot_id, "text": text, "clientNonce": nonce});
    if let Err(error) = crate::api::dispatch(hub, &Caller::Local, "send", body).await {
        *shared.lock().await = None;
        say(ep, &bot, chat, &format!("Could not reach the bot: {error:#}")).await;
    }
}

/// A stranger gets no answer, unless they send `/start CODE` (or `/pair CODE`) with a live code.
async fn pair(hub: &Arc<Hub>, ep: &Endpoint, bot: &TgBot, chat: i64, user: i64, text: &str) {
    let Some(rest) = text.strip_prefix("/start").or_else(|| text.strip_prefix("/pair")) else { return };
    let Some(code) = rest.split_whitespace().last() else { return };
    match pairing::redeem(&hub.store, &bot.name, code, crate::store::now_ms()) {
        Ok(true) => match allow(hub, &bot.name, user) {
            Ok(()) => say(ep, bot, chat, &format!("Paired. You can talk to {} here.", hub.bot_name(&bot.bot_id))).await,
            Err(error) => tracing::warn!(error = format!("{error:#}"), "A paired person was not saved"),
        },
        Ok(false) => say(ep, bot, chat, "That code didn't work. Make a new one in Codync.").await,
        Err(error) => tracing::warn!(error = format!("{error:#}"), "A pairing code could not be checked"),
    }
}

fn allow(hub: &Hub, name: &str, user: i64) -> anyhow::Result<()> {
    let _guard = crate::LockExt::locked(&hub.store.connector_lock);
    let mut config = load(&hub.store)?;
    let bot =
        config.bots.iter_mut().find(|b| b.name == name).ok_or_else(|| anyhow::anyhow!("The Telegram bot is gone."))?;
    if !bot.allowed.contains(&user) {
        bot.allowed.push(user);
    }
    super::save(&hub.store, &config)
}

// MARK: approval buttons

/// Identifies what a button was made for, so a button of a card that changed cannot answer another.
fn card_print(card: &Value) -> String {
    let mut digest = Sha256::new();
    digest.update(card["data"]["title"].as_str().unwrap_or_default());
    for option in card["data"]["options"].as_array().into_iter().flatten() {
        digest.update(option["optionId"].as_str().unwrap_or_default());
    }
    crate::chat::files::digest_hex(&digest.finalize()).chars().take(6).collect()
}

fn keyboard(card: &Value) -> Option<Value> {
    let id = card["id"].as_str()?;
    let print = card_print(card);
    let row: Vec<Value> = card["data"]["options"]
        .as_array()?
        .iter()
        .filter_map(|o| {
            let data = format!("p|{id}|{}|{print}", o["optionId"].as_str()?);
            // Telegram refuses callback data over 64 bytes.
            let text = o["name"].as_str()?;
            (data.len() <= 64).then(|| json!({"text": text, "callback_data": data}))
        })
        .collect();
    (!row.is_empty()).then(|| json!(row.chunks(2).collect::<Vec<_>>()))
}

async fn answer_button(hub: &Arc<Hub>, ep: &Endpoint, bot: &TgBot, query: &Value) {
    let reply = |text: String| async move {
        let body = json!({"callback_query_id": query["id"], "text": text});
        if let Err(error) = client::call(ep, &bot.token, "answerCallbackQuery", &body).await {
            tracing::warn!(error = format!("{error:#}"), "A button press was not answered");
        }
    };
    if !query["from"]["id"].as_i64().is_some_and(|u| bot.allowed.contains(&u)) {
        return reply("You are not allowed to answer this.".into()).await;
    }
    let mut parts = query["data"].as_str().unwrap_or_default().split('|');
    let (Some("p"), Some(id), Some(option), Some(print)) = (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return reply("That button is not for this.".into()).await;
    };
    let card = hub.store.entry(id).filter(|e| e.bot_id == bot.bot_id && e.kind == "permission");
    let Some(card) = card.and_then(|e| serde_json::to_value(e).ok()) else {
        return reply("That approval is gone.".into()).await;
    };
    if card_print(&card) != print {
        return reply("That approval changed.".into()).await;
    }
    // The first answer wins, from any client; a later one finds the card settled.
    let answered =
        crate::api::dispatch(hub, &Caller::Local, "respondPermission", json!({"entryId": id, "optionId": option}))
            .await;
    reply(match answered {
        Ok(_) => "Done.".into(),
        Err(error) => format!("{error:#}").chars().take(180).collect(),
    })
    .await;
}

// MARK: replying

async fn stream(hub: Arc<Hub>, ep: Endpoint, name: String, shared: Shared) {
    let mut events = hub.events.subscribe();
    loop {
        let event = match events.recv().await {
            Ok(event) => event,
            Err(RecvError::Lagged(_)) => continue,
            Err(RecvError::Closed) => return,
        };
        let Some(bot) = current(&hub, &name) else { return };
        let entry = &event["entry"];
        if event["type"] != "entry" || entry["botId"] != bot.bot_id.as_str() || entry.get("threadId").is_some() {
            continue;
        }
        let mut route = shared.lock().await;
        let Some(route) = route.as_mut().filter(|r| entry["turn"].as_i64().unwrap_or(0) >= r.turn) else { continue };
        let result = match entry["kind"].as_str() {
            Some("agent") => reply(&ep, &bot, route, entry).await.map(|finished| {
                if finished {
                    route.live = None;
                }
            }),
            Some("permission") => card(&ep, &bot, route, entry).await,
            _ => Ok(()),
        };
        if let Err(error) = result {
            tracing::warn!(error = format!("{error:#}"), telegram = name, "Telegram could not show a reply");
        }
    }
}

/// Writes the agent's text into the chat: one message that grows while it streams. True when the reply is complete.
async fn reply(ep: &Endpoint, bot: &TgBot, route: &mut Route, entry: &Value) -> anyhow::Result<bool> {
    let text = entry["data"]["text"].as_str().unwrap_or_default();
    let finished = entry["data"]["final"] == true;
    let id = entry["id"].as_str().unwrap_or_default();
    if text.is_empty() {
        return Ok(false);
    }
    match route.live.as_mut().filter(|l| l.entry == id) {
        None => {
            let message = client::message(ep, &bot.token, route.chat, text, None).await?;
            route.live = Some(Live { entry: id.into(), message, sent: text.into(), at: Instant::now() });
        }
        Some(live) if live.sent != text && (finished || live.at.elapsed() >= ep.edit_every) => {
            client::edit(ep, &bot.token, route.chat, live.message, text).await?;
            live.sent = text.into();
            live.at = Instant::now();
        }
        Some(_) => {}
    }
    if finished && text.chars().count() > client::MAX_TEXT {
        client::document(ep, &bot.token, route.chat, "reply.txt", text.as_bytes(), "The whole reply is in this file.")
            .await?;
    }
    Ok(finished)
}

/// An approval the bot asks becomes a message with buttons; once it is settled anywhere, the buttons go.
async fn card(ep: &Endpoint, bot: &TgBot, route: &mut Route, entry: &Value) -> anyhow::Result<()> {
    let id = entry["id"].as_str().unwrap_or_default();
    let title = entry["data"]["title"].as_str().unwrap_or("Approval");
    match (entry["data"]["status"].as_str(), route.cards.get(id)) {
        (Some("pending"), None) => {
            let Some(buttons) = keyboard(entry) else { return Ok(()) };
            let message =
                client::message(ep, &bot.token, route.chat, &format!("Needs you: {title}"), Some(buttons)).await?;
            route.cards.insert(id.into(), message);
        }
        (Some("pending") | None, _) | (Some(_), None) => {}
        (Some(status), Some(&message)) => {
            let chosen = entry["data"]["options"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|o| o["optionId"] == entry["data"]["selected"])
                .and_then(|o| o["name"].as_str());
            let line = match (status, chosen) {
                ("answered", Some(name)) => format!("{title}\n✓ {name}"),
                _ => format!("{title}\n(closed)"),
            };
            client::edit(ep, &bot.token, route.chat, message, &line).await?;
            route.cards.remove(id);
        }
    }
    Ok(())
}
