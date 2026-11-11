//! Durable ordered input, with transport commands kept independent of slow speech work.

use super::{Arc, Hub, Value, client::Endpoint, commands, worker};
use anyhow::{Context, Result};
use sha2::{Digest as _, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::watch;

#[cfg(test)]
mod tests;

fn prefix(name: &str) -> String {
    format!("telegram.inbox.{}.", crate::chat::files::digest_hex(&Sha256::digest(name.as_bytes())))
}

/// The configuration caller holds connector_lock while removing the connection.
pub(super) fn forget(store: &crate::store::Store, name: &str) -> Result<()> {
    for key in store.kv_prefix(&prefix(name)) {
        store.kv_delete(&key)?;
    }
    Ok(())
}

#[derive(Clone)]
pub(super) struct Inbox {
    key: String,
    tag: String,
    cancel: watch::Sender<u64>,
    control: Arc<AtomicBool>,
}

struct Control(Arc<AtomicBool>);

impl Drop for Control {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
        worker::configuration_changed();
    }
}

impl Inbox {
    pub(super) fn open(hub: &Hub, name: &str, tag: String) -> Result<Self> {
        let generation = crate::chat::files::digest_hex(&Sha256::digest(tag.as_bytes()));
        let prefix = prefix(name);
        let key = format!("{prefix}{generation}");
        let _guard = crate::LockExt::locked(&hub.store.connector_lock);
        for old in hub.store.kv_prefix(&prefix) {
            if old != key {
                hub.store.kv_delete(&old)?;
            }
        }
        Ok(Self { key, tag, cancel: watch::channel(0).0, control: Arc::new(AtomicBool::new(false)) })
    }

    pub(super) fn handling_control(&self) -> bool {
        self.control.load(Ordering::Acquire)
    }

    fn control(&self) -> Control {
        self.control.store(true, Ordering::Release);
        Control(self.control.clone())
    }

    fn read(&self, hub: &Hub) -> Result<Vec<Value>> {
        hub.store
            .kv_read(&self.key)?
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .context("Reading Telegram input queue")
            .map(Option::unwrap_or_default)
    }

    fn push(&self, hub: &Hub, name: &str, update: &Value) -> Result<bool> {
        let _guard = crate::LockExt::locked(&hub.store.connector_lock);
        if worker::current(hub, name).is_none_or(|bot| worker::fingerprint(&bot) != self.tag) {
            anyhow::bail!("Telegram connection changed; the old receiver must not acknowledge this update");
        }
        let mut queue = self.read(hub)?;
        if queue.iter().any(|old| old["update_id"] == update["update_id"]) {
            return Ok(true);
        }
        if queue.len() >= 32 || serde_json::to_vec(update)?.len() > 256 * 1024 {
            return Ok(false);
        }
        queue.push(update.clone());
        hub.store.kv_set(&self.key, &serde_json::to_string(&queue)?)?;
        Ok(true)
    }

    fn finish(&self, hub: &Hub, update: &Value) -> Result<()> {
        let _guard = crate::LockExt::locked(&hub.store.connector_lock);
        let mut queue = self.read(hub)?;
        queue.retain(|old| old["update_id"] != update["update_id"]);
        hub.store.kv_set(&self.key, &serde_json::to_string(&queue)?)
    }

    pub(super) async fn accept(
        &self,
        hub: &Arc<Hub>,
        ep: &Endpoint,
        name: &str,
        shared: &worker::Shared,
        update: &Value,
    ) -> Result<()> {
        let Some(bot) = worker::current(hub, name).filter(|bot| worker::fingerprint(bot) == self.tag) else {
            anyhow::bail!("Telegram connection changed; polling will retry with the current authorization");
        };
        if update.get("callback_query").is_some() {
            let _control = self.control();
            worker::handle(hub, ep, name, shared, update, &self.tag).await;
            worker::checkpoint(hub, name, &bot.token, update)?;
            return Ok(());
        }
        let message = &update["message"];
        if message["chat"]["type"] != "private" {
            return Ok(());
        }
        let text = message["text"].as_str().or_else(|| message["caption"].as_str()).unwrap_or_default().trim();
        let command = commands::parse(text, Some(&bot.username));
        let paired = message["from"]["id"].as_i64().is_some_and(|id| bot.allowed.contains(&id));
        if !paired || command.is_some() || text.starts_with("/start") {
            let _control = self.control();
            if paired && command == Some(commands::Command::Stop) {
                // Persist revocation before acknowledging Stop or cancelling the in-flight future.
                let _guard = crate::LockExt::locked(&hub.store.connector_lock);
                hub.store.kv_set(&self.key, "[]")?;
                self.cancel.send_modify(|version| *version = version.wrapping_add(1));
            }
            worker::handle(hub, ep, name, shared, update, &self.tag).await;
            worker::checkpoint(hub, name, &bot.token, update)?;
        } else if !self.push(hub, name, update)? {
            if let Some(chat) = message["chat"]["id"].as_i64() {
                worker::say(
                    ep,
                    &bot,
                    chat,
                    "The input queue is full or this message is too large. Please try again shortly.",
                )
                .await;
            }
        }
        Ok(())
    }

    pub(super) async fn run(self, hub: Arc<Hub>, ep: Endpoint, name: String, shared: worker::Shared) {
        let mut cancel = self.cancel.subscribe();
        loop {
            if worker::current(&hub, &name).is_none_or(|bot| worker::fingerprint(&bot) != self.tag) {
                return;
            }
            // Snapshot the cancellation version and front item under the same lock as Stop.
            // A Stop arriving after this snapshot makes changed() ready before handle is polled.
            let next = {
                let _guard = crate::LockExt::locked(&hub.store.connector_lock);
                let _generation = *cancel.borrow_and_update();
                self.read(&hub).map(|queue| queue.into_iter().next())
            };
            match next {
                Ok(Some(update)) => {
                    tokio::select! {
                        biased;
                        _ = cancel.changed() => {},
                        () = worker::handle(&hub, &ep, &name, &shared, &update, &self.tag) => {},
                    }
                    if let Err(error) = self.finish(&hub, &update) {
                        tracing::warn!(telegram = name, %error, "Telegram input acknowledgement will retry");
                    }
                }
                Ok(None) => {}
                Err(error) => tracing::warn!(telegram = name, %error, "Telegram input queue unavailable"),
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}
