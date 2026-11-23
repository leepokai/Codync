//! Threads without a bot ([threads without a bot](../../../../docs/features/work.md#threads-without-a-bot)): the
//! user runs an agent on its own, with the folder, approvals and model they choose. Their tasks wait in one box of
//! the computer's, a hidden bot that is never a bot to the user (`BotConfig::plain`), so they are steered, stopped
//! and listed as any thread is. What the box's threads run on is each one's `Setup`.

use crate::LockExt;
use crate::hub::Hub;
use crate::store::{MAX_THREADS, Store};
use anyhow::{Context, Result};
use serde_json::json;
use std::sync::{Arc, Mutex};

/// What an app from before this box shows where it lists hidden bots.
const NAME: &str = "Agents";

/// The box's id, if the computer has one.
fn find(store: &Store) -> Result<Option<String>> {
    Ok(store.bots()?.into_iter().find(|b| !b.deleted && b.config.plain).map(|b| b.config.id))
}

/// The computer's box for threads without a bot, made the first time one starts: hidden, in the user's home
/// folder, with no connectors, skills, shared memory or alerts of its own, and as many threads at once as a bot
/// may have. The harness's own memory stays the agent's.
pub async fn owner(hub: &Arc<Hub>) -> Result<String> {
    if let Some(id) = find(&hub.store)? {
        return Ok(id);
    }
    let home = dirs::home_dir().context("no home folder")?.to_string_lossy().into_owned();
    let defaults = json!({"name": NAME, "connectors": [], "skills": [], "cwd": home});
    let mut cfg = crate::agent::new_bot::with_defaults(hub, defaults).await?;
    cfg.plain = true;
    cfg.hidden = true;
    cfg.notify = Some(false);
    cfg.offered_to_network = false;
    cfg.reachable_by_assistants = false;
    cfg.self_fallbacks = false;
    cfg.shared_memory = Some(false);
    cfg.harness_memory = true;
    cfg.max_threads = MAX_THREADS;
    let hub = hub.clone();
    tokio::task::spawn_blocking(move || {
        // Two threads started at once make one box.
        static MAKING: Mutex<()> = Mutex::new(());
        let _g = MAKING.locked();
        if let Some(id) = find(&hub.store)? {
            return Ok(id);
        }
        let made = hub.create_bot(cfg)?;
        made["id"].as_str().map(str::to_owned).context("the box has no id")
    })
    .await?
}
