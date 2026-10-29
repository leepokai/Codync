//! Built-in stdio MCP servers: `chat` sends the user messages, reacts to messages, changes the bot's name and instructions and saves skills; `team` discovers and asks other bots; `manage`
//! creates bots and changes any bot's setup, and `computer` operates the desktop, when enabled. Calls go through authenticated loopback
//! HTTP to the running host, which owns delegation and screen control.

use anyhow::Result;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

mod remote;
pub use remote::serve_remote;

pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// ACP `McpServer` for `codync-host mcp <args…> --port <port>`. Its env names the data directory, where the token
/// is: some agents start MCP servers with a filtered environment (Hermes keeps only PATH, HOME and a few more), so a
/// custom `CODYNC_HOME` wouldn't reach the server otherwise.
pub fn acp_server(name: &str, exe: &std::path::Path, args: &[&str], port: u16) -> Value {
    let port = port.to_string();
    let args: Vec<&str> = ["mcp"].into_iter().chain(args.iter().copied()).chain(["--port", &port]).collect();
    json!({
        "name": name, "command": exe, "args": args,
        "env": [{"name": "CODYNC_HOME", "value": crate::service::data_dir()}],
    })
}

#[derive(Clone, Copy)]
pub enum Server {
    Chat,
    Connectors,
    Routines,
    Computer,
    Team,
    Manage,
    Memory,
    Composio,
    /// For agents on this computer that aren't Codync bots; `bot` is the agent's name.
    Network,
}

const INSTRUCTIONS: &str = "Operate this computer's apps like a person would, in the background where you can: \
find the window (list_windows, list_apps, launch_app), look at it (get_window_state: its controls and a screenshot), \
act on a control by its element_token, then look again. Use pixel coordinates only for what has no control. \
Never type a password yourself: use type_login with a saved login (request_login asks the user for one). \
Don't approve payments: ask the user to do that from their phone.";

/// Serves MCP on stdio until the agent closes it.
pub async fn serve(bot: String, port: u16, server: Server) -> Result<()> {
    let token = std::fs::read_to_string(crate::service::data_dir().join("token")).unwrap_or_default();
    let token: Arc<str> = token.trim().into();
    let bot: Arc<str> = bot.into();
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    // Requests run side by side (a manage approval can take minutes); their replies share stdout.
    let (replies, mut outbox) = mpsc::unbounded_channel::<Value>();
    tokio::spawn(async move {
        let mut out = tokio::io::stdout();
        while let Some(msg) = outbox.recv().await {
            let mut line = serde_json::to_vec(&msg)?;
            line.push(b'\n');
            out.write_all(&line).await?;
            out.flush().await?;
        }
        anyhow::Ok(())
    });
    let (name, instructions, available_tools) = match server {
        Server::Connectors => {
            ("codync-connectors", crate::market::requests::INSTRUCTIONS, crate::market::requests::tools())
        }
        // Listed by the host: they depend on this computer's driver.
        Server::Computer => ("codync-computer", INSTRUCTIONS, Value::Null),
        Server::Routines => ("codync-routines", crate::routines::INSTRUCTIONS, crate::routines::tools()),
        Server::Chat => {
            let mut tools = crate::chat::outbox::tools();
            if let Some(list) = tools.as_array_mut() {
                list.extend(crate::chat::artifacts::tools());
                list.extend(crate::chat::profile::tools());
                list.extend(crate::chat::fallbacks::tools());
                list.push(crate::chat::reactions::tool());
                list.push(crate::chat::skills::tool());
            }
            ("codync-chat", crate::chat::outbox::INSTRUCTIONS, tools)
        }
        Server::Team => ("codync-team", crate::chat::team::INSTRUCTIONS, crate::chat::team::tools()),
        Server::Manage => ("codync-manage", crate::chat::manage::INSTRUCTIONS, crate::chat::manage::tools()),
        Server::Memory => ("codync-memory", crate::chat::memory::INSTRUCTIONS, crate::chat::memory::tools()),
        Server::Composio => {
            ("codync-composio", crate::market::composio::INSTRUCTIONS, crate::market::composio::tools())
        }
        Server::Network => {
            ("codync-network", crate::chat::team::assistant::INSTRUCTIONS, crate::chat::team::assistant::tools())
        }
    };
    // Requests still running, by id: a cancelled one stops, and its host call with it (a team change's card expires).
    let mut running = std::collections::HashMap::<String, tokio::task::AbortHandle>::new();
    while let Some(line) = lines.next_line().await? {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        let Some(id) = msg.get("id").filter(|id| !id.is_null()).cloned() else {
            if msg["method"] == "notifications/cancelled"
                && let Some(request) = running.remove(&msg["params"]["requestId"].to_string())
            {
                request.abort();
            }
            continue; // other notifications (`notifications/initialized`…)
        };
        running.retain(|_, request| !request.is_finished());
        let key = id.to_string();
        let (replies, token, bot, available_tools) =
            (replies.clone(), token.clone(), bot.clone(), available_tools.clone());
        let request = tokio::spawn(async move {
            let params = &msg["params"];
            let reply = match msg["method"].as_str().unwrap_or_default() {
                "initialize" => Ok(json!({
                    "protocolVersion": params["protocolVersion"].as_str().unwrap_or(PROTOCOL_VERSION),
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": name, "version": env!("CARGO_PKG_VERSION")},
                    "instructions": instructions,
                })),
                "ping" => Ok(json!({})),
                "tools/list" => Ok(match server {
                    Server::Computer => computer_tools(port, &token).await,
                    _ => json!({"tools": available_tools}),
                }),
                "tools/call" => Ok(call(port, &token, &bot, params, server).await),
                method => Err(json!({"code": -32601, "message": format!("unknown method {method}")})),
            };
            let _ = replies.send(match reply {
                Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
            });
        });
        running.insert(key, request.abort_handle());
    }
    // The agent is gone: replies still pending have no one to read them.
    Ok(())
}

