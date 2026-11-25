//! stdio ACP bridge for iMate's HTTP/SSE ACP tunnel.
//!
//! The host drives agents over ACP on stdio. iMate serves ACP as a tunnelled
//! JSON-RPC stream over HTTP, so this module runs as a hidden host subcommand
//! and translates between the two without changing the shared ACP client.

use crate::LockExt;
use crate::http;
use anyhow::{Context, Result, bail};
use codync_agent::acp::INTERNAL_ERROR;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const BASE: &str = "https://imate.woa.com/server/web-api";
/// A frame POST or a colleague list. The event stream stays open and has no deadline of its own.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const TOKEN_VAR: &str = "IMATE_API_TOKEN";
/// The bot's chosen colleague (`clientUuid`). The host sets it before the process starts: the tunnel URL needs it
/// on `initialize`, and the model is chosen only after `session/new`.
const CLIENT_VAR: &str = "IMATE_CLIENT_UUID";
const CONNECTION_HEADER: &str = "acp-connection-id";
const PENDING_METHOD: &str = "openclaw/session/pendingTask";

/// Sends `request`, and once more when the first try dies while connecting. iMate's handshake has failed once
/// with `InvalidContentType` and then succeeded on the next try.
async fn connect(what: &'static str, request: impl Fn() -> reqwest::RequestBuilder) -> Result<reqwest::Response> {
    let mut failed = None;
    for attempt in 0..2 {
        match request().send().await {
            Ok(response) => return Ok(response),
            Err(error) if attempt == 0 && (error.is_connect() || error.is_timeout()) => {
                tracing::info!(error = %error, "{what} failed to connect; trying once more");
                tokio::time::sleep(Duration::from_millis(400)).await;
                failed = Some(error);
            }
            Err(error) => return Err(error).context(what),
        }
    }
    match failed {
        Some(error) => Err(error).context(what),
        None => bail!("{what} failed"),
    }
}

struct Client {
    http: reqwest::Client,
    headers: HeaderMap,
    base: String,
}

impl Client {
    fn new(token: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).context("invalid iMate token header")?,
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(Self { http: http().clone(), headers, base: BASE.to_owned() })
    }

    fn url(&self, suffix: &str) -> String {
        format!("{}{}", self.base, suffix)
    }

    async fn imates(&self) -> Result<Vec<Value>> {
        let url = self.url("/api/v1/open/imates");
        let response = connect("listing iMate agents", || {
            self.http.get(url.clone()).headers(self.headers.clone()).timeout(REQUEST_TIMEOUT)
        })
        .await?;
        let status = response.status();
        let body = response.text().await.context("reading the iMate agent list")?;
        if !status.is_success() {
            bail!("iMate agent list failed ({status}): {}", first_line(&body));
        }
        let value: Value = serde_json::from_str(first_line(&body)).context("decoding the iMate agent list")?;
        if value["code"] != 0 {
            bail!("iMate agent list failed: {}", value["message"].as_str().unwrap_or("unknown error"));
        }
        Ok(value["data"].as_array().cloned().unwrap_or_default())
    }

    async fn open(&self, client_uuid: &str, frame: &str) -> Result<(String, String)> {
        let url = self.url(&format!("/api/v1/open/imates/{client_uuid}/acp"));
        let response = connect("opening an iMate ACP tunnel", || {
            self.http.post(url.clone()).headers(self.headers.clone()).timeout(REQUEST_TIMEOUT).body(frame.to_owned())
        })
        .await?;
        let status = response.status();
        if !status.is_success() && status.as_u16() != 202 {
            let body = response.text().await.unwrap_or_default();
            bail!("iMate ACP tunnel failed ({status}): {}", first_line(&body));
        }
        let head =
            response.headers().get(CONNECTION_HEADER).and_then(|v| v.to_str().ok()).unwrap_or_default().to_owned();
        let body = response.text().await.unwrap_or_default();
        if head.is_empty() {
            bail!("iMate ACP tunnel returned no connection id: {}", first_line(&body));
        }
        Ok((head, body))
    }

    async fn send(&self, client_uuid: &str, connection: &str, frame: &str) -> Result<()> {
        let url = self.url(&format!("/api/v1/open/imates/{client_uuid}/acp"));
        let response = connect("sending an iMate ACP frame", || {
            self.http
                .post(url.clone())
                .headers(self.headers.clone())
                .timeout(REQUEST_TIMEOUT)
                .header(CONNECTION_HEADER, connection)
                .body(frame.to_owned())
        })
        .await?;
        let status = response.status();
        if !status.is_success() && status.as_u16() != 202 {
            let body = response.text().await.unwrap_or_default();
            bail!("iMate ACP frame failed ({status}): {}", first_line(&body));
        }
        Ok(())
    }

    async fn stream(&self, client_uuid: &str, connection: &str) -> Result<reqwest::Response> {
        let url = self.url(&format!("/api/v1/open/imates/{client_uuid}/acp"));
        let response = connect("opening the iMate ACP event stream", || {
            let mut headers = self.headers.clone();
            headers.insert(ACCEPT, HeaderValue::from_static("text/event-stream"));
            self.http.get(url.clone()).headers(headers).header(CONNECTION_HEADER, connection)
        })
        .await?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!("iMate ACP event stream failed ({status}): {}", first_line(&body));
        }
        Ok(response)
    }

    async fn close(&self, client_uuid: &str, connection: &str) -> Result<()> {
        let url = self.url(&format!("/api/v1/open/imates/{client_uuid}/acp"));
        let response = connect("closing the iMate ACP tunnel", || {
            self.http.delete(url.clone()).headers(self.headers.clone()).header(CONNECTION_HEADER, connection)
        })
        .await?;
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!("iMate ACP tunnel close failed: {}", first_line(&body));
        }
        Ok(())
    }
}

