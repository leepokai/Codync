//! The account this computer belongs to, kept on the computer: pairing checks a phone's account against it
//! even with the cloud unreachable, and a change of account drops the old account's devices
//! (docs/architecture/connections.md: "the host changed its account").

use super::cloud::{self, Approval, Owner, RelayKind};
use super::crypto;
use crate::LockExt;
use crate::chat::team::assistant;
use crate::hub::Hub;
use crate::store::Store;
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

const OWNER: &str = "account_owner";
/// The accounts this computer signed a claim for since its account last changed (`note_signed`).
const SIGNED: &str = "account_claims_signed";

/// The account as last seen from the cloud; `None` when the computer isn't in one (or never heard yet), and
/// with the Codync cloud off or a private relay, where it is in no account either (as its status says).
pub fn owner(store: &Store) -> Option<Owner> {
    cloud::current(store).filter(|address| address.kind == RelayKind::Codync)?;
    recorded(store)
}

/// The account last recorded, whichever relay is in use now.
fn recorded(store: &Store) -> Option<Owner> {
    store.kv_get(OWNER).and_then(|v| serde_json::from_str::<Option<Owner>>(&v).ok()).flatten()
}

/// `CloudStatus::joins_account_on_enable`: read from what's saved, as the status says nothing of it with the cloud off.
pub fn joins_account_on_enable(store: &Store) -> bool {
    cloud::when_on(store).is_some_and(|address| address.kind == RelayKind::Codync) && recorded(store).is_none()
}

/// Records the account the cloud reports. When it is another one than before (joining one, moving, or leaving),
/// every device let in for another account goes; joining a first account keeps the phones paired before devices
/// recorded their account. The first record after an upgrade only records. The devices go first, so a failure
/// leaves the old account recorded and the next pull tries again. Any change of account also forgets the accounts
/// claims were signed for before it (`note_signed`).
pub fn remember(hub: &Hub, now: Option<&Owner>) -> Result<()> {
    let saved = serde_json::to_string(&now)?;
    let _change = hub.cloud.account.locked();
    let before = hub.store.kv_get(OWNER);
    if before.as_deref() == Some(saved.as_str()) {
        return Ok(());
    }
    let before_user =
        before.as_deref().and_then(|b| serde_json::from_str::<Option<Owner>>(b).ok().flatten()).map(|o| o.user_id);
    let now_user = now.map(|o| o.user_id.as_str());
    if before_user.as_deref() != now_user {
        if before.is_some() {
            let gone = hub.store.keep_devices_of(now_user, before_user.is_none())?;
            tracing::info!(
                devices = gone.len(),
                "the computer changed its account; the old account's devices are gone"
            );
            if !gone.is_empty() {
                hub.auth_changed();
            }
        }
        hub.store.kv_delete(SIGNED)?;
    }
    hub.store.kv_set(OWNER, &saved)
}

/// Runs `f` while the account can't change (`remember` waits), so a phone `f` saves against the account it reads
/// can't come in after a change of account dropped the other accounts' devices.
pub fn while_unchanged<T>(hub: &Hub, f: impl FnOnce() -> T) -> T {
    let _change = hub.cloud.account.locked();
    f()
}

/// Out of an account, cloud assistants go off and the 6-digit check comes back on: the account this computer
/// joins next is asked again.
pub fn out_of_account(hub: &Hub) -> Result<()> {
    if assistant::enabled(&hub.store) {
        assistant::set_enabled(hub, false)?;
    }
    if cloud::approval(&hub.store) == Approval::Auto {
        cloud::set_approval(hub, Approval::Code)?;
    }
    Ok(())
}

/// The account the device `key` may bring this computer into (`joinAccount`), when a pairing let the phone in
/// without the computer signing a claim for its account (the phone couldn't start one, or the cloud was off or a
/// private relay). Only on the Codync cloud, with this connection's state pulled and no account yet, for the account
/// the phone paired from, only when no device of another account would lose access by it, and never for an account
/// this computer signed a claim for since its account last changed (`note_signed`).
pub fn joinable_by(hub: &Hub, key: &str) -> Result<Option<String>> {
    if cloud::codync_url(hub).is_err() || !hub.cloud_synced.load(Ordering::Acquire) || owner(&hub.store).is_some() {
        return Ok(None);
    }
    let Some(account) = hub.store.device(key).and_then(|d| d.account) else { return Ok(None) };
    let devices = hub.store.devices()?;
    let alone = devices.iter().all(|d| d.account.as_ref().is_none_or(|a| *a == account));
    Ok((alone && !signed_for(&hub.store)?.contains(&account)).then_some(account))
}

