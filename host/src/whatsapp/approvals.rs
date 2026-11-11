//! A group participant can answer only decisions for their own input.

use crate::hub::Hub;
use crate::store::Entry;
use anyhow::Result;

/// Mixed-author batches and old cards without an input receipt stay in the Codync app.
/// Looking up the last participant in a conversation would grant later writers another
/// person's pending decision.
pub(super) fn sender(hub: &Hub, card: &Entry, chat: &str) -> Result<Option<String>> {
    let Some(ids) = card.data["inputEntryIds"].as_array().filter(|ids| !ids.is_empty()) else {
        return Ok(None);
    };
    let mut sender: Option<String> = None;
    for id in ids {
        let Some(id) = id.as_str() else { return Ok(None) };
        let Some(input) = hub.store.entry_checked(id)? else { return Ok(None) };
        if input.kind != "user"
            || input.bot_id != card.bot_id
            || input.thread_id != card.thread_id
            || input.data["source"] != "whatsapp"
            || input.data["whatsappChat"] != chat
        {
            return Ok(None);
        }
        let Some(author) = input.data["whatsappFence"]["sender"].as_str().filter(|author| !author.is_empty()) else {
            return Ok(None);
        };
        if sender.as_deref().is_some_and(|sender| sender != author) {
            return Ok(None);
        }
        sender = Some(author.to_owned());
    }
    Ok(sender)
}
