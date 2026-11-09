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

/// The files in one folder of a bot's workspace, for its Files page. Paths stay inside that folder: a
/// relative path that leaves it (or a symlink pointing out) is refused.
pub async fn list(hub: &Arc<Hub>, body: &Value) -> Result<Value> {
    let bot = str_arg(body, "botId")?;
    let row = hub
        .store
        .bot(bot)?
        .filter(|row| !row.deleted && !row.config.is_group())
        .ok_or_else(|| anyhow!("unknown bot"))?;
    ensure!(!row.config.cwd.trim().is_empty(), "This bot has no folder yet");
    let root = row.config.cwd.clone();
    let relative = body["path"].as_str().unwrap_or_default().trim_matches('/').to_owned();
    ensure!(relative.len() <= 1024, "The path is too long");
    let dir = tokio::task::spawn_blocking(move || resolve(&root, &relative)).await??;
    let listing = tokio::task::spawn_blocking(move || entries(&dir)).await??;
    Ok(listing)
}

/// The folder `relative` names under `root`, refusing anything that resolves outside it.
fn resolve(root: &str, relative: &str) -> Result<std::path::PathBuf> {
    let base = std::fs::canonicalize(root).map_err(|_| anyhow!("This bot's folder is missing"))?;
    let candidate = base.join(relative);
    let path = std::fs::canonicalize(&candidate).map_err(|_| anyhow!("That folder no longer exists"))?;
    ensure!(path.starts_with(&base), "That path is outside this bot's folder");
    ensure!(path.is_dir(), "That path is not a folder");
    Ok(path)
}

/// One page of a folder: folders first, then files, each by name.
fn entries(dir: &std::path::Path) -> Result<Value> {
    const LIMIT: usize = 500;
    let mut rows: Vec<Value> = Vec::new();
    for entry in std::fs::read_dir(dir)?.flatten().take(LIMIT) {
        let Ok(meta) = entry.metadata() else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        rows.push(json!({
            "name": name,
            "dir": meta.is_dir(),
            "size": meta.len(),
            "modified": meta.modified().ok().and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok()).map(|span| i64::try_from(span.as_millis()).unwrap_or(i64::MAX)),
        }));
    }
    rows.sort_by(|a, b| {
        b["dir"].as_bool().cmp(&a["dir"].as_bool()).then_with(|| {
            a["name"]
                .as_str()
                .unwrap_or_default()
                .to_lowercase()
                .cmp(&b["name"].as_str().unwrap_or_default().to_lowercase())
        })
    });
    Ok(json!({"entries": rows}))
}

#[cfg(test)]
mod path_tests {
    use super::resolve;

    fn folder() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("codync-files-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/note.txt"), "hola").unwrap();
        dir
    }

    #[test]
    fn only_folders_inside_the_bot_folder_are_listed() {
        let root = folder();
        let root_text = root.to_string_lossy();
        assert!(resolve(&root_text, "").unwrap().is_dir());
        assert!(resolve(&root_text, "sub").unwrap().ends_with("sub"));
        assert!(resolve(&root_text, "../").is_err(), "a parent folder is outside");
        assert!(resolve(&root_text, "sub/note.txt").is_err(), "a file is not a folder");
        std::fs::remove_dir_all(root).ok();
    }
}
