//! A bot's routine runs give their MCP servers back, while its main chat keeps its conversation: through a real host
//! and `resource_agent.py`, whose every session holds a worker in a process group of its own. Unix only: on Windows the
//! agent's job holds every worker, and a worker that isn't Codync's keeps its agent (`background_work`).

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::time::Duration;

fn events(host: &Host, bot: &str) -> Vec<Value> {
    host.log(bot, "resources.jsonl")
}

fn count(rows: &[Value], event: &str) -> usize {
    rows.iter().filter(|r| r["event"] == event).count()
}

async fn routine(host: &Host, bot: &str) -> String {
    routine_doing(host, bot, "Report once").await
}

async fn routine_doing(host: &Host, bot: &str, instruction: &str) -> String {
    let routine = json!({"botId": bot, "name": "Resource run", "instruction": instruction,
        "triggers": [{"type": "webhook"}], "timeoutSeconds": 30});
    host.call("saveRoutine", routine).await["routine"]["id"].as_str().unwrap().to_owned()
}

/// Runs the routine and waits for it to succeed, the bot's `completed`th run to.
async fn run(host: &Host, bot: &str, routine: &str, completed: usize) {
    host.call("runRoutine", json!({"botId": bot, "id": routine})).await;
    succeeded(host, bot, completed).await;
}

/// Waits until `completed` of the bot's runs succeeded.
async fn succeeded(host: &Host, bot: &str, completed: usize) {
    common::eventually("the run succeeds", Duration::from_secs(30), async || {
        let listing = host.call("routines", json!({"botId": bot})).await;
        let runs = listing["runs"].as_array().unwrap();
        assert!(!runs.iter().any(|r| r["status"] == "failed"), "{listing}");
        runs.iter().filter(|r| r["status"] == "succeeded").count() >= completed
    })
    .await;
}

fn live(pid: &Value) -> bool {
    let out = std::process::Command::new("ps").args(["-p", &pid.to_string(), "-o", "stat="]).output().unwrap();
    out.status.success() && !String::from_utf8_lossy(&out.stdout).trim().starts_with('Z')
}

/// The workers (MCP servers) still running, of every session the agent opened.
fn live_workers(host: &Host, bot: &str) -> usize {
    events(host, bot).iter().filter(|r| r["event"] == "allocated" && live(&r["worker"])).count()
}

/// Waits until the workers of every agent but the latest one have ended.
async fn old_workers_gone(host: &Host, bot: &str) {
    common::eventually("the stopped agents' workers end", Duration::from_secs(20), async || {
        let rows = events(host, bot);
        let latest = &rows.iter().rev().find(|r| r["event"] == "started").unwrap()["pid"];
        rows.iter().filter(|r| r["event"] == "allocated" && r["pid"] != *latest).all(|r| !live(&r["worker"]))
    })
    .await;
}

/// The session of every prompt the main chat sent, oldest first.
fn main_sessions(rows: &[Value]) -> Vec<Value> {
    let main = |r: &&Value| r["event"] == "prompt" && !r["text"].as_str().unwrap().contains("Report once");
    rows.iter().filter(main).map(|r| r["session"].clone()).collect()
}

/// Cursor can't close a session: a finished run's MCP servers go with its agent, which stops as soon as nothing runs
/// or waits, even with the main chat open; the next message reopens the main session.
#[tokio::test]
async fn a_run_on_an_agent_that_cant_close_sessions_leaves_no_mcp_servers_behind() {
    let host = Host::start().await;
    let bot = host.fixture_bot("no-close", "resource_agent.py", "--no-close --stubborn-workers").await;
    host.reply(&bot, "Main").await;
    let id = routine(&host, &bot).await;
    for n in 1..=3 {
        run(&host, &bot, &id, n).await;
        common::eventually("the run's agent stops with its workers", Duration::from_secs(20), async || {
            live_workers(&host, &bot) == 0
        })
        .await;
        host.reply(&bot, "Continue").await;
        assert_eq!(live_workers(&host, &bot), 1, "only the main session's");
    }
    let rows = events(&host, &bot);
    let main = main_sessions(&rows);
    assert!(main.iter().all(|s| *s == main[0]), "the main chat kept its conversation: {main:?}");
    assert_eq!((count(&rows, "session/new"), count(&rows, "session/load")), (4, 3));
}

