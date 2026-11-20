//! Pairing proves the phone's account (docs/plans/require-sign-in.md, phase 2).

use super::*;
use crate::store::DeviceAccount;

async fn pairing_phone(hub: &Arc<Hub>) -> (Phone, String) {
    let code = hub.pairing.locked().issue().code;
    let mut p = Phone::direct(hub, SigningKey::from_bytes(&crypto::random()));
    assert!(matches!(p.hello(hub, true).await, Out::Msg(v) if v["t"] == "welcome"));
    (p, code)
}

/// Sends `pair` to a computer in no account and waits for it to ask: the question on its screens.
async fn asked(hub: &Hub, p: &mut Phone, body: Value) -> Value {
    p.send(json!({"id": 1, "m": "pair", "b": body})).await;
    while hub.pairing_requests.json().is_empty() {
        tokio::task::yield_now().await;
    }
    hub.pairing_requests.json()[0].clone()
}

fn signed_out_body(code: &str) -> Value {
    json!({"code": code, "name": "Pixel", "platform": "android"})
}

fn decide(hub: &Hub, request: &Value, approve: bool, join: Option<bool>) {
    crate::remote::pairing_requests::decide(hub, request["requestId"].as_str().unwrap(), approve, join).unwrap();
}

#[tokio::test]
async fn a_computer_in_an_account_turns_away_a_signed_out_phone_and_the_code_stays() {
    let hub = temp_hub();
    let (mut p, code) = pairing_phone(&hub).await;
    let reply = p.call(1, "pair", signed_out_body(&code)).await;
    assert_eq!(reply["err"]["status"], 403);
    assert!(reply["err"]["message"].as_str().unwrap().contains("Sign in to that account"));
    assert!(hub.store.device(&p.dk()).is_none());
    assert!(hub.pairing.locked().open(), "the code isn't used up");
}

#[tokio::test]
async fn a_computer_in_no_account_asks_about_a_signed_out_phone_and_never_joins() {
    let hub = unowned_hub();
    hub.store.kv_set("test_cloud_url", "https://dev-api.codync.dev").unwrap();
    let (mut p, code) = pairing_phone(&hub).await;
    let mut body = signed_out_body(&code);
    body["claim"] = json!({"claimId": "clm_1", "nonce": "n1"});
    let request = asked(&hub, &mut p, body).await;
    assert_eq!((request["canJoin"].as_bool(), request["phoneOnly"].as_bool()), (Some(false), Some(true)));
    decide(&hub, &request, true, None);
    let reply = p.recv().await.unwrap();
    assert_eq!(reply["ok"]["computerId"], hub.identity.computer_id());
    assert!(reply["ok"]["claim"].is_null(), "a phone with no account brings no join");
    let device = hub.store.device(&p.dk()).unwrap();
    assert_eq!(device.account, DeviceAccount::SignedOut);

    let mut p = Phone::direct(&hub, p.key.clone());
    assert!(matches!(p.hello(&hub, false).await, Out::Msg(v) if v["t"] == "welcome"));
    assert!(p.call(1, "hello", json!({})).await["ok"]["cloud"].is_null(), "phones go over Wi-Fi or Tailscale");
}

#[tokio::test]
async fn a_signed_out_phone_cant_pair_over_the_codync_relay() {
    let hub = unowned_hub();
    hub.store.kv_set("test_cloud_url", "https://dev-api.codync.dev").unwrap();
    let code = hub.pairing.locked().issue().code;
    let key = SigningKey::from_bytes(&crypto::random());
    let dk = crypto::b64(key.verifying_key().as_bytes());
    let mut p = Phone::connect(&hub, key, ChannelKind::Relay { link: "p".into(), dk, pair: true });
    p.hello(&hub, true).await;
    let reply = p.call(1, "pair", signed_out_body(&code)).await;
    assert_eq!(reply["err"]["status"], 403);
    assert!(hub.pairing_requests.json().is_empty(), "the computer isn't even asked");
}

