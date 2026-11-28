//! Built-in stdio MCP servers: `chat` sends the user messages, reacts to messages, changes the bot's name and instructions and saves skills; `team` discovers and asks other bots; `manage`
//! creates bots and changes any bot's setup, and `computer` operates the desktop, when enabled. Calls go through authenticated loopback
//! HTTP to the running host, which owns delegation and screen control.

use anyhow::Result;
use serde_json::{Value, json};
use std::sync::{Arc, LazyLock, OnceLock};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

mod push;
mod remote;
pub use remote::serve_remote;

pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// A call that may wait for a session's mail, with time for the host to answer.
const WAIT_REPLY: Duration = Duration::from_secs(crate::chat::team::mailbox::WAIT_MAX_SECS + 15);

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
    Workflows,
    Computer,
    Team,
    Manage,
    Memory,
    Composio,
    /// For coding agents' sessions on this computer that aren't Codync bots (`bot` is empty).
    Network,
}

const INSTRUCTIONS: &str = "Operate this computer's apps like a person would, in the background where you can: \
find the window (list_windows, list_apps, launch_app), look at it (get_window_state: its controls and a screenshot), \
act on a control by its element_token, then look again. Use pixel coordinates only for what has no control. \
Never type a password yourself: use type_login with a saved login (request_login asks the user for one). \
Don't approve payments: ask the user to do that from their phone.";

/// An outside app's token for the `codync` server (`codync-host mcp network --token`), sent with each of its calls.
static APP_TOKEN: OnceLock<String> = OnceLock::new();

