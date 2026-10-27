//! The visible meter follows the agent's main session across updates, compaction, reset and restart.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[allow(dead_code)]
mod common;
use common::host::Host;
use serde_json::{Value, json};
use std::time::Duration;

async fn reading(host: &Host, bot: &str) -> Value {
    common::bot(&host.snapshot().await, bot)["contextUsage"].clone()
}

#[tokio::test]
async fn usage_reaches_sync_and_live_events_and_survives_host_restart() {
    let mut host = Host::start().await;
    let bot = host.fixture_bot("Context", "instructions_agent.py", "claude").await;
    assert!(reading(&host, &bot).await.is_null());
    let mut events = host.events().await;
    host.reply(&bot, "USAGE 50000 SIZE 200000").await;
    let expected = json!({"usedTokens": 50_000, "maxTokens": 200_000, "percent": 25, "level": "normal"});
    assert_eq!(reading(&host, &bot).await, expected);
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(event) = events.recv().await {
            if event["bot"]["id"] == bot && event["bot"]["contextUsage"] == expected {
                return;
            }
        }
        panic!("no live context update");
    })
    .await
    .expect("live update");
    host.restart(|_| {}).await;
    assert_eq!(reading(&host, &bot).await, expected);
    host.call("newSession", json!({"botId": bot})).await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while !reading(&host, &bot).await.is_null() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("reset clears the previous session");
}

#[tokio::test]
async fn compaction_lowers_the_meter_and_a_missing_capacity_is_unknown() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Context", "instructions_agent.py", "reporting").await;
    host.reply(&bot, "USAGE 50000 SIZE 200000").await;
    assert_eq!(reading(&host, &bot).await["percent"], 25);
    host.reply(&bot, "COMPACT_NOW").await;
    assert_eq!(
        reading(&host, &bot).await,
        json!({"usedTokens": 20_000, "maxTokens": 1_000_000, "percent": 2, "level": "normal"})
    );
    host.reply(&bot, "USAGE 0 SIZE 200000").await;
    assert_eq!(reading(&host, &bot).await["percent"], 0);
    host.reply(&bot, "USAGE 100 SIZE 0").await;
    assert!(reading(&host, &bot).await.is_null());
}

#[tokio::test]
async fn an_agent_that_cannot_resume_does_not_reuse_an_old_reading_with_its_session_id() {
    let mut host = Host::start().await;
    let bot = host.fixture_bot("Fresh context", "instructions_agent.py", "reporting").await;
    host.reply(&bot, "USAGE 50000 SIZE 200000").await;
    assert_eq!(reading(&host, &bot).await["percent"], 25);
    host.restart(|_| {}).await;
    host.reply(&bot, "hello after restart").await;
    // This fixture starts session IDs at s1 in every process and cannot load the old one.
    assert!(reading(&host, &bot).await.is_null());
}

#[tokio::test]
async fn routine_usage_does_not_replace_the_main_session_reading() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Routine context", "instructions_agent.py", "reporting").await;
    host.reply(&bot, "USAGE 50000 SIZE 200000").await;
    let expected = reading(&host, &bot).await;
    let routine = host
        .call(
            "saveRoutine",
            json!({"botId": bot, "name": "Context check",
        "instruction": "USAGE 1000 SIZE 5000", "triggers": [{"type": "webhook"}]}),
        )
        .await;
    host.call("runRoutine", json!({"botId": bot, "id": routine["routine"]["id"]})).await;
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let runs = host.call("routines", json!({"botId": bot})).await;
            if runs["runs"].as_array().unwrap().iter().any(|r| r["status"] == "succeeded") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("routine completes");
    assert_eq!(reading(&host, &bot).await, expected);
}
