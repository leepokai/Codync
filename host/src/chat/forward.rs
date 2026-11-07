//! Sending a message from one chat into another (the message menu's "Send to a channel").
//!
//! The message arrives as the user's own in the target chat, marked as forwarded, with the asking
//! bot's name on it. Files are copied, so the target keeps them even if the source is deleted.

use crate::chat::{files, uploads};
use crate::hub::Hub;
use crate::store::{Entry, EntryKind, Lane};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Copies `entry_id`'s text and files into the chat of `to` as a forwarded user message.
pub async fn forward(hub: &Arc<Hub>, entry_id: &str, to: &str) -> Result<Entry> {
    let source = hub.store.entry(entry_id).ok_or_else(|| anyhow::anyhow!("unknown message"))?;
    ensure!(!crate::store::is_internal(&source.data), "this message can't be sent on");
    ensure!(
        source.kind == EntryKind::User.as_str() || source.kind == EntryKind::Agent.as_str(),
        "only messages can be sent on"
    );
    let target = hub.store.bot(to)?.filter(|r| !r.deleted).ok_or_else(|| anyhow::anyhow!("unknown bot"))?.config;
    let from =
        hub.store.bot(&source.bot_id)?.filter(|r| !r.deleted).ok_or_else(|| anyhow::anyhow!("unknown bot"))?.config;
    let text = source.data["text"].as_str().unwrap_or_default().trim().to_owned();
    let from_root = uploads::root(&from);
    let to_root = uploads::root(&target);
    let files_root = files::root(&from.id);
    let target_files = files::root(&target.id);
    let attachments = source.data["attachments"].as_array().cloned().unwrap_or_default();
    let shared = source.data["files"].as_array().cloned().unwrap_or_default();
    if text.is_empty() && attachments.is_empty() && shared.is_empty() {
        bail!("this message has nothing to send on");
    }
    let (attachments, shared) = tokio::task::spawn_blocking(move || {
        let attachments =
            attachments.iter().map(|meta| copy_attachment(&from_root, &to_root, meta)).collect::<Result<Vec<_>>>()?;
        let shared =
            shared.iter().map(|meta| copy_file(&files_root, &target_files, meta)).collect::<Result<Vec<_>>>()?;
        Ok::<_, anyhow::Error>((attachments, shared))
    })
    .await??;
    let author = if source.kind == EntryKind::User.as_str() {
        "You".to_owned()
    } else {
        source.data["author"]
            .as_str()
            .and_then(|id| hub.store.bot(id).ok().flatten())
            .map_or_else(|| from.name.clone(), |row| row.config.name)
    };
    let mut data = json!({
        "text": text,
        "attachments": attachments,
        "files": shared,
        "forwardedFrom": {"botId": from.id, "entryId": source.id, "author": author},
    });
    if let Some(reply) = source.data["replyTo"].as_object() {
        data["forwardedFrom"]["replyTo"] = Value::Object(reply.clone());
    }
    crate::chat::messages::post_in_lane(hub, &target, Lane::main(&target.id), data)
}

/// A copied upload under the target chat's own id, so both chats keep their copy.
fn copy_attachment(from_root: &Path, to_root: &Path, meta: &Value) -> Result<Value> {
    let origin = uploads::path_of(from_root, meta).context("unknown attachment")?;
    let name = meta["name"].as_str().context("unknown attachment")?;
    let id = uuid::Uuid::new_v4().to_string();
    let dir = to_root.join(&id);
    std::fs::create_dir_all(&dir).context("copying the attachment")?;
    let copied = dir.join(name);
    copy_regular(&origin, &copied)?;
    Ok(json!({"id": id, "name": name, "size": std::fs::metadata(&copied)?.len()}))
}

/// A copied shared file under the target bot's library id.
fn copy_file(from_root: &Path, to_root: &Path, meta: &Value) -> Result<Value> {
    // A shared file's name may be hidden (`.env`), which the uploads' name check would refuse.
    meta["name"].as_str().context("unknown file")?;
    let origin = from_root.join(uuid::Uuid::parse_str(meta["id"].as_str().context("unknown file")?)?.to_string());
    let id = uuid::Uuid::new_v4().to_string();
    std::fs::create_dir_all(to_root).context("copying the file")?;
    let copied: PathBuf = to_root.join(&id);
    copy_regular(&origin, &copied)?;
    let mut meta = meta.clone();
    meta["id"] = id.into();
    meta["size"] = json!(std::fs::metadata(&copied)?.len());
    Ok(meta)
}

fn copy_regular(origin: &Path, copied: &Path) -> Result<()> {
    std::fs::copy(origin, copied).with_context(|| format!("copying {}", origin.display()))?;
    Ok(())
}

/// Marks the target chat as having a forwarded message from `name`, for its prompt.
pub fn note(name: &str) -> String {
    format!("[Forwarded from {name}]\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_forwarded_file_keeps_hidden_names() {
        let root = std::env::temp_dir().join(format!("codync-forward-{}", uuid::Uuid::new_v4()));
        let (from, to) = (root.join("from"), root.join("to"));
        std::fs::create_dir_all(&from).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        std::fs::write(from.join(&id), b"secret").unwrap();
        let meta = json!({"id": id, "name": ".env", "size": 6, "sha256": "x"});
        let copied = copy_file(&from, &to, &meta).unwrap();
        assert_ne!(copied["id"], meta["id"]);
        assert_eq!(std::fs::read_to_string(to.join(copied["id"].as_str().unwrap())).unwrap(), "secret");
        std::fs::remove_dir_all(root).ok();
    }
}
