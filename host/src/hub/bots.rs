//! Creating, changing and deleting bots.

use super::{Hub, patch, projects, sections, validate};
use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::store::BotConfig;
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::sync::Arc;

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

    /// Adds `ids` to `id`'s enabled skills without re-validating its workspace folder, so a thread
    /// saving a skill from its own folder still turns it on for its bot if the bot's folder is away.
    /// Running bots read their enabled skills from the store.
    pub(crate) fn enable_skills(&self, id: &str, ids: &[String]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        {
            let _g = self.emit_lock.locked();
            let row = self.store.bot(id)?.filter(|b| !b.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
            let mut cfg = row.config;
            if cfg.is_group() || ids.iter().all(|s| cfg.skills.contains(s)) {
                return Ok(());
            }
            for s in ids {
                if !cfg.skills.contains(s) {
                    cfg.skills.push(s.clone());
                }
            }
            self.store.save_bot(&cfg)?;
        }
        self.emit_bot(id);
        Ok(())
    }

    pub fn delete_bot(&self, id: &str) -> Result<()> {
        // Its threads go first: they are the bot's own work. Its box goes below.
        for thread in crate::chat::work::of(&self.store, id) {
            self.delete_bot(&thread.id)?;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::calls::context::tests::hub;
    use crate::store::Permission;

    #[tokio::test]
    async fn enabling_a_skill_cannot_restore_automatic_approval() {
        let hub = hub();
        let cfg: BotConfig = serde_json::from_value(json!({
            "id": "a", "name": "A", "backend": "custom", "command": "unused",
            "cwd": std::env::temp_dir(), "permission": "auto", "mode": "auto",
        }))
        .unwrap();
        hub.store.save_bot(&cfg).unwrap();
        let mut inbox = hub.attach_bot_for_test("a");

        hub.enable_skills("a", &["release".into()]).unwrap();
        let delayed = inbox.try_recv().ok();
        let updated = hub.update_bot(&json!({"id": "a", "permission": "ask", "mode": "read-only"})).unwrap();
        let Ok(Cmd::Reconfigure(manual)) = inbox.try_recv() else {
            panic!("the actor must receive the user's approval setting");
        };

        // Apply the skill notification after the user's update, as in the race.
        let live = match delayed {
            Some(Cmd::Reconfigure(stale)) => stale,
            _ => manual,
        };
        assert_eq!((live.permission, live.mode.as_deref()), (Permission::Ask, Some("read-only")));
        assert_eq!(updated["skills"], json!(["release"]));
    }
}
