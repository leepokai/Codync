//! What a user message asks the agent: the note on the message it replies to, its text with a
//! `/skill …` command expanded, then the paths of the files attached to it.

use crate::chat::{replies, uploads};
use crate::hub::Hub;
use crate::store::{BotConfig, Store};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// `skills`: the asked bot's skills on. `root`: the chat's uploads (`uploads::root`; a group's
/// own in a group). `attachments`: the message's `{id, name}` upload metadata, as saved in the chat.
pub fn of_message(store: &Store, skills: &[String], root: &Path, text: &str, attachments: &[Value]) -> String {
    let asked = crate::market::expand_command(store, skills, text);
    let asked = asked.as_deref().unwrap_or(text);
    let paths: Vec<PathBuf> = attachments.iter().filter_map(|meta| uploads::path_of(root, meta)).collect();
    if paths.is_empty() { asked.to_owned() } else { format!("{asked}{}", uploads::prompt_suffix(&paths)) }
}

/// What a user message in `bot`'s own chat (its saved `data`) asks the agent.
pub fn of_entry(hub: &Hub, bot: &BotConfig, data: &Value) -> String {
    let text = data["text"].as_str().unwrap_or_default();
    let attachments = data["attachments"].as_array().map_or(&[][..], Vec::as_slice);
    let asked = of_message(&hub.store, &bot.skills, &uploads::root(bot), text, attachments);
    match replies::note(hub, &bot.id, &data["replyTo"]) {
        Some(note) => format!("{note}\n{asked}"),
        None => asked,
    }
}
