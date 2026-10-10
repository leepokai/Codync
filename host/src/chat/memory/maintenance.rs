//! Serialize forgetting against keeper writes, and invalidate every session's memory snapshot.

use crate::{LockExt, store::Store};
use anyhow::Result;
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
};

static LOCKS: LazyLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn lock(bot: &str) -> Arc<tokio::sync::Mutex<()>> {
    LOCKS.locked().entry(bot.to_owned()).or_default().clone()
}

pub fn keeper_lock(bot: &str) -> Arc<tokio::sync::Mutex<()>> {
    static KEEPERS: LazyLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    KEEPERS.locked().entry(bot.to_owned()).or_default().clone()
}

pub fn revision(store: &Store, bot: &str) -> String {
    store.kv_get(&format!("memory.revision.{bot}")).unwrap_or_default()
}

/// A correction (edit, forget, clear): it supersedes what the keeper is extracting from older
/// context, sessions are told memory changed, and their instructions re-render.
pub fn changed(store: &Store, bot: &str) -> Result<()> {
    store.kv_set(&format!("memory.revision.{bot}"), &uuid::Uuid::new_v4().to_string())?;
    crate::chat::context::invalidate(store, bot)
}

pub fn discard_pending(store: &Store, bot: &str) -> Result<()> {
    store.kv_set(&format!("memory.unprocessed.{bot}"), "[]")?;
    super::set_pending_episode(store, bot, &[])
}
