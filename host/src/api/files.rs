use super::str_arg;
use crate::chat::files::{self, SharedFile};
use crate::hub::Hub;
use anyhow::{Result, anyhow, ensure};
use base64::Engine as _;
use serde_json::{Value, json};
use std::sync::Arc;

pub async fn read(hub: &Arc<Hub>, body: &Value) -> Result<Value> {
    let entry = hub.store.entry(str_arg(body, "entryId")?).ok_or_else(|| anyhow!("unknown file message"))?;
    ensure!(entry.kind == crate::store::EntryKind::Agent.as_str() && entry.data["final"] == true, "not a file message");
    let bot = entry.data["author"].as_str().ok_or_else(|| anyhow!("missing file author"))?;
    ensure!(entry.bot_id == bot, "not a bot chat file");
    ensure!(hub.store.bot(bot)?.is_some_and(|row| !row.deleted), "unknown bot");
    let id = str_arg(body, "fileId")?;
    let meta = entry.data["files"]
        .as_array()
        .and_then(|files| files.iter().find(|meta| meta["id"] == id))
        .ok_or_else(|| anyhow!("file does not belong to this message"))?;
    let meta: SharedFile = serde_json::from_value(meta.clone())?;
    let offset = body["offset"].as_u64().ok_or_else(|| anyhow!("offset must be a nonnegative integer"))?;
    let root = files::root(bot);
    let size = meta.size;
    let data = tokio::task::spawn_blocking(move || files::read(&root, &meta, offset)).await??;
    Ok(json!({"data": base64::engine::general_purpose::STANDARD.encode(data), "size": size}))
}
