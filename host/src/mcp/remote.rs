//! A remote connector behind a local stdio MCP server, with the sign-in the host gives it.

use anyhow::Result;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

/// `codync-host mcp remote`: a stdio MCP server that forwards every message to a remote
/// connector with the headers and fresh sign-in token the host gives it. Speaks streamable
/// HTTP, and falls back to the older HTTP+SSE transport when the server doesn't.
pub async fn serve_remote(connector: String, port: u16) -> Result<()> {
    let token = std::fs::read_to_string(crate::service::data_dir().join("token")).unwrap_or_default();
    let remote = Remote { port, token: token.trim().to_owned(), connector };
    // Both transports answer through one writer: the SSE one answers from its own stream.
    let (tx, mut rx) = mpsc::unbounded_channel::<Value>();
    let writer = tokio::spawn(async move {
        let mut out = tokio::io::stdout();
        while let Some(reply) = rx.recv().await {
            let mut line = serde_json::to_vec(&reply)?;
            line.push(b'\n');
            out.write_all(&line).await?;
            out.flush().await?;
        }
        anyhow::Ok(())
    });
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut session: Option<String> = None;
    // The POST endpoint of a server on the older SSE transport, and the task reading its stream.
    let mut legacy: Option<String> = None;
    let mut stream: Option<tokio::task::JoinHandle<()>> = None;
    while let Some(line) = lines.next_line().await? {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        let replies = match &legacy {
            Some(endpoint) => remote.post_legacy(endpoint, &msg).await,
            None => match remote.forward(&msg, &mut session).await {
                Ok(Some(replies)) => Ok(replies),
                Ok(None) => match remote.open_legacy(tx.clone()).await {
                    Ok((endpoint, reader)) => {
                        let replies = remote.post_legacy(&endpoint, &msg).await;
                        legacy = Some(endpoint);
                        stream = Some(reader);
                        replies
                    }
                    Err(e) => Err(e),
                },
                Err(e) => Err(e),
            },
        };
        let replies = replies.unwrap_or_else(|e| match msg.get("id").filter(|id| !id.is_null()) {
            // Only requests get an answer; a failed notification is dropped.
            Some(id) => {
                vec![json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32603, "message": format!("{e:#}")}})]
            }
            None => vec![],
        });
        for reply in replies {
            tx.send(reply)?;
        }
    }
    // The agent is gone: stop reading the stream, then write what's left.
    if let Some(reader) = stream {
        reader.abort();
        let _ = reader.await;
    }
    drop(tx);
    writer.await??;
    Ok(())
}

struct Remote {
    port: u16,
    token: String,
    connector: String,
}

