//! A bot's model settings (reasoning effort) through a real host and a fixture agent that offers them the way
//! Codex does: on the session, beside the model. A bot on the agent's default model gets its own effort on new and
//! resumed sessions, a value the agent rejects says which setting failed, and choosing a model drops the old one's
//! settings (#177).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn python_missing() -> bool {
    let missing = std::process::Command::new(common::python()).arg("--version").output().is_err();
    if missing {
        eprintln!("skipping: Python not available");
    }
    missing
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
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Sends `text` and waits for the agent's answer to it.
async fn answer(host: &Host, bot: &str, text: &str) -> String {
    host.call("send", json!({"botId": bot, "text": text})).await;
    let found = |e: &Value| {
        e["kind"] == "agent"
            && e["data"]["final"] == true
            && e["data"]["text"].as_str().is_some_and(|t| t.ends_with(text))
    };
    wait_for(host, bot, text, found).await["data"]["text"].as_str().unwrap().to_owned()
}

/// The session calls the agent logged, as `method` or `configId=value`.
fn calls(host: &Host, bot: &str) -> Vec<String> {
    host.log(bot, "calls.jsonl")
        .iter()
        .map(|c| match c["configId"].as_str() {
            Some(id) => format!("{id}={}", c["value"].as_str().unwrap()),
            None => c["method"].as_str().unwrap().to_owned(),
        })
        .collect()
}

#[tokio::test]
async fn a_bot_on_the_default_model_gets_its_effort_on_new_and_resumed_sessions() {
    if python_missing() {
        return;
    }
    let mut host = Host::start().await;
    let bot = host.fixture_bot("Thinker", "effort_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "modelOptions": {"reasoning_effort": "high"}})).await;
    assert_eq!(answer(&host, &bot, "one").await, "sol high · one", "the agent's model, the bot's effort");
    assert_eq!(calls(&host, &bot), ["session/new", "reasoning_effort=high"], "no model is chosen for it");

    // A fresh agent process starts at its default again: the resumed session gets the effort back.
    host.restart(|_| {}).await;
    assert_eq!(answer(&host, &bot, "two").await, "sol high · two");
    assert_eq!(calls(&host, &bot)[2..], ["session/load", "reasoning_effort=high"]);

    // Like a new model, a new setting starts a new agent session after the turn (the chat is kept).
    host.call("updateBot", json!({"id": bot, "modelOptions": {"reasoning_effort": "low"}})).await;
    assert_eq!(answer(&host, &bot, "three").await, "sol low · three");
    assert_eq!(calls(&host, &bot)[4..], ["session/new", "reasoning_effort=low"]);
}

#[tokio::test]
async fn an_effort_the_agent_rejects_names_the_setting() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Rejected", "effort_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "modelOptions": {"reasoning_effort": "max"}})).await;
    host.call("send", json!({"botId": bot, "text": "one"})).await;
    let failed = wait_for(&host, &bot, "failure", |e| e["data"]["style"] == "error").await;
    let text = failed["data"]["text"].as_str().unwrap();
    assert!(text.contains("Couldn't set reasoning_effort to max"), "{text}");
}

#[tokio::test]
async fn choosing_a_model_drops_the_default_models_effort() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Switcher", "effort_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "modelOptions": {"reasoning_effort": "high"}})).await;
    assert_eq!(answer(&host, &bot, "one").await, "sol high · one");

    // mini has no high effort: an app that only changes the model doesn't carry it over.
    host.call("updateBot", json!({"id": bot, "model": "mini"})).await;
    let snapshot = host.snapshot().await;
    let saved = snapshot["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot.as_str()).unwrap();
    assert_eq!(saved["modelOptions"], json!({}));
    assert_eq!(answer(&host, &bot, "two").await, "mini low · two");
    assert_eq!(calls(&host, &bot)[2..], ["session/new", "model=mini"]);
}

#[tokio::test]
async fn clients_see_what_the_running_session_uses() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Shown", "effort_agent.py", "").await;
    let in_use = || async {
        let snapshot = host.snapshot().await;
        snapshot["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot.as_str()).unwrap()["sessionOptions"].clone()
    };
    assert_eq!(in_use().await, Value::Null, "nothing is confirmed before the agent runs");

    host.call("updateBot", json!({"id": bot, "modelOptions": {"reasoning_effort": "high"}})).await;
    answer(&host, &bot, "one").await;
    assert_eq!(in_use().await, json!({"model": "sol", "reasoning_effort": "high"}), "what the agent reports");
}

#[tokio::test]
async fn a_chosen_models_settings_come_from_the_session_running_it() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Chosen", "effort_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "model": "mini"})).await;
    answer(&host, &bot, "one").await;
    let snapshot = host.snapshot().await;
    let shown = snapshot["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot.as_str()).unwrap().clone();
    // Discovery only sees the agent's default model; the bot's own session lists what mini offers.
    assert_eq!(
        shown["sessionSettings"],
        json!([{"id": "reasoning_effort", "name": "Reasoning Effort", "default": "low",
                "values": [{"id": "low", "name": "Low"}]}])
    );
}
