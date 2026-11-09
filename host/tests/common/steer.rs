//! A real host's chat with the user's messages queued behind a running turn (`steer_agent.py`).

#![allow(dead_code)]

use super::eventually;
use super::host::Host;
use serde_json::{Value, json};
use std::time::Duration;

pub async fn entries(host: &Host, bot: &str) -> Vec<Value> {
    host.call("history", json!({"botId": bot})).await["entries"].as_array().unwrap().clone()
}

pub async fn entry(host: &Host, bot: &str, id: &str) -> Value {
    entries(host, bot).await.into_iter().find(|e| e["id"] == id).unwrap()
}

/// Whether `history` no longer lists a message: one taken back.
pub async fn gone(host: &Host, bot: &str, id: &str) -> bool {
    entries(host, bot).await.iter().all(|e| e["id"] != id)
}

/// Sends "first" and waits until its turn runs at the agent: its id.
pub async fn turn_running(host: &Host, bot: &str) -> String {
    let first =
        host.call("send", json!({"botId": bot, "text": "first"})).await["entry"]["id"].as_str().unwrap().to_owned();
    eventually("the agent got the first prompt", Duration::from_mins(1), async || {
        host.requests(bot, "session/prompt").len() == 1
    })
    .await;
    first
}

/// Sends "first" (its turn running at the agent) and then "second", which waits: the ids of both.
pub async fn queue_behind_a_turn(host: &Host, bot: &str) -> (String, String) {
    let first = turn_running(host, bot).await;
    let second = host.call("send", json!({"botId": bot, "text": "second"})).await["entry"].clone();
    assert_eq!(second["data"]["status"], "queued");
    (first, second["id"].as_str().unwrap().to_owned())
}

pub async fn idle(host: &Host, bot: &str) -> bool {
    super::bot(&host.snapshot().await, bot)["status"] == "idle"
}

pub async fn status(host: &Host, bot: &str, id: &str) -> Value {
    entry(host, bot, id).await["data"]["status"].clone()
}
