//! A remote connector's key edited while a bot whose agent can't reopen a session has it running, through a real host,
//! a fixture agent and a stateful MCP server: the agent starts again with a recap of the chat, and its next tool call
//! reaches the server with the new key.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::{Json, Router, http::HeaderMap, http::StatusCode, response::IntoResponse, routing};
use common::host::Host;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// A streamable HTTP server like the SDKs' stateful ones: `initialize` opens a session, and any other request without
/// one is a bad request. `whoami` answers the key it was sent.
async fn stateful_server() -> String {
    let sessions = Arc::new(Mutex::new(HashSet::new()));
    let mcp = move |headers: HeaderMap, Json(msg): Json<Value>| {
        let sessions = sessions.clone();
        async move {
            let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok()).map(str::to_owned);
            if msg["method"] == "initialize" {
                let session = uuid::Uuid::new_v4().to_string();
                sessions.lock().unwrap().insert(session.clone());
                let result = json!({"protocolVersion": "2025-06-18", "capabilities": {"tools": {}},
                    "serverInfo": {"name": "keyed", "version": "1"}});
                let reply = Json(json!({"jsonrpc": "2.0", "id": msg["id"], "result": result}));
                return ([("mcp-session-id", session)], reply).into_response();
            }
            if !header("mcp-session-id").is_some_and(|s| sessions.lock().unwrap().contains(&s)) {
                return (StatusCode::BAD_REQUEST, Json(json!({"error": "No valid session ID provided"})))
                    .into_response();
            }
            let result = match msg["method"].as_str() {
                None | Some("notifications/initialized") => return StatusCode::ACCEPTED.into_response(),
                Some("tools/list") => json!({"tools": [{"name": "whoami", "inputSchema": {"type": "object"}}]}),
                Some(_) => json!({"content": [{"type": "text", "text": header("x-key").unwrap_or_default()}]}),
            };
            Json(json!({"jsonrpc": "2.0", "id": msg["id"], "result": result})).into_response()
        }
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, Router::new().route("/mcp", routing::post(mcp))).await });
    url
}

/// The text of `bot`'s newest final reply.
async fn last_reply(host: &Host, bot: &str) -> String {
    let history = host.call("history", json!({"botId": bot})).await;
    let entries = history["entries"].as_array().unwrap();
    let reply = entries.iter().rfind(|e| e["kind"] == "agent" && e["data"]["final"] == true).unwrap();
    reply["data"]["text"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn an_edited_key_restarts_an_agent_that_cant_reopen_with_a_recap_on_the_new_connection() {
    if common::python_missing() {
        return;
    }
    let url = stateful_server().await;
    let host = Host::start().await;
    let connector = json!({"name": "Keyed", "url": url, "headers": {"X-Key": "old-key"}});
    let connector = host.call("installConnector", connector).await["connector"]["id"].as_str().unwrap().to_owned();
    let command = common::fixture_command("connector_agent.py", "");
    let bot =
        json!({"name": "Keys", "backend": "custom", "command": command, "connectors": [connector], "notify": false});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();

    host.reply(&bot, "Which key do you use?").await;
    assert_eq!(last_reply(&host, &bot).await, "connector said old-key");

    // The running proxy can't carry the new key to a server that needs its session: the agent starts again.
    let checked = json!({"id": connector, "fields": {"X-Key": "new-key"}});
    assert_eq!(host.call("credentialUpdateConnector", checked).await["status"], "ready");
    host.reply(&bot, "And now?").await;
    assert_eq!(last_reply(&host, &bot).await, "connector said new-key");
    assert_eq!(host.requests(&bot, "initialize").len(), 2, "a new agent process");
    let prompts = host.prompts(&bot);
    let last = prompts.last().unwrap();
    assert!(last.contains("<handoff>") && last.contains("Which key do you use?"), "told the chat so far: {last}");
    assert!(last.contains("connector said old-key") && last.contains("And now?"), "{last}");
}
