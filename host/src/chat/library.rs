//! A chat's Library, Grok Bot's way: the artifacts its bot published and the files shared in it, by the bot
//! (`send_file`) or the user (attachments), newest first. The apps open each one the way the chat does: an artifact with
//! `readArtifact`, a bot's file with `readFile`, an attachment with `readUpload`.

use crate::chat::artifacts;
use crate::hub::Hub;
use crate::store::EntryKind;
use anyhow::{Result, ensure};
use serde_json::{Value, json};

/// Messages with files it looks through, newest first.
const MAX_FILE_MESSAGES: i64 = 500;

/// `library {botId}` → `{items: [{kind: artifact | file | attachment, at, …}]}`.
pub fn list(hub: &Hub, bot: &str) -> Result<Value> {
    ensure!(hub.store.bot(bot)?.is_some_and(|row| !row.deleted), "unknown bot");
    let mut items: Vec<Value> = artifacts::list(&hub.store, bot)
        .into_iter()
        .map(|artifact| json!({"kind": "artifact", "at": artifact.updated_at, "artifact": artifact}))
        .collect();
    for e in hub.store.file_messages(bot, MAX_FILE_MESSAGES)? {
        let (kind, files) = if e.kind == EntryKind::User.as_str() {
            ("attachment", &e.data["attachments"])
        } else {
            ("file", &e.data["files"])
        };
        for file in files.as_array().into_iter().flatten() {
            items.push(json!({"kind": kind, "at": e.created_at, "entryId": e.id, "file": file}));
        }
    }
    items.sort_by_key(|item| std::cmp::Reverse(item["at"].as_i64().unwrap_or_default()));
    Ok(json!({"items": items}))
}
