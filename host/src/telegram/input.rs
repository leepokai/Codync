//! Trusted Telegram ingress shares the ordinary message poster, with a revocable actor fence.

use super::{Arc, Hub, TgBot, load, worker};
use crate::store::Entry;
use anyhow::{Context, Result, ensure};
use serde_json::json;
use sha2::{Digest as _, Sha256};

fn epoch(bot: &TgBot) -> String {
    crate::chat::files::digest_hex(&Sha256::digest(worker::fingerprint(bot).as_bytes()))
}

pub(super) fn post(hub: &Arc<Hub>, name: &str, user: i64, expected: &str, text: &str, nonce: &str) -> Result<Entry> {
    let _guard = crate::LockExt::locked(&hub.store.connector_lock);
    let bot = load(&hub.store)?
        .bots
        .into_iter()
        .find(|bot| bot.name == name)
        .context("Telegram connection is unavailable")?;
    ensure!(worker::fingerprint(&bot) == expected && bot.allowed.contains(&user), "Telegram authorization changed");
    ensure!(!text.trim().is_empty(), "Telegram message is empty");
    let row = hub
        .store
        .bot(&bot.bot_id)?
        .filter(|row| !row.deleted && !row.config.hidden)
        .context("Bound bot is unavailable")?;
    ensure!(!row.config.is_group() && !crate::external::is_external(&row.config), "Choose an agent bot");
    if let Some(entry) = hub.store.find_by_nonce_checked(&bot.bot_id, nonce)? {
        return Ok(entry);
    }
    crate::chat::messages::post(
        hub,
        &row.config,
        json!({
            "text":crate::channels::with_memory(&bot.memory, text),"attachments":[],"clientNonce":nonce,
            "source":"telegram","telegramFence":{"name":name,"user":user,"epoch":epoch(&bot)}
        }),
    )
}

pub(crate) fn claim_prompt(hub: &Hub, entry: &str) -> Result<bool> {
    let _guard = crate::LockExt::locked(&hub.store.connector_lock);
    let Some(entry) = hub.store.entry_checked(entry)?.filter(|entry| entry.data["status"] == "queued") else {
        return Ok(false);
    };
    let fence = &entry.data["telegramFence"];
    let (Some(name), Some(user), Some(expected)) =
        (fence["name"].as_str(), fence["user"].as_i64(), fence["epoch"].as_str())
    else {
        return Ok(false);
    };
    let Some(bot) = load(&hub.store)?.bots.into_iter().find(|bot| bot.name == name) else { return Ok(false) };
    let visible = hub.store.bot(&entry.bot_id)?.is_some_and(|row| {
        !row.deleted && !row.config.hidden && !row.config.is_group() && !crate::external::is_external(&row.config)
    });
    Ok(visible && bot.bot_id == entry.bot_id && bot.allowed.contains(&user) && epoch(&bot) == expected)
}