/// The `computer` tools the host offers; none when it can't be reached.
async fn computer_tools(port: u16, token: &str) -> Value {
    let res = crate::http()
        .post(format!("http://127.0.0.1:{port}/api/computerTools"))
        .bearer_auth(token)
        .json(&json!({}))
        .timeout(Duration::from_secs(30))
        .send()
        .await;
    match res {
        Ok(r) if r.status().is_success() => r.json::<Value>().await.unwrap_or_else(|_| json!({"tools": []})),
        _ => json!({"tools": []}),
    }
}

/// One tool call through the host. Failures are tool errors the agent can read, not protocol errors.
async fn call(port: u16, token: &str, bot: &str, params: &Value, server: Server) -> Value {
    let body = json!({
        "botId": bot,
        "name": params["name"],
        "arguments": params.get("arguments").filter(|a| a.is_object()).cloned().unwrap_or_else(|| json!({})),
    });
    let (method, timeout) = match server {
        Server::Connectors => ("connectorCall", Duration::from_secs(90)),
        Server::Routines => ("routineCall", Duration::from_secs(30)),
        Server::Computer => ("computerCall", Duration::from_mins(1)),
        Server::Chat => ("chatCall", Duration::from_secs(if params["name"] == "send_file" { 70 } else { 30 })),
        Server::Team => ("teamCall", Duration::from_secs(30)),
        Server::Manage => ("manageCall", crate::chat::manage::APPROVAL_TIMEOUT + Duration::from_secs(30)),
        Server::Memory => ("memoryCall", Duration::from_mins(2)),
        Server::Composio => ("composioCall", Duration::from_mins(2)),
        // The first listing may wait for the other computers.
        Server::Network => ("agentCall", Duration::from_secs(30)),
    };
    let res = crate::http()
        .post(format!("http://127.0.0.1:{port}/api/{method}"))
        .bearer_auth(token)
        .json(&body)
        .timeout(timeout)
        .send()
        .await;
    let error = match res {
        Ok(r) => {
            let ok = r.status().is_success();
            match r.json::<Value>().await {
                Ok(v) if ok => {
                    return match server {
                        Server::Computer if v["isError"] == true => json!({"content": v["content"], "isError": true}),
                        Server::Computer => json!({"content": v["content"]}),
                        Server::Chat
                        | Server::Team
                        | Server::Manage
                        | Server::Memory
                        | Server::Routines
                        | Server::Network
                        | Server::Connectors => {
                            json!({"content": [{"type": "text", "text": v.to_string()}]})
                        }
                        Server::Composio => json!({"content": [{"type": "text", "text": v["result"].to_string()}]}),
                    };
                }
                Ok(v) => v["error"].as_str().unwrap_or("the Codync host refused the call").to_owned(),
                Err(e) => format!("bad response from the Codync host: {e}"),
            }
        }
        Err(e) => format!("can't reach the Codync host: {e}"),
    };
    json!({"content": [{"type": "text", "text": error}], "isError": true})
}

/// Inject secrets into the connector process, never the agent's MCP definition.
pub async fn serve_local(connector: String, port: u16) -> Result<()> {
    use std::process::Stdio;
    let token = tokio::fs::read_to_string(crate::service::data_dir().join("token")).await?;
    let res = crate::http()
        .post(format!("http://127.0.0.1:{port}/api/connectorRuntime"))
        .bearer_auth(token.trim())
        .json(&json!({"id":connector}))
        .send()
        .await?;
    if !res.status().is_success() {
        anyhow::bail!("Connector credentials unavailable; check Credentials in Codync");
    }
    let v: Value = res.json().await?;
    let command = v["command"].as_str().ok_or_else(|| anyhow::anyhow!("Not a local connector"))?;
    let mut child = tokio::process::Command::new(crate::shell::program(command));
    child.args(v["args"].as_array().into_iter().flatten().filter_map(Value::as_str));
    for (k, v) in v["env"].as_object().into_iter().flatten() {
        if let Some(v) = v.as_str() {
            child.env(k, v);
        }
    }
    let status = child
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?
        .wait()
        .await?;
    if !status.success() {
        anyhow::bail!("Connector exited unsuccessfully");
    }
    Ok(())
}
