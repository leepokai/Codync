//! A real host and a scripted agent that runs out of steps: the turn is shown as paused, not finished, and one tap on
//! its card continues it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::process::Command;
use std::time::{Duration, Instant};

async fn paused_card(host: &Host, bot: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let history = host.call("history", json!({"botId": bot})).await;
        if let Some(card) =
            history["entries"].as_array().unwrap().iter().rev().find(|e| e["data"]["pausedTurn"].is_object())
        {
            return card.clone();
        }
        assert!(Instant::now() < deadline, "no paused card: {history}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The final reply of the bot that contains `needle`.
async fn final_reply(host: &Host, bot: &str, needle: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let history = host.call("history", json!({"botId": bot})).await;
        let found = history["entries"].as_array().unwrap().iter().find(|e| {
            e["kind"] == "agent"
                && e["data"]["final"] == true
                && e["data"]["text"].as_str().is_some_and(|t| t.contains(needle))
        });
        if let Some(reply) = found {
            return reply.clone();
        }
        assert!(Instant::now() < deadline, "no reply with {needle:?}: {history}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn a_turn_that_runs_out_of_steps_is_paused_and_can_be_continued_once() {
    if Command::new(common::python()).arg("--version").output().is_err() {
        eprintln!("skipping: Python not available");
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Worker", "pause_agent.py", "").await;

    host.call("send", json!({"botId": bot, "text": "do the big job", "clientNonce": "n1"})).await;
    let card = paused_card(&host, &bot).await;
    assert_eq!(card["data"]["pausedTurn"]["verdict"], "paused");
    assert_eq!(card["data"]["pausedTurn"]["status"], "waiting");

    // The user's message carries a result that says paused, not done.
    let history = host.call("history", json!({"botId": bot})).await;
    let user = history["entries"].as_array().unwrap().iter().find(|e| e["kind"] == "user").unwrap();
    assert_eq!(user["data"]["turnResult"]["state"], "paused", "{user}");

    host.call("continueTurn", json!({"entryId": card["id"]})).await;
    let reply = final_reply(&host, &bot, "All done now.").await;
    assert_eq!(reply["data"]["final"], true);

    // The card is used up: a second tap is refused.
    let error = host.refused("continueTurn", json!({"entryId": card["id"]})).await;
    assert!(error.contains("already"), "{error}");
}
