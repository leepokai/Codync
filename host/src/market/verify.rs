//! Check the MCP handshake and tool discovery before reporting a connector ready.
use crate::store::Store;
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
};

pub async fn verify(store: &Store, id: &str, port: u16) -> Result<Value> {
    if let Some(toolkit) = id.strip_prefix(super::composio::PREFIX) {
        let connections = super::composio::refresh(store).await?;
        if connections.iter().any(|c| c.toolkit == toolkit && c.active()) {
            return Ok(json!({"status":"ready"}));
        }
        bail!("Finish signing in before checking this connection.");
    }
    let c = super::connectors(store)?.into_iter().find(|c| c.id == id).ok_or_else(|| anyhow!("Unknown connector"))?;
    if c.oauth.as_ref().is_some_and(|o| !o.signed_in()) {
        bail!("Sign in before checking this connection.");
    }
    let mut command = if let Some(exe) = &c.command {
        let mut command = Command::new(exe);
        command.args(&c.args);
        for (name, value) in &c.env {
            command.env(name, super::passwords::resolve(store, value).await?);
        }
        command
    } else {
        let mut command = Command::new(std::env::current_exe()?);
        command.args(["mcp", "remote", "--connector", id, "--port", &port.to_string()]);
        command
    };
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| anyhow!("Could not start the connector. Install its runtime on this computer and retry."))?;
    let mut input = child.stdin.take().ok_or_else(|| anyhow!("Connector input unavailable"))?;
    let output = child.stdout.take().ok_or_else(|| anyhow!("Connector output unavailable"))?;
    let mut lines = BufReader::new(output).lines();
    let check = async {
        for (id, method, params) in [
            (
                1,
                "initialize",
                json!({"protocolVersion":crate::mcp::PROTOCOL_VERSION,"capabilities":{},"clientInfo":{"name":"Codync","version":env!("CARGO_PKG_VERSION")}}),
            ),
            (2, "tools/list", json!({})),
        ] {
            let wire = format!("{}\n", json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
            input.write_all(wire.as_bytes()).await?;
            input.flush().await?;
            loop {
                let line = lines.next_line().await?.ok_or_else(|| anyhow!("Connector exited during setup"))?;
                if line.len() > 4 * 1024 * 1024 {
                    bail!("Connector response too large");
                }
                let Ok(reply) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if reply["id"] != id {
                    continue;
                }
                if reply.get("error").is_some() {
                    bail!("Connector rejected setup. Check its credentials and try again.");
                }
                if id == 1 {
                    if !reply["result"]["protocolVersion"].is_string() {
                        bail!("Invalid MCP handshake");
                    }
                    input.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n").await?;
                } else {
                    let tools = reply["result"]["tools"]
                        .as_array()
                        .ok_or_else(|| anyhow!("Connector did not return its tools"))?;
                    return Ok(json!({"status":"ready","toolCount":tools.len()}));
                }
                break;
            }
        }
        bail!("Connector verification did not complete")
    };
    let result = tokio::time::timeout(Duration::from_secs(90), check)
        .await
        .map_err(|_| anyhow!("Connector setup timed out. Check the runtime and connection, then retry."));
    let _ = child.kill().await;
    let _ = child.wait().await;
    result?
}
