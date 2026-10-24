//! Routine results and subsequent replies each count once in the main chat.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::time::Duration;

async fn unread(host: &Host, bot: &str) -> Value {
    host.snapshot().await["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot).unwrap()["unread"].clone()
}

async fn wait_for_run_cleanup(host: &Host, bot: &str, routine: &str) {
    let db = rusqlite::Connection::open(host.home.join("codync.db")).unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let listing = host.call("routines", json!({"botId": bot})).await;
            let sessions: i64 = db
                .query_row("SELECT COUNT(*) FROM kv WHERE k GLOB 'lane.session.*/routine-*'", [], |r| r.get(0))
                .unwrap();
            if listing["runs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["routineId"] == routine && r["status"] == "succeeded")
                && sessions == 0
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

    host.call("send", json!({"botId": bot, "text": "FOLLOWUP", "clientNonce": uuid::Uuid::new_v4().to_string()})).await;
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let history = host.call("history", json!({"botId": bot})).await;
            // Streaming the text does not make it unread until the host finalizes the reply.
            if history["entries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["kind"] == "agent" && e["data"]["text"] == "reply: FOLLOWUP" && e["data"]["final"] == true)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("follow-up must be finalized before checking its unread count");
    assert_eq!(unread(&host, &bot).await, 1);
    host.call("markRead", json!({"botId": bot})).await;
    assert_eq!(unread(&host, &bot).await, 0);
}
