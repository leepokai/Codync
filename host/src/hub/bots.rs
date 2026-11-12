//! Creating, changing and deleting bots.

use super::{Hub, copy_name, sections, skill_updates, validate};
use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::store::BotConfig;
use anyhow::{Context as _, Result, anyhow, bail};
use serde_json::{Value, json};
use std::sync::Arc;

impl Hub {
    /// Copies a bot's setup into a new one: same agent, folder, permissions, skills and persona, but no
    /// conversation, no pinned/lead place and no owner. Groups are not copied.
    pub fn duplicate_bot(self: &Arc<Self>, id: &str) -> Result<Value> {
        let row = self.store.bot(id)?.filter(|b| !b.deleted).context("unknown bot")?;
        if row.config.is_group() {
            bail!("a group is made from its bots, not copied");
        }
        let mut cfg = row.config;
        let taken: Vec<String> = self.store.bots()?.into_iter().map(|b| b.config.name).collect();
        cfg.name = copy_name(&taken, &cfg.name);
        cfg.pinned = false;
        cfg.lead = false;
        cfg.hidden = false;
        cfg.created_by = None;
        cfg.auto_name = false;
        self.create_bot(cfg)
    }

    pub fn create_bot(self: &Arc<Self>, mut cfg: BotConfig) -> Result<Value> {
        if crate::external::is_external(&cfg) {
            bail!("a terminal session is created by its terminal, not by an app");
        }
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
        crate::agent::fallback::tidy(&mut cfg);
        cfg.section = sections::normalize_section(cfg.section.as_deref())?;
        validate(&cfg)?;
        crate::agent::workspace::prepare(&mut cfg)?;
        {
            let _g = self.emit_lock.locked();
            self.store.save_bot(&cfg)?;
        }
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
        // From reading the bot to saving it, so no other update saves a stale copy in between.
        let _one = crate::chat::instruction_history::lock();
        let row = self.store.bot(&id)?.filter(|b| !b.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
        skill_updates::check(&row.config, patch)?;
        let before = row.config.clone();
        let mut merged = serde_json::to_value(&row.config)?;
        for (k, v) in patch.as_object().into_iter().flatten() {
            if !matches!(k.as_str(), "id" | "createdAt" | "createdBy" | "lead" | "ifSkills") {
                merged[k] = v.clone();
            }
        }
        let mut cfg: BotConfig = serde_json::from_value(merged)?;
        if cfg.name.trim().is_empty() {
            cfg.name.clone_from(&row.config.name);
        }
        // A name the user picked ends automatic naming; the namer passes `autoName` itself.
        if cfg.name != row.config.name && patch.get("autoName").is_none() {
            cfg.auto_name = false;
        }
        // Options belong to one model; an app that doesn't know them can't clear them itself.
        if (cfg.model != row.config.model || cfg.backend != row.config.backend) && patch.get("modelOptions").is_none() {
            cfg.model_options.clear();
        }
        if cfg.kind != row.config.kind {
            bail!("a bot can't become a group, or a group a bot");
        }
        if crate::external::is_external(&row.config) && cfg.backend != row.config.backend {
            bail!("this bot runs in your terminal: its agent can't change");
        }
        if cfg.is_group() {
            cfg.members = crate::chat::group::valid_members(&self.store, &cfg.id, &cfg.members)?;
        }
        crate::agent::fallback::tidy(&mut cfg);
        validate(&cfg)?;
        let mut edit = crate::chat::instruction_history::begin(&row.config, &cfg, patch, &self.store)?;
        crate::agent::workspace::prepare(&mut cfg)?;
        crate::agent::own_memory::settle(&mut cfg, &row.config, patch);
        if let Some(edit) = &mut edit {
            edit.record(&self.store, &row.config, &cfg)?;
        }
        let saved = {
            let _connectors = self.store.connector_lock.locked();
            let _g = self.emit_lock.locked();
            self.save_bot_access(&id, &row.config, &cfg)
        };
        if let Err(error) = saved {
            if let Some(edit) = edit {
                edit.undo(&self.store)?;
            }
            return Err(error);
        }
        crate::network::publish(self);
        if !cfg.is_group() {
            if crate::chat::memory::shared(&cfg) != crate::chat::memory::shared(&before) {
                crate::chat::memory::changed(self, Some(&id));
            }
            // A terminal session has no actor to reconfigure: its settings are read on the next event.
            if !crate::external::is_external(&cfg) {
                self.send_cmd(&id, Cmd::Reconfigure(Box::new(cfg)))?;
            }
        }
        self.emit_bot(&id);
        crate::usage::bots_changed(self);
        Ok(self.bot_value(&id).unwrap_or(Value::Null))
    }

    /// Saves `cfg`, first revoking the connectors and computer access it dropped. Called under the
    /// connector and emit locks.
    fn save_bot_access(&self, id: &str, before: &BotConfig, cfg: &BotConfig) -> Result<()> {
        if before.computer && !cfg.computer {
            self.screen.forget_bot_snapshots(id);
        }
        for connector in &before.connectors {
            if !cfg.connectors.contains(connector) {
                self.store.revoke_mcp_connector(Some(id), connector)?;
            }
        }
        self.store.save_bot(cfg)?;
        self.computer.changed(id, before, cfg);
        if before.computer_permissions.browser && !cfg.computer_permissions.browser {
            self.screen.forget_bot_snapshots(id);
        }
        Ok(())
    }

    pub fn delete_bot(&self, id: &str) -> Result<()> {
        crate::chat::lead::may_delete(&self.store, id)?;
        // A terminal session leaves with its bot, and a hook waiting on one of its cards is freed at once.
        self.external.forget(self, id);
        self.computer.revoke_all(id);
        self.screen.forget_bot_snapshots(id);
        crate::routines::flow::forget_bot(self, id);
        self.routines.disable_bot(&self.store, id)?;
        // A locked vault keeps them; Credentials lists them under "A removed bot" to remove.
        if let Err(error) = crate::market::bot_secrets::remove_bot(&self.store, id) {
            tracing::warn!(bot = id, error = format!("{error:#}"), "couldn't forget a deleted bot's secrets");
        }
        self.team.cancel_bot(id);
        crate::chat::group::stop(self, id);
        let _ = self.send_cmd(id, Cmd::Shutdown);
        self.bots.locked().remove(id);
        self.runtime.locked().remove(id);
        self.dev_servers.forget_bot(&self.store, id);
        {
            let _g = self.emit_lock.locked();
            let rev = self.store.delete_bot(id)?;
            self.computer.revoke_all(id);
            let _ =
                self.events.send(json!({"type": "bot", "rev": rev, "bot": {"id": id, "deleted": true, "rev": rev}}));
        }
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
        crate::usage::bots_changed(self);
        self.leave_groups(id)
    }
}
