//! Persistent, automatically allocated bot workspaces. A cwd is an execution
//! default, not an access boundary; explicit project folders remain opt-in.

use crate::service;
use crate::store::BotConfig;
use anyhow::{Context as _, Result};
use std::path::PathBuf;

pub fn path(id: &str) -> PathBuf {
    service::data_dir().join("bots").join(id).join("workspace")
}

/// The folder a workflow's chat works in by default, beside the bots' own: Codync keeps it.
pub fn workflow_path(id: &str) -> PathBuf {
    service::data_dir().join("workflows").join(id)
}

/// `cfg` is a bot working in its own folder.
pub fn is_managed(cfg: &BotConfig) -> bool {
    !cfg.is_thread() && own_folder(cfg).is_some()
}

/// The bot's own folder, when `cfg` works in it: the bot itself, or a thread of its that works there (not in a
/// worktree).
pub fn own_folder(cfg: &BotConfig) -> Option<PathBuf> {
    // A thread without a bot works in a folder of its own, unless the user picked one (`chat::work::plain`).
    if cfg.plain && cfg.is_thread() && crate::chat::work::plain::is_own_folder(&cfg.cwd) {
        return Some(PathBuf::from(&cfg.cwd));
    }
    let dir = path(cfg.owner());
    (!cfg.is_group() && std::path::Path::new(&cfg.cwd) == dir).then_some(dir)
}

/// The bot's default project (its first), if it has one.
pub fn default_project(cfg: &BotConfig) -> Option<&str> {
    cfg.projects.first().map(String::as_str)
}

/// Whether `folder` is the full path of a folder that exists: one a project or a task may work in.
pub fn is_existing_folder(folder: &str) -> bool {
    let path = std::path::Path::new(folder);
    path.is_absolute() && path.is_dir()
}

/// Where `cfg`'s code work happens: a thread's own folder, else the bot's default project, else its own folder.
pub fn work_folder(cfg: &BotConfig) -> &str {
    if cfg.is_thread() { &cfg.cwd } else { default_project(cfg).unwrap_or(&cfg.cwd) }
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
