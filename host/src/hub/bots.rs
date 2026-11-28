//! Creating, changing and deleting bots.

use super::{Hub, patch, projects, sections, validate};
use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::store::BotConfig;
use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use std::sync::Arc;

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
        // A pause and a resume are transactions: the older one finishes every effect before the newer one starts.
        let _transition = self.pause.transition();
        // `paused` and kind change only in this function, which already holds the transition, so the wait is
        // decided from the stored row. The merge happens after the wait and is what gets saved.
        let pausing = {
            let row = self.store.bot(&id)?.filter(|b| !b.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
            let asks_for_pause = patch.get("paused").and_then(Value::as_bool) == Some(true);
            !row.config.is_group() && !row.config.paused && asks_for_pause
        };
        if pausing {
            // Before the flag is saved: no new prompt is admitted, and one already being written reaches the agent
            // before this pause continues. Never hold every bot's event lock while waiting for that write.
            self.pause.block(&id);
        }
        // One lock for the read, the merge and the save. A section rename during the wait takes this same lock,
        // so it commits before this read and is not written back.
        let saved = (|| -> Result<_> {
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
            Ok((row.config, cfg))
        })();
        let (before, cfg) = match saved {
            Ok(pair) => pair,
            Err(error) => {
                // `block` already advanced the epoch. Drop the in-memory block when the row was not saved.
                // The epoch stays advanced (a failed save still looks paused to an in-flight admission).
                if pausing {
                    self.pause.unblock(&id);
                }
                return Err(error);
            }
        };
        projects::changed(self, &before.projects, &cfg.projects);
        crate::network::publish(self);
        if !cfg.is_group() {
            let pause_changed = before.paused != cfg.paused;
            if pause_changed && !cfg.paused {
                // Resume starts nothing from the pause: runs and due schedules that lived through it are discarded
                // before admission opens. Routine work is held by the same transition while this runs.
                if let Err(error) = self.routines.resume_after_pause(&self.store, &id) {
                    tracing::error!(
                        bot = %id, error = format!("{error:#}"),
                        "couldn't settle a resumed bot's routine runs"
                    );
                }
                self.pause.unblock(&id);
            }
            if pause_changed && cfg.paused {
                // A pause reaches every worker, not only the bot's own actor: an active routine run (boxed or
                // delivered by a webhook) is cancelled, and a request still waiting in the box is answered now.
                if let Err(error) = self.routines.cancel_for_pause(&self.store, &id) {
                    tracing::error!(
                        bot = %id, error = format!("{error:#}"),
                        "couldn't cancel a paused bot's routine runs"
                    );
                }
                crate::chat::work::paused(self, &id);
            }
            if crate::chat::memory::shared(&cfg) != crate::chat::memory::shared(&before) {
                crate::chat::memory::changed(self, Some(&id));
            }
            if cfg.fallbacks != before.fallbacks
                || cfg.thread_agent != before.thread_agent
                || cfg.thread_fallbacks != before.thread_fallbacks
            {
                crate::chat::work::fallbacks_changed(self, &cfg);
            }
            // One generation for this save. `block` has already stored it when the save is a pause.
            let pause_epoch = self.pause.epoch(&id);
            if pause_changed {
                // The bot's own actor learns from `Reconfigure` below; each live thread that reads its owner's
                // pause (`Actor::paused`) is told separately, so a running thread turn stops now too.
                for thread in crate::chat::work::of(&self.store, &id) {
                    if self.has_actor(&thread.id)
                        && let Err(error) = self.send_cmd(&thread.id, Cmd::Paused { paused: cfg.paused, pause_epoch })
                    {
                        tracing::warn!(
                            thread = %thread.id, error = format!("{error:#}"),
                            "couldn't give a thread its bot's pause"
                        );
                    }
                }
            }
            self.send_cmd(&id, Cmd::Reconfigure { base: Box::new(cfg), pause_epoch })?;
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
    use crate::hub::Hub;
    use crate::remote::identity::Identity;
    use crate::store::{BotConfig, Store};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    /// A pause waits out an admitted prompt write with `emit_lock` released, then reads and saves.
    /// Two properties: the rename during that wait does not sit on the pause's lock, and the save
    /// keeps the renamed section.
    #[test]
    fn a_section_rename_during_a_pause_wait_is_not_written_back() {
        let dir = std::env::temp_dir().join(format!("codync-pause-rename-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let cwd = dir.join("work");
        std::fs::create_dir_all(&cwd).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let hub = Hub::new(store, "pause-rename".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0);
        let cfg: BotConfig = serde_json::from_value(json!({
            "id": "b", "name": "b", "backend": "claude", "cwd": cwd, "section": "Old",
        }))
        .unwrap();
        hub.store.save_bot(&cfg).unwrap();

        let admitted = hub.pause.admit_prompt("b", hub.pause.epoch("b")).expect("the prompt is admitted");
        let pausing = hub.clone();
        let saved = std::thread::spawn(move || pausing.update_bot(&json!({"id": "b", "paused": true})));
        let started = Instant::now();
        while !hub.pause.blocked("b") {
            assert!(started.elapsed() < Duration::from_secs(2), "the pause never closed admission");
            std::thread::sleep(Duration::from_millis(5));
        }
        // Time only the rename, and only while the pause is still inside block(). The lock is free
        // during that wait, so the rename returns at once. If the pause held emit_lock across the
        // wait, this would sit until the admitted write was dropped.
        let renaming = hub.clone();
        let (finished, waited) = mpsc::channel();
        let rename_thread = std::thread::spawn(move || {
            let rename_started = Instant::now();
            let result = renaming.rename_section("Old", Some("Kept"));
            let _ = finished.send((rename_started.elapsed(), result));
        });
        let rename_finished = waited.recv_timeout(Duration::from_secs(1));
        assert!(!saved.is_finished(), "the pause finished before the rename, so the timing says nothing");
        drop(admitted);
        let _ = rename_thread.join();
        let (rename_wait, renamed) = rename_finished.expect("the rename waited on the pause's emit_lock");
        renamed.expect("the section rename");
        assert!(rename_wait < Duration::from_secs(1), "the rename waited on the pause's emit_lock ({rename_wait:?})");
        let saved = saved.join().expect("the pause thread panicked");
        // No actor is running: the row is saved, then Reconfigure has nobody to take it.
        let err = saved.expect_err("saving the pause still reports that the actor is absent");
        assert!(err.to_string().contains("unknown bot"), "{err:#}");
        let row = hub.store.bot("b").unwrap().unwrap();
        assert!(row.config.paused, "the pause was saved");
        assert_eq!(
            row.config.section.as_deref(),
            Some("Kept"),
            "the rename during the pause wait was written back over"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A pause is saved in the bot's config, but the gate that refuses messages is in memory: at start it has to be
    /// closed again, or a message that arrives after a restart is queued and answered on resume instead of refused.
    #[tokio::test]
    async fn a_pause_saved_before_the_restart_is_closed_again_at_start() {
        let dir = std::env::temp_dir().join(format!("codync-pause-start-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("work")).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let hub = Hub::new(store, "pause-start".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0);
        let cfg: BotConfig = serde_json::from_value(json!({
            "id": "b", "name": "b", "backend": "claude", "cwd": dir.join("work"), "paused": true,
        }))
        .unwrap();
        hub.store.save_bot(&cfg).unwrap();
        assert!(!hub.pause.blocked("b"), "a fresh host has not blocked anything yet");

        hub.start().unwrap();

        assert!(hub.pause.blocked("b"), "a message that arrives after the restart is refused, not queued");
        assert_eq!(hub.pause.epoch("b"), 1, "the start is the one pause generation this bot is on");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