/// Notes the account a claim is about to be signed for. That claim may complete and the computer be taken out of
/// the account again before it pulls its state, which then still says no account: until its account changes, the
/// computer doesn't offer that account a later join, so it never comes back into an account it was taken out of.
fn note_signed(hub: &Hub, user_id: &str) -> Result<()> {
    let _change = hub.cloud.account.locked();
    let mut signed = signed_for(&hub.store)?;
    if !signed.iter().any(|u| u == user_id) {
        signed.push(user_id.to_owned());
        hub.store.kv_set(SIGNED, &serde_json::to_string(&signed)?)?;
    }
    Ok(())
}

fn signed_for(store: &Store) -> Result<Vec<String>> {
    store.kv_read(SIGNED)?.map_or(Ok(vec![]), |s| Ok(serde_json::from_str(&s)?))
}

/// Signs this computer into an account claim (§4.2 A) for a signed-in app to complete: the desktop app's
/// Add to account, a phone pairing with a computer in no account, or a phone finishing that join later.
pub async fn sign_claim(hub: &Hub, claim_id: &str, nonce: &str, user_id: &str) -> Result<Value> {
    cloud::require_accounts(&hub.store)?;
    for (k, v) in [("claimId", claim_id), ("nonce", nonce), ("userId", user_id)] {
        // Fields are newline-separated in the signed string: a newline would forge another field.
        if v.is_empty() || v.contains('\n') {
            bail!("`{k}` is invalid");
        }
    }
    note_signed(hub, user_id)?;
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
    fn a_phone_brings_a_computer_in_no_account_into_the_account_it_paired_from() {
        let hub = hub();
        device(&hub, "mine", Some("user_a"));
        device(&hub, "legacy", None);
        assert_eq!(joinable_by(&hub, "mine").unwrap(), None, "not before this connection's state pull");
        hub.cloud_synced.store(true, Ordering::Release);
        assert_eq!(joinable_by(&hub, "mine").unwrap().as_deref(), Some("user_a"));
        assert_eq!(
            joinable_by(&hub, "legacy").unwrap(),
            None,
            "a phone that paired before devices recorded their account"
        );
        assert_eq!(joinable_by(&hub, "local").unwrap(), None);
        device(&hub, "theirs", Some("user_b"));
        assert_eq!(joinable_by(&hub, "mine").unwrap(), None, "another account's phone would lose access");
        hub.store.remove_device("theirs").unwrap();
        remember(&hub, Some(&account("user_a"))).unwrap();
        assert_eq!(joinable_by(&hub, "mine").unwrap(), None, "already in an account");
        remember(&hub, None).unwrap();
        device(&hub, "mine", Some("user_a"));
        hub.store.kv_set("cloud_enabled", "false").unwrap();
        assert_eq!(joinable_by(&hub, "mine").unwrap(), None, "no account with the cloud off");
    }

    #[test]
    fn a_computer_taken_out_before_it_heard_it_joined_stays_out() {
        let hub = hub();
        remember(&hub, None).unwrap();
        hub.cloud_synced.store(true, Ordering::Release);
        device(&hub, "a", Some("user_a"));
        device(&hub, "b", Some("user_a"));
        // Phone a's claim completes and the computer is taken out again, both between two state pulls.
        note_signed(&hub, "user_a").unwrap();
        remember(&hub, None).unwrap();
        assert_eq!(joinable_by(&hub, "a").unwrap(), None);
        assert_eq!(joinable_by(&hub, "b").unwrap(), None, "nor through another phone of that account");

        // Seen in the account and taken out, its phones go: one let in again may bring it back.
        remember(&hub, Some(&account("user_a"))).unwrap();
        remember(&hub, None).unwrap();
        assert!(hub.store.device("a").is_none());
        device(&hub, "a", Some("user_a"));
        assert_eq!(joinable_by(&hub, "a").unwrap().as_deref(), Some("user_a"));
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
