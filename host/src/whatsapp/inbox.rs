//! Ordered durable message metadata; the bridge reader remains free for link events and approvals.

use super::{Hub, Value, WhatsApp, entry_id};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, json};
use sha2::{Digest as _, Sha256};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

const KEY: &str = "whatsapp.inbox.v1";

#[cfg(test)]
mod tests;

#[derive(Serialize, Deserialize)]
struct Pending {
    id: String,
    generation: String,
    event: Value,
}

fn read(hub: &Hub) -> Result<Vec<Pending>> {
    Ok(hub.store.kv_read(KEY)?.as_deref().map(serde_json::from_str).transpose()?.unwrap_or_default())
}

pub(super) fn invalidate_link(hub: &Hub) -> Result<()> {
    hub.whatsapp.revocation_pending.store(true, Ordering::SeqCst);
    let _guard = crate::LockExt::locked(&hub.store.connector_lock);
    let result = hub.store.with_connection(|connection| {
        let tx = connection.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO kv(k,v) VALUES('whatsapp.link.generation',?1) ON CONFLICT(k) DO UPDATE SET v=excluded.v",
            [uuid::Uuid::new_v4().to_string()],
        )?;
        tx.execute("DELETE FROM kv WHERE k=?1", [KEY])?;
        tx.execute_batch("DELETE FROM whatsapp_threads; DELETE FROM whatsapp_cards;")?;
        tx.commit()?;
        Ok(())
    });
    if result.is_ok() {
        hub.whatsapp.revocation_pending.store(false, Ordering::SeqCst);
    }
    result
}

/// Caller holds `connector_lock` when using this as a publication/turn-start fence.
pub(super) fn snapshot(hub: &Hub, event: &Value, me: &str) -> Result<Option<String>> {
    let Some(bot) = event["_codyncBotId"].as_str() else { return Ok(None) };
    let Some(row) = hub.store.bot(bot)?.filter(|row| {
        !row.deleted && !row.config.hidden && !row.config.is_group() && !crate::external::is_external(&row.config)
    }) else {
        return Ok(None);
    };
    if me.is_empty() || event["_codyncMe"] != me {
        return Ok(None);
    }
    if hub.store.kv_read("whatsapp.link.account")?.as_deref() != Some(me) {
        return Ok(None);
    }
    let uses_default = event["_codyncUsesDefault"] == true;
    if uses_default && hub.store.kv_read("whatsapp.bot")?.as_deref() != Some(bot) {
        return Ok(None);
    }
    let mut allowed: Vec<String> =
        hub.store.kv_read("whatsapp.allowed")?.as_deref().map(serde_json::from_str).transpose()?.unwrap_or_default();
    allowed.sort();
    let (Some(chat), Some(sender)) = (event["chat"].as_str(), event["sender"].as_str()) else { return Ok(None) };
    let own = chat == me || sender == me;
    let principal = if event["group"] == true { chat } else { sender };
    if !own && !allowed.is_empty() && !allowed.iter().any(|id| id == principal) {
        return Ok(None);
    }
    let default_epoch = if uses_default { hub.store.kv_read("whatsapp.bot.generation")? } else { None };
    let configuration = json!([
        me,
        chat,
        sender,
        event["group"],
        allowed,
        row.config.id,
        default_epoch,
        hub.store.kv_read("whatsapp.allowed.generation")?,
        hub.store.kv_read("whatsapp.link.generation")?
    ]);
    Ok(Some(crate::chat::files::digest_hex(&Sha256::digest(serde_json::to_vec(&configuration)?))))
}

async fn fingerprint(app: &WhatsApp, hub: &Hub, event: &Value) -> Result<Option<String>> {
    let state = app.state().await;
    if state.state != "connected" {
        return Ok(None);
    }
    snapshot(hub, event, state.me.as_deref().unwrap_or_default())
}

/// Actor-side authorization uses persisted account/access epochs, including after a reconnect.
/// It never treats a queued prompt as permission to use a newly linked account or another bot.
pub(crate) fn claim_prompt(hub: &Hub, entry: &str) -> Result<bool> {
    let _guard = crate::LockExt::locked(&hub.store.connector_lock);
    let Some(entry) = hub.store.entry_checked(entry)?.filter(|entry| entry.data["status"] == "queued") else {
        return Ok(false);
    };
    let fence = &entry.data["whatsappFence"];
    if fence["_codyncBotId"].as_str() != Some(entry.bot_id.as_str()) {
        return Ok(false);
    }
    let (Some(me), Some(expected)) = (fence["_codyncMe"].as_str(), entry.data["whatsappGeneration"].as_str()) else {
        return Ok(false);
    };
    Ok(snapshot(hub, fence, me)?.as_deref() == Some(expected))
}