/// A run that ends while a message waits keeps the agent for that message; the agent stops after it.
#[tokio::test]
async fn a_waiting_message_keeps_the_agent_until_its_turn_ends() {
    let host = Host::start().await;
    let bot = host.fixture_bot("no-close-queued", "resource_agent.py", "--no-close --stubborn-workers").await;
    host.reply(&bot, "Main").await;
    let id = routine_doing(&host, &bot, "Report once HOLD").await;
    host.call("runRoutine", json!({"botId": bot, "id": id})).await;
    common::eventually("the run is under way", Duration::from_secs(20), async || {
        events(&host, &bot).iter().any(|r| r["event"] == "prompt" && r["text"].as_str().unwrap().contains("HOLD"))
    })
    .await;
    host.call("send", json!({"botId": bot, "text": "Meanwhile", "clientNonce": "meanwhile"})).await;
    std::fs::write(host.home.join("bots").join(&bot).join("workspace").join("release"), "").unwrap();
    succeeded(&host, &bot, 1).await;
    common::eventually("the agent stops after the message's turn", Duration::from_secs(20), async || {
        live_workers(&host, &bot) == 0
    })
    .await;
    let rows = events(&host, &bot);
    assert_eq!(count(&rows, "started"), 1, "the message's turn ran on the run's agent");
    assert!(rows.iter().any(|r| r["event"] == "prompt" && r["text"].as_str().unwrap().contains("Meanwhile")));
    host.reply(&bot, "Continue").await;
    let main = main_sessions(&events(&host, &bot));
    assert!(main.iter().all(|s| *s == main[0]), "{main:?}");
}

#[tokio::test]
async fn routines_without_close_release_connectors_and_resume_the_same_main_chat() {
    let host = Host::start().await;
    let bot = host.fixture_bot("resource-no-close", "resource_agent.py", "--no-close").await;
    host.reply(&bot, "Remember our main conversation").await;
    let id = routine(&host, &bot).await;
    for n in 1..=20 {
        run(&host, &bot, &id, n).await;
        host.reply(&bot, "Continue our main conversation").await;
        old_workers_gone(&host, &bot).await;
        let rows = events(&host, &bot);
        let main = main_sessions(&rows);
        assert!(main.iter().all(|s| *s == main[0]), "{main:?}");
        let mut per_agent = std::collections::HashMap::new();
        for r in rows.iter().filter(|r| r["event"] == "allocated") {
            *per_agent.entry(r["pid"].as_i64().unwrap()).or_insert(0) += 1;
        }
        assert!(per_agent.values().all(|n| *n <= 2), "{per_agent:?}");
    }
}

#[tokio::test]
async fn a_failed_close_stops_the_agent_with_its_mcp_servers() {
    let host = Host::start().await;
    let bot = host.fixture_bot("resource-close-failed", "resource_agent.py", "--fail-close --stubborn-workers").await;
    host.reply(&bot, "Main").await;
    let id = routine(&host, &bot).await;
    run(&host, &bot, &id, 1).await;
    host.reply(&bot, "Continue").await;
    assert_eq!(count(&events(&host, &bot), "started"), 2);
    old_workers_gone(&host, &bot).await;
}

#[tokio::test]
async fn resume_only_agents_release_routine_sessions_and_keep_their_main_conversation() {
    let host = Host::start().await;
    let bot = host.fixture_bot("resource-resume-only", "resource_agent.py", "--resume-only --no-close").await;
    host.reply(&bot, "Main").await;
    let id = routine(&host, &bot).await;
    for n in 1..=5 {
        run(&host, &bot, &id, n).await;
        host.reply(&bot, "Continue main").await;
        old_workers_gone(&host, &bot).await;
    }
    let rows = events(&host, &bot);
    let main = main_sessions(&rows);
    assert!(main.iter().all(|s| *s == main[0]), "{main:?}");
    assert_eq!((count(&rows, "session/resume"), count(&rows, "session/load")), (5, 0));
}
