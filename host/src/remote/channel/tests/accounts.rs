//! Pairing proves the phone's account (docs/plans/require-sign-in.md, phase 2).

use super::*;

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

#[tokio::test]
async fn an_app_without_a_certificate_is_told_to_update_and_the_code_stays() {
    let hub = temp_hub();
    let (mut p, code) = pairing_phone(&hub).await;
    p.send(json!({"id": 1, "m": "pair", "b": {"code": code, "name": "Old app", "platform": "ios"}})).await;
    let (close, reject) = p.recv().await.unwrap_err();
    assert_eq!((close, reject["code"].as_str()), (4400, Some("unsupportedVersion")));
    assert!(hub.pairing.locked().open(), "the code isn't used up");
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
        crate::remote::pairing_requests::decide(&hub, request["requestId"].as_str().unwrap(), approve).unwrap();
        let reply = p.recv().await.unwrap();
        if approve {
            assert_eq!(reply["ok"]["computerId"], hub.identity.computer_id());
            assert_eq!(hub.store.device(&p.dk()).unwrap().account.as_deref(), Some("user_2"));
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
    crate::remote::pairing_requests::decide(&hub, request["requestId"].as_str().unwrap(), true).unwrap();
    let reply = p.recv().await.unwrap();
    assert_eq!(reply["err"]["status"], 403);
    assert!(reply["err"]["message"].as_str().unwrap().contains("a•••@example.com's account"));
    assert!(hub.store.device(&p.dk()).is_none(), "user_2's phone stays out of user_3's computer");
    assert!(hub.pairing.locked().valid(&code), "a refused phone leaves the code");
}
