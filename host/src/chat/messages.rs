//! The user's messages in a chat: typed (`send`) or relayed from a voice call (`chat::calls`).

use crate::agent::bot::Cmd;
use crate::hub::Hub;
use crate::store::{BotConfig, Entry, EntryKind, Lane};
use anyhow::{Result, anyhow};
use serde_json::Value;
use std::sync::Arc;

/// Posts `data` (the message's text and what it carries) as the user's newest message in `cfg`'s chat and hands it
/// on: to the bot's agent, or to a group's members.
pub fn post(hub: &Arc<Hub>, cfg: &BotConfig, data: Value) -> Result<Entry> {
    let e = save(hub, cfg, data)?;
    hand_on(hub, cfg, &e)?;
    Ok(e)
}

/// [`post`]'s first half, `data` saved as the user's newest message in `cfg`'s chat: for a caller to whom a message
/// that was never saved differs from one that was saved and not taken.
pub fn save(hub: &Hub, cfg: &BotConfig, mut data: Value) -> Result<Entry> {
    // `/side` is answered by a throwaway agent, not a turn of the bot's own: it isn't queued behind one.
    if !cfg.is_group() && data["text"].as_str().and_then(crate::chat::side::parse).is_some() {
        data["side"] = true.into();
    }
    // A group has no agent of its own to queue behind: its message is simply sent.
    data["status"] = (if cfg.is_group() || data["side"] == true { "sent" } else { "queued" }).into();
    let turn = hub.store.max_turn(&cfg.id) + 1;
    let e = hub
        .add_entry(&Lane::main(&cfg.id), EntryKind::User, turn, &data)
        .ok_or_else(|| anyhow!("couldn't save the message"))?;
    // Before the turn starts, so its reply lands unread.
    if let Err(error) = hub.mark_read(&cfg.id) {
        tracing::warn!(%error, bot = %cfg.id, "couldn't mark bot read after send");
    }
    Ok(e)
}

/// [`post`]'s second half: the saved message `e` goes to the bot's agent, or to a group's members.
pub fn hand_on(hub: &Arc<Hub>, cfg: &BotConfig, e: &Entry) -> Result<()> {
    let lane = Lane::main(&cfg.id);
    // `/side <question>`: answered off to the side, never as a turn the bot's queue has to run.
    if let Some(question) = e.data["text"].as_str().and_then(crate::chat::side::parse) {
        if cfg.is_group() {
            crate::chat::side::not_in_a_group(hub, cfg);
        } else if question.is_empty() {
            crate::chat::side::asked_nothing(hub, cfg);
        } else {
            crate::chat::side::start(hub, cfg, question);
        }
        return Ok(());
    }
    if cfg.is_group() {
        // Each member reads it in its room prompt (`chat::group`).
        crate::chat::group::start(hub, cfg, lane);
    } else {
        // `/skill …` reaches the agent as "read this skill first"; the chat keeps what was typed.
        let prompt = crate::chat::prompt::of_entry(hub, cfg, e);
        hub.send_cmd(&cfg.id, Cmd::Send { lane, entry_id: e.id.clone(), text: prompt })?;
    }
    Ok(())
}
