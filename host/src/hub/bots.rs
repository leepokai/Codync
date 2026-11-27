//! Creating, changing and deleting bots.

use super::{Hub, patch, projects, sections, validate};
use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::store::BotConfig;
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::sync::Arc;

/// Clears a bot's "being deleted" mark on every exit, including an error or a panic mid-cleanup.
struct DeletingGuard<'a> {
    hub: &'a Hub,
    id: String,
}

impl Drop for DeletingGuard<'_> {
    fn drop(&mut self) {
        self.hub.deleting.locked().remove(&self.id);
    }
}

impl Hub {
    /// Makes `cfg` ready to save over `before` (none: a new bot). Called under `emit_lock`, with the save: its login
    /// profile can't be removed in between (`login_profiles::remove`).
    fn settle_config(&self, cfg: &mut BotConfig, before: Option<&BotConfig>) -> Result<()> {
        crate::agent::login_profiles::settle(&self.store, cfg)?;
        crate::agent::fallback::tidy(cfg);
        cfg.section = sections::normalize_section(cfg.section.as_deref())?;
        projects::settle(cfg, before)?;
        // ponytail: validated under the lock, so a workspace folder on a hung network share (its `is_dir`)
        // stalls every bot's events until it answers. Upgrade: build and validate outside the lock from a
        // snapshot, then save under it only if the stored config is unchanged (compare the serialized configs,
        // not rev), else retry.
        validate(cfg)?;
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
            self.settle_config(&mut cfg, None)?;
            self.store.save_bot(&cfg)?;
        }
        projects::changed(self, &[], &cfg.projects);
        crate::network::publish(self);
        if !cfg.is_group() {
            self.spawn_bot(cfg.clone());
            crate::market::agent_sign_in::ask_before_first_message(self, &cfg);
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
            // Only the host makes the box of threads without a bot (`chat::work::plain`).
            cfg.plain = row.config.plain;
            self.settle_config(&mut cfg, Some(&row.config))?;
            crate::agent::own_memory::settle(&mut cfg, &row.config, patch);
            self.store.save_bot(&cfg)?;
            (row.config, cfg)
        };
        projects::changed(self, &before.projects, &cfg.projects);
        crate::network::publish(self);
        if !cfg.is_group() {
            if crate::chat::memory::shared(&cfg) != crate::chat::memory::shared(&before) {
                crate::chat::memory::changed(self, Some(&id));
            }
            if cfg.fallbacks != before.fallbacks
                || cfg.thread_agent != before.thread_agent
                || cfg.thread_fallbacks != before.thread_fallbacks
            {
                crate::chat::work::fallbacks_changed(self, &cfg);
            }
            self.send_cmd(&id, Cmd::Reconfigure(Box::new(cfg)))?;
        }
        self.emit_bot(&id);
        crate::usage::bots_changed(self);
        Ok(self.bot_value(&id).unwrap_or(Value::Null))
    }

    pub fn delete_bot(&self, id: &str) -> Result<()> {
        self.delete_bot_if(id, |_| Ok(()))
    }

    /// Deletes `id`, running `check` on its live config inside the same `emit_lock` that `make_lead` holds while
    /// setting a bot's lead flag. The check runs before any cleanup and again immediately before the store delete,
    /// still under the lock: a lead pick can't land in between. A refused check changes nothing.
    pub fn delete_bot_if(&self, id: &str, check: impl Fn(&BotConfig) -> Result<()>) -> Result<()> {
        // List the threads before the first check: a check that clears a finished thread then becomes the
        // same situation as a concurrent cleanup between the listing and the recursion below.
        let threads = crate::chat::work::of(&self.store, id);
        {
            let _g = self.emit_lock.locked();
            let row = self.store.bot(id)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
            check(&row.config)?;
            // Mark under the same lock `make_lead` takes, so no lead pick can land in the
            // unlocked cleanup window below. The guard clears it on any exit.
            self.deleting.locked().insert(id.to_owned());
        }
        let _marked = DeletingGuard { hub: self, id: id.to_owned() };
        // Its threads go first: they are the bot's own work. Its box goes below.
        for thread in threads {
            self.delete_owned_thread(&thread.id)?;
        }
        self.routines.disable_bot(&self.store, id)?;
        crate::workflows::clear_bot(self, id);
        self.team.end_turn(id);
        self.ceilings.set(id, codync_network::Access::Allow);
        crate::chat::group::stop(self, id);
        let _ = self.send_cmd(id, Cmd::Shutdown);
        self.bots.locked().remove(id);
        self.runtime.locked().remove(id);
        self.dev_servers.forget_bot(&self.store, id);
        {
            let _g = self.emit_lock.locked();
            // Last guard under the same lock `make_lead` uses: re-read the live config and refuse a
            // bot that became lead after the first check, so no pick can land in between.
            let row = self.store.bot(id)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
            check(&row.config)?;
            let rev = self.store.delete_bot(id)?;
            // No client ever listed a thread bot, so none hears it go.
            if !crate::store::is_thread_id(id) {
                let deleted = json!({"type": "bot", "rev": rev, "bot": {"id": id, "deleted": true, "rev": rev}});
                let _ = self.events.send(deleted);
            }
        }
        crate::chat::work::forget(self, id);
        crate::network::publish(self);
        // Its shared files, artifacts and the files sent to it (a thread's, a group's) are unreachable once it is deleted.
        let folders =
            [crate::chat::files::root(id), crate::chat::artifacts::root(id), crate::chat::uploads::data_root(id)];
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
        crate::usage::bots_changed(self);
        self.leave_groups(id)
    }

    /// Clears a child thread while its owner is being deleted. A finished thread can be cleaned up
    /// by `dispatch` between the owner listing its threads and here: it is already gone, so skip it
    /// instead of letting `unknown bot` abort the rest of the owner's cleanup.
    fn delete_owned_thread(&self, id: &str) -> Result<()> {
        if self.store.bot(id)?.is_none_or(|row| row.deleted) {
            return Ok(());
        }
        match self.delete_bot(id) {
            Err(error) if error.to_string() == "unknown bot" => Ok(()),
            other => other,
        }
    }
}
