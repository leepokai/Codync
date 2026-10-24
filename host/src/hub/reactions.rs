//! Emoji reactions on chat messages: the user's (`data.reactions`, toggled from any client) and
//! the bots' (`data.botReactions`, one `{bot, emoji}` per bot, set with the `react` tool).

use super::Hub;
use crate::store::{Entry, EntryKind};
use anyhow::{Result, anyhow, bail};
use serde_json::json;

/// The user's reactions per message: a handful of distinct emoji.
const MAX_REACTIONS: usize = 8;

impl Hub {
    /// Toggles the user's `emoji` reaction on a chat message (`data.reactions`, oldest first).
    /// A message keeps `reactions: []` once reacted to, so it never turns unread again.
    pub fn react(&self, id: &str, emoji: &str) -> Result<Entry> {
        let emoji = one_emoji(emoji)?;
        let mut e = self.chat_message(id)?;
        let mut reactions: Vec<String> = serde_json::from_value(e.data["reactions"].clone()).unwrap_or_default();
        if let Some(i) = reactions.iter().position(|r| r == emoji) {
            reactions.remove(i);
        } else if reactions.len() >= MAX_REACTIONS {
            bail!("a message takes at most {MAX_REACTIONS} reactions");
        } else {
            reactions.push(emoji.to_owned());
        }
        e.data["reactions"] = json!(reactions);
        self.set_entry(id, &e.data).ok_or_else(|| anyhow!("couldn't save the reaction"))
    }

    /// `bot` reacts to a chat message with `emoji`, replacing its earlier reaction there.
    /// The message keeps the rev of its content (`contentRev`), so a message the user read stays read.
    pub fn bot_react(&self, id: &str, bot: &str, emoji: &str) -> Result<Entry> {
        let emoji = one_emoji(emoji)?;
        let mut e = self.chat_message(id)?;
        let mut reactions = e.data["botReactions"].as_array().cloned().unwrap_or_default();
        if reactions.iter().any(|r| r["bot"] == bot && r["emoji"] == emoji) {
            return Ok(e);
        }
        reactions.retain(|r| r["bot"] != bot);
        reactions.push(json!({"bot": bot, "emoji": emoji}));
        e.data["botReactions"] = json!(reactions);
        if e.data["contentRev"].is_null() {
            e.data["contentRev"] = e.rev.into();
        }
        self.set_entry(id, &e.data).ok_or_else(|| anyhow!("couldn't save the reaction"))
    }

    /// A message the user sees: theirs, or a bot's final one.
    fn chat_message(&self, id: &str) -> Result<Entry> {
        let e = self.store.entry(id).ok_or_else(|| anyhow!("unknown message"))?;
        let visible =
            e.kind == EntryKind::User.as_str() || (e.kind == EntryKind::Agent.as_str() && e.data["final"] == true);
        if !visible {
            bail!("only chat messages take reactions");
        }
        Ok(e)
    }
}

fn one_emoji(emoji: &str) -> Result<&str> {
    let emoji = emoji.trim();
    if emoji.is_empty() || emoji.len() > 32 || emoji.chars().any(|c| c.is_alphanumeric() || c.is_whitespace()) {
        bail!("`emoji` must be one emoji");
    }
    Ok(emoji)
}
