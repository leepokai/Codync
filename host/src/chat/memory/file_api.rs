//! The notes on the apps' memory screen: each layer's list, and one note to read with its recent versions.
//! `MEMORY.md` shows as the layer's facts instead.

use super::files;
use crate::hub::Hub;
use crate::store::BotConfig;
use anyhow::{Result, anyhow};
use codync_mem::files::{self as mf, MAIN};
use serde_json::{Value, json};

/// Versions the detail view lists.
const VERSIONS_SHOWN: usize = 20;

/// Both layers' notes for the memory screen. The caller has scanned the folders.
pub fn listing(hub: &Hub, cfg: &BotConfig) -> Result<Value> {
    let layer = |bot: Option<&str>| -> Result<Value> {
        let notes: Vec<mf::File> =
            hub.store.memory_read(|c| mf::layer(c, bot))?.into_iter().filter(|f| f.name != MAIN).collect();
        Ok(json!({"files": notes, "folder": files::folder(bot).display().to_string()}))
    };
    Ok(json!({
        "yours": layer(None)?,
        "bot": layer(Some(&cfg.id))?,
        "cap": mf::FILE_CAP,
        "folderCap": mf::FOLDER_CAP,
    }))
}

/// A note as it is on disk now, and its recent versions.
pub fn detail(hub: &Hub, cfg: &BotConfig, b: &Value) -> Result<Value> {
    let name = b["file"].as_str().ok_or_else(|| anyhow!("file required"))?;
    let layer = (b["shared"] != true).then_some(cfg.id.as_str());
    files::scan(hub, cfg);
    let content = files::read(layer, name)?;
    let versions = hub.store.memory_read(|c| mf::versions(c, layer, name, VERSIONS_SHOWN))?;
    Ok(json!({
        "file": name,
        "shared": layer.is_none(),
        "content": content,
        "hash": content.as_deref().map(mf::hash),
        "path": files::folder(layer).join(name).display().to_string(),
        "versions": versions,
    }))
}
