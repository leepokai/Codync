//! The fork's synchronous tools coexist with upstream's deferred reply turns.

use super::*;
use crate::store::runtime::{AssignmentOutcome, AssignmentStatus};

#[test]
fn task_deadlines_are_explicit_bounded_integers() {
    assert_eq!(blocking::task_timeout(&json!({})).unwrap(), ASK_TIMEOUT);
    assert_eq!(blocking::task_timeout(&json!({"timeoutSeconds":1})).unwrap(), Duration::from_secs(1));
    assert_eq!(blocking::task_timeout(&json!({"timeoutSeconds":600})).unwrap(), ASK_TIMEOUT);
    for value in [json!(0), json!(-1), json!(601), json!(1.5), json!(null), json!("30")] {
        assert!(blocking::task_timeout(&json!({"timeoutSeconds":value})).is_err());
    }
}

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
async fn invalid_later_deadlines_do_not_start_earlier_batch_tasks_or_pipeline_stages() {
    let f = Fixture::new();
    for (name, key) in [("delegate_batch", "tasks"), ("run_pipeline", "stages")] {
        let mut args = json!({});
        args[key] = json!([
            {"botId":"b","goal":"must not start","timeoutSeconds":5},
            {"botId":"c","goal":"invalid deadline","timeoutSeconds":0}
        ]);
        let error = call_async(&f.hub, "a", name, &args).await.unwrap_err();
        assert!(error.to_string().contains("task 2: timeoutSeconds"));
        assert_eq!(f.prompts().len(), 0);
        assert!(f.hub.team.0.locked().pending.is_empty());
    }
    f.shutdown().await;
}

#[tokio::test]
async fn a_batch_deadline_cancels_its_session_without_cancelling_another_task() {
    let f = Fixture::new();
    call_async(&f.hub, "a", "ask_bot", &json!({"botId":"b","message":"warm up"})).await.unwrap();
    call_async(&f.hub, "a", "ask_bot", &json!({"botId":"c","message":"warm up"})).await.unwrap();
    let args = json!({"tasks":[
        {"botId":"b","goal":"BLOCK slow task","timeoutSeconds":1},
        {"botId":"c","goal":"independent task","timeoutSeconds":30}
    ]});
    let result = tokio::time::timeout(Duration::from_secs(40), call_async(&f.hub, "a", "delegate_batch", &args))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result["tasks"][0]["status"], "failed");
    assert!(result["tasks"][0]["error"].as_str().unwrap().contains("timed out"));
    assert_eq!(result["tasks"][1]["status"], "replied");
    f.until(|| f.idle("b") && f.idle("c")).await;
    assert!(f.dir.join("b/cancel-received").exists(), "deadline must reach the execution owner before disconnect");
    assert!(!f.dir.join("c/cancel-received").exists(), "a settled peer must not be interrupted");
    assert!(f.hub.team.0.locked().pending.is_empty());
    f.shutdown().await;
}

#[tokio::test]
async fn a_pipeline_deadline_cancels_its_session_and_does_not_start_the_next_stage() {
    let f = Fixture::new();
    call_async(&f.hub, "a", "ask_bot", &json!({"botId":"b","message":"warm up"})).await.unwrap();
    let args = json!({"stages":[
        {"botId":"b","goal":"BLOCK first stage","timeoutSeconds":1},
        {"botId":"c","goal":"must never start","timeoutSeconds":30}
    ]});
    let result = tokio::time::timeout(Duration::from_secs(10), call_async(&f.hub, "a", "run_pipeline", &args))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result["stoppedAt"], 1);
    assert_eq!(result["stages"][0]["status"], "failed");
    assert_eq!(result["stages"][1]["status"], "skipped");
    f.until(|| f.idle("b")).await;
    assert!(f.dir.join("b/cancel-received").exists());
    assert_eq!(f.prompts_of("c").len(), 0);
    assert!(f.hub.team.0.locked().pending.is_empty());
    f.shutdown().await;
}

#[tokio::test]
async fn expiry_during_session_setup_never_prompts_or_marks_the_recipient_as_broken() {
    let f = Fixture::new();
    call_async(&f.hub, "a", "ask_bot", &json!({"botId":"b","message":"warm up"})).await.unwrap();
    f.until(|| f.idle("b")).await;
    let before = f.prompts_of("b");
    std::fs::write(f.dir.join("b/delay-session"), "").unwrap();
    f.hub.send_cmd("b", Cmd::NewSession).unwrap();
    let result = call_async(
        &f.hub,
        "a",
        "ask_bot",
        &json!({
            "botId":"b","message":"must not execute after setup","timeoutSeconds":1
        }),
    )
    .await;
    assert!(result.unwrap_err().to_string().contains("timed out"));
    f.until(|| f.idle("b")).await;
    assert_eq!(f.prompts_of("b"), before);
    assert!(f.hub.team.0.locked().pending.is_empty());
    let session = f.hub.store.bot("b").unwrap().unwrap().session_id.unwrap();
    let config = visible_bot(&f.hub, "b").unwrap();
    assert!(crate::chat::context::resolve(&f.hub.store, &config, &session).unwrap().unprompted);
    call_async(&f.hub, "a", "ask_bot", &json!({"botId":"b","message":"real follow-up"})).await.unwrap();
    f.until(|| f.idle("b")).await;
    assert_eq!(f.prompts_of("b").len(), before.len() + 1);
    assert!(!crate::chat::context::resolve(&f.hub.store, &config, &session).unwrap().unprompted);
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
    let result =
        call_async(&f.hub, "a", "ask_bot", &json!({"botId":"b","message":"expired ask","timeoutSeconds":1})).await;
    assert!(result.unwrap_err().to_string().contains("timed out"));
    f.send_user("unrelated follow-up").await;
    std::fs::write(f.dir.join("b/release"), "").unwrap();
    f.until(|| f.took("b", 2)).await;
    assert_eq!(f.prompts()[1], "unrelated follow-up");
    assert!(f.hub.team.0.locked().pending.is_empty());
    f.shutdown().await;
}