#[derive(Default)]
struct Tunnel {
    client_uuid: String,
    connection: String,
    pending: Arc<Mutex<Vec<String>>>,
}

/// Runs the hidden ACP bridge. The process needs the token, and the colleague's `clientUuid` when the bot has one.
pub async fn run() -> Result<()> {
    let token = std::env::var(TOKEN_VAR).context("IMATE_API_TOKEN is not set")?;
    if token.trim().is_empty() {
        bail!("IMATE_API_TOKEN is empty");
    }
    let client = Arc::new(Client::new(token.trim())?);
    let tunnel = Arc::new(Mutex::new(Tunnel::default()));
    let stdin = io::stdin();
    let mut line = String::new();
    while stdin.lock().read_line(&mut line)? > 0 {
        let text = line.trim();
        if text.is_empty() {
            line.clear();
            continue;
        }
        let frame: Value = match serde_json::from_str(text) {
            Ok(frame) => frame,
            Err(error) => {
                tracing::warn!(error = %error, "invalid ACP JSON-RPC from the host");
                line.clear();
                continue;
            }
        };
        let method = frame["method"].as_str().unwrap_or_default().to_owned();
        let id = frame["id"].clone();
        line.clear();

        // The colleague is the tunnel. Codync picks it as the bot's model and only says so after `session/new`,
        // so those calls are answered here. Forwarding them would ask iMate to switch models inside one tunnel.
        if matches!(method.as_str(), "session/set_model" | "session/set_config_option") {
            respond(&id, &json!({}))?;
            continue;
        }
        if method == PENDING_METHOD {
            respond(&id, &json!({"pendingTask": []}))?;
            continue;
        }

        if method == "initialize" && tunnel.locked().connection.is_empty() {
            if let Err(error) = open_tunnel(&client, &tunnel, &frame).await {
                // `-32000` is ACP's "authentication required". A tunnel that failed to connect is not that.
                respond_error(&id, INTERNAL_ERROR, &format!("{error:#}"))?;
            }
            continue;
        }

        let raw = forward_frame(&method, &serde_json::to_string(&frame)?);
        let (uuid, connection) = {
            let state = tunnel.locked();
            (state.client_uuid.clone(), state.connection.clone())
        };
        if uuid.is_empty() || connection.is_empty() {
            respond_error(&id, INTERNAL_ERROR, "iMate ACP tunnel is not open")?;
            continue;
        }
        if method == "session/close" {
            match client.close(&uuid, &connection).await {
                Ok(()) => respond(&id, &json!({}))?,
                Err(error) => respond_error(&id, INTERNAL_ERROR, &format!("{error:#}"))?,
            }
            continue;
        }
        if let Err(error) = client.send(&uuid, &connection, &raw).await {
            respond_error(&id, INTERNAL_ERROR, &format!("{error:#}"))?;
        }
    }

    let (uuid, connection) = {
        let state = tunnel.locked();
        (state.client_uuid.clone(), state.connection.clone())
    };
    if !uuid.is_empty()
        && !connection.is_empty()
        && let Err(error) = client.close(&uuid, &connection).await
    {
        tracing::warn!(error = format!("{error:#}"), "couldn't close the iMate ACP tunnel");
    }
    Ok(())
}

