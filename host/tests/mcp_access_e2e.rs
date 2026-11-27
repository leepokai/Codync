//! Outside apps driving threads over MCP (docs/features/mcp-access.md), against a real host and its local API
//! `agentCall` (what `codync-host mcp network --token` calls), with a scripted ACP agent: an app given every bot
//! starts a thread, waits on it, reads it, writes to it and stops another; a retry with the same request id starts
//! or sends nothing more; a narrowed call can't write; a revoked token stops at once.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn call(token: &str, name: &str, arguments: Value) -> Value {
    let agent = json!({"kind": "codex", "session": "mcp-e2e", "process": "1-e2e", "cwd": "/work"});
    let mut body = json!({"agent": agent, "name": name, "mcpToken": token});
    body["arguments"] = arguments;
    body
}

/// The bot's task `id` on the wire, once `check` holds for it.
async fn task(host: &Host, bot: &str, id: &str, check: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let snapshot = host.snapshot().await;
        let row = snapshot["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot).unwrap();
        let found = row["work"].as_array().unwrap().iter().find(|t| t["id"] == id && check(t)).cloned();
        if let Some(found) = found {
            return found;
        }
        assert!(Instant::now() < deadline, "timed out waiting on task {id}: {row}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Gives an app access as the desktop app and the terminal UI do (the host's defaults: every bot, threads without
/// one too) at `access`.
async fn app(host: &Host, label: &str, access: &str) -> (String, Value) {
    let made = host.call("mcpClientCreate", json!({"label": label, "access": access})).await;
    let token = made["token"].as_str().unwrap().to_owned();
    assert!(token.starts_with("cdy_mcp_"), "{made}");
    assert_eq!(made["command"], format!("CODYNC_MCP_TOKEN={token} codync-host mcp network"));
    (token, made["client"].clone())
}

fn with(thread: &Value, more: Value) -> Value {
    let mut args = thread.clone();
    let Value::Object(more) = more else { panic!("arguments are an object") };
    args.as_object_mut().unwrap().extend(more);
    args
}

#[tokio::test]
async fn an_app_at_ask_starts_follows_and_writes_to_a_thread_that_runs_no_freer_than_ask() {
    let host = Host::start().await;
    // An agent with modes, so an Ask thread runs in one that asks (`modes::sync_mode_under`).
    let bot = host.fixture_bot("Mika", "mode_agent.py", "").await;
    let (token, _) = app(&host, "Editor", "ask").await;
    let listed = host.call("mcpClients", json!({})).await;
    assert!(!listed.to_string().contains(&token), "the token is shown once, never listed");
    let tool = async |name: &str, arguments: Value| host.call("agentCall", call(&token, name, arguments)).await;
    let refused = async |name: &str, arguments: Value| host.refused("agentCall", call(&token, name, arguments)).await;

    // It starts a thread on the bot; the same request id again returns that thread.
    let launch = json!({"message": "Fix the build", "botId": bot, "clientRequestId": "r1"});
    let launched = tool("thread_launch", launch.clone()).await;
    assert_eq!(launched["replayed"], false, "{launched}");
    let thread = launched["thread"].clone();
    let id = thread["taskId"].as_str().unwrap().to_owned();
    let again = tool("thread_launch", launch).await;
    assert_eq!((&again["thread"], &again["replayed"]), (&thread, &json!(true)));

    // It waits for it to end, then reads it: it ran in a mode that asks, never Full access.
    let ended = tool("thread_wait", with(&thread, json!({"timeoutSecs": 30}))).await;
    assert_eq!(ended["status"], "done", "{ended}");
    let read = tool("thread_read", thread.clone()).await;
    assert_eq!((&read["thread"]["status"], &read["thread"]["startedBy"]), (&json!("done"), &json!("Editor")), "{read}");
    let row = task(&host, &bot, &id, |t| t["status"] == "done").await;
    assert_eq!((&row["startedBy"]["label"], &row["startedBy"]["level"]), (&json!("Editor"), &json!("ask")), "{row}");
    let result = row["result"].as_str().unwrap();
    assert!(result.starts_with("mode ") && !result.starts_with("mode agent-full-access"), "{result}");

    // A message to it is the app's, framed as such for the agent; the same request id sends nothing more (the
    // launch's id is another tool's: it sends).
    let send = with(&thread, json!({"message": "and the docs", "clientRequestId": "r1"}));
    let first = tool("thread_send", send.clone()).await;
    let second = tool("thread_send", send).await;
    assert!(first["replayed"] == false && first["entryId"].is_string(), "{first}");
    assert_eq!((&second["replayed"], &second["entryId"]), (&json!(true), &first["entryId"]), "{second}");
    let again =
        task(&host, &bot, &id, |t| t["status"] == "done" && t["result"].as_str().unwrap().contains("the docs")).await;
    let result = again["result"].as_str().unwrap();
    assert!(result.contains("a message from Editor, an outside app"), "{result}");
    let steps = tool("thread_read", with(&thread, json!({"view": "steps", "limit": 100}))).await;
    let written = steps.to_string().matches("and the docs").count();
    assert!(written >= 1, "{steps}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let after = tool("thread_read", with(&thread, json!({"view": "steps", "limit": 100}))).await;
    assert_eq!(after.to_string().matches("and the docs").count(), written, "sent once");

    // A call narrowed to reading can't write; nor can it ask for more than Ask.
    let narrowed = json!({"message": "x", "botId": bot, "accessLimit": "readOnly"});
    assert!(refused("thread_launch", narrowed).await.contains("capability_denied"));
    let freer = json!({"message": "x", "botId": bot, "approval": "full"});
    assert!(refused("thread_launch", freer).await.contains("mode_escalation_denied"));
}

#[tokio::test]
async fn an_app_with_full_access_stops_its_thread_and_a_revoked_token_stops_at_once() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Rin", "work_agent.py", "").await;
    // Full only once the computer's maximum allows it.
    host.call("setMcpAccess", json!({"max": "full"})).await;
    let (token, client) = app(&host, "Runner", "full").await;
    let tool = async |name: &str, arguments: Value| host.call("agentCall", call(&token, name, arguments)).await;
    let refused = async |name: &str, arguments: Value| host.refused("agentCall", call(&token, name, arguments)).await;

    let launched =
        tool("thread_launch", json!({"message": "Long one HOLD:two", "botId": bot, "approval": "full"})).await;
    let thread = launched["thread"].clone();
    let id = thread["taskId"].as_str().unwrap().to_owned();
    task(&host, &bot, &id, |t| t["status"] == "working").await;
    let waited = tool("thread_wait", with(&thread, json!({"timeoutSecs": 1}))).await;
    assert_eq!((&waited["status"], &waited["timedOut"]), (&json!("working"), &json!(true)), "{waited}");
    assert!(waited["cursor"].is_string(), "{waited}");
    let listed = tool("thread_list", json!({})).await;
    assert_eq!(listed["threads"][0]["taskId"], json!(id), "{listed}");

    tool("thread_interrupt", thread.clone()).await;
    task(&host, &bot, &id, |t| t["status"] == "stopped").await;

    host.call("mcpClientRevoke", json!({"id": client["id"]})).await;
    assert!(refused("thread_list", json!({})).await.contains("revoked"));
}
