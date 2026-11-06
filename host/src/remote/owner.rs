//! The account this computer belongs to, kept on the computer: pairing checks a phone's account against it
//! even with the cloud unreachable, and a change of account drops the old account's devices
//! (docs/architecture/connections.md: "the host changed its account").

use super::cloud::{self, Approval, Owner, RelayKind};
use super::pairing_requests::{self, Answer, Question};
use super::{certificate, crypto};
use crate::LockExt;
use crate::chat::team::assistant;
use crate::hub::Hub;
use crate::store::{DeviceAccount, Store, now_ms};
use anyhow::{Context as _, Result, anyhow, bail};
use serde_json::{Value, json};

const OWNER: &str = "account_owner";
/// How long the relay stays up after signing a claim: the phone completes it, and the cloud says so over the relay.
const JOIN_WAIT_MS: i64 = 5 * 60 * 1000;

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
/// recorded their account. In an account, phones that paired with no account always go, and a claim being completed
/// is over, so it can't outlive the account. The first record after an upgrade only records. The devices go first, so
/// a failure leaves the old account recorded and the next pull tries again.
pub fn remember(hub: &Hub, now: Option<&Owner>) -> Result<()> {
    let saved = serde_json::to_string(&now)?;
    let _change = hub.cloud.account.locked();
    if now.is_some() {
        *hub.cloud.join_pending_until.locked() = 0;
        let gone = hub.store.drop_no_account_devices()?;
        if !gone.is_empty() {
            tracing::info!(devices = gone.len(), "the computer is in an account; phones with no account are gone");
            hub.auth_changed();
        }
    }
    let before = hub.store.kv_get(OWNER);
    if before.as_deref() == Some(saved.as_str()) {
        return Ok(());
    }
    let before_user =
        before.as_deref().and_then(|b| serde_json::from_str::<Option<Owner>>(b).ok().flatten()).map(|o| o.user_id);
    let now_user = now.map(|o| o.user_id.as_str());
    if before_user.as_deref() != now_user && before.is_some() {
        let gone = hub.store.keep_devices_of(now_user, before_user.is_none())?;
        tracing::info!(devices = gone.len(), "the computer changed its account; the old account's devices are gone");
        if !gone.is_empty() {
            hub.auth_changed();
        }
    }
    hub.store.kv_set(OWNER, &saved)?;
    // Phones read the relay address in `hello`, and the relay stays up only in an account.
    hub.cloud.account_changed.send_modify(|n| *n += 1);
    hub.cloud.relay_wake.notify_one();
    Ok(())
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

/// Whether this computer could join an account: on the Codync cloud, and in none yet.
pub fn can_join(hub: &Hub) -> bool {
    cloud::codync_url(hub).is_ok() && owner(&hub.store).is_none()
}

/// A claim was signed moments ago, and the phone or the app is completing it.
fn joining(hub: &Hub) -> bool {
    now_ms() < *hub.cloud.join_pending_until.locked()
}

/// A private relay is the user's own; the Codync relay is for a computer in an account
/// (docs/plans/optional-sign-in.md).
fn relay_is_open(hub: &Hub, address: &cloud::Address) -> bool {
    address.kind == RelayKind::Private || owner(&hub.store).is_some()
}

/// The relay address the phone with `key` gets in `hello`: on an open relay, or while a claim is being completed if
/// it has an account, as the phone finishing the join goes on through the relay.
pub fn url_for_phones(hub: &Hub, key: &str) -> Option<String> {
    let address = cloud::current(&hub.store)?;
    let has_account = || hub.store.device(key).is_some_and(|d| matches!(d.account, DeviceAccount::Of(_)));
    (relay_is_open(hub, &address) || (joining(hub) && has_account())).then_some(address.url)
}

/// Whether this computer needs its relay socket. In no account it has one only while a phone may be pairing or
/// finishing a join through it, so an unused computer holds nothing open on the Codync relay.
pub fn wants_relay(hub: &Hub, address: &cloud::Address) -> bool {
    relay_is_open(hub, address) || joining(hub) || hub.pairing.locked().open()
}

/// The names of the phones that lose access if this computer joins `user_id`'s account, leaving out `key`, which
/// asks to bring it there.
pub fn removed_by_joining(hub: &Hub, user_id: &str, key: &str) -> Vec<String> {
    let devices = hub.store.devices().unwrap_or_else(|e| {
        tracing::warn!(error = format!("{e:#}"), "couldn't list the phones a join would remove");
        vec![]
    });
    let goes = |d: &crate::store::Device| match &d.account {
        DeviceAccount::Unrecorded => false,
        DeviceAccount::SignedOut => true,
        DeviceAccount::Of(account) => account != user_id,
    };
    devices.into_iter().filter(|d| d.key != key && goes(d)).map(|d| d.name).collect()
}

/// `joinAccount`: a paired phone asks to bring this computer into its account. The computer confirms, then signs the
/// claim the phone made; the phone, which holds the account's session, completes it.
pub async fn join_from_phone(hub: &Hub, key: &str, certificate: &Value, claim_id: &str, nonce: &str) -> Result<Value> {
    if !can_join(hub) {
        bail!("This computer can't join an account now.");
    }
    let account = certificate::check(certificate, key, now_ms())
        .map_err(|_| anyhow!("Couldn't confirm your Codync account. Sign in again."))?;
    let device = hub.store.device(key).context("this phone isn't paired with this computer")?;
    let about = Question::about(hub, key, &device.name, &device.platform, Some(&account))?;
    let answer = pairing_requests::ask(hub, Question { can_join: true, phone_only: false, ..about }).await;
    crate::analytics::join_from_phone_answered(hub, answer == Answer::Join);
    if answer != Answer::Join {
        bail!("The computer stayed out of this account.");
    }
    // The phone stays when the computer joins, even if it paired with no account.
    hub.store.set_device_account(key, &account.user_id)?;
    sign_claim(hub, claim_id, nonce, &account.user_id).await
}

/// Signs this computer into an account claim (§4.2 A) for a signed-in app to complete: the desktop app's
/// Add to account, or a phone pairing with a computer in no account or adding it later.
pub async fn sign_claim(hub: &Hub, claim_id: &str, nonce: &str, user_id: &str) -> Result<Value> {
    cloud::require_accounts(&hub.store)?;
    for (k, v) in [("claimId", claim_id), ("nonce", nonce), ("userId", user_id)] {
        // Fields are newline-separated in the signed string: a newline would forge another field.
        if v.is_empty() || v.contains('\n') {
            bail!("`{k}` is invalid");
        }
    }
    *hub.cloud.join_pending_until.locked() = now_ms() + JOIN_WAIT_MS;
    hub.cloud.relay_wake.notify_one();
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

    fn device(hub: &Hub, key: &str, account: DeviceAccount) {
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
                account,
            })
            .unwrap();
    }

    fn of(user: &str) -> DeviceAccount {
        DeviceAccount::Of(user.into())
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
        device(&hub, "a", of("user_a"));
        device(&hub, "b", of("user_b"));
        device(&hub, "legacy", DeviceAccount::Unrecorded);
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
        device(&hub, "legacy", DeviceAccount::Unrecorded);
        device(&hub, "mine", of("user_a"));
        device(&hub, "theirs", of("user_b"));
        remember(&hub, Some(&account("user_a"))).unwrap();
        assert_eq!(keys(&hub), ["legacy", "mine"]);
    }

    #[test]
    fn phones_with_no_account_go_when_the_computer_joins_one() {
        let hub = hub();
        remember(&hub, None).unwrap();
        device(&hub, "signed-out", DeviceAccount::SignedOut);
        device(&hub, "legacy", DeviceAccount::Unrecorded);
        device(&hub, "mine", of("user_a"));
        remember(&hub, None).unwrap();
        assert_eq!(keys(&hub), ["signed-out", "legacy", "mine"], "they stay while it is in no account");
        remember(&hub, Some(&account("user_a"))).unwrap();
        assert_eq!(keys(&hub), ["legacy", "mine"]);
    }

    #[test]
    fn a_first_record_in_an_account_also_removes_phones_with_no_account() {
        let hub = hub();
        device(&hub, "signed-out", DeviceAccount::SignedOut);
        device(&hub, "legacy", DeviceAccount::Unrecorded);
        remember(&hub, Some(&account("user_a"))).unwrap();
        assert_eq!(keys(&hub), ["legacy"]);
    }

    #[test]
    fn a_join_warns_about_the_phones_it_removes() {
        let hub = hub();
        device(&hub, "signed-out", DeviceAccount::SignedOut);
        device(&hub, "legacy", DeviceAccount::Unrecorded);
        device(&hub, "mine", of("user_a"));
        device(&hub, "theirs", of("user_b"));
        let removed = |key| removed_by_joining(&hub, "user_a", key).len();
        assert_eq!(removed(""), 2, "the signed-out phone and the other account's");
        assert_eq!(removed("signed-out"), 1, "not the phone that asks");
    }

    #[test]
    fn phones_get_the_relay_address_only_for_a_computer_in_an_account() {
        let hub = hub();
        device(&hub, "mine", of("user_a"));
        device(&hub, "signed-out", DeviceAccount::SignedOut);
        assert!(can_join(&hub));
        assert_eq!(url_for_phones(&hub, "mine"), None);
        *hub.cloud.join_pending_until.locked() = now_ms() + 60_000;
        assert!(url_for_phones(&hub, "mine").is_some(), "the phone finishing a join goes on through the relay");
        assert_eq!(url_for_phones(&hub, "signed-out"), None, "a signed-out phone couldn't use it");
        remember(&hub, Some(&account("user_a"))).unwrap();
        assert!(!can_join(&hub));
        assert!(url_for_phones(&hub, "mine").is_some());
        remember(&hub, None).unwrap();
        device(&hub, "mine", of("user_a"));
        assert_eq!(url_for_phones(&hub, "mine"), None, "the claim ended with the join, so leaving closes the relay");
        hub.store.kv_set("test_cloud_token", "relay-token").unwrap();
        assert!(url_for_phones(&hub, "local").is_some(), "a private relay is the user's own");
        assert!(!can_join(&hub), "and has no accounts to join");
    }

    #[test]
    fn a_computer_in_no_account_holds_the_relay_only_for_a_pairing_or_a_join() {
        let hub = hub();
        let address = cloud::current(&hub.store).unwrap();
        assert!(!wants_relay(&hub, &address));
        let code = hub.pairing.locked().issue().code;
        assert!(wants_relay(&hub, &address), "a phone may pair through it");
        assert!(hub.pairing.locked().redeem(&code));
        assert!(!wants_relay(&hub, &address));
        *hub.cloud.join_pending_until.locked() = now_ms() + 60_000;
        assert!(wants_relay(&hub, &address), "the cloud says over it that the claim completed");
        remember(&hub, Some(&account("user_a"))).unwrap();
        assert!(wants_relay(&hub, &address));
        remember(&hub, None).unwrap();
        assert!(!wants_relay(&hub, &address), "the claim ended with the join");
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
