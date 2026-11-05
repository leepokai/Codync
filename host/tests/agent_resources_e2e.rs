//! Busy bots must release actual connector processes while preserving saved conversations.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use common::host::Host;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn events(host: &Host, bot: &str) -> Vec<Value> {
    host.log(bot, "resources.jsonl")
}

async fn wait_for(host: &Host, bot: &str, event: &str, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if events(host, bot).iter().filter(|v| v["event"] == event).count() >= count {
            return;
        }
        assert!(Instant::now() < deadline, "missing {event}: {:?}", events(host, bot));
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn reply(host: &Host, bot: &str, text: &str) {
    let before = events(host, bot).iter().filter(|r| r["event"] == "prompt").count();
    host.call("send", json!({"botId":bot,"text":text,"clientNonce":uuid::Uuid::new_v4().to_string()})).await;
    wait_for(host, bot, "prompt", before + 1).await;
}

async fn routine(host: &Host, bot: &str) -> String {
    let saved = host
        .call(
            "saveRoutine",
            json!({"botId":bot,"name":"Resource run","instruction":"Report once",
        "triggers":[{"type":"webhook"}], "timeoutSeconds":30}),
        )
        .await;
    saved["routine"]["id"].as_str().unwrap().into()
}

async fn run(host: &Host, bot: &str, routine: &str, completed: usize) {
    host.call("runRoutine", json!({"botId":bot,"id":routine})).await;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let listing = host.call("routines", json!({"botId":bot})).await;
        let runs = listing["runs"].as_array().unwrap();
        assert!(!runs.iter().any(|r| r["status"] == "failed"), "{listing}");
        if runs.iter().filter(|r| r["status"] == "succeeded").count() >= completed {
            return;
        }
        assert!(Instant::now() < deadline, "run did not complete: {listing}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(unix)]
fn live(pid: i32) -> bool {
    let out = std::process::Command::new("ps").args(["-p", &pid.to_string(), "-o", "stat="]).output().unwrap();
    out.status.success() && !String::from_utf8_lossy(&out.stdout).trim().starts_with('Z')
}

#[cfg(unix)]
fn assert_old_workers_gone(rows: &[Value]) {
    let latest = rows.iter().rev().find(|r| r["event"] == "started").unwrap()["pid"].as_i64().unwrap();
    for r in rows.iter().filter(|r| r["event"] == "allocated" && r["pid"] != latest) {
        assert!(!live(i32::try_from(r["worker"].as_i64().unwrap()).unwrap()), "old connector survived: {r}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn routines_without_close_release_connectors_and_resume_the_same_main_chat() {
    let host = Host::start().await;
    let bot = host.fixture_bot("resource-no-close", "resource_agent.py", "--no-close").await;
    reply(&host, &bot, "Remember our main conversation").await;
    let main = events(&host, &bot).into_iter().find(|r| r["event"] == "prompt").unwrap()["session"].clone();
    let id = routine(&host, &bot).await;
    for n in 1..=100 {
        run(&host, &bot, &id, n).await;
        reply(&host, &bot, "Continue our main conversation").await;
        let rows = events(&host, &bot);
        assert_eq!(rows.iter().rev().find(|r| r["event"] == "prompt").unwrap()["session"], main);
        #[cfg(unix)]
        assert_old_workers_gone(&rows);
        let mut generations = std::collections::HashMap::new();
        for r in rows.iter().filter(|r| r["event"] == "allocated") {
            *generations.entry(r["pid"].as_i64().unwrap()).or_insert(0) += 1;
        }
        assert!(generations.values().all(|n| *n <= 2), "{generations:?}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn successful_close_still_bounds_hidden_resources_over_many_runs() {
    let host = Host::start().await;
    let bot = host.fixture_bot("resource-close", "resource_agent.py", "").await;
    reply(&host, &bot, "Main").await;
    let id = routine(&host, &bot).await;
    for n in 1..=12 {
        run(&host, &bot, &id, n).await;
    }
    reply(&host, &bot, "Still here").await;
    let rows = events(&host, &bot);
    assert!(rows.iter().filter(|r| r["event"] == "started").count() >= 2);
    #[cfg(unix)]
    assert_old_workers_gone(&rows);
}

#[cfg(unix)]
#[tokio::test]
async fn failed_close_recycles_before_the_next_session() {
    let host = Host::start().await;
    let bot = host.fixture_bot("resource-close-failed", "resource_agent.py", "--fail-close --stubborn-workers").await;
    reply(&host, &bot, "Main").await;
    let id = routine(&host, &bot).await;
    run(&host, &bot, &id, 1).await;
    reply(&host, &bot, "Continue").await;
    wait_for(&host, &bot, "started", 2).await;
    #[cfg(unix)]
    assert_old_workers_gone(&events(&host, &bot));
}

#[cfg(unix)]
#[tokio::test]
async fn failed_resume_keeps_the_saved_id_and_does_not_start_fresh() {
    let host = Host::start().await;
    let bot = host.fixture_bot("resource-load-failed", "resource_agent.py", "--no-close").await;
    reply(&host, &bot, "Main").await;
    let id = routine(&host, &bot).await;
    run(&host, &bot, &id, 1).await;
    let workspace = host.home.join("bots").join(&bot).join("workspace");
    std::fs::write(workspace.join("fail-load"), "").unwrap();
    host.call("send", json!({"botId":bot,"text":"Do not replace main","clientNonce":uuid::Uuid::new_v4().to_string()}))
        .await;
    wait_for(&host, &bot, "load_failed", 1).await;
    let rows = events(&host, &bot);
    assert_eq!(rows.iter().filter(|r| r["event"] == "session/new").count(), 2);
    std::fs::remove_file(workspace.join("fail-load")).unwrap();
    reply(&host, &bot, "Retry main").await;
    let rows = events(&host, &bot);
    assert_eq!(rows.iter().filter(|r| r["event"] == "session/new").count(), 2);
    let prompts: Vec<_> = rows.iter().filter(|r| r["event"] == "prompt").collect();
    assert_eq!(prompts.first().unwrap()["session"], prompts.last().unwrap()["session"]);
}

#[tokio::test]
async fn an_unreloadable_main_is_preserved_when_session_capacity_is_exhausted() {
    let host = Host::start().await;
    let bot = host.fixture_bot("resource-no-load", "resource_agent.py", "--no-load --no-close").await;
    reply(&host, &bot, "Main").await;
    let id = routine(&host, &bot).await;
    run(&host, &bot, &id, 1).await;
    host.call("runRoutine", json!({"botId":bot,"id":id})).await;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let listing = host.call("routines", json!({"botId":bot})).await;
        if listing["runs"].as_array().unwrap().iter().any(|r| r["status"] == "failed") {
            break;
        }
        assert!(Instant::now() < deadline, "capacity should refuse another session: {listing}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    reply(&host, &bot, "Continue main").await;
    let rows = events(&host, &bot);
    assert_eq!(rows.iter().filter(|r| r["event"] == "allocated").count(), 2);
    assert_eq!(rows.iter().filter(|r| r["event"] == "started").count(), 1);
}

#[cfg(unix)]
#[tokio::test]
async fn refreshing_the_same_session_releases_its_previous_tool_allocation() {
    let host = Host::start().await;
    let bot = host.fixture_bot("resource-refresh", "resource_agent.py", "--claude --no-close").await;
    reply(&host, &bot, "Main").await;
    for n in 1..=10 {
        host.write_memory(Some(&bot), "MEMORY.md", &format!("- Remember item {n}\n"));
        reply(&host, &bot, "COMPACT_RESOURCE_SESSION").await;
        reply(&host, &bot, "Continue").await;
    }
    let rows = events(&host, &bot);
    assert!(rows.iter().filter(|r| r["event"] == "started").count() >= 2, "{rows:?}");
    assert_eq!(rows.iter().filter(|r| r["event"] == "session/new").count(), 1);
    #[cfg(unix)]
    assert_old_workers_gone(&rows);
}

async fn model_bot(host: &Host, args: &str) -> String {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/resource_agent.py");
    let command = format!("{} {args} --model-options", common::python_agent(&fixture));
    let saved = host
        .call(
            "createBot",
            json!({"name":"Model retry","description":"Preserve the mango instruction",
        "backend":"custom","command":command,"model":"fixture-model","connectors":[],"notify":false}),
        )
        .await;
    saved["bot"]["id"].as_str().unwrap().into()
}

#[tokio::test]
async fn failed_initial_model_setup_retries_the_same_unreloadable_allocation_with_its_instructions() {
    let host = Host::start().await;
    let bot = model_bot(&host, "--no-load --fail-model-once").await;
    host.call("send", json!({"botId":bot,"text":"First attempt"})).await;
    wait_for(&host, &bot, "model_failed", 1).await;
    assert!(!events(&host, &bot).iter().any(|r| r["event"] == "prompt"));
    reply(&host, &bot, "Retry").await;
    let rows = events(&host, &bot);
    assert_eq!(rows.iter().filter(|r| r["event"] == "allocated").count(), 1);
    let prompt = rows.iter().find(|r| r["event"] == "prompt").unwrap();
    assert!(prompt["text"].as_str().unwrap().contains("Preserve the mango instruction"));
    let failed = rows.iter().find(|r| r["event"] == "model_failed").unwrap();
    assert_eq!(failed["session"], prompt["session"]);
    let selected = rows.iter().position(|r| r["event"] == "model_selected").unwrap();
    assert!(selected < rows.iter().position(|r| r["event"] == "prompt").unwrap());
}

#[cfg(unix)]
#[tokio::test]
async fn failed_model_setup_after_resume_reuses_the_loaded_session() {
    let host = Host::start().await;
    let bot = model_bot(&host, "--no-close").await;
    reply(&host, &bot, "Main").await;
    let id = routine(&host, &bot).await;
    run(&host, &bot, &id, 1).await;
    let workspace = host.home.join("bots").join(&bot).join("workspace");
    std::fs::write(workspace.join("fail-model"), "").unwrap();
    host.call("send", json!({"botId":bot,"text":"Resume main"})).await;
    wait_for(&host, &bot, "model_failed", 1).await;
    let before = events(&host, &bot);
    assert_eq!(before.iter().filter(|r| r["event"] == "prompt").count(), 2);
    let allocations = before.iter().filter(|r| r["event"] == "allocated").count();
    reply(&host, &bot, "Retry main").await;
    let rows = events(&host, &bot);
    assert_eq!(rows.iter().filter(|r| r["event"] == "allocated").count(), allocations);
    let prompts: Vec<_> = rows.iter().filter(|r| r["event"] == "prompt").collect();
    assert_eq!(prompts.first().unwrap()["session"], prompts.last().unwrap()["session"]);
}

#[cfg(unix)]
#[tokio::test]
async fn resume_only_agents_apply_the_same_budget_and_keep_their_main_conversation() {
    let host = Host::start().await;
    let bot = host.fixture_bot("resource-resume-only", "resource_agent.py", "--resume-only --no-close").await;
    reply(&host, &bot, "Main").await;
    let id = routine(&host, &bot).await;
    for n in 1..=10 {
        run(&host, &bot, &id, n).await;
        reply(&host, &bot, "Continue main").await;
        let rows = events(&host, &bot);
        assert_old_workers_gone(&rows);
        let prompts: Vec<_> = rows.iter().filter(|r| r["event"] == "prompt").collect();
        assert_eq!(prompts.first().unwrap()["session"], prompts.last().unwrap()["session"]);
    }
    let rows = events(&host, &bot);
    assert_eq!(rows.iter().filter(|r| r["event"] == "session/resume").count(), 10);
    assert!(!rows.iter().any(|r| r["event"] == "session/load"));
}
