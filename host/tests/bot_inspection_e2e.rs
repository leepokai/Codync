//! Cross-bot reads and approvals through the real team MCP subprocess and disposable hosts.
//! No provider calls, production data, direct forwarding shortcuts or parallel host tests.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

async fn wait(host: &Host, bot: &str, found: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let history = host.call("history", json!({"botId": bot})).await;
        if let Some(entry) = history["entries"].as_array().unwrap().iter().find(|e| found(e)) {
            return entry.clone();
        }
        assert!(Instant::now() < deadline, "{bot}: {history}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn fixture(host: &Host, name: &str) -> String {
    let bot = host.fixture_bot(name, "team_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "permission": "ask"})).await;
    bot
}

async fn permission(host: &Host, bot: &str) -> Value {
    host.call("send", json!({"botId": bot, "text": "PERMISSION inspect this change"})).await;
    wait(host, bot, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await
}

async fn ask(host: &Host, lead: &str, name: &str, args: Value) -> Value {
    host.call("send", json!({"botId": lead, "text": format!("TEAM {name} {args}")})).await;
    wait(host, lead, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await
}

fn result(entry: &Value) -> Value {
    let text = entry["data"]["text"].as_str().unwrap();
    serde_json::from_str(text.strip_prefix("Done: ").unwrap_or_else(|| panic!("team tool failed: {text}"))).unwrap()
}

async fn approve(host: &Host, lead: &str, card: &Value) -> Value {
    host.call("respondPermission", json!({"entryId": card["id"], "optionId": "allow"})).await;
    let answer =
        wait(host, lead, |e| e["kind"] == "agent" && e["data"]["final"] == true && e["turn"] == card["turn"]).await;
    result(&answer)
}

async fn read(host: &Host, lead: &str, name: &str, args: Value) -> Value {
    host.reply(lead, &format!("TEAM {name} {args}")).await;
    let history = host.call("history", json!({"botId": lead})).await;
    let answer = history["entries"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|e| e["kind"] == "agent" && e["data"]["final"] == true)
        .unwrap();
    result(answer)
}

#[tokio::test]
async fn mcp_reads_another_bots_status_and_recent_messages_without_waking_it() {
    let host = Host::start().await;
    let lead = fixture(&host, "Lead").await;
    let worker = fixture(&host, "Worker").await;
    let idle = read(&host, &lead, "bot_status", json!({"botId": worker})).await;
    assert_eq!(idle["status"], "idle");
    assert_eq!(idle["activity"], "");
    host.reply(&worker, "first message").await;
    host.reply(&worker, "second message").await;
    let card = permission(&host, &worker).await;
    let before = host.call("history", json!({"botId": worker})).await;
    let prompts = host.log(&worker, "prompts.jsonl");

    let status = read(&host, &lead, "bot_status", json!({"botId": worker, "messages": 3})).await;
    assert_eq!(status["id"], worker);
    assert_eq!(status["status"], "needsInput");
    assert!(status["activity"].as_str().unwrap().contains("Edit a file"));
    assert_eq!(status["approval"]["request"], "Edit a file");
    assert_eq!(status["approval"]["options"], json!(["Allow"]));
    let recent = read(&host, &lead, "recent_messages", json!({"botId": worker, "count": 3})).await;
    assert_eq!(
        recent["messages"],
        json!([
            {"from": "User", "text": "second message"},
            {"from": "Worker", "text": "reply: second message"},
            {"from": "User", "text": "PERMISSION inspect this change"}
        ])
    );
    assert_eq!(status["recentMessages"], recent["messages"]);
    assert_eq!(host.call("history", json!({"botId": worker})).await, before);
    assert_eq!(host.log(&worker, "prompts.jsonl"), prompts, "reading queues no target turn");
    assert_eq!(card["data"]["status"], "pending");
}

#[tokio::test]
async fn mcp_forwards_an_approval_after_more_than_thirty_seconds() {
    let host = Host::start().await;
    let lead = fixture(&host, "Lead").await;
    let worker = fixture(&host, "Worker").await;
    let original = permission(&host, &worker).await;
    let card = ask(&host, &lead, "answer_approval", json!({"botId": worker, "option": "Allow"})).await;
    let started = Instant::now();
    tokio::time::sleep(Duration::from_secs(31)).await;
    let still_waiting = wait(&host, &worker, |e| e["id"] == original["id"]).await;
    assert_eq!(still_waiting["data"]["status"], "pending");
    assert_eq!(approve(&host, &lead, &card).await, json!({"answered": "Allow"}));
    assert!(started.elapsed() > Duration::from_secs(30));
    // No direct respondPermission on the worker: the team tool must deliver the response.
    wait(&host, &worker, |e| e["id"] == original["id"] && e["data"]["status"] == "answered").await;
    wait(&host, &worker, |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
}

#[tokio::test]
async fn mcp_stop_also_waits_more_than_thirty_seconds_for_the_user() {
    let host = Host::start().await;
    let lead = fixture(&host, "Lead").await;
    let worker = fixture(&host, "Worker").await;
    let original = permission(&host, &worker).await;
    let card = ask(&host, &lead, "stop_bot", json!({"botId": worker})).await;
    tokio::time::sleep(Duration::from_secs(31)).await;
    assert_eq!(approve(&host, &lead, &card).await, json!({"stopRequested": true}));
    wait(&host, &worker, |e| e["id"] == original["id"] && e["data"]["status"] != "pending").await;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let status = read(&host, &lead, "bot_status", json!({"botId": worker})).await;
        if status["status"] == "idle" {
            break;
        }
        assert!(Instant::now() < deadline, "worker did not finish cancelling: {status}");
    }
}

#[tokio::test]
async fn a_stale_stop_card_leaves_a_new_turn_and_queued_messages_alone() {
    let host = Host::start().await;
    let lead = fixture(&host, "Lead").await;
    let worker = fixture(&host, "Worker").await;
    let original = permission(&host, &worker).await;
    let card = ask(&host, &lead, "stop_bot", json!({"botId": worker})).await;
    host.call("respondPermission", json!({"entryId": original["id"], "optionId": "allow"})).await;
    wait(&host, &worker, |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    let replacement = permission(&host, &worker).await;
    let queued = host.call("send", json!({"botId": worker, "text": "next task"})).await["entry"]["id"].clone();
    let outcome = approve(&host, &lead, &card).await;
    assert_eq!(outcome["done"], false);
    assert_eq!(outcome["stale"], true);
    assert!(outcome.get("stopRequested").is_none());
    let untouched = wait(&host, &worker, |e| e["id"] == replacement["id"]).await;
    assert_eq!(untouched["data"]["status"], "pending");
    let pending = wait(&host, &worker, |e| e["id"] == queued).await;
    assert_eq!(pending["data"]["status"], "queued");
    host.call("respondPermission", json!({"entryId": replacement["id"], "optionId": "allow"})).await;
    wait(&host, &worker, |e| {
        e["kind"] == "agent" && e["data"]["final"] == true && e["data"]["text"] == "reply: next task"
    })
    .await;
}

async fn stale_answer_after_target_changed(answer_directly: bool) {
    let host = Host::start().await;
    let lead = fixture(&host, "Lead").await;
    let worker = fixture(&host, "Worker").await;
    let original = permission(&host, &worker).await;
    let card = ask(&host, &lead, "answer_approval", json!({"botId": worker, "option": "Allow"})).await;
    if answer_directly {
        // Deliberately create the race: the user answers the original card while the lead is waiting.
        host.call("respondPermission", json!({"entryId": original["id"], "optionId": "allow"})).await;
        wait(&host, &worker, |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    } else {
        host.call("stop", json!({"botId": worker})).await;
    }
    wait(&host, &worker, |e| e["id"] == original["id"] && e["data"]["status"] != "pending").await;
    let replacement = permission(&host, &worker).await;
    assert_ne!(replacement["id"], original["id"]);
    let outcome = approve(&host, &lead, &card).await;
    assert_eq!(outcome["done"], false);
    assert_eq!(outcome["stale"], true);
    assert!(outcome.get("answered").is_none());
    let untouched = wait(&host, &worker, |e| e["id"] == replacement["id"]).await;
    assert_eq!(untouched["data"]["status"], "pending", "never answer the replacement card");
}

#[tokio::test]
async fn mcp_reports_a_directly_answered_card_as_stale() {
    stale_answer_after_target_changed(true).await;
}

#[tokio::test]
async fn mcp_reports_a_stopped_turns_card_as_stale() {
    stale_answer_after_target_changed(false).await;
}

/// A real stdio MCP process for initialization, approval expiry and cancellation checks.
struct Mcp {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
}

impl Mcp {
    fn start(home: &Path, port: u16, bot: &str, instructions: Option<&str>) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_codync-host"));
        command
            .args(["mcp", "team", "--bot", bot, "--port", &port.to_string()])
            .env("CODYNC_HOME", home)
            .env_remove("CODYNC_TEAM_INSTRUCTIONS")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true);
        if let Some(instructions) = instructions {
            command.env("CODYNC_TEAM_INSTRUCTIONS", instructions);
        }
        let mut child = command.spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap()).lines();
        Self { child, stdin, stdout }
    }

    async fn send(&mut self, request: Value) {
        self.stdin.write_all(format!("{request}\n").as_bytes()).await.unwrap();
        self.stdin.flush().await.unwrap();
    }

    async fn rpc(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).await;
        self.receive(id, Duration::from_secs(15)).await
    }

    async fn receive(&mut self, id: u64, within: Duration) -> Value {
        let line = tokio::time::timeout(within, self.stdout.next_line()).await.unwrap().unwrap().unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["id"], id);
        assert!(reply.get("error").is_none(), "{reply}");
        reply["result"].clone()
    }

    async fn close(mut self) {
        drop(self.stdin);
        let status = tokio::time::timeout(Duration::from_secs(10), self.child.wait()).await.unwrap().unwrap();
        assert!(status.success(), "{status}");
    }
}

/// Both actions use `confirm`, but each must keep its target untouched through the real 270-second deadline.
/// Run serially: the elapsed wall time is intentional, never shortened with a test-only timeout or paused clock.
async fn expired_action_recovers(name: &str) {
    let host = Host::start().await;
    let lead = fixture(&host, "Lead").await;
    let worker = fixture(&host, "Worker").await;
    let original = permission(&host, &worker).await;
    let before = host.call("history", json!({"botId": worker})).await;
    let prompts = host.log(&worker, "prompts.jsonl");
    host.call("send", json!({"botId": lead, "text": "BLOCK"})).await;
    let deadline = Instant::now() + Duration::from_secs(30);
    while host.log(&lead, "prompts.jsonl").is_empty() {
        assert!(Instant::now() < deadline, "lead did not start its turn");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let port = host.base.rsplit(':').next().unwrap().parse().unwrap();
    let mut mcp = Mcp::start(&host.home, port, &lead, None);
    mcp.rpc(1, "initialize", json!({"protocolVersion": "2025-06-18"})).await;
    let arguments = json!({"botId": worker, "option": "Allow"});
    let started = Instant::now();
    mcp.send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
        "name": name, "arguments": arguments,
    }}))
    .await;
    let expired = wait(&host, &lead, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await;
    let outcome = mcp.receive(2, Duration::from_secs(300)).await;
    let elapsed = started.elapsed();
    eprintln!("{name}: natural approval expiry after {:.3}s; MCP result: {outcome}", elapsed.as_secs_f64());
    assert!(elapsed >= Duration::from_secs(270), "approval ended early: {elapsed:?}");
    assert_eq!(outcome["isError"], true, "expiry must not report success: {outcome}");
    assert!(outcome["content"][0]["text"].as_str().unwrap().contains("four and a half minutes: nothing changed"));
    wait(&host, &lead, |e| e["id"] == expired["id"] && e["data"]["status"] == "expired").await;
    assert_eq!(
        host.call("history", json!({"botId": worker})).await,
        before,
        "expiry must not answer or stop the target"
    );
    assert_eq!(host.log(&worker, "prompts.jsonl"), prompts, "expiry must not start another target turn");

    // Use the same MCP connection and source turn after expiry; approve only the new source card.
    mcp.send(json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {
        "name": name, "arguments": arguments,
    }}))
    .await;
    let next = wait(&host, &lead, |e| {
        e["kind"] == "permission" && e["data"]["status"] == "pending" && e["id"] != expired["id"]
    })
    .await;
    host.call("respondPermission", json!({"entryId": next["id"], "optionId": "allow"})).await;
    let resumed = mcp.receive(3, Duration::from_secs(15)).await;
    assert_ne!(resumed["isError"], true, "{resumed}");
    let resumed: Value = serde_json::from_str(resumed["content"][0]["text"].as_str().unwrap()).unwrap();
    let expected =
        if name == "answer_approval" { json!({"answered": "Allow"}) } else { json!({"stopRequested": true}) };
    assert_eq!(resumed, expected, "the next action must recover normally");
    wait(&host, &lead, |e| e["id"] == next["id"] && e["data"]["status"] == "answered").await;
    let target_status = if name == "answer_approval" { "answered" } else { "cancelled" };
    wait(&host, &worker, |e| e["id"] == original["id"] && e["data"]["status"] == target_status).await;
    eprintln!(
        "{name}: source expired; target stayed unchanged; next source approval accepted and target {target_status}"
    );
    mcp.close().await;
}