#[tokio::test]
async fn a_signed_in_phone_on_the_relay_is_let_in_only_if_the_computer_joins() {
    let hub = unowned_hub();
    hub.store.kv_set("test_cloud_url", "https://dev-api.codync.dev").unwrap();
    let code = hub.pairing.locked().issue().code;
    let key = SigningKey::from_bytes(&crypto::random());
    let dk = crypto::b64(key.verifying_key().as_bytes());
    let mut p = Phone::connect(&hub, key, ChannelKind::Relay { link: "p".into(), dk, pair: true });
    p.hello(&hub, true).await;
    let mut body = pair_body(&p, &code, "Pixel", "android", "user_2");
    body["claim"] = json!({"claimId": "clm_1", "nonce": "n1"});
    let request = asked(&hub, &mut p, body).await;
    assert_eq!((request["canJoin"].as_bool(), request["phoneOnly"].as_bool()), (Some(true), Some(false)));
    decide(&hub, &request, true, None);
    assert_eq!(p.recv().await.unwrap()["ok"]["claim"]["computerId"], hub.identity.computer_id());
}

#[tokio::test]
async fn a_signed_in_phone_on_the_relay_without_a_claim_tries_again_and_the_code_stays() {
    let hub = unowned_hub();
    hub.store.kv_set("test_cloud_url", "https://dev-api.codync.dev").unwrap();
    let code = hub.pairing.locked().issue().code;
    let key = SigningKey::from_bytes(&crypto::random());
    let dk = crypto::b64(key.verifying_key().as_bytes());
    let mut p = Phone::connect(&hub, key, ChannelKind::Relay { link: "p".into(), dk, pair: true });
    p.hello(&hub, true).await;
    let reply = p.call(1, "pair", pair_body(&p, &code, "Pixel", "android", "user_2")).await;
    assert_eq!(reply["err"]["status"], 403);
    assert!(hub.pairing_requests.json().is_empty(), "the computer isn't asked about a join that can't happen");
    assert!(hub.pairing.locked().valid(&code));
}

#[tokio::test]
async fn without_a_claim_a_direct_phone_is_only_let_in_and_can_add_the_computer_later() {
    let hub = unowned_hub();
    hub.store.kv_set("test_cloud_url", "https://dev-api.codync.dev").unwrap();
    let (mut p, code) = pairing_phone(&hub).await;
    let body = pair_body(&p, &code, "Pixel", "android", "user_2");
    let request = asked(&hub, &mut p, body).await;
    assert_eq!((request["canJoin"].as_bool(), request["phoneOnly"].as_bool()), (Some(false), Some(true)));
    decide(&hub, &request, true, None);
    assert!(p.recv().await.unwrap()["ok"].is_object());
}

#[tokio::test]
async fn a_claim_that_cant_be_signed_leaves_the_phone_out_and_the_code_valid() {
    let hub = unowned_hub();
    hub.store.kv_set("test_cloud_url", "https://dev-api.codync.dev").unwrap();
    let (mut p, code) = pairing_phone(&hub).await;
    let mut body = pair_body(&p, &code, "Pixel", "android", "user_2");
    body["claim"] = json!({"claimId": "clm\n1", "nonce": "n1"});
    let request = asked(&hub, &mut p, body).await;
    decide(&hub, &request, true, None);
    assert_eq!(p.recv().await.unwrap()["err"]["status"], 403);
    assert!(hub.store.device(&p.dk()).is_none());
    assert!(hub.pairing.locked().valid(&code));
}

