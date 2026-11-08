use super::{Arc, Hub, LockExt, Result, SLOT, client, json, load};
use crate::{
    agent::bot::Cmd,
    store::{EntryKind, Lane, discord::DiscordIncoming},
};
use std::time::Duration;

pub(crate) async fn run(hub: Arc<Hub>) {
    tokio::join!(sending(hub.clone()), watching(hub));
}

/// Prompt and reply queues, durable across restarts.
async fn sending(hub: Arc<Hub>) {
    loop {
        if let Err(error) = tick(&hub).await {
            tracing::warn!(%error,"Discord channel needs attention");
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Approval cards: posted when one is asked in a bound conversation, settled once anyone answers.
async fn watching(hub: Arc<Hub>) {
    let mut events = hub.events.subscribe();
    loop {
        let event = match events.recv().await {
            Ok(event) => event,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
        };
        if event["type"] != "entry" || event["entry"]["kind"] != "permission" {
            continue;
        }
        if let Err(error) = card(&hub, &event["entry"]).await {
            tracing::warn!(error = format!("{error:#}"), "Discord approval card was not shown");
        }
    }
}

async fn card(hub: &Arc<Hub>, entry: &serde_json::Value) -> Result<()> {
    let Some(id) = entry["id"].as_str() else { return Ok(()) };
    let Some(thread) = entry["threadId"].as_str() else { return Ok(()) };
    let Some(root) = hub.store.entry(thread) else { return Ok(()) };
    let Some(channel) = root.data["discordChannel"].as_str() else { return Ok(()) };
    let Some(bot) = entry["botId"].as_str() else { return Ok(()) };
    let Some(c) = load(&hub.store)? else { return Ok(()) };
    let Some(binding) = c.bindings.iter().find(|b| b.bot_id == bot) else { return Ok(()) };
    if root.data["discordUser"].as_str().is_none_or(|u| !binding.allowed_users.iter().any(|a| a == u)) {
        return Ok(());
    }
    let shown = entry["data"]["discordCard"].as_str().is_some();
    let settled = entry["data"]["discordSettled"] == true;
    match (entry["data"]["status"].as_str(), shown, settled) {
        (Some("pending"), false, _) => {
            let buttons: Vec<(String, String)> = entry["data"]["options"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|o| {
                    Some((o["name"].as_str()?.to_owned(), super::interactions::button(entry, o["optionId"].as_str()?)))
                })
                .collect();
            if buttons.is_empty() {
                return Ok(());
            }
            let message = client::card(
                &super::Endpoint::discord(),
                &c.bot_token,
                channel,
                &super::interactions::text(entry),
                &buttons,
            )
            .await?;
            let mut data = entry["data"].clone();
            data["discordCard"] = message.into();
            hub.set_entry(id, &data);
        }
        (Some(status), true, false) if status != "pending" => {
            let message = entry["data"]["discordCard"].as_str().unwrap_or_default();
            let chosen = entry["data"]["options"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|o| o["optionId"] == entry["data"]["selected"])
                .and_then(|o| o["name"].as_str());
            let text = match chosen {
                Some(name) => format!("{}\n✓ {name}", super::interactions::text(entry)),
                None => format!("{}\n(closed)", super::interactions::text(entry)),
            };
            client::settle(&super::Endpoint::discord(), &c.bot_token, channel, message, &text).await?;
            let mut data = entry["data"].clone();
            data["discordSettled"] = true.into();
            hub.set_entry(id, &data);
        }
        _ => {}
    }
    Ok(())
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
        let sent = client::message(&ep, &c.bot_token, &reply.channel, &reply.reply_to, &reply.text, &reply.files).await;
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
    let Some(binding) = c
        .bindings
        .iter()
        .find(|b| b.channel_id == event.channel && b.bot_id == event.bot && b.allowed_users.contains(&event.user))
    else {
        hub.store.reject_discord(&event.event)?;
        return Ok(());
    };
    let text = crate::channels::with_memory(&binding.memory, &event.text);
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
        "source":"discord","status":"sent","discordChannel":event.channel,"discordUser":event.user}),
    )?;
    let lane = Lane::in_thread(&event.bot, &root);
    let (entry, _) = hub.add_entry_once(
        &event.entry,
        &lane,
        EntryKind::User,
        hub.store.max_turn(&event.bot) + 1,
        &json!({"text":text,"status":"queued","source":"discord",
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
