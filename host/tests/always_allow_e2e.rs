//! Always allow through a real host and a fixture agent that asks before every action anyway (as `MiniMax` Code
//! does): once the user always allows a kind of action, Codync answers that kind itself in that session, and still
//! asks for another kind or in another session.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// What happened after sending `text`: the approval card it waits on, or the bot's reply when no card came.
enum Outcome {
    Card(Value),
    Reply(String),
}

fn finals(history: &Value) -> Vec<String> {
    let entries = history["entries"].as_array().unwrap();
    let finals = entries.iter().filter(|e| e["kind"] == "agent" && e["data"]["final"] == true);
    finals.map(|e| e["data"]["text"].as_str().unwrap_or_default().to_owned()).collect()
}

async fn send(host: &Host, bot: &str, text: &str) -> Outcome {
    let before = finals(&host.call("history", json!({"botId": bot})).await).len();
    let nonce = uuid::Uuid::new_v4().to_string();
    host.call("send", json!({"botId": bot, "text": text, "clientNonce": nonce})).await;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let history = host.call("history", json!({"botId": bot})).await;
        let entries = history["entries"].as_array().unwrap();
        if let Some(card) = entries.iter().find(|e| e["kind"] == "permission" && e["data"]["status"] == "pending") {
            return Outcome::Card(card.clone());
        }
        if let Some(reply) = finals(&history).get(before) {
            return Outcome::Reply(reply.clone());
        }
        assert!(Instant::now() < deadline, "{bot} didn't answer {text:?}: {history}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Answers `card` with `option` and waits for the reply that follows.
async fn answer(host: &Host, bot: &str, card: &Value, option: &str) -> String {
    let before = finals(&host.call("history", json!({"botId": bot})).await).len();
    host.call("respondPermission", json!({"entryId": card["id"], "optionId": option})).await;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(reply) = finals(&host.call("history", json!({"botId": bot})).await).get(before) {
            return reply.clone();
        }
        assert!(Instant::now() < deadline, "no reply after answering the card");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn card(outcome: Outcome, what: &str) -> Value {
    match outcome {
        Outcome::Card(card) => card,
        Outcome::Reply(reply) => panic!("{what}: expected a card, got the reply {reply:?}"),
    }
}

#[tokio::test]
async fn always_allow_answers_the_same_kind_in_the_same_session_only() {
    if common::python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("MiniMax", "always_allow_agent.py", "").await;

    // The first command asks; the user always allows commands.
    let first = card(send(&host, &bot, "ls").await, "the first command");
    assert_eq!(answer(&host, &bot, &first, "always").await, "s1 always");

    // The next command is answered by Codync, with no card.
    match send(&host, &bot, "cargo test").await {
        Outcome::Reply(reply) => assert_eq!(reply, "s1 always"),
        Outcome::Card(card) => panic!("a command was asked again after Always allow: {card}"),
    }

    // Another kind of action still asks, and allowing it once is not remembered.
    let edit = card(send(&host, &bot, "EDIT a.txt").await, "an edit");
    assert_eq!(answer(&host, &bot, &edit, "once").await, "s1 once");
    let edit = card(send(&host, &bot, "EDIT a.txt again").await, "a second edit");
    assert_eq!(answer(&host, &bot, &edit, "no").await, "s1 no");

    // A new session asks again, even for commands.
    host.call("newSession", json!({"botId": bot})).await;
    let fresh = card(send(&host, &bot, "ls").await, "a command in a new session");
    assert_eq!(answer(&host, &bot, &fresh, "once").await, "s2 once");
}
