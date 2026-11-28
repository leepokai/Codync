//! Creating, changing and deleting bots.

use super::{Hub, patch, projects, sections, validate};
use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::store::BotConfig;
use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::Ordering;

impl Hub {
    /// Whether `id` (a bot, or one of its threads) is paused: it answers nothing and spends nothing until it is
    /// resumed. A thread stands for its bot's pause.
    pub fn paused(&self, id: &str) -> bool {
        let owner = if crate::store::is_thread_id(id) {
            self.store.bot(id).ok().flatten().filter(|b| !b.deleted).and_then(|b| b.config.thread_of)
        } else {
            Some(id.to_owned())
        };
        owner.is_some_and(|owner| {
            self.store.bot(&owner).ok().flatten().filter(|b| !b.deleted).is_some_and(|b| b.config.paused)
        })
    }

    /// Makes `cfg` ready to save over `before` (none: a new bot). Called under `emit_lock`, with the save: its login
    /// profile can't be removed in between (`login_profiles::remove`).
    fn settle_config(&self, cfg: &mut BotConfig, before: Option<&BotConfig>) -> Result<()> {
        // ponytail: validated under the lock, so a workspace folder on a hung network share (its `is_dir`)
        // stalls every bot's events until it answers. Upgrade: build and validate outside the lock from a
        // snapshot, then save under it only if the stored config is unchanged (compare the serialized configs,
        // not rev), else retry.
        self.check_config(cfg, before)?;
        crate::agent::workspace::prepare(cfg)
    }

    /// What `settle_config` holds a change to before it prepares the workspace: the checks a save refuses on, with
    /// nothing written and no folder made. A dry run calls this outside the lock.
    fn check_config(&self, cfg: &mut BotConfig, before: Option<&BotConfig>) -> Result<()> {
        crate::agent::login_profiles::settle(&self.store, cfg)?;
        crate::agent::fallback::tidy(cfg);
        cfg.section = sections::normalize_section(cfg.section.as_deref())?;
        projects::settle(cfg, before)?;
        validate(cfg)
    }

    /// The config `patch` would save over its bot, checked the way `update_bot` checks it, without saving, emitting
    /// or preparing a workspace folder (`validateOnly` and `dryRun`).
    pub(crate) fn preview_update(&self, patch: &Value) -> Result<BotConfig> {
        let id = patch["id"].as_str().ok_or_else(|| anyhow!("id required"))?.to_owned();
        let row = self.store.bot(&id)?.filter(|b| !b.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
        let mut cfg = patch::patched(&row.config, patch)?;
        if cfg.is_group() {
            cfg.members = crate::chat::group::valid_members(&self.store, &cfg.id, &cfg.members)?;
        }
        // Only the host makes the box of threads without a bot (`chat::work::plain`).
        cfg.plain = row.config.plain;
        self.check_config(&mut cfg, Some(&row.config))?;
        // A dry run doesn't make the folder `workspace::prepare` would: it only refuses an id that can't become one.
        if !cfg.is_group() && cfg.cwd.is_empty() {
            uuid::Uuid::parse_str(&cfg.id).context("invalid bot workspace id")?;
        }
        Ok(cfg)
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
        let (before, cfg, pause_generation) = {
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
            // A pause takes the runs it cancels while it still holds the lock the flag was saved under, and ends them
            // here, in this same lock: a resume saves under this lock too, so it can neither be saved between the
            // snapshot and the cancellation, nor capture a run the resume admitted. A pause reaches every worker, not
            // only the bot's own actor: an active routine run (boxed or delivered by a webhook) is cancelled now.
            if !row.config.paused && cfg.paused {
                let runs = self.routines.active_ids(&cfg.id);
                if !runs.is_empty()
                    && let Err(error) = self.routines.cancel_ids_for_pause(&self.store, &runs)
                {
                    tracing::error!(
                        bot = %id, error = format!("{error:#}"),
                        "couldn't cancel a paused bot's routine runs"
                    );
                }
            }
            // Every save takes the next number here, under the lock, whether it changes the pause flag or not: a
            // save's command can arrive after a later save's, and every field it carries (name, instructions,
            // model), not just the pause flag, must not be reverted by the older one.
            let pause_generation = self.pause_generation.fetch_add(1, Ordering::SeqCst) + 1;
            (row.config, cfg, pause_generation)
        };
        projects::changed(self, &before.projects, &cfg.projects);
        crate::network::publish(self);
        if !cfg.is_group() {
            let pause_changed = before.paused != cfg.paused;
            if crate::chat::memory::shared(&cfg) != crate::chat::memory::shared(&before) {
                crate::chat::memory::changed(self, Some(&id));
            }
            if cfg.fallbacks != before.fallbacks
                || cfg.thread_agent != before.thread_agent
                || cfg.thread_fallbacks != before.thread_fallbacks
            {
                crate::chat::work::fallbacks_changed(self, &cfg);
            }
            if pause_changed {
                // The bot's own actor learns from `Reconfigure` below; each live thread that stands for its
                // owner's pause (`Hub::paused`) is told separately, so a running thread turn stops now too.
                for thread in crate::chat::work::of(&self.store, &id) {
                    if self.has_actor(&thread.id)
                        && let Err(error) =
                            self.send_cmd(&thread.id, Cmd::Paused { paused: cfg.paused, pause_generation })
                    {
                        tracing::warn!(
                            thread = %thread.id, error = format!("{error:#}"),
                            "couldn't give a thread its bot's pause"
                        );
                    }
                }
            }
            self.send_cmd(&id, Cmd::Reconfigure { base: Box::new(cfg), pause_generation: Some(pause_generation) })?;
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
    use crate::agent::bot::Cmd;
    use crate::chat::calls::context::tests::{bot, hub};

    fn dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("codync-pause-generation-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Every save takes the number its commands carry under the lock its config was saved with, so a bot's actor
    /// can drop the older of two saves whose commands arrive out of order (#582): not only the pause flag, but every
    /// field the older snapshot carries.
    #[tokio::test]
    async fn a_save_stamps_the_number_its_commands_carry() {
        let dir = dir("save");
        let hub = hub();
        hub.store.save_bot(&bot("b1", "Mika", &dir.to_string_lossy())).expect("saved");
        let mut rx = hub.attach_bot_for_test("b1");
        let reconfigured = |rx: &mut tokio::sync::mpsc::UnboundedReceiver<Cmd>| match rx.try_recv() {
            Ok(Cmd::Reconfigure { base, pause_generation }) => (base.paused, pause_generation),
            Ok(_) => panic!("a save that paused a bot said something else"),
            Err(error) => panic!("a save that paused a bot said nothing: {error}"),
        };

        hub.update_bot(&json!({"id": "b1", "paused": true})).expect("paused");
        assert_eq!(reconfigured(&mut rx), (true, Some(1)), "a pause change takes the next number");

        hub.update_bot(&json!({"id": "b1", "paused": false})).expect("resumed");
        assert_eq!(reconfigured(&mut rx), (false, Some(2)), "a resume after it takes the one after");

        hub.update_bot(&json!({"id": "b1", "name": "Mika Two"})).expect("renamed");
        assert_eq!(reconfigured(&mut rx), (false, Some(3)), "every save takes its own number, pause or not");
        assert_eq!(hub.pause_generation(), 3);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
