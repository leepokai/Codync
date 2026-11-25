//! At host start, once (kv `projects.migrated`): a bot that worked in a project moves to its own folder, the project
//! becoming its first ([bots, projects and threads](../../../../docs/plans/bots-projects-threads.md#migration)). Before
//! anything changes, every bot config and routine it rewrites is written to `<data dir>/backups/bot-projects-<ms>.json`.
//! Per bot, the config is saved last: a host that stops in between does the rest at its next start.

use crate::LockExt;
use crate::analytics::{self, Event};
use crate::hub::Hub;
use crate::store::{BotConfig, Store, now_ms};
use anyhow::{Context as _, Result};
use serde_json::json;
use std::path::{Path, PathBuf};

/// Set once every bot has moved.
const DONE: &str = "projects.migrated";

/// Moves every bot that works in a project to its own folder, once. A failure is logged and tried again at the next
/// start; the host starts either way.
pub(crate) fn migrate(hub: &Hub) {
    if hub.store.kv_get(DONE).is_some() {
        return;
    }
    match migrate_all(hub) {
        Ok(moved) => {
            if moved > 0 {
                let properties = json!({"count": moved}).as_object().cloned().unwrap_or_default();
                analytics::capture(hub, Event::BotProjectsMigrated, properties);
            }
            if let Err(error) = hub.store.kv_set(DONE, "1") {
                tracing::warn!(error = format!("{error:#}"), "couldn't mark the projects migration done");
            }
        }
        Err(error) => tracing::error!(error = format!("{error:#}"), "couldn't move bots to their own folders"),
    }
}

/// The bots to move, the backup, then each bot. How many moved.
fn migrate_all(hub: &Hub) -> Result<usize> {
    let bots: Vec<(BotConfig, Option<String>)> = hub
        .store
        .bots()?
        .into_iter()
        .filter(|row| !row.deleted && moves(&row.config))
        .map(|row| (row.config, row.session_id))
        .collect();
    if bots.is_empty() {
        return Ok(0);
    }
    let configs: Vec<&BotConfig> = bots.iter().map(|(cfg, _)| cfg).collect();
    let backup = back_up(&hub.store, &configs)?;
    let count = bots.len();
    tracing::info!(bots = count, backup = %backup.display(), "moving bots that work in a project to their own folders");
    for (cfg, session) in bots {
        let id = cfg.id.clone();
        move_bot(hub, cfg, session).with_context(|| format!("moving bot {id}"))?;
    }
    Ok(count)
}

/// Whether `cfg` is a bot that works in a folder other than its own (not a group, a thread or the box of threads
/// without a bot). Only ids Codync issued become folder names.
fn moves(cfg: &BotConfig) -> bool {
    let mut moved = cfg.clone();
    super::adopt_cwd(&mut moved);
    moved.cwd != cfg.cwd && uuid::Uuid::parse_str(&cfg.id).is_ok()
}

/// Writes every config in `bots` and their routines, as they are, to a new file in the data folder's `backups`.
fn back_up(store: &Store, bots: &[&BotConfig]) -> Result<PathBuf> {
    let ids: Vec<&str> = bots.iter().map(|b| b.id.as_str()).collect();
    let routines = crate::routines::of_bots(store, &ids)?;
    let dir = crate::service::data_dir().join("backups");
    std::fs::create_dir_all(&dir).context("creating the backups folder")?;
    let file = dir.join(format!("bot-projects-{}.json", now_ms()));
    let backup = json!({"bots": bots, "routines": routines});
    std::fs::write(&file, serde_json::to_vec_pretty(&backup)?).context("writing the backup")?;
    Ok(file)
}

/// One bot: its uploads move home, its routines keep the old folder, it owes its chat a recap of the session it
/// leaves, and it saves with the old folder as its first project. `session`: its main session now.
fn move_bot(hub: &Hub, mut cfg: BotConfig, session: Option<String>) -> Result<()> {
    let old = cfg.cwd.clone();
    let home = crate::agent::workspace::path(&cfg.id);
    std::fs::create_dir_all(&home).context("creating the bot's own folder")?;
    move_uploads(&crate::chat::uploads::data_root(&cfg.id), &home.join("uploads"))?;
    // A bot whose routines run in its chat (computer use, Manage the team) keeps them there, with those tools; a
    // folder would move each run into a thread without them. They run at home from here.
    if crate::chat::work::boxed(&cfg) {
        crate::routines::stamp_folder(&hub.store, &cfg.id, &old)?;
    }
    // Unset, "yours" follows its own folder (`chat::memory::shared`): a project bot didn't use it.
    cfg.shared_memory.get_or_insert(false);
    crate::agent::bot::owe_move(&hub.store, &cfg, session)?;
    // As a move by the user does (`restart_agent`): the next turn starts new sessions in the new folder.
    hub.store.set_session(&cfg.id, None)?;
    for key in hub.store.kv_prefix(&crate::store::run_session_key(&cfg.id, "")) {
        hub.store.kv_delete(&key)?;
    }
    super::adopt_cwd(&mut cfg);
    super::dedupe(&mut cfg.projects);
    cfg.cwd = home.to_string_lossy().into_owned();
    let _g = hub.emit_lock.locked();
    hub.store.save_bot(&cfg)?;
    Ok(())
}

/// The files sent to a bot in a project (`from`, `uploads::data_root`) go to its own folder's `uploads` (`to`), one
/// upload at a time: one already there stays where it is.
fn move_uploads(from: &Path, to: &Path) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(from) else { return Ok(()) };
    std::fs::create_dir_all(to).context("creating the bot's uploads folder")?;
    for entry in entries {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if target.exists() {
            tracing::warn!(upload = %entry.path().display(), "an upload is in the bot's own folder already; left where it was");
            continue;
        }
        std::fs::rename(entry.path(), &target).context("moving an upload")?;
    }
    if let Err(error) = std::fs::remove_dir(from) {
        tracing::info!(error = %error, "left the bot's old uploads folder");
    }
    Ok(())
}
