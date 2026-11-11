//! A queued turn must retain its pairing identity even while another turn awaits approval.

use super::*;
use crate::api::devices::Caller;

#[tokio::test]
async fn hermes_telegram_queued_input_never_runs_after_revoke_and_reauthorize() {
    let fixture = Fixture::new();
    let fake = fake_telegram().await;
    let bot = fixture.asking_agent("Runner");
    fixture.add(&fake, &bot).await;
    let receiver = serve(&fixture, &fake);
    fixture.pair(&fake, HOME).await;
    // command_agent.py only requests approval and reports the answer; it executes no command.
    fake.push(fixture.message(HOME, "run:fixture-only"));
    until("the first turn's approval", || {
        fixture
            .hub
            .store
            .history(&bot, i64::MAX, 100)
            .unwrap()
            .iter()
            .any(|entry| entry.kind == "permission" && entry.data["status"] == "pending")
    })
    .await;
    fake.push(fixture.message(HOME, "must not reach a model after revocation"));
    until("the second turn to be queued", || {
        fixture.hub.store.history(&bot, i64::MAX, 100).unwrap().iter().any(|entry| {
            entry.kind == "user"
                && entry.data["text"] == "must not reach a model after revocation"
                && entry.data["status"] == "queued"
                && entry.data["source"] == "telegram"
        })
    })
    .await;
    for allowed in [false, true] {
        call_with(
            &fixture.hub,
            &fake.endpoint,
            "setTelegramAllowed",
            json!({"name":"home","userId":HOME,"allowed":allowed}),
        )
        .await
        .unwrap();
    }
    let card = fixture
        .hub
        .store
        .history(&bot, i64::MAX, 100)
        .unwrap()
        .into_iter()
        .find(|entry| entry.kind == "permission" && entry.data["status"] == "pending")
        .unwrap();
    crate::api::dispatch(
        &fixture.hub,
        &Caller::Local,
        "respondPermission",
        json!({"entryId":card.id,"optionId":"yes"}),
    )
    .await
    .unwrap();
    until("the revoked queued message to be cancelled", || {
        fixture.hub.store.history(&bot, i64::MAX, 100).unwrap().iter().any(|entry| {
            entry.kind == "user"
                && entry.data["text"] == "must not reach a model after revocation"
                && entry.data["status"] == "cancelled"
        })
    })
    .await;
    assert_eq!(
        fixture
            .hub
            .store
            .history(&bot, i64::MAX, 100)
            .unwrap()
            .iter()
            .filter(|entry| entry.kind == "permission")
            .count(),
        1,
        "the revoked message must not cause a second model turn"
    );
    receiver.abort();
}