#[tokio::test]
async fn mcp_answer_naturally_expires_after_270_seconds_and_recovers() {
    expired_action_recovers("answer_approval").await;
}

#[tokio::test]
async fn mcp_stop_naturally_expires_after_270_seconds_and_recovers() {
    expired_action_recovers("stop_bot").await;
}

#[tokio::test]
async fn mcp_uses_supplied_instructions_without_a_second_http_request() {
    let host = Host::start().await;
    // A listening socket deliberately never answers: any unnecessary lookup remains detectable.
    let trap = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut mcp = Mcp::start(&host.home, trap.local_addr().unwrap().port(), "lead", Some("Already rendered"));
    let initialized = mcp.rpc(1, "initialize", json!({"protocolVersion": "2025-06-18"})).await;
    assert_eq!(initialized["instructions"], "Already rendered");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), trap.accept()).await.is_err(),
        "looked up instructions despite env"
    );
    mcp.close().await;
}

#[tokio::test]
async fn mcp_fallback_instructions_render_the_conservative_ask_policy() {
    let host = Host::start().await;
    // Port zero cannot be a reachable host: initialization must render the local fallback.
    let mut mcp = Mcp::start(&host.home, 0, "lead", None);
    let initialized = mcp.rpc(1, "initialize", json!({"protocolVersion": "2025-06-18"})).await;
    let instructions = initialized["instructions"].as_str().unwrap();
    assert!(instructions.contains("bot_status") && instructions.contains("they choose"));
    assert!(!instructions.contains("{inspect}") && !instructions.contains("{auto}"));
    assert!(!instructions.contains("act at once"));
    mcp.close().await;
}

#[tokio::test]
async fn cancelling_a_team_mcp_call_expires_its_card_without_answering_the_target() {
    let host = Host::start().await;
    let lead = fixture(&host, "Lead").await;
    let worker = fixture(&host, "Worker").await;
    let original = permission(&host, &worker).await;
    host.call("send", json!({"botId": lead, "text": "BLOCK"})).await;
    let deadline = Instant::now() + Duration::from_secs(30);
    while host.log(&lead, "prompts.jsonl").is_empty() {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let port = host.base.rsplit(':').next().unwrap().parse().unwrap();
    let mut mcp = Mcp::start(&host.home, port, &lead, None);
    mcp.rpc(1, "initialize", json!({"protocolVersion": "2025-06-18"})).await;
    mcp.send(json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": {
        "name": "answer_approval", "arguments": {"botId": worker, "option": "Allow"}
    }}))
    .await;
    let card = wait(&host, &lead, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await;
    mcp.send(json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 7}})).await;
    wait(&host, &lead, |e| e["id"] == card["id"] && e["data"]["status"] == "expired").await;
    let untouched = wait(&host, &worker, |e| e["id"] == original["id"]).await;
    assert_eq!(untouched["data"]["status"], "pending");
    mcp.close().await;
}
