//! Cancellation through the actual chat MCP stdio process and HTTP bridge, not a direct chatCall.
//! Synthetic history only. Run with --nocapture to retain the request IDs and protocol replies.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::mpsc;

struct ChatMcp {
    child: Child,
    input: Option<ChildStdin>,
    replies: mpsc::UnboundedReceiver<Value>,
    reader: Option<tokio::task::JoinHandle<()>>,
    seen: Vec<Value>,
}

impl Drop for ChatMcp {
    fn drop(&mut self) {
        if let Some(reader) = self.reader.take() {
            reader.abort(); // Only this probe's reader; the child has kill_on_drop for failed tests.
        }
    }
}

impl ChatMcp {
    async fn start(host: &Host, bot: &str) -> Self {
        let descriptor = host.home.join("bots").join(bot).join("workspace/servers.json");
        let mut servers = Value::Null;
        common::eventually("the fixture's complete MCP descriptor", Duration::from_secs(20), async || {
            let parsed = std::fs::read(&descriptor).ok().and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
            if let Some(parsed) =
                parsed.filter(|v| v.as_array().is_some_and(|list| list.iter().any(|s| s["name"] == "chat")))
            {
                servers = parsed;
                true
            } else {
                false
            }
        })
        .await;
        let chat = servers.as_array().unwrap().iter().find(|s| s["name"] == "chat").unwrap();
        let binary = PathBuf::from(chat["command"].as_str().unwrap());
        assert_eq!(
            binary.canonicalize().unwrap(),
            PathBuf::from(env!("CARGO_BIN_EXE_codync-host")).canonicalize().unwrap()
        );
        let args: Vec<_> = chat["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
        assert!(args.contains(&bot));
        assert!(args.contains(&host.base.rsplit(':').next().unwrap()));
        let env: Vec<_> = chat["env"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (e["name"].as_str().unwrap(), e["value"].as_str().unwrap()))
            .collect();
        let home = env.iter().find(|(key, _)| *key == "CODYNC_HOME").unwrap().1;
        assert_eq!(PathBuf::from(home).canonicalize().unwrap(), host.home.canonicalize().unwrap());
        let mut child = Command::new(&binary)
            .args(args)
            .env_remove("CODYNC_HOME")
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        eprintln!("chat MCP binding: pid={:?}, candidate={}", child.id(), binary.display());
        let input = child.stdin.take();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let (tx, replies) = mpsc::unbounded_channel();
        let reader = tokio::spawn(async move {
            while let Some(line) = lines.next_line().await.unwrap() {
                let reply: Value = serde_json::from_str(&line).unwrap();
                if tx.send(reply).is_err() {
                    break;
                }
            }
        });
        let mut probe = Self { child, input, replies, reader: Some(reader), seen: vec![] };
        probe.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"decision-cancel-test","version":"1"}}})).await;
        assert_eq!(probe.reply(1).await["result"]["serverInfo"]["name"], "codync-chat");
        probe.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"})).await;
        probe.send(json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}})).await;
        assert!(
            probe.reply(2).await["result"]["tools"].as_array().unwrap().iter().any(|t| t["name"] == "request_decision")
        );
        probe
    }

    async fn send(&mut self, request: Value) {
        eprintln!("MCP request: {request}"); // These tests send only synthetic content, no credentials.
        self.input.as_mut().unwrap().write_all(format!("{request}\n").as_bytes()).await.unwrap();
        self.input.as_mut().unwrap().flush().await.unwrap();
    }

    async fn reply(&mut self, id: i64) -> Value {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let response = self.replies.recv().await.expect("MCP stdout closed before its reply");
                eprintln!("MCP response: {response}");
                self.seen.push(response.clone());
                if response["id"] == id {
                    return response;
                }
            }
        })
        .await
        .expect("MCP did not answer in time")
    }

    async fn ask(&mut self, id: i64) {
        self.send(json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{
            "name":"request_decision","arguments":{"question":format!("Synthetic question {id}"),"options":["Left","Right"]}}})).await;
    }

    async fn ping(&mut self, id: i64) {
        self.send(json!({"jsonrpc":"2.0","id":id,"method":"ping","params":{}})).await;
        assert_eq!(self.reply(id).await["result"], json!({}));
    }

    async fn close(&mut self) {
        drop(self.input.take()); // EOF, not kill: the MCP server should exit on its own.
        let status = tokio::time::timeout(Duration::from_secs(10), self.child.wait())
            .await
            .expect("MCP did not exit after stdin EOF")
            .unwrap();
        assert!(status.success());
        let mut reader = self.reader.take().unwrap();
        let finished = tokio::time::timeout(Duration::from_secs(5), &mut reader).await;
        if finished.is_err() {
            reader.abort();
        }
        finished.expect("MCP stdout reader did not reach EOF").expect("MCP stdout reader failed");
        while let Ok(response) = self.replies.try_recv() {
            eprintln!("MCP trailing response: {response}");
            self.seen.push(response);
        }
    }
}

