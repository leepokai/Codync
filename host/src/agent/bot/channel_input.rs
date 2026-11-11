//! Platform authorization is checked when queued input actually reaches the model turn.

use crate::hub::Hub;
use anyhow::Result;

pub(super) fn claim(hub: &Hub, id: &str) -> Result<bool> {
    let Some(entry) = hub.store.entry_checked(id)? else { return Ok(false) };
    match entry.data["source"].as_str() {
        Some("native-channel") => crate::native_channels::claim_prompt(hub, id),
        Some("slack") => crate::slack::claim_prompt(hub, id),
        Some("discord") => crate::discord::claim_prompt(hub, id),
        Some("whatsapp") => crate::whatsapp::claim_prompt(hub, id),
        Some("telegram") => crate::telegram::claim_prompt(hub, id),
        _ => Ok(true),
    }
}

pub(super) fn cancel(hub: &Hub, id: &str) {
    if let Some(mut entry) = hub.store.entry(id).filter(|entry| entry.data["status"] == "queued") {
        entry.data["status"] = "cancelled".into();
        hub.set_entry(id, &entry.data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::identity::Identity;
    use crate::store::{BotConfig, EntryKind, Lane, Store};
    use serde_json::json;

    #[test]
    fn hermes_channel_authorization_distinguishes_missing_entries_from_failed_database_reads() {
        let root = std::env::temp_dir().join(format!("codync-channel-claim-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let hub = Hub::new(
            Store::open(std::path::Path::new(":memory:")).unwrap(),
            "test".into(),
            Identity::load_or_create(&root).unwrap(),
            "test".into(),
            0,
        );
        let bot: BotConfig =
            serde_json::from_value(json!({"id":"a","name":"A","backend":"fixture","cwd":root})).unwrap();
        hub.store.save_bot(&bot).unwrap();
        assert!(!claim(&hub, "missing").unwrap());
        let entry = hub
            .add_entry(
                &Lane::main("a"),
                EntryKind::User,
                1,
                &json!({"text":"ordinary owner input","status":"queued","clientNonce":"n"}),
            )
            .unwrap();
        assert!(claim(&hub, &entry.id).unwrap(), "ordinary validated owner input still uses the usual path");
        hub.store
            .with_connection(|connection| {
                connection.execute_batch("DROP TABLE entries")?;
                Ok(())
            })
            .unwrap();
        assert!(
            claim(&hub, &entry.id).is_err(),
            "a failed read must be deferred, never accepted as an unfenced message"
        );
        assert!(hub.store.find_by_nonce_checked("a", "n").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
