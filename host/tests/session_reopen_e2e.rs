//! An agent that can't load a session but can resume one, through a real host and a fixture agent: the conversation
//! carries on after the agent restarts, and the sessions it's done with go back like any other agent's.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::json;
use std::time::Duration;

#[tokio::test]
async fn an_agent_that_only_resumes_keeps_its_conversation_and_gives_sessions_back() {
    if common::python_missing() {
        return;
    }
    let mut host = Host::start().await;
    let bot = host.fixture_bot("Resumes", "resume_agent.py", "").await;

    host.reply(&bot, "one").await;
    let opened = host.requests(&bot, "session/new");
    assert_eq!(opened.len(), 1);

    // The agent starts again with the host: the same session comes back, with its tools.
    host.restart(|_| {}).await;
    host.reply(&bot, "two").await;
    assert_eq!(host.requests(&bot, "session/new").len(), 1, "a new session would lose the conversation");
    assert!(host.requests(&bot, "session/load").is_empty(), "the agent didn't offer session/load");
    let resumed = host.requests(&bot, "session/resume");
    assert_eq!(resumed.len(), 1);
    let main = resumed[0]["params"]["sessionId"].as_str().unwrap();
    assert!(resumed[0]["params"]["mcpServers"].as_array().is_some_and(|s| !s.is_empty()), "{resumed:?}");

    // A routine run's session is spent when the run is; its MCP servers shouldn't stay until the agent stops.
    let routine = json!({"botId": bot, "name": "Check", "instruction": "Report", "triggers": [{"type": "webhook"}]});
    let id = host.call("saveRoutine", routine).await["routine"]["id"].as_str().unwrap().to_owned();
    host.call("runRoutine", json!({"botId": bot, "id": id})).await;
    common::eventually("the run's session is closed", Duration::from_secs(30), async || {
        !host.requests(&bot, "session/close").is_empty()
    })
    .await;
    let closed = host.requests(&bot, "session/close");
    assert_ne!(closed[0]["params"]["sessionId"], main, "the bot's own conversation stays open");
    assert_eq!(host.requests(&bot, "session/new").len(), 2);
}

#[tokio::test]
async fn a_model_refused_after_a_resume_leaves_the_session_to_resume_again() {
    if common::python_missing() {
        return;
    }
    let mut host = Host::start().await;
    let bot = host.fixture_bot("Resumes", "resume_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "model": "fixture"})).await;
    host.reply(&bot, "one").await;

    let refused = host.home.join("bots").join(&bot).join("workspace").join("model.refused");
    std::fs::write(&refused, "").unwrap();
    host.restart(|_| {}).await;
    host.call("send", json!({"botId": bot, "text": "two", "clientNonce": "n-two"})).await;
    common::eventually("the turn fails on its model", Duration::from_secs(30), async || {
        let history = host.call("history", json!({"botId": bot})).await;
        history["entries"].as_array().unwrap().iter().any(|e| e["data"]["style"] == "error")
    })
    .await;

    std::fs::remove_file(&refused).unwrap();
    host.reply(&bot, "three").await;
    assert_eq!(host.requests(&bot, "session/resume").len(), 2);
    assert_eq!(host.requests(&bot, "session/new").len(), 1, "the agent refused to resume a session it still held");
}