async fn wait_card(host: &Host, bot: &str, question: &str, status: &str) -> Value {
    let mut found = Value::Null;
    common::eventually("the choice card state", Duration::from_secs(20), async || {
        let history = host.call("history", json!({"botId":bot})).await;
        found = history["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| {
                e["data"]["decision"] == true && e["data"]["question"] == question && e["data"]["status"] == status
            })
            .cloned()
            .unwrap_or(Value::Null);
        !found.is_null()
    })
    .await;
    found
}

async fn working_bot(host: &Host) -> String {
    let bot = host.fixture_bot("Synthetic cancellation", "team_agent.py", "").await;
    host.call("send", json!({"botId":bot,"text":"BLOCK"})).await;
    let log = host.home.join("bots").join(&bot).join("workspace/prompts.jsonl");
    common::eventually("the fixture received its BLOCK prompt", Duration::from_secs(20), async || {
        std::fs::read_to_string(&log)
            .ok()
            .and_then(|raw| serde_json::from_str::<String>(&raw).ok())
            .is_some_and(|prompt| prompt.contains("BLOCK"))
    })
    .await;
    bot
}

async fn assert_unanswered_and_working(host: &Host, bot: &str) {
    let history = host.call("history", json!({"botId":bot})).await;
    assert!(history["entries"].as_array().unwrap().iter().all(|e| e["data"]["from"] != "decision"));
    let snapshot = host.snapshot().await;
    assert_eq!(snapshot["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot).unwrap()["status"], "working");
    assert_eq!(host.log(bot, "prompts.jsonl").len(), 1, "cancellation must not prompt another turn");
}

#[tokio::test]
async fn cancelled_mcp_request_expires_without_answer_and_same_server_recovers() {
    let host = Host::start().await;
    let bot = working_bot(&host).await;
    let mut mcp = ChatMcp::start(&host, &bot).await;
    mcp.ask(7).await;
    let card = wait_card(&host, &bot, "Synthetic question 7", "pending").await;
    mcp.send(json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":7,"reason":"synthetic cancellation"}})).await;
    mcp.ping(8).await;
    wait_card(&host, &bot, "Synthetic question 7", "expired").await;
    assert_unanswered_and_working(&host, &bot).await;
    host.refused("respondDecision", json!({"entryId":card["id"],"option":"Left"})).await;
    mcp.ask(17).await;
    let next = wait_card(&host, &bot, "Synthetic question 17", "pending").await;
    assert_eq!(next["turn"], card["turn"]);
    host.call("respondDecision", json!({"entryId":next["id"],"option":"Right"})).await;
    let reply = mcp.reply(17).await;
    assert_ne!(reply["result"]["isError"], true);
    let choice: Value = serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(choice["choice"], "Right");
    mcp.ping(18).await;
    assert_eq!(host.log(&bot, "prompts.jsonl").len(), 1);
    mcp.close().await;
    assert!(mcp.seen.iter().all(|r| r["id"] != 7), "a cancelled request must not send a response");
    host.call("stop", json!({"botId":bot})).await;
}

#[tokio::test]
async fn unknown_and_differently_typed_cancel_ids_leave_the_live_choice_answerable() {
    let host = Host::start().await;
    let bot = working_bot(&host).await;
    let mut mcp = ChatMcp::start(&host, &bot).await;
    mcp.ask(7).await;
    let card = wait_card(&host, &bot, "Synthetic question 7", "pending").await;
    for request in [json!(999), json!("7")] {
        mcp.send(json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":request}})).await;
    }
    mcp.ping(8).await;
    host.call("respondDecision", json!({"entryId":card["id"],"option":"Left"})).await;
    let reply = mcp.reply(7).await;
    assert_ne!(reply["result"]["isError"], true);
    let choice: Value = serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(choice["choice"], "Left");
    wait_card(&host, &bot, "Synthetic question 7", "answered").await;
    // Cancelling a completed ID cannot change the completed card.
    mcp.send(json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":7}})).await;
    mcp.ping(9).await;
    wait_card(&host, &bot, "Synthetic question 7", "answered").await;
    mcp.close().await;
    host.call("stop", json!({"botId":bot})).await;
}

#[tokio::test]
async fn mcp_stdin_eof_expires_the_unanswered_card_without_stopping_the_bot() {
    let host = Host::start().await;
    let bot = working_bot(&host).await;
    let mut mcp = ChatMcp::start(&host, &bot).await;
    mcp.ask(7).await;
    let card = wait_card(&host, &bot, "Synthetic question 7", "pending").await;
    mcp.close().await;
    wait_card(&host, &bot, "Synthetic question 7", "expired").await;
    assert_unanswered_and_working(&host, &bot).await;
    host.refused("respondDecision", json!({"entryId":card["id"],"option":"Left"})).await;
    host.call("stop", json!({"botId":bot})).await;
}
