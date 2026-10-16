use super::*;
use crate::remote::channel::subscriptions::send_screen_event;

#[tokio::test]
async fn screen_candidate_rpc_and_private_subscription_require_screen_scope() {
    let hub = temp_hub();
    let key = paired_phone(&hub).await;
    let dk = crypto::b64(key.verifying_key().as_bytes());
    let mut device = hub.store.device(&dk).unwrap();
    device.scopes = vec![Scope::Control];
    hub.store.put_device(&device).unwrap();
    let mut phone = Phone::direct(&hub, key.clone());
    let welcome = phone.hello(&hub, false).await;
    assert!(matches!(welcome, Out::Msg(v) if v["t"] == "welcome"));
    let reply = phone
        .call(1, "screenCandidate", json!({"session": "someone-elses-session", "candidate": {"type": "complete"}}))
        .await;
    assert_eq!(reply["err"]["status"], 403);
    phone.send(json!({"id": 2, "sub": "screenCandidates", "b": {"session": "someone-elses-session"}})).await;
    let reply = phone.recv().await;
    assert_eq!(reply.unwrap()["err"]["status"], 403);
    let reply = phone.call(3, "hello", json!({})).await;
    assert!(reply["ok"].is_object(), "denied screen access doesn't end authorized chat access");
}

#[tokio::test]
async fn queued_screen_candidates_stop_when_screen_permission_is_removed() {
    let hub = temp_hub();
    let key = paired_phone(&hub).await;
    let owner = crypto::b64(key.verifying_key().as_bytes());
    let mut auth = hub.auth.subscribe();
    let (tx, mut rx) = mpsc::channel(1);
    let filled = tx.send(ToDevice::Inner(json!({"filler": true}))).await;
    filled.unwrap();
    let candidate = json!({"id": 1, "ev": {"type": "candidate"}});
    let sending = send_screen_event(&hub, &owner, &mut auth, &tx, candidate.clone());
    tokio::pin!(sending);
    assert!(futures::poll!(&mut sending).is_pending());

    let mut device = hub.store.device(&owner).unwrap();
    device.scopes = vec![Scope::Control];
    hub.store.put_device(&device).unwrap();
    hub.auth_changed();
    let sent = timeout(Duration::from_secs(1), sending).await;
    assert!(!sent.unwrap(), "revocation cancels a send without waiting for queue space");
    let queued = rx.recv().await;
    assert!(matches!(queued, Some(ToDevice::Inner(v)) if v["filler"] == true));
    assert!(rx.try_recv().is_err());

    // Also reject a stale caller whose scope changed before its task began.
    let mut auth = hub.auth.subscribe();
    let sent = send_screen_event(&hub, &owner, &mut auth, &tx, candidate).await;
    assert!(!sent);
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn unrelated_permission_updates_preserve_a_backpressured_screen_candidate() {
    let hub = temp_hub();
    let key = paired_phone(&hub).await;
    let owner = crypto::b64(key.verifying_key().as_bytes());
    let mut auth = hub.auth.subscribe();
    let (tx, mut rx) = mpsc::channel(1);
    let filled = tx.send(ToDevice::Inner(json!({"filler": true}))).await;
    filled.unwrap();
    let candidate = json!({"id": 1, "ev": {"type": "candidate"}});
    let sending = send_screen_event(&hub, &owner, &mut auth, &tx, candidate.clone());
    tokio::pin!(sending);
    assert!(futures::poll!(&mut sending).is_pending());
    hub.auth_changed();
    assert!(futures::poll!(&mut sending).is_pending());
    let _ = rx.recv().await;
    let sent = sending.await;
    assert!(sent);
    let queued = rx.recv().await;
    assert!(matches!(queued, Some(ToDevice::Inner(v)) if v == candidate));
}