impl Remote {
    /// The connector's URL and headers, with a fresh sign-in token (`stale` forces a refresh).
    async fn target(&self, stale: bool) -> Result<(String, reqwest::header::HeaderMap)> {
        let res = crate::http()
            .post(format!("http://127.0.0.1:{}/api/connectorTarget", self.port))
            .bearer_auth(&self.token)
            .json(&json!({"id": self.connector, "stale": stale}))
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("can't reach the Codync host: {e}"))?;
        let ok = res.status().is_success();
        let target: Value = res.json().await?;
        if !ok {
            anyhow::bail!("{}", target["error"].as_str().unwrap_or("the Codync host refused"));
        }
        let mut headers = reqwest::header::HeaderMap::new();
        for (k, v) in target["headers"].as_object().into_iter().flatten() {
            headers.insert(
                reqwest::header::HeaderName::from_bytes(k.as_bytes())?,
                reqwest::header::HeaderValue::from_str(v.as_str().unwrap_or_default())?,
            );
        }
        Ok((target["url"].as_str().unwrap_or_default().to_owned(), headers))
    }

    /// Sends one message over streamable HTTP; returns what the server answered (JSON, or
    /// the events of an SSE stream). None: the server doesn't speak streamable HTTP.
    async fn forward(&self, msg: &Value, session: &mut Option<String>) -> Result<Option<Vec<Value>>> {
        for stale in [false, true] {
            let (url, headers) = self.target(stale).await?;
            let mut req = crate::http()
                .post(&url)
                .headers(headers)
                .header("accept", "application/json, text/event-stream")
                .json(msg)
                .timeout(Duration::from_mins(10));
            if let Some(s) = session.as_deref() {
                req = req.header("mcp-session-id", s);
            }
            let res = req.send().await.map_err(|e| anyhow::anyhow!("can't reach {url}: {e}"))?;
            if res.status() == reqwest::StatusCode::UNAUTHORIZED && !stale {
                continue;
            }
            if res.status() == reqwest::StatusCode::NOT_FOUND && session.is_some() && !stale {
                // The server forgot our session; start over without it.
                *session = None;
                continue;
            }
            // The spec's test for an older server: its first POST fails with one of these.
            if session.is_none() && matches!(res.status().as_u16(), 400 | 404 | 405) {
                return Ok(None);
            }
            if let Some(s) = res.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()) {
                *session = Some(s.to_owned());
            }
            if !res.status().is_success() {
                anyhow::bail!("{url} answered {}", res.status());
            }
            if !is_sse(&res) {
                return Ok(Some(json_replies(&res.bytes().await?)?));
            }
            // Read events until the answer to this request arrives (servers may keep the stream open).
            let id = msg.get("id").filter(|id| !id.is_null());
            let mut got = vec![];
            let mut events = Events::new(res);
            while let Some(event) = events.next().await? {
                if let Some(v) = sse_data(&event) {
                    let done = id.is_some_and(|id| v.get("id") == Some(id) && v.get("method").is_none());
                    got.push(v);
                    if done {
                        break;
                    }
                }
            }
            return Ok(Some(got));
        }
        anyhow::bail!("{} still refuses the sign-in; sign in again in Extensions", self.connector)
    }

    /// Opens the older transport's event stream, which carries every answer from now on
    /// (sent to `tx`); returns the URL messages are posted to and the task reading the stream.
    async fn open_legacy(&self, tx: mpsc::UnboundedSender<Value>) -> Result<(String, tokio::task::JoinHandle<()>)> {
        for stale in [false, true] {
            let (url, headers) = self.target(stale).await?;
            let res = crate::http()
                .get(&url)
                .headers(headers)
                .header("accept", "text/event-stream")
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("can't reach {url}: {e}"))?;
            if res.status() == reqwest::StatusCode::UNAUTHORIZED && !stale {
                continue;
            }
            if !res.status().is_success() || !is_sse(&res) {
                anyhow::bail!("{url} speaks neither streamable HTTP nor SSE ({})", res.status());
            }
            let mut events = Events::new(res);
            let endpoint = loop {
                let event =
                    events.next().await?.ok_or_else(|| anyhow::anyhow!("{url} closed before naming its endpoint"))?;
                if event.lines().any(|l| l.trim() == "event: endpoint" || l.trim() == "event:endpoint") {
                    let path = sse_text(&event);
                    break reqwest::Url::parse(&url)?.join(path.trim())?.to_string();
                }
            };
            let connector = self.connector.clone();
            let reader = tokio::spawn(async move {
                while let Ok(Some(event)) = events.next().await {
                    if let Some(v) = sse_data(&event)
                        && tx.send(v).is_err()
                    {
                        return;
                    }
                }
                // ponytail: the session lives on this stream, so losing it ends the proxy and the
                // agent sees the server exit. Reconnect and re-initialize if that proves too blunt.
                eprintln!("{connector}: the server closed its event stream");
                std::process::exit(1);
            });
            return Ok((endpoint, reader));
        }
        anyhow::bail!("{} still refuses the sign-in; sign in again in Extensions", self.connector)
    }

    /// Posts one message on the older transport. Answers normally arrive on the stream;
    /// a server that answers in the body is heard too.
    async fn post_legacy(&self, endpoint: &str, msg: &Value) -> Result<Vec<Value>> {
        for stale in [false, true] {
            let (_, headers) = self.target(stale).await?;
            let res = crate::http()
                .post(endpoint)
                .headers(headers)
                .json(msg)
                .timeout(Duration::from_mins(1))
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("can't reach {endpoint}: {e}"))?;
            if res.status() == reqwest::StatusCode::UNAUTHORIZED && !stale {
                continue;
            }
            if !res.status().is_success() {
                anyhow::bail!("{endpoint} answered {}", res.status());
            }
            let body = res.bytes().await?;
            return Ok(json_replies(&body).unwrap_or_default());
        }
        anyhow::bail!("{} still refuses the sign-in; sign in again in Extensions", self.connector)
    }
}

fn is_sse(res: &reqwest::Response) -> bool {
    res.headers().get("content-type").and_then(|v| v.to_str().ok()).is_some_and(|t| t.starts_with("text/event-stream"))
}

/// A JSON body as messages (one, or a batch); an empty body is none.
fn json_replies(body: &[u8]) -> Result<Vec<Value>> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(vec![]);
    }
    Ok(match serde_json::from_slice::<Value>(body)? {
        Value::Array(all) => all,
        one => vec![one],
    })
}

/// The raw events of an SSE response, one at a time.
struct Events {
    res: reqwest::Response,
    buf: String,
}

impl Events {
    fn new(res: reqwest::Response) -> Self {
        Self { res, buf: String::new() }
    }

    async fn next(&mut self) -> Result<Option<String>> {
        loop {
            if let Some(end) = self.buf.find("\n\n") {
                return Ok(Some(self.buf.drain(..end + 2).collect()));
            }
            match self.res.chunk().await? {
                Some(chunk) => self.buf.push_str(&String::from_utf8_lossy(&chunk).replace("\r\n", "\n")),
                None => return Ok(None),
            }
        }
    }
}

/// One SSE event's `data:` lines, joined.
fn sse_text(event: &str) -> String {
    event
        .lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(|d| d.strip_prefix(' ').unwrap_or(d))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The JSON in one SSE event's `data:` lines.
fn sse_data(event: &str) -> Option<Value> {
    serde_json::from_str(&sse_text(event)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sse_events() {
        assert_eq!(sse_data("event: message\ndata: {\"id\":1}\n\n").unwrap()["id"], 1);
        assert_eq!(sse_data("data:{\"a\":\ndata: 2}\n\n").unwrap()["a"], 2);
        assert!(sse_data(": ping\n\n").is_none());
    }
}