/// Opens the tunnel with the host's `initialize` frame. The colleague comes from [`CLIENT_VAR`], or the first one
/// the token can see. The server's answer is its HTTP body or the event stream; this doesn't invent one.
async fn open_tunnel(client: &Client, tunnel: &Arc<Mutex<Tunnel>>, frame: &Value) -> Result<()> {
    let uuid = match colleague(std::env::var(CLIENT_VAR).ok().as_deref()) {
        Some(uuid) => uuid,
        None => default_colleague(client.imates().await?).context("no iMate agent is available for this token")?,
    };
    let raw = serde_json::to_string(frame)?;
    let (connection, body) = client.open(&uuid, &raw).await?;
    {
        let mut state = tunnel.locked();
        state.client_uuid.clone_from(&uuid);
        state.connection.clone_from(&connection);
    }
    let stream = client.stream(&uuid, &connection).await?;
    tokio::spawn(pump(stream, Arc::clone(tunnel)));
    relay_http_body(&body);
    Ok(())
}

/// `preferred` when the bot already chose a colleague. Empty means the first by name, the picker's default.
fn colleague(preferred: Option<&str>) -> Option<String> {
    preferred.map(str::trim).filter(|id| !id.is_empty()).map(str::to_owned)
}

/// The colleague a bot with no model of its own talks to. Same order as the picker: name, then id.
fn default_colleague(mut imates: Vec<Value>) -> Option<String> {
    imates
        .sort_by(|a, b| crate::agent::auth::imate::colleague_name(a).cmp(crate::agent::auth::imate::colleague_name(b)));
    imates.into_iter().find_map(|imate| imate["clientUuid"].as_str().map(str::to_owned))
}

/// A session request without the local MCP servers: they are processes on this computer, which iMate can't start.
/// The field itself stays. iMate's schema requires `mcpServers`, and dropping the key is `Invalid params`.
fn forward_frame(method: &str, frame: &str) -> String {
    if !matches!(method, "session/new" | "session/load" | "session/resume") {
        return frame.to_owned();
    }
    let Ok(mut value) = serde_json::from_str::<Value>(frame) else { return frame.to_owned() };
    if let Some(params) = value.get_mut("params").and_then(|params| params.as_object_mut()) {
        params.insert("mcpServers".to_owned(), json!([]));
        params.remove("additionalDirectories");
    }
    serde_json::to_string(&value).unwrap_or_else(|_| frame.to_owned())
}

fn first_line(body: &str) -> &str {
    body.split('\n').next().unwrap_or(body)
}

/// Writes `body`'s first line when it is a JSON-RPC frame. iMate sometimes appends a framework
/// suffix on the next line (`docs/upstream-agents/imate.md`).
fn relay_http_body(body: &str) {
    let line = first_line(body).trim();
    if line.is_empty() {
        return;
    }
    let Ok(value) = serde_json::from_str::<Value>(line) else { return };
    let rpc = value.get("id").is_some() && (value.get("result").is_some() || value.get("error").is_some());
    if rpc || value.get("method").is_some() {
        let _ = write_line(line);
    }
}

fn respond(id: &Value, result: &Value) -> Result<()> {
    if id.is_null() {
        return Ok(());
    }
    write_line(&json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string())
}

fn respond_error(id: &Value, code: i64, message: &str) -> Result<()> {
    if id.is_null() {
        return Ok(());
    }
    write_line(&json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string())
}

fn write_line(line: &str) -> Result<()> {
    let mut out = io::stdout().lock();
    writeln!(out, "{line}")?;
    out.flush()?;
    Ok(())
}

async fn pump(mut response: reqwest::Response, tunnel: Arc<Mutex<Tunnel>>) {
    // A chunk often ends in the middle of a line. Treating that tail as a finished line drops the rest of a
    // large `session/new` result, so the host waits until the session times out.
    let mut pending = Vec::new();
    let mut data = String::new();
    while let Some(bytes) = response.chunk().await.ok().flatten() {
        for event in take_events(&mut pending, &bytes, &mut data) {
            dispatch_event(&event, &tunnel);
        }
    }
    // The host is still waiting on a reply this process accepted. Leaving it running hangs that turn.
    tracing::warn!("iMate ACP event stream closed; stopping the bridge");
    std::process::exit(1);
}

