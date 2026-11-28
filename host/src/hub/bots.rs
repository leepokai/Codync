//! Creating, changing and deleting bots.

use super::{Hub, patch, projects, sections, validate};
use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::store::BotConfig;
use anyhow::{Context, Result, anyhow, bail};
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
        // The outer option distinguishes an old client (no expectation) from a new client expecting
        // the default login (`null`); the inner option names that default or one saved profile.
        let expected_login: Option<Option<String>> = patch
            .get("expectedLoginProfile")
            .map(|expected| serde_json::from_value::<Option<String>>(expected.clone()))
            .transpose()
            .context("expectedLoginProfile must be a string or null")?
            .map(|expected| expected.filter(|id| !id.is_empty()));
        // Read, merge and save under one lock: a section rename in between would otherwise be written back.
        let (before, cfg) = {
            let _g = self.emit_lock.locked();
            let row = self.store.bot(&id)?.filter(|b| !b.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
            if expected_login.is_some_and(|expected| row.config.login_profile != expected) {
                bail!("the bot's login profile changed while the model manager was applying; try the row again");
            }
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
    use crate::remote::identity::Identity;
    use crate::store::Store;

    /// The Bot Manager read the default login's catalog, then another device chose a named login
    /// before its update arrived. Compare-and-save under `emit_lock` must preserve that newer choice.
    #[test]
    fn a_stale_expected_login_profile_cannot_restore_the_old_login_or_model() {
        let dir = std::env::temp_dir().join(format!("codync-bots-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let hub = Hub::new(store, "bots-test".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0);
        let cfg: BotConfig = serde_json::from_value(json!({
            "id": "dev", "name": "dev", "backend": "claude", "cwd": dir,
            "loginProfile": "before", "model": "old-model", "modelOptions": {"effort": "low"}
        }))
        .unwrap();
        hub.store.save_bot(&cfg).unwrap();
        let mut cfg = cfg;
        cfg.login_profile = Some("after".into());
        hub.store.save_bot(&cfg).unwrap();

        let error = hub
            .update_bot(&json!({
                "id": "dev", "expectedLoginProfile": "before", "loginProfile": "before",
                "model": "stale-model", "modelOptions": {"effort": "high"}
            }))
            .expect_err("a changed login refuses the stale apply");
        assert!(error.to_string().contains("login profile changed"), "{error}");
        let saved = hub.store.bot("dev").unwrap().unwrap().config;
        assert_eq!(
            serde_json::to_value(saved).unwrap(),
            serde_json::to_value(cfg).unwrap(),
            "neither the newer login nor its model is overwritten"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn an_apply_refuses_to_restore_a_login_changed_since_its_catalog_check() {
        let dir = std::env::temp_dir().join(format!("codync-bot-update-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let hub = Hub::new(store, "bot-update-test".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0);
        let cfg: BotConfig = serde_json::from_value(json!({
            "id": "dev", "name": "dev", "backend": "claude", "cwd": dir, "model": "original"
        }))
        .unwrap();
        hub.store.save_bot(&cfg).unwrap();
        let _commands = hub.attach_bot_for_test("dev");
        let profile =
            crate::agent::login_profiles::add(&hub.store, "claude", crate::agent::login_profiles::Kind::ApiKey)
                .await
                .unwrap()["id"]
                .as_str()
                .unwrap()
                .to_owned();

        // Another window moves the bot from the default login while Apply is still loading catalogs.
        hub.update_bot(&json!({
            "id": "dev", "expectedLoginProfile": null, "loginProfile": profile, "model": "other-window"
        }))
        .unwrap();
        let current = hub.store.bot("dev").unwrap().unwrap();

        let refused = hub
            .update_bot(&json!({
                "id": "dev", "expectedLoginProfile": null, "loginProfile": null,
                "model": "stale-apply", "modelOptions": {"effort": "low"}
            }))
            .unwrap_err()
            .to_string();
        assert!(refused.contains("login profile changed"), "{refused}");
        let after = hub.store.bot("dev").unwrap().unwrap();
        assert_eq!(after.rev, current.rev, "a refused stale patch writes nothing");
        assert_eq!(after.config.login_profile.as_deref(), Some(profile.as_str()));
        assert_eq!(after.config.model.as_deref(), Some("other-window"));
        assert!(after.config.model_options.is_empty());

        // Retrying against the fresh login and catalog works without restoring the old one.
        hub.update_bot(&json!({
            "id": "dev", "expectedLoginProfile": profile, "loginProfile": profile,
            "model": "fresh-apply", "modelOptions": {"effort": "high"}
        }))
        .unwrap();
        let saved = hub.store.bot("dev").unwrap().unwrap().config;
        assert_eq!(saved.login_profile.as_deref(), Some(profile.as_str()));
        assert_eq!(saved.model.as_deref(), Some("fresh-apply"));
        assert_eq!(saved.model_options.get("effort").map(String::as_str), Some("high"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
