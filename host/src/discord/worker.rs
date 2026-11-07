use super::{Arc, Hub, LockExt, Result, SLOT, client, json, load};
use crate::{
    agent::bot::Cmd,
    store::{EntryKind, Lane, discord::DiscordIncoming},
};
use std::time::Duration;

pub(crate) async fn run(hub: Arc<Hub>) {
    loop {
        if let Err(error) = tick(&hub).await {
            tracing::warn!(%error,"Discord channel needs attention");
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

pub(crate) fn claim_prompt(hub: &Hub, entry: &str) -> Result<bool> {
    let _configuration = hub.store.connector_lock.locked();
    let Some(c) = load(&hub.store)? else { return Ok(false) };
    let Some(e) = hub.store.entry(entry) else { return Ok(false) };
    if e.data["discordGeneration"] != c.generation
        || !c.bindings.iter().any(|b| {
            b.bot_id == e.bot_id
                && e.data["discordChannel"] == b.channel_id
                && e.data["discordUser"].as_str().is_some_and(|user| b.allowed_users.iter().any(|u| u == user))
        })
    {
        return Ok(false);
    }
    hub.store.claim_discord_prompt(entry, &c.generation)
}

async fn tick(hub: &Arc<Hub>) -> Result<()> {
    if hub.store.kv_read(SLOT)?.is_none() {
        return Ok(());
    }
    crate::market::vault::unlock(hub.clone()).await?;
    let Some(c) = load(&hub.store)? else { return Ok(()) };
    let ep = super::Endpoint::discord();
    for event in hub.store.pending_discord()? {
        if let Err(error) = deliver(hub, &event) {
            tracing::warn!(%error,event=%event.event,"Discord prompt was not queued");
        }
    }
    for reply in hub.store.pending_discord_replies(&c.generation)? {
        {
            let _configuration = hub.store.connector_lock.locked();
            if load(&hub.store)?.is_none_or(|current| current.generation != c.generation) {
                return Ok(());
            }
            if hub.store.bot(&reply.bot)?.is_none_or(|bot| bot.deleted || bot.config.is_group()) {
                continue;
            }
            if !hub.store.claim_discord_reply(&reply.entry)? {
                continue;
            }
        }
        let sent = client::message(&ep, &c.bot_token, &reply.channel, &reply.reply_to, &reply.text).await;
        match sent {
            Ok(()) => hub.store.finish_discord_reply(&reply.entry, "sent")?,
            Err(error) => {
                tracing::warn!(%error,entry=%reply.entry,"Discord reply was not sent");
                hub.store.finish_discord_reply(&reply.entry, "failed")?;
            }
        }
    }
    Ok(())
}

fn deliver(hub: &Hub, event: &DiscordIncoming) -> Result<()> {
    let _configuration = hub.store.connector_lock.locked();
    let Some(c) = load(&hub.store)? else {
        hub.store.reject_discord(&event.event)?;
        return Ok(());
    };
    if c.generation != event.generation
        || !c
            .bindings
            .iter()
            .any(|b| b.channel_id == event.channel && b.bot_id == event.bot && b.allowed_users.contains(&event.user))
    {
        hub.store.reject_discord(&event.event)?;
        return Ok(());
    }
    let Some(bot) = hub.store.bot(&event.bot)?.filter(|b| !b.deleted && !b.config.is_group()) else {
        hub.store.reject_discord(&event.event)?;
        return Ok(());
    };
    let root = super::ingress::stable_id(&[&c.generation, &event.channel, "root"]);
    hub.add_entry_once(
        &root,
        &Lane::main(&event.bot),
        EntryKind::User,
        hub.store.max_turn(&event.bot),
        &json!({"text":format!("Discord conversation · {}",event.channel),
        "source":"discord","status":"sent","discordChannel":event.channel}),
    )?;
    let lane = Lane::in_thread(&event.bot, &root);
    let (entry, _) = hub.add_entry_once(
        &event.entry,
        &lane,
        EntryKind::User,
        hub.store.max_turn(&event.bot) + 1,
        &json!({"text":event.text,"status":"queued","source":"discord",
        "discordGeneration":event.generation,"discordChannel":event.channel,"discordUser":event.user}),
    )?;
    if entry.data["status"] != "queued" {
        hub.store.reject_discord(&event.event)?;
        return Ok(());
    }
    let text = crate::chat::prompt::of_entry(hub, &bot.config, &entry.data);
    hub.send_cmd(&event.bot, Cmd::Send { lane, entry_id: entry.id, text })?;
    Ok(())
}