#[tokio::test]
async fn a_signed_in_phone_may_stay_out_of_the_account_and_add_the_computer_later() {
    let hub = unowned_hub();
    hub.store.kv_set("test_cloud_url", "https://dev-api.codync.dev").unwrap();
    let (mut p, code) = pairing_phone(&hub).await;
    let mut body = pair_body(&p, &code, "Pixel", "android", "user_2");
    body["claim"] = json!({"claimId": "clm_1", "nonce": "n1"});
    let request = asked(&hub, &mut p, body).await;
    assert_eq!((request["canJoin"].as_bool(), request["phoneOnly"].as_bool()), (Some(true), Some(true)));
    decide(&hub, &request, true, Some(false));
    assert!(p.recv().await.unwrap()["ok"]["claim"].is_null(), "not asked to join, the claim isn't signed");
    assert_eq!(hub.store.device(&p.dk()).unwrap().account, DeviceAccount::Of("user_2".into()));

    let mut p = Phone::direct(&hub, p.key.clone());
    assert!(matches!(p.hello(&hub, false).await, Out::Msg(v) if v["t"] == "welcome"));
    let hello = p.call(1, "hello", json!({})).await;
    assert_eq!(hello["ok"]["canJoinAccount"], true);
    assert!(hello["ok"]["cloud"].is_null());

    // Adding it later asks the computer again; saying no signs nothing.
    let join = json!({"claimId": "clm_2", "nonce": "n2",
                      "certificate": crate::remote::certificate::test_certificate(&p.dk(), "user_2")});
    p.send(json!({"id": 2, "m": "joinAccount", "b": join})).await;
    while hub.pairing_requests.json().is_empty() {
        tokio::task::yield_now().await;
    }
    let request = hub.pairing_requests.json()[0].clone();
    assert_eq!((request["canJoin"].as_bool(), request["phoneOnly"].as_bool()), (Some(true), Some(false)));
    decide(&hub, &request, false, None);
    assert!(p.recv().await.unwrap()["err"].is_object());

    p.send(json!({"id": 3, "m": "joinAccount", "b": join})).await;
    while hub.pairing_requests.json().is_empty() {
        tokio::task::yield_now().await;
    }
    decide(&hub, &hub.pairing_requests.json()[0].clone(), true, None);
    let claim = p.recv().await.unwrap();
    let input = crypto::claim_input("clm_2", "n2", "user_2", &hub.identity.computer_id(), &hub.identity.box_pub_b64());
    let sig = crypto::unb64_n::<64>(claim["ok"]["sig"].as_str().unwrap()).unwrap();
    crypto::verify(&hub.identity.sign_pub(), input.as_bytes(), &sig).unwrap();
    assert!(p.call(4, "hello", json!({})).await["ok"]["cloud"].is_string(), "the relay is theirs once it has joined");
}

#[tokio::test]
async fn a_signed_out_phone_that_signed_in_stays_when_it_brings_the_computer_along() {
    let hub = unowned_hub();
    hub.store.kv_set("test_cloud_url", "https://dev-api.codync.dev").unwrap();
    let (mut p, code) = pairing_phone(&hub).await;
    let request = asked(&hub, &mut p, signed_out_body(&code)).await;
    decide(&hub, &request, true, None);
    p.recv().await.unwrap();
    let mut p = Phone::direct(&hub, p.key.clone());
    assert!(matches!(p.hello(&hub, false).await, Out::Msg(v) if v["t"] == "welcome"));
    let join = json!({"claimId": "clm_2", "nonce": "n2",
                      "certificate": crate::remote::certificate::test_certificate(&p.dk(), "user_2")});
    p.send(json!({"id": 1, "m": "joinAccount", "b": join})).await;
    while hub.pairing_requests.json().is_empty() {
        tokio::task::yield_now().await;
    }
    decide(&hub, &hub.pairing_requests.json()[0].clone(), true, None);
    assert!(p.recv().await.unwrap()["ok"]["sig"].is_string());
    let owner = crate::remote::cloud::Owner { user_id: "user_2".into(), email: None };
    crate::remote::owner::remember(&hub, Some(&owner)).unwrap();
    assert!(hub.store.device(&p.dk()).is_some(), "it recorded its account before the computer joined");
}

#[tokio::test]
async fn a_phone_of_another_account_is_refused_with_the_owner_masked() {
    let hub = temp_hub();
    let (mut p, code) = pairing_phone(&hub).await;
    let body = pair_body(&p, &code, "Pixel", "android", "user_2");
    let reply = p.call(1, "pair", body).await;
    assert_eq!(reply["err"]["status"], 403);
    let message = reply["err"]["message"].as_str().unwrap();
    assert!(message.contains("k•••@example.com's account") && !message.contains("kevin"), "{message}");
    assert!(hub.store.device(&p.dk()).is_none());
    assert!(hub.pairing.locked().open(), "the code isn't used up");

    let (mut p, code) = pairing_phone(&hub).await;
    let mut forged = pair_body(&p, &code, "Pixel", "android", "user_1");
    forged["certificate"] = crate::remote::certificate::test_certificate("someone-else", "user_1");
    assert_eq!(p.call(1, "pair", forged).await["err"]["status"], 403, "a certificate for another key");
}

