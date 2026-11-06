//! Retry cleanup after bot deletion, including when the cloud was unavailable at deletion time.
//! Any icon whose target isn't a live bot goes, including agent icons saved before agents lost them.
use super::{Icon, commit, list, live_bot, storage};
use crate::{hub::Hub, store::Store};
use anyhow::Result;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

pub(crate) async fn run(hub: Arc<Hub>) {
    let mut timer = tokio::time::interval(Duration::from_mins(1));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        timer.tick().await;
        if let Err(error) = sweep(&hub).await {
            tracing::warn!(error = format!("{error:#}"), "custom icon cleanup will retry");
        }
    }
}

pub(super) async fn sweep(hub: &Hub) -> Result<()> {
    let Ok(_guard) = hub.store.icon_lock.try_lock() else { return Ok(()) };
    let mut icons = list(&hub.store)?;
    for target in stale(&hub.store, &icons)?.into_iter().take(8) {
        let Some(icon) = icons.remove(&target) else { continue };
        if let Err(error) = storage::delete(hub, icon.slot).await {
            // A deleted bot's image stays until the cloud removes it. Agent icons leave the cache
            // even when the cloud refuses (unclaimed, signed out, cloud off): their slot is
            // overwritten when reused, and account deletion wipes every slot.
            if target.starts_with("bot:") {
                return Err(error);
            }
            tracing::warn!(icon = target, error = format!("{error:#}"), "agent icon left in cloud storage");
        }
        // Commit each deletion so a later network failure resumes at the next slot.
        icons = commit(hub, icons)?.icons;
    }
    Ok(())
}

/// Targets that aren't a live bot: deleted bots, and agents (which no longer take custom icons).
pub(super) fn stale(store: &Store, icons: &BTreeMap<String, Icon>) -> Result<Vec<String>> {
    let mut stale = Vec::new();
    for target in icons.keys() {
        if !live_bot(store, target)? {
            stale.push(target.clone());
        }
    }
    Ok(stale)
}
