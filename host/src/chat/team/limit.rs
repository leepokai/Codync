//! The user's way past the handoff limit: a chain stopped there leaves one notice in the bot's chat whose Continue
//! button (`continueChain`) sends the bot "Continue" as the user, which starts a fresh chain.

use crate::LockExt;
use crate::analytics::{self, Event};
use crate::chat::messages;
use crate::hub::Hub;
use crate::store::{EntryKind, Lane};
use anyhow::{Result, anyhow, bail};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::sync::{Arc, Mutex};

/// The notice's text, which older clients show without the button. A translation key, so the limits it names are
/// held to it where they are set (`requests::MAX_REQUEST_HOPS`, `work::MAX_HOPS`).
const NOTICE: &str = "This work went on 8 steps in a row without you, the most it can. Continue to let it go on.";

/// Which limit stopped the chain; the `via` of `chain_limit_reached`.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Via {
    /// `message_bot`, to a bot here or on another computer.
    Message,
    /// `add_task`.
    Work,
}

/// `turn`'s running turn, acting in `chat`, was refused at a limit: the user's notice, once a turn.
pub fn stopped(hub: &Hub, chat: &str, turn: &str, via: Via) {
    if !hub.team.first_stop(turn) {
        return;
    }
    let data = json!({"text": NOTICE, "style": "info", "continueChain": true});
    if hub.add_entry(&Lane::main(chat), EntryKind::Notice, hub.store.max_turn(chat), &data).is_none() {
        tracing::warn!(bot = chat, "couldn't save the handoff limit notice");
        return;
    }
    analytics::capture(hub, Event::ChainLimitReached, Map::from_iter([("via".into(), json!(via))]));
}

/// `continueChain`: the notice's button, once. The bot gets "Continue" as the user's own message.
pub fn continue_chain(hub: &Arc<Hub>, entry_id: &str) -> Result<Value> {
    // Two clients pressing at once: only the call that clears the flag posts.
    // ponytail: one host-wide lock; presses are rare and the critical section is one read and one write.
    static PRESS: Mutex<()> = Mutex::new(());
    let _press = PRESS.locked();
    let mut notice = hub
        .store
        .entry(entry_id)
        .filter(|e| e.kind == "notice" && e.data["continueChain"] == true)
        .ok_or_else(|| anyhow!("that chain was already continued"))?;
    let row = hub.store.bot(&notice.bot_id)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
    // Refused before anything is saved, so pressing again doesn't stack undelivered messages.
    if !row.config.is_group() && !hub.has_actor(&row.config.id) {
        bail!("that bot isn't running");
    }
    // The button goes before the message does: left on after a send, another press would send a second one.
    notice.data["continueChain"] = false.into();
    hub.set_entry(entry_id, &notice.data).ok_or_else(|| anyhow!("couldn't save the notice"))?;
    let entry = match messages::save(hub, &row.config, json!({"text": "Continue"})) {
        Ok(entry) => entry,
        Err(error) => {
            // Nothing is in the chat: the button comes back to press again.
            notice.data["continueChain"] = true.into();
            if hub.set_entry(entry_id, &notice.data).is_none() {
                tracing::warn!(entry = entry_id, "couldn't put the Continue button back on its notice");
            }
            return Err(error);
        }
    };
    // Saved, the message is in the chat even when the bot can't take it: with the button back, each press would add one.
    messages::hand_on(hub, &row.config, &entry)?;
    Ok(json!({"entry": entry}))
}
