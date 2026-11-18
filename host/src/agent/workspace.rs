//! Persistent, automatically allocated bot workspaces. A cwd is an execution
//! default, not an access boundary; explicit project folders remain opt-in.

use crate::service;
use crate::store::BotConfig;
use anyhow::{Context as _, Result};
use std::path::PathBuf;

pub fn path(id: &str) -> PathBuf {
    service::data_dir().join("bots").join(id).join("workspace")
}

/// `cfg` is a bot working in its own folder.
pub fn is_managed(cfg: &BotConfig) -> bool {
    !cfg.is_task() && own_folder(cfg).is_some()
}

/// The bot's own folder, when `cfg` works in it: the bot itself, or a task of its that works there (not in a
/// worktree).
pub fn own_folder(cfg: &BotConfig) -> Option<PathBuf> {
    let dir = path(cfg.owner());
    (!cfg.is_group() && std::path::Path::new(&cfg.cwd) == dir).then_some(dir)
}

/// Called on a blocking thread before persisting a bot configuration.
/// Empty cwd opts into automatic allocation, including when editing a bot.
pub fn prepare(cfg: &mut BotConfig) -> Result<()> {
    if cfg.is_group() || !cfg.cwd.is_empty() {
        return Ok(());
    }
    // Only host-issued IDs may become directory components.
    uuid::Uuid::parse_str(&cfg.id).context("invalid bot workspace id")?;
    let dir = path(&cfg.id);
    std::fs::create_dir_all(&dir).context("creating bot workspace")?;
    cfg.cwd = dir.to_string_lossy().into_owned();
    Ok(())
}
