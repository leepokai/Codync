//! A turn whose agent goes silent, through a real host with a 3 s limit (`CODYNC_AGENT_SILENCE_MINUTES`): the agent
//! is stopped and its fallback carries the turn on, or the chat says why; a long tool call isn't silence.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const LIMIT: (&str, &str) = ("CODYNC_AGENT_SILENCE_MINUTES", "0.05");

fn fixture(file: &str, args: &str) -> String {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(file);
    format!("{} {args}", common::python_agent(&script))
}

/// A bot on `command` with `fallbacks`, sent `text` without waiting for its answer.
async fn ask(host: &Host, command: String, fallbacks: Value, text: &str) -> String {
    let bot = json!({"name": "Quiet", "backend": "custom", "command": command, "connectors": [], "notify": false});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    host.call("updateBot", json!({"id": bot, "fallbacks": fallbacks})).await;
    host.call("send", json!({"botId": bot, "text": text, "clientNonce": uuid::Uuid::new_v4().to_string()})).await;
    bot
}

/// The newest entry of `bot`'s chat that `found` picks, within 30 s.
async fn wait_for(host: &Host, bot: &str, what: &str, found: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let history = host.call("history", json!({"botId": bot})).await;
        if let Some(e) = history["entries"].as_array().unwrap().iter().rev().find(|e| found(e)) {
            return e.clone();
        }
        assert!(Instant::now() < deadline, "no {what}: {history}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn is_reply(e: &Value) -> bool {
    e["kind"] == "agent" && e["data"]["final"] == true
}

#[tokio::test]
async fn a_silent_agent_hands_the_turn_to_its_fallback() {
    let host = Host::start_with(&[LIMIT]).await;
    let fallbacks = json!([{"backend": "codex", "command": fixture("relay_agent.py", "standin")}]);
    let bot = ask(&host, fixture("stubborn_agent.py", ""), fallbacks, "Fix the login tests").await;

    let reply = wait_for(&host, &bot, "reply", is_reply).await;
    assert_eq!(reply["data"]["text"], "standin answered");
    let line = wait_for(&host, &bot, "switch line", |e| e["data"]["agentSwitch"].is_object()).await;
    assert_eq!(line["data"]["agentSwitch"]["reason"], "notStarting", "{line}");
}

#[tokio::test]
async fn a_silent_agent_without_fallbacks_ends_its_turn_with_why() {
    let host = Host::start_with(&[LIMIT]).await;
    let bot = ask(&host, fixture("stubborn_agent.py", ""), json!([]), "Fix the login tests").await;

    let notice =
        wait_for(&host, &bot, "failure notice", |e| e["kind"] == "notice" && e["data"]["style"] == "error").await;
    // It says how long the agent was actually silent: a busy machine may tick a second or two past the limit.
    let text = notice["data"]["text"].as_str().unwrap();
    assert!(text.starts_with("The agent failed: it went silent for "), "{text}");
    assert!(text.ends_with(" s, so Codync stopped it"), "{text}");
    // The bot is free for the next message, showing the failure.
    let deadline = Instant::now() + Duration::from_secs(10);
    while common::bot(&host.snapshot().await, &bot)["status"] != "error" {
        assert!(Instant::now() < deadline, "the bot is still working");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn a_long_tool_call_is_not_silence() {
    let host = Host::start_with(&[LIMIT]).await;
    let bot = ask(&host, fixture("slow_tool_agent.py", "6"), json!([]), "Build it").await;

    let reply = wait_for(&host, &bot, "reply", is_reply).await;
    assert_eq!(reply["data"]["text"], "built");
    let history = host.call("history", json!({"botId": bot})).await;
    let notices: Vec<&Value> =
        history["entries"].as_array().unwrap().iter().filter(|e| e["kind"] == "notice").collect();
    assert!(notices.is_empty(), "the turn ended normally: {notices:?}");
}
