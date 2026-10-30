//! Custom agent/bot icons: host validation, local cache and private encrypted R2 storage.
mod cleanup;
mod image;
pub(crate) use cleanup::run as cleanup_loop;
mod source;
mod storage;
#[cfg(test)]
mod tests;

use crate::{hub::Hub, store::Store};
use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;

const KEY: &str = "custom_icons_v1";
const SLOTS: u8 = 128;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Icon {
    slot: u8,
    /// A normalized, metadata-free PNG. Sent only over existing authenticated channels.
    pub data_url: String,
    pub tinted: bool,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Snapshot {
    pub icons: BTreeMap<String, Icon>,
    pub icons_revision: i64,
}

pub(crate) fn snapshot(store: &Store) -> Result<Snapshot> {
    store.kv_read(KEY)?.map_or_else(|| Ok(Snapshot::default()), |v| Ok(serde_json::from_str(&v)?))
}

pub(crate) fn list(store: &Store) -> Result<BTreeMap<String, Icon>> {
    Ok(snapshot(store)?.icons)
}

fn commit(hub: &Hub, icons: BTreeMap<String, Icon>) -> Result<Snapshot> {
    let mut snapshot = Snapshot { icons, icons_revision: 0 };
    hub.store.kv_set_stamped(KEY, |revision| {
        snapshot.icons_revision = revision;
        Ok(serde_json::to_string(&snapshot)?)
    })?;
    let _ =
        hub.events.send(json!({"type": "icons", "icons": snapshot.icons, "iconsRevision": snapshot.icons_revision}));
    Ok(snapshot)
}

pub(crate) async fn set(hub: &Arc<Hub>, b: &Value) -> Result<Value> {
    let target = b["target"].as_str().context("Icon target is required")?;
    validate_target(hub, target)?;
    // Bound concurrent decoding and serialize R2 replacements with local commits.
    let _guard = hub.store.icon_lock.try_lock().context("Another icon is being saved; try again shortly")?;
    let mut icons = list(&hub.store)?;
    if b["remove"] == true {
        if let Some(old) = icons.get(target) {
            storage::delete(hub, old.slot).await?;
            icons.remove(target);
        }
    } else if b["data"].is_null() && b["url"].is_null() {
        let icon = icons.get_mut(target).context("Choose an image first")?;
        icon.tinted = b["tinted"].as_bool().context("Icon color mode is required")?;
    } else {
        ensure!(b["data"].is_null() || b["url"].is_null(), "Choose a file or an image URL, not both");
        let bytes = if let Some(url) = b["url"].as_str() {
            source::download(url).await?
        } else {
            let data = b["data"].as_str().context("Image data is required")?;
            ensure!(data.len() <= image::MAX_INPUT.div_ceil(3) * 4, "Choose an image up to 5 MiB");
            STANDARD.decode(data).context("Invalid image encoding")?
        };
        let png = tokio::task::spawn_blocking(move || image::normalize(&bytes)).await??;
        // Two slots per update are unnecessary: a failed local commit leaves the old local
        // cache intact, and retrying the same target replaces the same bounded remote slot.
        let slot = icons
            .get(target)
            .map(|i| i.slot)
            .or_else(|| (0..SLOTS).find(|slot| !icons.values().any(|i| i.slot == *slot)))
            .context("This computer has 128 custom icons; remove one before adding another")?;
        storage::put(hub, slot, &png).await?;
        icons.insert(
            target.to_owned(),
            Icon {
                slot,
                data_url: format!("data:image/png;base64,{}", STANDARD.encode(png)),
                tinted: b["tinted"].as_bool().unwrap_or(false),
            },
        );
    }
    Ok(serde_json::to_value(commit(hub, icons)?)?)
}

fn validate_target(hub: &Hub, target: &str) -> Result<()> {
    ensure!(target.len() <= 256, "Invalid icon target");
    if let Some(id) = target.strip_prefix("bot:") {
        ensure!(hub.store.bot(id)?.is_some_and(|b| !b.deleted), "Unknown bot");
    } else if let Some(id) = target.strip_prefix("agent:") {
        ensure!(crate::agent::backends::list().iter().any(|b| b["id"] == id), "Unknown agent");
    } else {
        anyhow::bail!("Choose an agent or bot icon");
    }
    Ok(())
}
