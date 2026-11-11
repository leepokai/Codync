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
struct Tasks(Vec<JoinHandle<()>>, super::inbox::Inbox);

static CONFIGURATION: tokio::sync::Notify = tokio::sync::Notify::const_new();

pub(super) fn configuration_changed() {
    CONFIGURATION.notify_waiters();
}

impl Drop for Tasks {
    fn drop(&mut self) {
        self.0.iter().for_each(JoinHandle::abort);
    }
}

/// Where the reply to the last message of a Telegram chat goes.
pub(super) struct Route {
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

pub(super) type Shared = Arc<Mutex<Option<Route>>>;

pub(super) async fn run(hub: Arc<Hub>, ep: Endpoint) {
    let mut running: HashMap<String, (String, Tasks)> = HashMap::new();
    loop {
        let refresh = CONFIGURATION.notified();
        tokio::pin!(refresh);
        refresh.as_mut().enable();
        if let Err(error) = reconcile(&hub, &ep, &mut running).await {
            tracing::warn!(error = format!("{error:#}"), "Telegram channels need attention");
        }
        tokio::select! {
            () = refresh => {},
            () = tokio::time::sleep(ep.reconcile) => {},
        }
    }
}

pub(super) fn fingerprint(bot: &TgBot) -> String {
    let mut allowed = bot.allowed.clone();
    allowed.sort_unstable();
    json!([bot.token, bot.bot_id, bot.username, allowed, bot.generation, bot.memory]).to_string()
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
    running.retain(|name, (print, tasks)| {
        wanted.iter().any(|bot| &bot.name == name)
            && (tasks.1.handling_control()
                || (!tasks.0.iter().any(JoinHandle::is_finished)
                    && wanted.iter().any(|bot| &bot.name == name && &fingerprint(bot) == print)))
    });
    for bot in wanted {
        if running.contains_key(&bot.name) {
            continue;
        }
        let shared = Shared::default();
        let inbox = super::inbox::Inbox::open(hub, &bot.name, fingerprint(&bot))?;
        let monitor = inbox.clone();
        let tasks = Tasks(
            vec![
                tokio::spawn(poll(hub.clone(), ep.clone(), bot.name.clone(), shared.clone(), inbox.clone())),
                tokio::spawn(inbox.run(hub.clone(), ep.clone(), bot.name.clone(), shared.clone())),
                tokio::spawn(stream(hub.clone(), ep.clone(), bot.name.clone(), shared)),
            ],
            monitor,
        );
        running.insert(bot.name.clone(), (fingerprint(&bot), tasks));
    }
    Ok(())
}

pub(super) fn current(hub: &Hub, name: &str) -> Option<TgBot> {
    load(&hub.store).ok()?.bots.into_iter().find(|b| b.name == name)
}

fn set(hub: &Hub, key: &str, name: &str, value: &str) {
    if let Err(error) = hub.store.kv_set(&format!("telegram.{key}.{name}"), value) {
        tracing::warn!(error = format!("{error:#}"), "Telegram state was not saved");
    }
}

pub(super) fn checkpoint(hub: &Hub, name: &str, token: &str, update: &Value) -> anyhow::Result<()> {
    let _guard = crate::LockExt::locked(&hub.store.connector_lock);
    let current = load(&hub.store)?.bots.into_iter().find(|bot| bot.name == name);
    if current.is_none_or(|bot| bot.token != token) {
        return Ok(());
    }
    let Some(id) = update["update_id"].as_i64() else { return Ok(()) };
    let next = id.checked_add(1).ok_or_else(|| anyhow::anyhow!("Invalid Telegram update id"))?;
    let key = format!("telegram.offset.{name}");
    let previous = hub.store.kv_read(&key)?.as_deref().map(str::parse::<i64>).transpose()?.unwrap_or(0);
    hub.store.kv_set(&key, &previous.max(next).to_string())
}

// MARK: receiving

async fn poll(hub: Arc<Hub>, ep: Endpoint, name: String, shared: Shared, inbox: super::inbox::Inbox) {
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
                    if let Err(error) = inbox.accept(&hub, &ep, &name, &shared, &update).await {
                        tracing::warn!(telegram = name, %error, "Telegram input was not saved; polling will retry");
                        break;
                    }
                    if let Err(error) = checkpoint(&hub, &name, &bot.token, &update) {
                        tracing::warn!(telegram = name, %error, "Telegram acknowledgement will retry");
                        break;
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

pub(super) async fn say(ep: &Endpoint, bot: &TgBot, chat: i64, text: &str) {
    if let Err(error) = client::message(ep, &bot.token, chat, text, None).await {
        tracing::warn!(error = format!("{error:#}"), telegram = bot.name, "Telegram message was not sent");
    }
}

pub(super) async fn handle(hub: &Arc<Hub>, ep: &Endpoint, name: &str, shared: &Shared, update: &Value, expected: &str) {
    let Some(bot) = current(hub, name) else { return };
    if fingerprint(&bot) != expected {
        return;
    }
    if let Some(query) = update.get("callback_query") {
        return answer_button(hub, ep, &bot, query).await;
    }
    let message = &update["message"];
    // Only a private chat: a group would let everyone in it speak to the bot.
    if message["chat"]["type"] != "private" {
        return;
    }
    let (Some(chat), Some(user)) = (message["chat"]["id"].as_i64(), message["from"]["id"].as_i64()) else { return };
    let text = message["text"].as_str().or_else(|| message["caption"].as_str()).unwrap_or_default().trim();
    let voice = message.get("voice").or_else(|| message.get("audio"));
    if text.is_empty() && voice.is_none() {
        return;
    }
    set(hub, "seen", name, &crate::store::now_ms().to_string());
    if !bot.allowed.contains(&user) {
        return pair(hub, ep, &bot, chat, user, text).await;
    }
    if text.to_lowercase().contains(&format!("@{}", bot.username.to_lowercase())) {
        hub.routines.receive_event(
            &hub.store,
            &json!({
                "source": "telegram", "event": "mention", "chat": chat, "user": user,
                "username": message["from"]["username"], "text": text,
            }),
            Some(format!("tg-{name}-{}", update["update_id"])),
        );
    }
    if let Some(command) = commands::parse(text, Some(&bot.username)) {
        return say(ep, &bot, chat, &commands::run(hub, &bot.bot_id, &command)).await;
    }
    if text.starts_with("/start") {
        let help = "Write here to talk to your bot. /estado shows what it is doing, /rutinas lists its routines, /detener stops it.";
        return say(ep, &bot, chat, help).await;
    }
    // Authorization and text commands precede audio I/O. A transcript never becomes a transport
    // command such as /detener; it is ordinary message content for this bound bot.
    let text = if let Some(voice) = voice {
        let transcript = async {
            let id = voice["file_id"].as_str().ok_or_else(|| anyhow::anyhow!("Telegram voice note has no file id"))?;
            let bytes = client::voice_note(ep, &bot.token, id).await?;
            crate::voice::transcription::note(&hub.store, &bytes).await
        }
        .await;
        match transcript {
            Ok(transcript) => format!("{text}\n\n[Voice note transcript]\n{transcript}").trim().to_owned(),
            Err(error) => {
                tracing::warn!(
                    telegram = name,
                    error = format!("{error:#}"),
                    "Telegram voice note could not be transcribed"
                );
                return say(ep, &bot, chat, "Could not transcribe that voice note. Check the speech provider in Codync call settings, or send the message as text.").await;
            }
        }
    } else {
        text.to_owned()
    };
    if current(hub, name).is_none_or(|active| fingerprint(&active) != expected || !active.allowed.contains(&user)) {
        return;
    }
    // The route exists before the message is sent: a quick reply must not beat it. Entries of earlier turns are not ours.
    let nonce = format!("tg-{name}-{}", update["update_id"]);
    let turn = hub
        .store
        .find_by_nonce(&bot.bot_id, &nonce)
        .map_or_else(|| hub.store.max_turn(&bot.bot_id) + 1, |entry| entry.turn);
    {
        let mut route = shared.lock().await;
        if route.as_ref().is_none_or(|route| route.chat != chat || route.turn != turn) {
            *route = Some(Route { chat, turn, live: None, cards: HashMap::new() });
        }
    }
    if let Err(error) = super::input::post(hub, name, user, expected, &text, &nonce) {
        *shared.lock().await = None;
        say(ep, &bot, chat, &format!("Could not reach the bot: {error:#}")).await;
    }
}

/// A stranger gets no answer, unless they send `/start CODE` (or `/pair CODE`) with a live code.
async fn pair(hub: &Arc<Hub>, ep: &Endpoint, bot: &TgBot, chat: i64, user: i64, text: &str) {
    let Some(rest) = text.strip_prefix("/start").or_else(|| text.strip_prefix("/pair")) else { return };
    let Some(code) = rest.split_whitespace().last() else { return };
    match pairing::redeem(&hub.store, &bot.name, code, crate::store::now_ms()) {
        Ok(true) => match allow(hub, bot, user) {
            Ok(()) => say(ep, bot, chat, &format!("Paired. You can talk to {} here.", hub.bot_name(&bot.bot_id))).await,
            Err(error) => tracing::warn!(error = format!("{error:#}"), "A paired person was not saved"),
        },
        Ok(false) => say(ep, bot, chat, "That code didn't work. Make a new one in Codync.").await,
        Err(error) => tracing::warn!(error = format!("{error:#}"), "A pairing code could not be checked"),
    }
}

fn allow(hub: &Hub, expected: &TgBot, user: i64) -> anyhow::Result<()> {
    let _guard = crate::LockExt::locked(&hub.store.connector_lock);
    let mut config = load(&hub.store)?;
    let bot = config
        .bots
        .iter_mut()
        .find(|b| b.name == expected.name)
        .ok_or_else(|| anyhow::anyhow!("The Telegram bot is gone."))?;
    anyhow::ensure!(fingerprint(bot) == fingerprint(expected), "Telegram connection changed before pairing completed");
    if !bot.allowed.contains(&user) {
        bot.allowed.push(user);
        bot.generation = uuid::Uuid::new_v4().to_string();
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