#[tokio::test]
async fn without_a_live_code_a_phone_learns_nothing_about_the_account() {
    let hub = temp_hub();
    let (mut p, _) = pairing_phone(&hub).await;
    let reply = p.call(1, "pair", pair_body(&p, "CAgICAgICAgICAgICAgICA", "Pixel", "android", "user_2")).await;
    let message = reply["err"]["message"].as_str().unwrap();
    assert!(message.contains("expired") && !message.contains("example.com"), "{message}");
}

#[tokio::test]
async fn a_phone_that_leaves_while_the_computer_asks_is_not_let_in() {
    let hub = unowned_hub();
    let (mut p, code) = pairing_phone(&hub).await;
    let body = pair_body(&p, &code, "Pixel", "android", "user_2");
    asked(&hub, &mut p, body).await;
    // The computer still has the offer while it asks: the relay keeps the phone's socket.
    assert!(hub.pairing.locked().open());
    let dk = p.dk();
    drop(p.tx);
    p.run.await.unwrap();
    assert!(hub.pairing_requests.json().is_empty(), "the question goes with the phone");
    assert!(hub.store.device(&dk).is_none());
}

#[tokio::test]
async fn a_computer_in_no_account_asks_then_joins_the_phones() {
    let hub = unowned_hub();
    // With the Codync cloud on there is an account to join, so the computer signs the phone's claim.
    hub.store.kv_set("test_cloud_url", "https://dev-api.codync.dev").unwrap();
    for approve in [false, true] {
        let (mut p, code) = pairing_phone(&hub).await;
        let mut body = pair_body(&p, &code, "Pixel", "android", "user_2");
        body["claim"] = json!({"claimId": "clm_1", "nonce": "n1"});
        let request = asked(&hub, &mut p, body).await;
        assert_eq!(request["email"], "user_2@example.com");
        let device_key = crypto::unb64_n::<32>(&p.dk()).unwrap();
        let expected = crypto::pair_check_code(&device_key, &hub.identity.sign_pub());
        assert_eq!(request["code"], expected.as_str(), "the code the phone shows too");
        decide(&hub, &request, approve, None);
        let reply = p.recv().await.unwrap();
        if approve {
            assert_eq!(reply["ok"]["computerId"], hub.identity.computer_id());
            assert_eq!(hub.store.device(&p.dk()).unwrap().account, DeviceAccount::Of("user_2".into()));
            let claim = &reply["ok"]["claim"];
            assert_eq!(claim["computerId"], hub.identity.computer_id());
            let input =
                crypto::claim_input("clm_1", "n1", "user_2", &hub.identity.computer_id(), &hub.identity.box_pub_b64());
            let sig = crypto::unb64_n::<64>(claim["sig"].as_str().unwrap()).unwrap();
            crypto::verify(&hub.identity.sign_pub(), input.as_bytes(), &sig).unwrap();
        } else {
            assert_eq!(reply["err"]["status"], 403);
            assert!(hub.store.device(&p.dk()).is_none());
        }
    }
}

#[tokio::test]
async fn a_phone_let_in_after_the_computer_joined_another_account_stays_out() {
    let hub = unowned_hub();
    hub.store.kv_set("test_cloud_url", "https://dev-api.codync.dev").unwrap();
    let (mut p, code) = pairing_phone(&hub).await;
    let body = pair_body(&p, &code, "Pixel", "android", "user_2");
    let request = asked(&hub, &mut p, body).await;
    // While the computer asks, another phone brings it into user_3's account.
    let owner = crate::remote::cloud::Owner { user_id: "user_3".into(), email: Some("ann@example.com".into()) };
    crate::remote::owner::remember(&hub, Some(&owner)).unwrap();
    decide(&hub, &request, true, None);
    let reply = p.recv().await.unwrap();
    assert_eq!(reply["err"]["status"], 403);
    assert!(reply["err"]["message"].as_str().unwrap().contains("a•••@example.com's account"));
    assert!(hub.store.device(&p.dk()).is_none(), "user_2's phone stays out of user_3's computer");
    assert!(hub.pairing.locked().valid(&code), "a refused phone leaves the code");
}
