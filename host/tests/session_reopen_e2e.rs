//! An agent that can't load a session but can resume one, through a real host and a fixture agent: the conversation
//! carries on after the agent restarts, and the sessions it's done with go back like any other agent's.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::json;
use std::time::{Duration, Instant};

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
    // That is a run in the chat's own agent process; one in the bot's box ends with its thread's process.
    host.works_in_chat(&bot).await;
    // Changing the bot restarted its agent; its conversation is open again before the run, so the run's session
    // isn't the only one (an agent holding nothing else is stopped instead of asked to close).
    host.reply(&bot, "three").await;
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

#[tokio::test]
async fn stop_reaches_a_retry_after_an_unanswered_close_on_a_failed_resume() {
    if common::python_missing() {
        return;
    }
    let mut host = Host::start().await;
    let bot = host.fixture_bot("Resumes", "resume_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "model": "fixture"})).await;
    host.reply(&bot, "before restart").await;
    let original = host.requests(&bot, "session/prompt")[0]["params"]["sessionId"].clone();

    let workspace = host.workspace(&bot);
    let refused = workspace.join("model.refused");
    std::fs::write(&refused, "").unwrap();
    std::fs::write(workspace.join("close.unanswered"), "").unwrap();
    host.restart(|_| {}).await;
    host.call("send", json!({"botId": bot, "text": "refuse the resumed model", "clientNonce": "refuse"})).await;
    common::eventually("the refused resume to wait for close", Duration::from_secs(30), async || {
        host.requests(&bot, "session/close").iter().any(|e| e["params"]["sessionId"] == original)
    })
    .await;
    let closing = &host.requests(&bot, "session/close")[0];
    assert_eq!(closing["params"]["sessionId"], original);
    assert_eq!(closing["held"], json!([original]));
    assert_eq!(closing["pid"], host.requests(&bot, "session/resume")[0]["pid"]);

    std::fs::remove_file(&refused).unwrap();
    host.call("send", json!({"botId": bot, "text": "HOLD[resume-stop]", "clientNonce": "retry"})).await;
    let started = Instant::now();
    let cancelled = tokio::time::timeout(Duration::from_secs(15), async {
        host.call("stop", json!({"botId": bot})).await;
        loop {
            if let Some(cancel) = host.requests(&bot, "session/cancel").into_iter().next() {
                return cancel;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    eprintln!(
        "close unanswered on PID {} for session {original}, Stop reached ACP={} after {:.2?}",
        closing["pid"],
        cancelled.is_ok(),
        started.elapsed()
    );
    let cancelled = cancelled.expect("Stop reaches the ACP prompt within 15 seconds");
    assert_eq!(cancelled["params"]["sessionId"], original);
    assert_eq!(cancelled["held"], json!([original]));
    assert_ne!(cancelled["pid"], closing["pid"]);
    common::eventually("the agent that did not answer close to exit", Duration::from_secs(10), async || {
        host.requests(&bot, "exit").iter().any(|e| e["pid"] == closing["pid"])
    })
    .await;
    let exited = host.requests(&bot, "exit").into_iter().find(|e| e["pid"] == closing["pid"]).unwrap();
    let close_wait = Duration::from_secs_f64(exited["at"].as_f64().unwrap() - closing["at"].as_f64().unwrap());
    let history = host.call("history", json!({"botId": bot})).await;
    assert!(
        history["entries"].as_array().unwrap().iter().any(|e| {
            e["data"]["text"]
                .as_str()
                .is_some_and(|t| t.contains("Couldn't select model fixture: unknown model option"))
        }),
        "{history}"
    );

    host.reply(&bot, "after Stop").await;
    let prompts = host.requests(&bot, "session/prompt");
    let [_, retry, after]: [serde_json::Value; 3] = prompts.try_into().unwrap();
    for prompt in [&retry, &after] {
        assert_eq!(prompt["params"]["sessionId"], original);
        assert_eq!(prompt["held"], json!([original]));
        assert_eq!(prompt["pid"], cancelled["pid"]);
    }
    assert_eq!(host.requests(&bot, "session/new").len(), 1);
    eprintln!(
        "retired PID {} after {close_wait:.2?}, cancellation and the later chat used PID {} holding {original}",
        closing["pid"], cancelled["pid"]
    );
}
