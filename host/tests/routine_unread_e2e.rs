//! Routine results and subsequent replies each count once in the main chat.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::time::Duration;

async fn unread(host: &Host, bot: &str) -> Value {
    common::bot(&host.snapshot().await, bot)["unread"].clone()
}

async fn wait_for_run_cleanup(host: &Host, bot: &str, routine: &str) {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let listing = host.call("routines", json!({"botId": bot})).await;
            if listing["runs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["routineId"] == routine && r["status"] == "succeeded")
                && common::routine_sessions(&host.home) == 0
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("routine must finish and release its session before the follow-up");
}

#[tokio::test]
async fn a_routine_result_and_a_followup_each_become_read_without_hidden_unreads() {
    let host = Host::start().await;
    let bot = host.fixture_bot("routine-unread", "team_agent.py", "").await;
    let saved = host
        .call(
            "saveRoutine",
            json!({"botId": bot, "name": "Status", "instruction": "Report status", "triggers": [{"type": "webhook"}]}),
        )
        .await;
    let routine = saved["routine"]["id"].as_str().unwrap();
    host.call("runRoutine", json!({"botId": bot, "id": routine})).await;
    wait_for_run_cleanup(&host, &bot, routine).await;
    let history = host.call("history", json!({"botId": bot})).await;
    let results: Vec<_> = history["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "agent" && e["data"]["final"] == true)
        .collect();
    assert_eq!(results.len(), 1, "a routine has one visible result, without a second published copy");
    assert_eq!(unread(&host, &bot).await, 1);
    host.call("markRead", json!({"botId": bot})).await;
    assert_eq!(unread(&host, &bot).await, 0);

    // `reply` returns once the reply is final; that a streaming one isn't unread yet is
    // `store::bots`' `rev_sync_and_unread`.
    host.reply(&bot, "FOLLOWUP").await;
    assert_eq!(unread(&host, &bot).await, 1);
    host.call("markRead", json!({"botId": bot})).await;
    assert_eq!(unread(&host, &bot).await, 0);
}
