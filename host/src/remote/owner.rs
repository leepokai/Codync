//! The account this computer belongs to, kept on the computer: pairing checks a phone's account against it
//! even with the cloud unreachable, and a change of account drops the old account's devices
//! (docs/architecture/connections.md: "the host changed its account").

use super::cloud::{Owner, RelayKind};
use super::crypto;
use crate::hub::Hub;
use crate::store::Store;
use anyhow::{Result, bail};
use serde_json::{Value, json};

const OWNER: &str = "account_owner";

/// The account as last seen from the cloud; `None` when the computer isn't in one (or never heard yet), and
/// with the Codync cloud off or a private relay, where it is in no account either (as its status says).
pub fn owner(store: &Store) -> Option<Owner> {
    super::cloud::current(store).filter(|address| address.kind == RelayKind::Codync)?;
    store.kv_get(OWNER).and_then(|v| serde_json::from_str::<Option<Owner>>(&v).ok()).flatten()
}

/// Records the account the cloud reports. When it is another one than before (joining one, moving, or leaving),
/// every device let in for another account goes; joining a first account keeps the phones paired before devices
/// recorded their account. The first record after an upgrade only records. The devices go first, so a failure
/// leaves the old account recorded and the next pull tries again.
pub fn remember(hub: &Hub, now: Option<&Owner>) -> Result<()> {
    let saved = serde_json::to_string(&now)?;
    let before = hub.store.kv_get(OWNER);
    if before.as_deref() == Some(saved.as_str()) {
        return Ok(());
    }
    if let Some(before) = before {
        let before = serde_json::from_str::<Option<Owner>>(&before).ok().flatten().map(|o| o.user_id);
        let now = now.map(|o| o.user_id.as_str());
        if before.as_deref() != now {
            let gone = hub.store.keep_devices_of(now, before.is_none())?;
            tracing::info!(
                devices = gone.len(),
                "the computer changed its account; the old account's devices are gone"
            );
            if !gone.is_empty() {
                hub.auth_changed();
            }
        }
    }
    hub.store.kv_set(OWNER, &saved)
}

/// Signs this computer into an account claim (§4.2 A) for a signed-in app to complete: the desktop app's
/// Add to account, or a phone pairing with a computer in no account.
pub async fn sign_claim(hub: &Hub, claim_id: &str, nonce: &str, user_id: &str) -> Result<Value> {
    super::cloud::require_accounts(&hub.store)?;
    for (k, v) in [("claimId", claim_id), ("nonce", nonce), ("userId", user_id)] {
        // Fields are newline-separated in the signed string: a newline would forge another field.
        if v.is_empty() || v.contains('\n') {
            bail!("`{k}` is invalid");
        }
    }
    let computer_id = hub.identity.computer_id();
    let box_key = hub.identity.box_pub_b64();
    let input = crypto::claim_input(claim_id, nonce, user_id, &computer_id, &box_key);
    Ok(json!({
        "computerId": computer_id,
        "signKey": hub.identity.sign_pub_b64(),
        "boxKey": box_key,
        "name": crate::service::host_name(),
        "platform": std::env::consts::OS,
        "device": tokio::task::spawn_blocking(crate::service::device).await?,
        "version": env!("CARGO_PKG_VERSION"),
        "sig": crypto::b64(&hub.identity.sign(input.as_bytes())),
    }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::remote::identity::Identity;
    use crate::store::{Device, DeviceSource, Scope};
    use std::sync::Arc;

    /// On the Codync cloud, where a computer can be in an account.
    fn hub() -> Arc<Hub> {
        let dir = std::env::temp_dir().join(format!("codync-owner-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("t.db")).unwrap();
        store.kv_set("test_cloud_url", "https://dev-api.codync.dev").unwrap();
        Hub::new(store, "host".into(), Identity::load_or_create(&dir).unwrap(), "t".into(), 0)
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

    #[test]
    fn with_the_cloud_off_or_a_private_relay_the_computer_is_in_no_account() {
        let hub = hub();
        remember(&hub, Some(&account("user_a"))).unwrap();
        hub.store.kv_set("test_cloud_token", "relay-token").unwrap();
        assert!(owner(&hub.store).is_none(), "a private relay has no accounts");
        hub.store.kv_set("cloud_enabled", "false").unwrap();
        assert!(owner(&hub.store).is_none(), "nor does a computer with its cloud off");
    }
}