/// Serves the `codync` server for a session on this computer, with an outside app's `token` when it has one.
pub async fn serve_network(port: u16, token: Option<String>) -> Result<()> {
    if let Some(token) = token.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()) {
        let _ = APP_TOKEN.set(token);
    }
    serve(String::new(), port, Server::Network).await
}

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
            ("codync-connectors", crate::market::requests::INSTRUCTIONS.to_owned(), crate::market::requests::tools())
        }
        // Listed by the host: they depend on this computer's driver.
        Server::Computer => ("codync-computer", INSTRUCTIONS.to_owned(), Value::Null),
        Server::Routines => ("codync-routines", crate::routines::INSTRUCTIONS.to_owned(), crate::routines::tools()),
        Server::Workflows => {
            ("codync-workflows", crate::workflows::INSTRUCTIONS.to_owned(), crate::workflows::tool_list())
        }
        Server::Chat => {
            let thread = crate::store::is_thread_id(&bot);
            ("codync-chat", crate::chat::outbox::instructions(thread), chat_tools(thread))
        }
        Server::Team => ("codync-team", crate::chat::team::INSTRUCTIONS.to_owned(), crate::chat::team::tools()),
        Server::Manage => ("codync-manage", crate::chat::manage::INSTRUCTIONS.to_owned(), crate::chat::manage::tools()),
        Server::Memory => ("codync-memory", crate::chat::memory::INSTRUCTIONS.to_owned(), crate::chat::memory::tools()),
        Server::Composio => {
            ("codync-composio", crate::market::composio::INSTRUCTIONS.to_owned(), crate::market::composio::tools())
        }
        // The one server agents that aren't bots add as `codync` (the cloud connector's name too); more tools join it.
        Server::Network => {
            ("codync", crate::chat::team::assistant::INSTRUCTIONS.to_owned(), crate::chat::team::assistant::tools())
        }
    };
    // The client's name from `initialize` (`clientInfo.name`): which agent the network server serves.
    let client: Arc<OnceLock<String>> = Arc::default();
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
        let (instructions, client) = (instructions.clone(), client.clone());
        let request = tokio::spawn(async move {
            let params = &msg["params"];
            let reply = match msg["method"].as_str().unwrap_or_default() {
                "initialize" => {
                    let agent = params["clientInfo"]["name"].as_str().unwrap_or_default().to_owned();
                    let first = client.set(agent.clone()).is_ok();
                    if first && matches!(server, Server::Network) && push::wanted(&agent, |k| std::env::var_os(k)) {
                        tokio::spawn(push::run(port));
                    }
                    Ok(json!({
                        "protocolVersion": params["protocolVersion"].as_str().unwrap_or(PROTOCOL_VERSION),
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": name, "version": env!("CARGO_PKG_VERSION")},
                        "instructions": instructions,
                    }))
                }
                "ping" => Ok(json!({})),
                "tools/list" => Ok(match server {
                    Server::Computer => computer_tools(port, &token).await,
                    _ => json!({"tools": available_tools}),
                }),
                "tools/call" => Ok(match server {
                    Server::Network => {
                        network_call(port, &token, client.get().map_or("", String::as_str), params).await
                    }
                    _ => call(port, &token, &bot, params, server).await,
                }),
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

/// The `chat` server's tools, from each part of the host that answers them; a thread's (`thread`) without
/// `work::chat_only`'s, with `ask_chat`.
pub(crate) fn chat_tools(thread: bool) -> Value {
    let mut tools = crate::chat::outbox::tools();
    if let Some(list) = tools.as_array_mut() {
        list.extend(crate::chat::artifacts::tools());
        list.extend(crate::chat::profile::tools());
        list.extend(crate::chat::fallbacks::tools());
        list.push(crate::chat::reactions::tool());
        list.push(crate::chat::skills::tool());
        list.extend(crate::dev_servers::tools());
        list.extend(crate::chat::work::tools());
        if thread {
            list.retain(|tool| !crate::chat::work::chat_only(tool["name"].as_str().unwrap_or_default()));
            list.push(crate::chat::work::ask_chat::tool());
        }
    }
    tools
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

/// A `network` tool call: the host learns which session calls (`network::sessions`). A Codync bot's
/// agent that loads the user's own `codync` server gets nothing from it: the bot has its team tools.
async fn network_call(port: u16, token: &str, client: &str, params: &Value) -> Value {
    // This server's process, with a nonce so a reused pid never takes over another's session.
    static PROCESS: LazyLock<String> =
        LazyLock::new(|| format!("{}-{}", std::process::id(), &uuid::Uuid::new_v4().simple().to_string()[..8]));
    if let Some(refusal) = refuse_bot(std::env::var_os(BOT_ENV)) {
        return refusal;
    }
    let kind = codync_network::AgentKind::of_mcp_client(client);
    let meta = params.get("_meta").cloned().unwrap_or(Value::Null);
    // Claude Code's session is read from its registry: file work, off the async workers.
    let looked = tokio::task::spawn_blocking(move || {
        codync_network::sessions::session_id(kind, |k| std::env::var(k).ok(), &meta)
    });
    let caller = crate::network::sessions::Caller {
        kind,
        session: looked.await.unwrap_or_default(),
        process: PROCESS.clone(),
        cwd: std::env::current_dir().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default(),
    };
    let mut body = json!({"agent": caller});
    if let Some(app) = APP_TOKEN.get() {
        body["mcpToken"] = app.as_str().into();
    }
    send(port, token, Server::Network, params, body).await
}

/// The network server's answer to an agent the host runs for a bot (`BOT_ENV` set): nothing.
fn refuse_bot(bot: Option<std::ffi::OsString>) -> Option<Value> {
    bot.map(|_| {
        let text = "You are a Codync bot: message others with your team tools (list_bots, message_bot).";
        json!({"content": [{"type": "text", "text": text}], "isError": true})
    })
}

/// Set in the environment of every agent the host runs for a bot (its id).
pub const BOT_ENV: &str = "CODYNC_BOT";

/// One tool call through the host. Failures are tool errors the agent can read, not protocol errors.
async fn call(port: u16, token: &str, bot: &str, params: &Value, server: Server) -> Value {
    send(port, token, server, params, json!({"botId": bot})).await
}

/// Posts a tool call (`body` says who calls) to the host's method for `server`.
async fn send(port: u16, token: &str, server: Server, params: &Value, mut body: Value) -> Value {
    body["name"] = params["name"].clone();
    body["arguments"] = params.get("arguments").filter(|a| a.is_object()).cloned().unwrap_or_else(|| json!({}));
    let (method, timeout) = match server {
        Server::Connectors => ("connectorCall", Duration::from_secs(90)),
        Server::Routines => ("routineCall", Duration::from_secs(30)),
        Server::Workflows => ("workflowCall", crate::chat::manage::APPROVAL_TIMEOUT + Duration::from_secs(30)),
        Server::Computer => ("computerCall", Duration::from_mins(1)),
        // Starting a dev server may wait on an approval card, then for its port.
        Server::Chat if params["name"] == crate::dev_servers::START => {
            ("chatCall", crate::chat::manage::APPROVAL_TIMEOUT + Duration::from_secs(25))
        }
        // The actor's 270-second choice window must fit inside the actual MCP HTTP request.
        Server::Chat if params["name"] == crate::chat::decision::TOOL => {
            ("chatCall", crate::chat::decision::WAIT + Duration::from_secs(20))
        }
        // `ask_task` waits for the thread's answer.
        Server::Chat if params["name"] == crate::chat::work::ask::TOOL => {
            ("chatCall", crate::chat::work::ask::WAIT + Duration::from_secs(20))
        }
        // `ask_chat` waits for the bot's answer.
        Server::Chat if params["name"] == crate::chat::work::ask_chat::TOOL => {
            ("chatCall", crate::chat::work::ask_chat::WAIT + Duration::from_secs(20))
        }
        Server::Chat => ("chatCall", Duration::from_secs(if params["name"] == "send_file" { 70 } else { 30 })),
        Server::Team => ("teamCall", Duration::from_secs(30)),
        Server::Manage => ("manageCall", crate::chat::manage::APPROVAL_TIMEOUT + Duration::from_secs(30)),
        Server::Memory => ("memoryCall", Duration::from_mins(2)),
        Server::Composio => ("composioCall", Duration::from_mins(2)),
        // A session may wait for its mail, or on a thread.
        Server::Network if params["name"] == "check_inbox" || params["name"] == "thread_wait" => {
            ("agentCall", WAIT_REPLY)
        }
        // Listing an agent's models starts it.
        Server::Network if params["name"] == "worker_capabilities" => ("agentCall", Duration::from_mins(2)),
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
                        | Server::Workflows
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
    let mut child = tokio::process::Command::new(codync_agent::shell::program(command));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_threads_chat_server_lists_only_what_a_thread_may_call() {
        let names = |thread: bool| -> Vec<String> {
            let tools = chat_tools(thread);
            tools.as_array().into_iter().flatten().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect()
        };
        let (chat, thread) = (names(false), names(true));
        let kept =
            ["request_decision", "send_file", "publish_artifact", "list_artifacts", "start_dev_server", "ask_chat"];
        for kept in kept {
            assert!(thread.iter().any(|n| n == kept), "{kept} missing from {thread:?}");
        }
        let told = crate::chat::outbox::instructions(true);
        for gone in ["send_message", "react", "save_skill", "add_task", "answer_task", "set_name", "set_fallbacks"] {
            assert!(chat.iter().any(|n| n == gone), "the chat has {gone}");
            assert!(!thread.iter().any(|n| n == gone), "{gone} offered to a thread");
            assert!(!told.contains(gone), "a thread is told of {gone}: {told}");
        }
        assert!(!told.contains("(pass)"), "{told}");
        assert!(!chat.iter().any(|n| n == "ask_chat"), "the chat has no bot to ask");
    }

    #[test]
    fn the_network_server_refuses_a_bots_agent() {
        assert!(refuse_bot(Some("bot-1".into())).is_some_and(|v| v["isError"] == true));
        assert!(refuse_bot(None).is_none());
    }
}
