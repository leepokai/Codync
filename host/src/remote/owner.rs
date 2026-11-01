//! The account this computer belongs to, kept on the computer: pairing checks a phone's account against it
//! even with the cloud unreachable, and a change of account drops the old account's devices
//! (docs/architecture/connections.md: "the host changed its account").

use super::cloud::Owner;
use crate::hub::Hub;
use crate::store::Store;
use anyhow::Result;

const OWNER: &str = "account_owner";

/// The account as last seen from the cloud; `None` when the computer isn't in one (or never heard yet).
pub fn owner(store: &Store) -> Option<Owner> {
    store.kv_get(OWNER).and_then(|v| serde_json::from_str::<Option<Owner>>(&v).ok()).flatten()
}

/// Records the account the cloud reports. When it is another one than before (joining one, moving, or leaving),
/// every device let in for another account goes; joining a first account keeps the phones paired before devices
/// recorded their account. The first record after an upgrade only records.
pub fn remember(hub: &Hub, now: Option<&Owner>) -> Result<()> {
    let saved = serde_json::to_string(&now)?;
    let before = hub.store.kv_get(OWNER);
    if before.as_deref() == Some(saved.as_str()) {
        return Ok(());
    }
    hub.store.kv_set(OWNER, &saved)?;
    let Some(before) = before else { return Ok(()) };
    let before = serde_json::from_str::<Option<Owner>>(&before).ok().flatten().map(|o| o.user_id);
    let now = now.map(|o| o.user_id.as_str());
    if before.as_deref() != now {
        let gone = hub.store.keep_devices_of(now, before.is_none())?;
        tracing::info!(devices = gone.len(), "the computer changed its account; the old account's devices are gone");
        if !gone.is_empty() {
            hub.auth_changed();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::remote::identity::Identity;
    use crate::store::{Device, DeviceSource, Scope};
    use std::sync::Arc;

    fn hub() -> Arc<Hub> {
        let dir = std::env::temp_dir().join(format!("codync-owner-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Hub::new(
            Store::open(&dir.join("t.db")).unwrap(),
            "host".into(),
            Identity::load_or_create(&dir).unwrap(),
            "t".into(),
            0,
        )
    }

    fn device(hub: &Hub, key: &str, account: Option<&str>) {
        hub.store
            .put_device(&Device {
                key: key.into(),
                name: "Phone".into(),
                platform: "ios".into(),
                source: DeviceSource::Local,
                grant_id: None,
                scopes: vec![Scope::Control],
                lease_until: None,
                created_at: 1,
                last_seen_at: None,
                account: account.map(Into::into),
            })
            .unwrap();
    }

    fn keys(hub: &Hub) -> Vec<String> {
        hub.store.devices().unwrap().into_iter().map(|d| d.key).collect()
    }

    fn account(id: &str) -> Owner {
        Owner { user_id: id.into(), email: Some(format!("{id}@example.com")) }
    }

    #[test]
    fn changing_accounts_drops_the_old_accounts_devices() {
        let hub = hub();
        device(&hub, "a", Some("user_a"));
        device(&hub, "b", Some("user_b"));
        device(&hub, "legacy", None);
        // An upgraded computer records what it finds and drops nothing.
        remember(&hub, Some(&account("user_a"))).unwrap();
        assert_eq!(keys(&hub), ["a", "b", "legacy"]);
        assert_eq!(owner(&hub.store).unwrap().user_id, "user_a");
        // The same account again (a new email included) changes nothing.
        remember(&hub, Some(&Owner { user_id: "user_a".into(), email: None })).unwrap();
        assert_eq!(keys(&hub), ["a", "b", "legacy"]);
        // Moving to B keeps only B's devices.
        remember(&hub, Some(&account("user_b"))).unwrap();
        assert_eq!(keys(&hub), ["b"]);
        // Leaving the account drops the rest.
        remember(&hub, None).unwrap();
        assert!(keys(&hub).is_empty(), "leaving drops every device");
        assert!(owner(&hub.store).is_none());
    }

    #[test]
    fn joining_a_first_account_keeps_the_phones_it_had() {
        let hub = hub();
        remember(&hub, None).unwrap();
        device(&hub, "legacy", None);
        device(&hub, "mine", Some("user_a"));
        device(&hub, "theirs", Some("user_b"));
        remember(&hub, Some(&account("user_a"))).unwrap();
        assert_eq!(keys(&hub), ["legacy", "mine"]);
    }
}
