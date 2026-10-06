use super::super::requests::MAX_PENDING_MESSAGES_PER_SENDER;
use super::*;

#[tokio::test]
async fn sender_quota_survives_sender_stop_and_frees_on_recipient_stop() {
    let f = Fixture::new();
    f.send_user("BLOCK user work").await;
    f.until(|| f.prompts().len() == 1).await;
    for _ in 0..MAX_PENDING_MESSAGES_PER_SENDER {
        let result = f.message("queued message").await;
        assert!(result.is_ok());
    }
    let result = f.message("overflow").await;
    assert!(result.unwrap_err().to_string().contains("from this bot"));
    assert!(f.hub.team.0.locked().pending.is_empty());
    f.hub.send_cmd("a", Cmd::Stop).unwrap();
    f.until(|| !f.hub.team.0.locked().active.contains_key("a")).await;
    f.hub.team.start_turn("a", None);
    let result = f.message("a new sender turn cannot bypass its quota").await;
    assert!(result.unwrap_err().to_string().contains("from this bot"));
    f.hub.set_runtime("c", |r| r.status = BotStatus::Working);
    f.hub.team.start_turn("c", None);
    let result = call(&f.hub, "c", "message_bot", &json!({"botId": "b", "message": "another sender"})).await;
    assert!(result.is_ok());
    f.hub.send_cmd("b", Cmd::Stop).unwrap();
    f.until(|| f.hub.team.0.locked().messages == 0 && f.hub.runtime("b").status == BotStatus::Idle).await;
    assert!(f.hub.team.0.locked().messages_by_sender.is_empty());
    let result = f.message("after capacity released").await;
    assert!(result.is_ok());
    f.until(|| f.hub.team.0.locked().messages == 0).await;
    assert_eq!(f.prompts().len(), 2);
    f.shutdown().await;
}

#[tokio::test]
async fn running_messages_hold_sender_slots_until_recipient_completion() {
    let f = Fixture::new();
    let result = f.message("BLOCK active message").await;
    assert!(result.is_ok());
    f.until(|| f.prompts().len() == 1).await;
    for _ in 1..MAX_PENDING_MESSAGES_PER_SENDER {
        let result = f.message("queued message").await;
        assert!(result.is_ok());
    }
    let result = f.message("running plus queued reaches the quota").await;
    assert!(result.unwrap_err().to_string().contains("from this bot"));
    f.hub.delete_bot("a").unwrap();
    assert_eq!(f.hub.team.0.locked().messages_by_sender["a"], MAX_PENDING_MESSAGES_PER_SENDER);
    std::fs::write(f.dir.join("b/release"), "").unwrap();
    f.until(|| f.hub.team.0.locked().messages == 0).await;
    assert!(f.hub.team.0.locked().messages_by_sender.is_empty());
    assert_eq!(f.prompts().len(), MAX_PENDING_MESSAGES_PER_SENDER);
    f.shutdown().await;
}

#[tokio::test]
async fn failed_admission_and_recipient_failure_return_sender_slots() {
    let f = Fixture::new();
    let db = rusqlite::Connection::open(f.dir.join("test.db")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER reject_message BEFORE INSERT ON entries
        WHEN NEW.bot_id = 'b' AND NEW.kind = 'notice'
        BEGIN SELECT RAISE(ABORT, 'fixture save failure'); END;",
    )
    .unwrap();
    let result = f.message("cannot persist recipient notice").await;
    assert!(result.is_err());
    assert!(f.hub.team.0.locked().messages_by_sender.is_empty());
    assert_eq!(f.hub.team.0.locked().messages, 0);
    db.execute_batch("DROP TRIGGER reject_message;").unwrap();
    let result = f.message("FAIL recipient prompt").await;
    let receipt = result.unwrap();
    f.until(|| f.hub.team.0.locked().messages == 0).await;
    assert!(f.notices(&receipt).iter().all(|e| e.data["status"] == "failed"));
    assert!(f.hub.team.0.locked().messages_by_sender.is_empty());
    drop(db);
    f.shutdown().await;
}
