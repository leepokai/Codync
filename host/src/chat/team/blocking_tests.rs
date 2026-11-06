//! The fork's synchronous tools coexist with upstream's deferred reply turns.

use super::*;
use crate::store::runtime::{AssignmentOutcome, AssignmentStatus};

#[tokio::test]
async fn a_synchronous_ask_returns_its_reply_without_waking_a_sender_turn() {
    let f = Fixture::new();
    let result = call_async(&f.hub, "a", "ask_bot", &json!({"botId": "b", "message": "review"})).await.unwrap();
    assert_eq!(result["reply"], "reply: review");
    f.until(|| f.idle("b")).await;
    assert_eq!(f.prompts_of("a").len(), 0);
    assert!(f.finals("b").is_empty(), "a request's answer belongs to the sender");
    let assignment = f.hub.store.assignment(result["requestId"].as_str().unwrap()).unwrap().unwrap();
    assert_eq!(assignment.status, AssignmentStatus::Settled);
    assert_eq!(assignment.outcome, Some(AssignmentOutcome::Completed));
    assert!(f.hub.team.0.locked().pending.is_empty());
    assert_eq!(f.hub.team.0.locked().messages, 0);
    f.shutdown().await;
}

#[tokio::test]
async fn a_sender_stop_cancels_its_running_synchronous_ask() {
    let f = Fixture::new();
    let source = visible_bot(&f.hub, "a").unwrap();
    let (result, ()) = tokio::join!(ask(&f.hub, &source, "b", "BLOCK review", ASK_TIMEOUT), async {
        f.until(|| f.prompts().len() == 1).await;
        f.hub.send_cmd("a", Cmd::Stop).unwrap();
    });
    assert!(result.unwrap_err().to_string().contains("cancelled"));
    f.until(|| f.idle("b")).await;
    assert!(f.hub.team.0.locked().pending.is_empty());
    assert_eq!(f.prompts_of("a").len(), 0);
    f.shutdown().await;
}

#[tokio::test]
async fn an_expired_queued_ask_keeps_unrelated_user_messages() {
    let f = Fixture::new();
    f.send_user("BLOCK user work").await;
    f.until(|| f.prompts().len() == 1).await;
    let result = ask(&f.hub, &visible_bot(&f.hub, "a").unwrap(), "b", "expired ask", Duration::from_millis(100)).await;
    assert!(result.unwrap_err().to_string().contains("timed out"));
    f.send_user("unrelated follow-up").await;
    std::fs::write(f.dir.join("b/release"), "").unwrap();
    f.until(|| f.took("b", 2)).await;
    assert_eq!(f.prompts()[1], "unrelated follow-up");
    assert!(f.hub.team.0.locked().pending.is_empty());
    f.shutdown().await;
}
