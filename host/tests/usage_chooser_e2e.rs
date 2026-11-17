//! The Usage chooser on the wire: a choice or an order reaches every client, and what they get leaves
//! out the agents the user hides, also when one reconnects.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;

/// The next `usage` a client gets that `holds`: from a catch-up's `hello`, or a live event.
async fn usage_until(events: &mut UnboundedReceiver<Value>, what: &str, holds: impl Fn(&Value) -> bool) -> Value {
    let wait = async {
        loop {
            let event = events.recv().await.expect("the stream stays open");
            if matches!(event["type"].as_str(), Some("hello" | "usage")) && holds(&event["usage"]) {
                return event;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(20), wait).await.unwrap_or_else(|_| panic!("no usage where {what}"))
}

fn shown(usage: &Value, id: &str) -> Option<bool> {
    usage["services"].as_array()?.iter().find(|service| service["id"] == id)?["shown"].as_bool()
}

/// The first `count` agents listed: a host may also list what the computer it runs on has installed.
fn first_agents(usage: &Value, count: usize) -> Vec<&str> {
    usage["services"].as_array().map_or_else(Vec::new, |services| {
        services.iter().filter_map(|service| service["id"].as_str()).take(count).collect()
    })
}

fn has_reading(usage: &Value, id: &str) -> bool {
    usage["providers"].as_array().is_some_and(|providers| providers.iter().any(|provider| provider["id"] == id))
}

/// Claude's status line, which needs no CLI: the one reading a test host can get.
async fn claude_reading(host: &Host) {
    let response = reqwest::Client::new()
        .post(format!("{}/ingest/statusline", host.base))
        .bearer_auth(&host.token)
        .json(&json!({"rate_limits": {"five_hour": {"used_percentage": 12.0}}}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
}

#[tokio::test]
async fn a_choice_reaches_the_other_clients_and_a_hidden_reading_stays_on_the_host() {
    let host = Host::start().await;
    let mut other_client = host.events().await;
    let bot = json!({"name": "Claude bot", "backend": "claude", "connectors": [], "permission": "ask"});
    host.call("createBot", bot).await;

    usage_until(&mut other_client, "a new bot's agent is listed and on", |usage| shown(usage, "claude") == Some(true))
        .await;
    claude_reading(&host).await;
    let landed = usage_until(&mut other_client, "Claude's reading landed", |usage| has_reading(usage, "claude")).await;
    assert_eq!(landed["usage"]["providers"][0]["registry"], "claude-acp");

    let answer = host.call("setUsageShown", json!({"id": "claude", "shown": false})).await;
    assert_eq!(shown(&answer, "claude"), Some(false));
    assert!(!has_reading(&answer, "claude"), "{answer}");
    usage_until(&mut other_client, "Claude was hidden", |usage| {
        shown(usage, "claude") == Some(false) && !has_reading(usage, "claude")
    })
    .await;

    let mut reconnected = host.events().await;
    let hello = reconnected.recv().await.unwrap();
    assert_eq!(hello["type"], "hello", "{hello}");
    assert_eq!(shown(&hello["usage"], "claude"), Some(false));
    assert!(!has_reading(&hello["usage"], "claude"), "{hello}");
    assert!(!has_reading(&host.call("usage", json!({})).await, "claude"));

    // A bot still runs Claude, so the host kept reading it: the bars are back as soon as it's on.
    host.call("setUsageShown", json!({"id": "claude", "shown": true})).await;
    usage_until(&mut other_client, "Claude was shown again", |usage| {
        shown(usage, "claude") == Some(true) && has_reading(usage, "claude")
    })
    .await;
}

#[tokio::test]
async fn an_agent_it_does_not_know_can_not_be_chosen() {
    let host = Host::start().await;
    let error = host.refused("setUsageShown", json!({"id": "not-an-agent", "shown": true})).await;
    assert!(error.contains("unknown usage service"), "{error}");
}

#[tokio::test]
async fn an_order_reaches_the_other_clients_and_stays_when_one_is_refused() {
    let host = Host::start().await;
    let mut other_client = host.events().await;
    let bot = json!({
        "name": "Two agents", "backend": "claude", "fallbacks": [{"backend": "codex"}],
        "connectors": [], "permission": "ask",
    });
    host.call("createBot", bot).await;
    let listed = usage_until(&mut other_client, "both agents are listed", |usage| {
        shown(usage, "claude") == Some(true) && shown(usage, "codex") == Some(true)
    })
    .await;
    assert_eq!(first_agents(&listed["usage"], 2), ["claude", "codex"]);

    let answer = host.call("setUsageOrder", json!({"ids": ["codex", "claude"]})).await;
    assert_eq!(first_agents(&answer, 2), ["codex", "claude"]);
    usage_until(&mut other_client, "the order arrived", |usage| first_agents(usage, 2) == ["codex", "claude"]).await;

    let mut reconnected = host.events().await;
    let hello = reconnected.recv().await.unwrap();
    assert_eq!(hello["type"], "hello", "{hello}");
    assert_eq!(first_agents(&hello["usage"], 2), ["codex", "claude"]);

    let error = host.refused("setUsageOrder", json!({"ids": ["not-an-agent"]})).await;
    assert!(error.contains("unknown usage service"), "{error}");
    let error = host.refused("setUsageOrder", json!({})).await;
    assert!(error.contains("ids is required"), "{error}");
    assert_eq!(first_agents(&host.call("usage", json!({})).await, 2), ["codex", "claude"]);
}