pub(super) async fn receive(app: &WhatsApp, hub: &Arc<Hub>, event: &Value) -> Result<()> {
    let epoch = event["_codyncTransportEpoch"].as_u64().context("WhatsApp message has no transport epoch")?;
    if app.transport_epoch.load(Ordering::SeqCst) != epoch {
        return Ok(());
    }
    let chat = event["chat"].as_str().context("WhatsApp message has no chat")?;
    let sender = event["sender"].as_str().context("WhatsApp message has no sender")?;
    let group = event["group"] == true;
    let me = event["_codyncTransportMe"]
        .as_str()
        .filter(|me| !me.is_empty())
        .context("WhatsApp message has no linked identity")?;
    if group && event["mentioned"] != true && event["quoted"] != true {
        return Ok(());
    }
    let allowed: Vec<String> =
        hub.store.kv_read("whatsapp.allowed")?.as_deref().map(serde_json::from_str).transpose()?.unwrap_or_default();
    let own = !me.is_empty() && (me == chat || me == sender);
    if !own && !allowed.is_empty() && !allowed.iter().any(|id| id == if group { chat } else { sender }) {
        return Ok(());
    }
    // Approval responses bypass only the slow input queue, with the usual sender/card checks.
    if event["media"].is_null()
        && event["text"].as_str().is_some_and(|text| text.trim().parse::<usize>().is_ok())
        && hub.store.whatsapp_card(chat, sender).is_some()
    {
        app.answer_card(hub, chat, sender, event["text"].as_str().unwrap_or_default()).await;
        return Ok(());
    }
    let roster: Vec<_> = hub
        .store
        .bots()?
        .into_iter()
        .filter(|row| {
            !row.deleted && !row.config.hidden && !row.config.is_group() && !crate::external::is_external(&row.config)
        })
        .collect();
    let (chosen, text) = super::mentioned_bot(&roster, event["text"].as_str().unwrap_or_default());
    let bot = match chosen {
        Some(row) => Some(row.config.id.clone()),
        None => hub.store.kv_read("whatsapp.bot")?,
    };
    let Some(bot) = bot.filter(|bot| roster.iter().any(|row| row.config.id == *bot)) else {
        app.send_message(chat, "No hay ningún bot asignado a este WhatsApp. Elige uno en Ajustes.", None, None, None)
            .await?;
        return Ok(());
    };
    let mut small: Map<String, Value> = event
        .as_object()
        .context("Invalid WhatsApp message")?
        .iter()
        .filter(|(key, _)| key.as_str() != "media")
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    small.insert("text".into(), text.into());
    small.insert("_codyncBotId".into(), bot.into());
    small.insert("_codyncUsesDefault".into(), chosen.is_none().into());
    small.insert("_codyncMe".into(), me.into());
    let Some(generation) = snapshot(hub, &Value::Object(small.clone()), me)? else { return Ok(()) };
    if let Some(media) = event["media"].as_object() {
        // Keep only metadata in SQLite; the existing attachment store owns the bytes.
        let mut metadata: Map<String, Value> = media
            .iter()
            .filter(|(key, _)| key.as_str() != "data")
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        match super::save_media(media).await {
            Ok(Some(path)) => {
                metadata.insert("_codyncPath".into(), path.into());
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "WhatsApp attachment could not be saved"),
        }
        small.insert("media".into(), Value::Object(metadata));
    }
    let event = Value::Object(small);
    ensure!(serde_json::to_vec(&event)?.len() <= 256 * 1024, "WhatsApp message metadata exceeds 256 KiB");
    let id = entry_id(&["inbox", chat, event["id"].as_str().context("WhatsApp message has no id")?]);
    let full = {
        let _guard = crate::LockExt::locked(&hub.store.connector_lock);
        if app.transport_epoch.load(Ordering::SeqCst) != epoch {
            return Ok(());
        }
        if snapshot(hub, &event, me)?.as_deref() != Some(generation.as_str()) {
            return Ok(());
        }
        let mut queue = read(hub)?;
        if queue.iter().any(|pending| pending.id == id) {
            return Ok(());
        }
        if queue.len() >= 32 {
            true
        } else {
            queue.push(Pending { id, generation, event });
            hub.store.kv_set(KEY, &serde_json::to_string(&queue)?)?;
            false
        }
    };
    if full {
        app.send_message(chat, "The input queue is full. Please try again shortly.", None, None, None).await?;
    }
    Ok(())
}

async fn revoked(app: &WhatsApp, hub: &Hub, pending: &Pending) -> Result<bool> {
    loop {
        if app.state().await.state != "connected" {
            return Ok(false);
        }
        match fingerprint(app, hub, &pending.event).await? {
            Some(current) if current == pending.generation => {}
            _ => return Ok(true),
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn tick(app: &WhatsApp, hub: &Arc<Hub>) -> Result<()> {
    let next = {
        let _guard = crate::LockExt::locked(&hub.store.connector_lock);
        read(hub)?.into_iter().next()
    };
    let Some(pending) = next else { return Ok(()) };
    if app.state().await.state != "connected" {
        return Ok(());
    }
    if fingerprint(app, hub, &pending.event).await?.as_deref() == Some(pending.generation.as_str()) {
        tokio::select! {
            biased;
            result = revoked(app, hub, &pending) => { if !result? { return Ok(()) } },
            () = app.on_message(hub, &pending.event, &pending.generation) => {},
        }
    }
    if app.state().await.state != "connected" {
        return Ok(());
    }
    let _guard = crate::LockExt::locked(&hub.store.connector_lock);
    let mut queue = read(hub)?;
    queue.retain(|job| job.id != pending.id);
    hub.store.kv_set(KEY, &serde_json::to_string(&queue)?)
}

pub(super) fn start(app: &Arc<WhatsApp>, hub: &Arc<Hub>) {
    if app.incoming_worker.swap(true, Ordering::SeqCst) {
        return;
    }
    let (app, hub) = (Arc::downgrade(app), Arc::downgrade(hub));
    tokio::spawn(async move {
        loop {
            let (Some(app), Some(hub)) = (app.upgrade(), hub.upgrade()) else { return };
            if app.state().await.state == "connected"
                && let Err(error) = tick(&app, &hub).await
            {
                tracing::warn!(%error, "WhatsApp input queue needs attention");
            }
            drop((app, hub));
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });
}
