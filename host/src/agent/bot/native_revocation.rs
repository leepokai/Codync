//! Revoke only a matching native transport generation or sender, preserving unrelated queued work.
use super::{Actor, Queued};

impl Actor {
    pub(super) async fn revoke_native_inputs(
        &mut self,
        connection: &str,
        generation: Option<&str>,
        user: Option<&str>,
    ) {
        let matches = |id: &str| {
            self.hub.store.entry(id).is_some_and(|entry| {
                let fence = &entry.data["nativeFence"];
                entry.data["source"] == "native-channel"
                    && fence["connection"].as_str() == Some(connection)
                    && generation.is_none_or(|value| fence["generation"].as_str() == Some(value))
                    && user.is_none_or(|value| fence["user"].as_str() == Some(value))
            })
        };
        let revoked: std::collections::HashSet<_> = self
            .queue
            .iter()
            .filter_map(|item| match item {
                Queued::User { entry_id, .. } if matches(entry_id) => Some(entry_id.clone()),
                _ => None,
            })
            .collect();
        let active = self.turn_entries.iter().any(|id| matches(id));
        self.queue.retain(|item| !matches!(item, Queued::User { entry_id, .. } if revoked.contains(entry_id)));
        for id in revoked {
            super::channel_input::cancel(&self.hub, &id);
        }
        if active {
            self.stop_owner_after_turn |= self.cfg.backend == "hermes";
            self.cancel_turn().await;
        }
    }
}
