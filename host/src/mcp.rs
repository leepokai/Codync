//! Built-in stdio MCP servers: `team` discovers and asks other bots; `computer`
//! operates the desktop when enabled. Calls go through authenticated loopback
//! HTTP to the running host, which owns delegation and screen control.

use anyhow::Result;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Clone, Copy)]
pub enum Server {
    Computer,
    Team,
}

const INSTRUCTIONS: &str = "Operate this computer's desktop like a person would. Start with `screenshot` \
(or `ui_tree` to find controls precisely), then act; every action returns a fresh screenshot. \
Coordinates are pixels in the latest screenshot. Prefer keyboard shortcuts and `open_app` over hunting with the mouse. \
Don't type passwords or approve payments: ask the user to do that from their phone.";

fn tools() -> Value {
    let display = json!({"type": "integer", "description": "Display id; defaults to the main display."});
    let xy = |what: &str| json!({"type": "number", "description": format!("{what} in screenshot pixels.")});
    json!([
        {
            "name": "screenshot",
            "description": "Capture the screen. Returns a JPEG whose pixels are the coordinate space for every other tool.",
            "inputSchema": {"type": "object", "properties": {"display": display}},
            "annotations": {"readOnlyHint": true},
        },
        {
            "name": "ui_tree",
            "description": "Accessibility tree of the frontmost app: roles, titles, values and frames in screenshot pixels. Cheaper and more precise than reading pixels when you need to find a control.",
            "inputSchema": {"type": "object", "properties": {"display": display}},
            "annotations": {"readOnlyHint": true},
        },
        {
            "name": "click",
            "description": "Click at a point. Returns a screenshot afterwards.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "x": xy("X"), "y": xy("Y"),
                    "button": {"type": "string", "enum": ["left", "right", "middle"], "default": "left"},
                    "count": {"type": "integer", "minimum": 1, "maximum": 3, "default": 1, "description": "2 = double-click, 3 = triple-click."},
                    "modifiers": {"type": "array", "items": {"type": "string", "enum": ["cmd", "option", "ctrl", "shift"]}, "description": "Keys held during the click."},
                    "display": display,
                },
                "required": ["x", "y"],
            },
        },
        {
            "name": "move",
            "description": "Move the pointer (for hover menus and tooltips). Returns a screenshot afterwards.",
            "inputSchema": {"type": "object", "properties": {"x": xy("X"), "y": xy("Y"), "display": display}, "required": ["x", "y"]},
        },
        {
            "name": "drag",
            "description": "Press at (x, y), drag to (to_x, to_y), release. Returns a screenshot afterwards.",
            "inputSchema": {
                "type": "object",
                "properties": {"x": xy("Start x"), "y": xy("Start y"), "to_x": xy("End x"), "to_y": xy("End y"), "display": display},
                "required": ["x", "y", "to_x", "to_y"],
            },
        },
        {
            "name": "scroll",
            "description": "Scroll with the pointer over (x, y). Returns a screenshot afterwards.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "x": xy("X"), "y": xy("Y"),
                    "dx": {"type": "number", "description": "Lines to scroll right (negative = left)."},
                    "dy": {"type": "number", "description": "Lines to scroll down (negative = up)."},
                    "display": display,
                },
                "required": ["x", "y"],
            },
        },
        {
            "name": "type",
            "description": "Type text into the focused field (any Unicode). Returns a screenshot afterwards.",
            "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]},
        },
        {
            "name": "key",
            "description": "Press a key or shortcut, e.g. `return`, `escape`, `tab`, `cmd+t`, `cmd+shift+4`, `ctrl+c`. On Linux `cmd` is Super. Returns a screenshot afterwards.",
            "inputSchema": {"type": "object", "properties": {"keys": {"type": "string"}}, "required": ["keys"]},
        },
        {
            "name": "open_app",
            "description": "Open or bring an application to the front by name (e.g. `Safari`, `Simulator`, `firefox`). Returns a screenshot afterwards.",
            "inputSchema": {"type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]},
        },
    ])
}

/// Serves MCP on stdio until the agent closes it.
pub async fn serve(bot: String, port: u16, server: Server) -> Result<()> {
    let token = std::fs::read_to_string(crate::service::data_dir().join("token")).unwrap_or_default();
    let token = token.trim().to_owned();
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut out = tokio::io::stdout();
    let (name, instructions, available_tools) = match server {
        Server::Computer => ("codync-computer", INSTRUCTIONS, tools()),
        Server::Team => ("codync-team", crate::team::INSTRUCTIONS, crate::team::tools()),
    };
    while let Some(line) = lines.next_line().await? {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        let Some(id) = msg.get("id").filter(|id| !id.is_null()).cloned() else {
            continue; // notifications (`notifications/initialized`, cancellations)
        };
        let params = &msg["params"];
        let reply = match msg["method"].as_str().unwrap_or_default() {
            "initialize" => Ok(json!({
                "protocolVersion": params["protocolVersion"].as_str().unwrap_or(PROTOCOL_VERSION),
                "capabilities": {"tools": {}},
                "serverInfo": {"name": name, "version": env!("CARGO_PKG_VERSION")},
                "instructions": instructions,
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools": available_tools})),
            "tools/call" => Ok(call(port, &token, &bot, params, server).await),
            method => Err(json!({"code": -32601, "message": format!("unknown method {method}")})),
        };
        let msg = match reply {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
        };
        let mut line = serde_json::to_vec(&msg)?;
        line.push(b'\n');
        out.write_all(&line).await?;
        out.flush().await?;
    }
    Ok(())
}

/// One tool call through the host. Failures are tool errors the agent can read, not protocol errors.
async fn call(port: u16, token: &str, bot: &str, params: &Value, server: Server) -> Value {
    let body = json!({
        "botId": bot,
        "name": params["name"],
        "arguments": params.get("arguments").filter(|a| a.is_object()).cloned().unwrap_or_else(|| json!({})),
    });
    let (method, timeout) = match server {
        Server::Computer => ("computerCall", Duration::from_secs(60)),
        Server::Team => ("teamCall", crate::team::ASK_TIMEOUT + Duration::from_secs(30)),
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
                        Server::Computer => json!({"content": v["content"]}),
                        Server::Team => json!({"content": [{"type": "text", "text": v.to_string()}]}),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_parses_as_a_computer_tool() {
        for t in tools().as_array().unwrap() {
            let name = t["name"].as_str().unwrap();
            let required = t["inputSchema"]["required"].as_array().cloned().unwrap_or_default();
            let mut args = json!({});
            for r in required {
                let k = r.as_str().unwrap();
                args[k] = if t["inputSchema"]["properties"][k]["type"] == "string" { json!("x") } else { json!(1) };
            }
            let parsed =
                serde_json::from_value::<crate::screen::ComputerTool>(json!({"name": name, "arguments": args}));
            assert!(parsed.is_ok(), "{name}: {parsed:?}");
        }
    }
}
