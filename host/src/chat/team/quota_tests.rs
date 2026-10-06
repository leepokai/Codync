use super::*;

#[tokio::test]
async fn message_capacity_is_bounded_and_freed_by_recipient_stop() {
    let f = Fixture::new();
    f.send_user("BLOCK user work").await;
    f.until(|| f.prompts().len() == 1).await;
    for _ in 0..MAX_PENDING_MESSAGES {
        let result = f.message("queued message").await;
        assert!(result.is_ok());
    }
    let result = f.message("overflow").await;
    assert!(result.unwrap_err().to_string().contains("too many pending bot messages"));
    assert!(f.hub.team.0.locked().pending.is_empty());
    f.hub.send_cmd("b", Cmd::Stop).unwrap();
    f.until(|| f.hub.team.0.locked().messages == 0 && f.hub.runtime("b").status == BotStatus::Idle).await;
    let result = f.message("after capacity released").await;
    assert!(result.is_ok());
    f.until(|| f.hub.team.0.locked().messages == 0).await;
    assert_eq!(f.prompts().len(), 2);
    f.shutdown().await;
}
