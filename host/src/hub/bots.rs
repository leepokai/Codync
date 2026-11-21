//! Creating, changing and deleting bots.

use super::{Hub, patch, sections, validate_with};
use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::store::BotConfig;
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::sync::Arc;

impl Hub {
    /// Makes `cfg` ready to save. Called under `emit_lock`, with the save: its login profile can't be removed in
    /// between (`login_profiles::remove`).
    fn settle_config(&self, cfg: &mut BotConfig, workspace_checked: bool) -> Result<()> {
        crate::agent::login_profiles::settle(&self.store, cfg)?;
        crate::agent::fallback::tidy(cfg);
        cfg.section = sections::normalize_section(cfg.section.as_deref())?;
        // ponytail: validated under the lock, so a workspace folder on a hung network share (a bot's `cwd` or a
        // group's shared folder, both an `is_dir` here) stalls every bot's events until it answers. Upgrade: build
        // and validate outside the lock from a snapshot, then save under it only if the stored config is unchanged
        // (compare the serialized configs, not rev), else retry.
        validate_with(cfg, workspace_checked)?;
        crate::agent::workspace::prepare(cfg)
    }

    pub fn create_bot(self: &Arc<Self>, mut cfg: BotConfig) -> Result<Value> {
        cfg.id = uuid::Uuid::new_v4().to_string();
        cfg.created_at = crate::store::now_ms();
        cfg.position = self.new_bot_position()?;
        // No name yet (Grok Bot's flow): it gets one after its first few conversations.
        cfg.auto_name = !cfg.is_group() && cfg.name.trim().is_empty();
        if cfg.auto_name {
            crate::chat::naming::PLACEHOLDER.clone_into(&mut cfg.name);
        }
        if cfg.is_group() {
            cfg.members = crate::chat::group::valid_members(&self.store, &cfg.id, &cfg.members)?;
            // The same bots again: open the group they already share (Grok Bot's rule).
            if let Some(existing) = crate::chat::group::with_members(&self.store, &cfg.members)? {
                return Ok(self.bot_value(&existing).unwrap_or(Value::Null));
            }
        }
        {
            let _g = self.emit_lock.locked();
            self.settle_config(&mut cfg, true)?;
            self.store.save_bot(&cfg)?;
        }
        crate::network::publish(self);
        if !cfg.is_group() {
            self.spawn_bot(cfg.clone());
            crate::market::agent_sign_in::ask_before_first_message(self, &cfg);
        } else if cfg.workspace.is_some() {
            // A session carries `additionalDirectories` when it opens: a new team folder reaches members that
            // already had a session by starting their agent again before its next turn.
            self.refresh_members(&cfg.id, cfg.members.iter().map(String::as_str));
        }
        self.emit_bot(&cfg.id);
        crate::usage::bots_changed(self);
        Ok(self.bot_value(&cfg.id).unwrap_or(Value::Null))
    }

    pub fn update_bot(&self, patch: &Value) -> Result<Value> {
        let id = patch["id"].as_str().ok_or_else(|| anyhow!("id required"))?.to_owned();
        // Read, merge and save under one lock: a section rename in between would otherwise be written back.
        let (before, cfg) = {
            let _g = self.emit_lock.locked();
            let row = self.store.bot(&id)?.filter(|b| !b.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
            let mut cfg = patch::patched(&row.config, patch)?;
            if cfg.is_group() {
                cfg.members = crate::chat::group::valid_members(&self.store, &cfg.id, &cfg.members)?;
            }
            // A folder gone since it was saved doesn't block a rename or a member change.
            let workspace_checked = cfg.workspace != row.config.workspace;
            self.settle_config(&mut cfg, workspace_checked)?;
            crate::agent::own_memory::settle(&mut cfg, &row.config, patch);
            self.store.save_bot(&cfg)?;
            (row.config, cfg)
        };
        crate::network::publish(self);
        if !cfg.is_group() {
            if crate::chat::memory::shared(&cfg) != crate::chat::memory::shared(&before) {
                crate::chat::memory::changed(self, Some(&id));
            }
            if cfg.fallbacks != before.fallbacks {
                crate::chat::work::fallbacks_changed(self, &cfg);
            }
            self.send_cmd(&id, Cmd::Reconfigure(Box::new(cfg)))?;
        } else if cfg.workspace != before.workspace || cfg.members != before.members {
            // A session carries `additionalDirectories` when it opens: a new team folder (or a new roster, which
            // decides whose folders it gets) reaches members by starting their agent again before its next turn.
            let members: std::collections::BTreeSet<&str> =
                before.members.iter().chain(cfg.members.iter()).map(String::as_str).collect();
            self.refresh_members(&cfg.id, members);
        }
        self.emit_bot(&id);
        crate::usage::bots_changed(self);
        Ok(self.bot_value(&id).unwrap_or(Value::Null))
    }

    pub fn delete_bot(&self, id: &str) -> Result<()> {
        // A group's members lose its shared folder when it goes: their next session must not keep it.
        let shared = self
            .store
            .bot(id)?
            .map(|row| row.config)
            .filter(|cfg| cfg.is_group() && cfg.workspace.is_some())
            .map(|cfg| cfg.members);
        // Its threads go first: they are the bot's own work. Its box goes below.
        for thread in crate::chat::work::of(&self.store, id) {
            self.delete_bot(&thread.id)?;
        }
        self.routines.disable_bot(&self.store, id)?;
        self.team.end_turn(id);
        self.ceilings.set(id, codync_network::Access::Allow);
        crate::chat::group::stop(self, id);
        let _ = self.send_cmd(id, Cmd::Shutdown);
        self.bots.locked().remove(id);
        self.runtime.locked().remove(id);
        self.dev_servers.forget_bot(&self.store, id);
        {
            let _g = self.emit_lock.locked();
            let rev = self.store.delete_bot(id)?;
            // No client ever listed a thread bot, so none hears it go.
            if !crate::store::is_thread_id(id) {
                let deleted = json!({"type": "bot", "rev": rev, "bot": {"id": id, "deleted": true, "rev": rev}});
                let _ = self.events.send(deleted);
            }
        }
        crate::chat::work::forget(self, id);
        crate::network::publish(self);
        // Its shared files and artifacts are unreachable once the bot is deleted.
        let folders = [crate::chat::files::root(id), crate::chat::artifacts::root(id)];
        let bot = id.to_owned();
        tokio::task::spawn_blocking(move || {
            crate::chat::memory::files::remove_bot(&bot);
            for folder in folders {
                if let Err(error) = std::fs::remove_dir_all(&folder)
                    && error.kind() != std::io::ErrorKind::NotFound
                {
                    tracing::warn!(error = %error, "couldn't remove a deleted bot's shared files or artifacts");
                }
            }
        });
        if let Some(members) = shared {
            self.refresh_members(id, members.iter().map(String::as_str));
        }
        crate::usage::bots_changed(self);
        self.leave_groups(id)
    }

    /// Starts each member's agent again before its next turn, so a changed set of shared folders (a group's
    /// workspace or roster, a group that went) reaches the sessions that already opened. A member deleted meanwhile
    /// has no actor left: there is nothing to refresh, and the store already holds what its next start reads.
    fn refresh_members<'a>(&self, group: &str, members: impl IntoIterator<Item = &'a str>) {
        for member in members {
            if let Err(error) = self.send_cmd(member, Cmd::RefreshTools) {
                tracing::warn!(group, member, error = format!("{error:#}"), "couldn't refresh a group member");
            }
        }
    }
}
