//! Connection settings and revocation epochs are published together under the connector lock.

use crate::hub::Hub;
use anyhow::{Context, Result, ensure};
use rusqlite::params;

fn publish(hub: &Hub, key: &str, value: &str, epoch: &str) -> Result<()> {
    hub.store.with_connection(|connection| {
        let tx = connection.unchecked_transaction()?;
        for (key, value) in [(key, value.to_owned()), (epoch, uuid::Uuid::new_v4().to_string())] {
            tx.execute(
                "INSERT INTO kv(k,v) VALUES(?1,?2) ON CONFLICT(k) DO UPDATE SET v=excluded.v",
                params![key, value],
            )?;
        }
        if key == "whatsapp.link.account" {
            tx.execute_batch("DELETE FROM whatsapp_threads; DELETE FROM whatsapp_cards;")?;
        }
        tx.commit()?;
        Ok(())
    })
}

pub(super) fn record_account(hub: &Hub, me: &str) -> Result<()> {
    ensure!(!me.is_empty(), "WhatsApp did not identify the linked account");
    let _guard = crate::LockExt::locked(&hub.store.connector_lock);
    if hub.store.kv_read("whatsapp.link.account")?.as_deref() != Some(me) {
        publish(hub, "whatsapp.link.account", me, "whatsapp.link.generation")?;
    }
    Ok(())
}

pub(crate) fn set_bot(hub: &Hub, bot: &str) -> Result<()> {
    let _guard = crate::LockExt::locked(&hub.store.connector_lock);
    if !bot.is_empty() {
        let row = hub.store.bot(bot)?.filter(|row| !row.deleted && !row.config.hidden).context("Bot is unavailable")?;
        ensure!(!row.config.is_group() && !crate::external::is_external(&row.config), "Choose an agent bot");
    }
    if hub.store.kv_read("whatsapp.bot")?.as_deref() != Some(bot) {
        publish(hub, "whatsapp.bot", bot, "whatsapp.bot.generation")?;
    }
    Ok(())
}

pub(crate) fn set_allowed(hub: &Hub, jid: &str, allowed: bool) -> Result<Vec<String>> {
    ensure!(
        !jid.is_empty() && jid.len() <= 120 && !jid.contains(char::is_whitespace),
        "Use a WhatsApp id, such as 5215555555555@s.whatsapp.net."
    );
    let _guard = crate::LockExt::locked(&hub.store.connector_lock);
    let mut list: Vec<String> =
        hub.store.kv_read("whatsapp.allowed")?.as_deref().map(serde_json::from_str).transpose()?.unwrap_or_default();
    let before = list.clone();
    list.retain(|entry| entry != jid);
    if allowed {
        ensure!(list.len() < 200, "That is as many chats as one number allows (200).");
        list.push(jid.to_owned());
    }
    list.sort();
    let mut previous = before;
    previous.sort();
    if previous != list {
        publish(hub, "whatsapp.allowed", &serde_json::to_string(&list)?, "whatsapp.allowed.generation")?;
    }
    Ok(list)
}