/// Completed SSE event payloads in `incoming`. Bytes after the last newline stay in `pending`.
fn take_events(pending: &mut Vec<u8>, incoming: &[u8], data: &mut String) -> Vec<String> {
    pending.extend_from_slice(incoming);
    let mut events = Vec::new();
    while let Some(pos) = pending.iter().position(|byte| *byte == b'\n') {
        let mut line = pending.drain(..=pos).collect::<Vec<_>>();
        line.pop();
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        let line = String::from_utf8_lossy(&line);
        if let Some(value) = line.strip_prefix("data:") {
            data.push_str(value.trim_start());
        } else if line.is_empty() {
            if !data.is_empty() {
                events.push(std::mem::take(data));
            }
        } else if let Some(reason) = line.strip_prefix("event: error") {
            tracing::warn!(reason = reason.trim(), "iMate ACP tunnel error");
        }
    }
    events
}

fn dispatch_event(data: &str, tunnel: &Mutex<Tunnel>) {
    let Ok(mut frame) = serde_json::from_str::<Value>(data) else { return };
    let inside = |id: &str| tunnel.locked().pending.locked().iter().any(|task| task == id);
    if frame.get("method").is_some() {
        if frame["method"] == "session/update"
            && let Some(update) = frame["params"]["update"].as_object_mut()
            && let Some(meta) = update.get_mut("_meta")
            && let Some(task) = meta.get_mut("task")
            && task["final"] == true
            && let Some(id) = task["taskId"].as_str()
            && inside(id)
        {
            tunnel.locked().pending.locked().retain(|pending| pending != id);
        }
        print_frame(&frame);
        return;
    }
    if frame.get("error").is_some() {
        print_frame(&frame);
        return;
    }
    if let Some(result) = frame.get_mut("result")
        && let Some(meta) = result.get_mut("_meta")
        && let Some(tasks) = meta.get_mut("pendingTask")
        && let Some(tasks) = tasks.as_array()
    {
        let ids = tasks.iter().filter_map(|t| t["taskId"].as_str().map(str::to_owned)).collect();
        *tunnel.locked().pending.locked() = ids;
    }
    print_frame(&frame);
}

fn print_frame(frame: &Value) {
    let Ok(line) = serde_json::to_string(frame) else { return };
    let mut out = io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_line_ignores_framework_suffix() {
        assert_eq!(first_line("{\"error\":\"x\"}\n{\"code\":0,\"message\":\"\"}"), "{\"error\":\"x\"}");
    }

    #[test]
    fn a_configured_colleague_is_kept_and_a_blank_one_is_not() {
        assert_eq!(colleague(Some("  abc  ")).as_deref(), Some("abc"));
        assert_eq!(colleague(Some("  ")), None);
        assert_eq!(colleague(None), None);
    }

    #[test]
    fn the_default_colleague_is_the_first_by_name() {
        let listed = vec![
            json!({"clientUuid": "b", "botName": "Zoe"}),
            json!({"clientUuid": "a", "botName": "Alice"}),
            json!({"clientUuid": "c", "botName": ""}),
        ];
        assert_eq!(default_colleague(listed).as_deref(), Some("a"));
    }

    #[test]
    fn session_frames_drop_local_servers() {
        let frame = r#"{"jsonrpc":"2.0","id":1,"method":"session/new","params":{"cwd":"/tmp","mcpServers":[{"name":"chat"}],"additionalDirectories":["/mem"]}}"#;
        let sent = forward_frame("session/new", frame);
        let value: Value = serde_json::from_str(&sent).expect("frame");
        assert_eq!(value["params"]["mcpServers"], json!([]));
        assert!(value["params"].get("additionalDirectories").is_none());
        assert_eq!(value["params"]["cwd"], "/tmp");
        assert_eq!(forward_frame("session/prompt", frame), frame);
    }

    #[test]
    fn an_event_split_across_chunks_stays_one_frame() {
        let mut pending = Vec::new();
        let mut data = String::new();
        let first = take_events(&mut pending, br#"data: {"id":2,"result":{"sessionId":"a"#, &mut data);
        assert_eq!(first, Vec::<String>::new());
        let second = take_events(
            &mut pending,
            br#"bc"}}

"#,
            &mut data,
        );
        assert_eq!(second, vec![r#"{"id":2,"result":{"sessionId":"abc"}}"#]);
    }

    #[test]
    fn a_final_task_is_removed_from_pending() {
        let tunnel = Arc::new(Mutex::new(Tunnel::default()));
        *tunnel.locked().pending.locked() = vec!["image".into()];
        let frame = json!({"method":"session/update", "params":{"update":{
            "sessionUpdate":"agent_message_chunk", "content":{"type":"text","text":""},
            "_meta":{"task":{"taskId":"image","final":true}}}}});
        dispatch_event(&frame.to_string(), &tunnel);
        assert_eq!(tunnel.locked().pending.locked().clone(), Vec::<String>::new());
    }
}
