//! Retry cleanup after bot deletion, including when the cloud was unavailable at deletion time.
use super::{commit, list, storage};
use crate::hub::Hub;
use anyhow::Result;
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

async fn sweep(hub: &Hub) -> Result<()> {
    let Ok(_guard) = hub.store.icon_lock.try_lock() else { return Ok(()) };
    let mut icons = list(&hub.store)?;
    let mut removed = 0;
    for (target, icon) in icons.clone() {
        let Some(id) = target.strip_prefix("bot:") else { continue };
        if hub.store.bot(id)?.is_some_and(|b| !b.deleted) {
            continue;
        }
        storage::delete(hub, icon.slot).await?;
        icons.remove(&target);
        // Commit each deletion so a later network failure resumes at the next slot.
        icons = commit(hub, icons)?.icons;
        removed += 1;
        if removed == 8 {
            break;
        }
    }
    Ok(())
}
