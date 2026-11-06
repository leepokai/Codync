use super::cancellation::Cancellation;
use super::{Events, is_sse, json_replies, policy, sse_data, sse_text};
use crate::LockExt;
use crate::egress::Policy;
use anyhow::Result;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("TLS setup failed: install the ca-certificates package")
    });
    &CLIENT
}

#[cfg(test)]
mod tests;

/// `codync-host mcp remote`: a stdio MCP server that forwards every message to a remote
/// connector with the headers and fresh sign-in token the host gives it. Speaks streamable
/// HTTP, and falls back to the older HTTP+SSE transport when the server doesn't.
pub async fn serve_remote(connector: String, port: u16) -> Result<()> {
    let token = std::fs::read_to_string(crate::service::data_dir().join("token")).unwrap_or_default();
    let cancellation = Arc::new(Cancellation::default());
    let observed = Arc::new(super::proxy_replies::ProxyReplies::new(port, token.trim(), &connector).await?);
    let mut tasks = super::tasks::Tasks::default();
    let mut remote = Remote {
        port,
        token: token.trim().to_owned(),
        connector,
        approval: None,
        cancellation: cancellation.clone(),
        request_id: Value::Null,
        target_url: std::sync::Mutex::default(),
    };
    // Both transports answer through one writer: the SSE one answers from its own stream.
    let (tx, mut rx) = mpsc::channel::<Value>(16);
    let seen = observed.clone();
    let writer = tokio::spawn(async move {
        let mut out = tokio::io::stdout();
        while let Some(reply) = rx.recv().await {
            seen.observe(&reply).await;
            let mut line = serde_json::to_vec(&reply)?;
            line.push(b'\n');
            out.write_all(&line).await?;
            out.flush().await?;
        }
        anyhow::Ok(())
    });
    let (incoming, mut inbox) = mpsc::channel(100);
    tasks.track(&writer);
    let immediate = tx.clone();
    let input_cancellation = cancellation.clone();
    let input_reader = tokio::spawn(async move {
        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let Ok(message) = serde_json::from_str::<Value>(&line) else {
                        continue;
                    };
                    input_cancellation.observe(&message);
                    if message["method"] == "notifications/cancelled" {
                        continue;
                    }
                    if message["method"] == "ping" {
                        if let Some(id) = message.get("id")
                            && immediate.try_send(json!({"jsonrpc":"2.0","id":id,"result":{}})).is_err()
                        {
                            break;
                        }
                        continue;
                    }
                    // Fail closed on overload; never block the reader ahead of cancellation.
                    if incoming.try_send(message).is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    tracing::warn!(%error, "MCP input closed");
                    break;
                }
            }
        }
        input_cancellation.close();
    });
    tasks.track(&input_reader);
    let mut session: Option<String> = None;
    // The POST endpoint of a server on the older SSE transport, and the task reading its stream.
    let mut legacy: Option<Legacy> = None;
    let mut stream: Option<tokio::task::JoinHandle<()>> = None;
    loop {
        let msg = tokio::select! {
            () = cancellation.wait_closed() => break,
            msg = inbox.recv() => { let Some(msg) = msg else { break; }; msg }
        };
        let id = msg.get("id").cloned().unwrap_or(Value::Null);
        remote.request_id = id.clone();
        let msg = match policy::authorize(
            port,
            &remote.token,
            &remote.connector,
            msg,
            &remote.cancellation,
            &observed.fingerprint,
        )
        .await
        {
            Ok((message, approval)) => {
                remote.approval = approval;
                message
            }
            Err(error) => {
                tx.send(json!({"jsonrpc":"2.0","id":id,"error":{"code":-32603,"message":format!("{error:#}")}}))
                    .await?;
                remote.cancellation.finish(&id);
                continue;
            }
        };
        observed.track(&msg, remote.approval.as_deref());
        let replies = match &legacy {
            Some(l) => remote.post_legacy(l, &msg).await,
            None => match remote.forward(&msg, &mut session).await {
                Ok(Some(replies)) => Ok(replies),
                Ok(None) => match remote.open_legacy(tx.clone()).await {
                    Ok((l, reader)) => {
                        tasks.track(&reader);
                        let replies = remote.post_legacy(&l, &msg).await;
                        legacy = Some(l);
                        stream = Some(reader);
                        replies
                    }
                    Err(e) => Err(e),
                },
                Err(e) => Err(e),
            },
        };
        if replies.is_err() && cancellation.cancelled(&Value::Null) {
            break;
        }
        let replies = replies.unwrap_or_else(|e| {
            observed.forget(&id);
            match msg.get("id").filter(|id| !id.is_null()) {
                // Only requests get an answer; a failed notification is dropped.
                Some(id) => {
                    vec![json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32603, "message": format!("{e:#}")}})]
                }
                None => vec![],
            }
        });
        for reply in replies {
            tx.send(reply).await?;
        }
        remote.cancellation.finish(&id);
    }
    // The agent is gone: stop reading the stream, then write what's left.
    input_reader.abort();
    let _ = input_reader.await;
    if let Some(reader) = stream {
        reader.abort();
        let _ = reader.await;
    }
    drop(tx);
    match tokio::time::timeout(Duration::from_secs(35), writer).await {
        Ok(result) => result??,
        Err(_) => anyhow::bail!("MCP reply drain timed out; an unrecorded outcome may be unknown"),
    }
    Ok(())
}

/// The older transport's POST endpoint (on the connector's own origin) and what it may reach.
struct Legacy {
    endpoint: String,
    policy: Policy,
}

struct Remote {
    port: u16,
    token: String,
    connector: String,
    approval: Option<String>,
    cancellation: Arc<Cancellation>,
    request_id: Value,
    target_url: std::sync::Mutex<Option<String>>,
}

impl Remote {
    async fn check(&self) -> Result<()> {
        if self.approval.is_some() && self.cancellation.cancelled(&self.request_id) {
            anyhow::bail!("the agent cancelled this tool request");
        }
        policy::check(self.port, &self.token, self.approval.as_deref()).await?;
        if self.approval.is_some() && self.cancellation.cancelled(&self.request_id) {
            anyhow::bail!("the agent cancelled this tool request");
        }
        Ok(())
    }
    /// The connector's URL, headers with a fresh sign-in token (`stale` forces a refresh), and
    /// what that URL may reach.
    async fn target(&self, stale: bool) -> Result<(String, reqwest::header::HeaderMap, Policy)> {
        let res = client()
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
        let url = target["url"].as_str().ok_or_else(|| anyhow::anyhow!("connector has no URL"))?.to_owned();
        let mut bound = self.target_url.locked();
        if bound.as_ref().is_some_and(|previous| previous != &url) {
            anyhow::bail!("connector URL changed; restart its MCP session before sending more requests");
        }
        *bound = Some(url.clone());
        let policy = if target["public"] == true { Policy::Public } else { Policy::UserChosen };
        Ok((url, headers, policy))
    }

    /// Sends one message over streamable HTTP; returns what the server answered (JSON, or
    /// the events of an SSE stream). None: the server doesn't speak streamable HTTP.
    async fn forward(&self, msg: &Value, session: &mut Option<String>) -> Result<Option<Vec<Value>>> {
        for stale in [false, true] {
            let (url, headers, policy) = self.target(stale).await?;
            let mut req = crate::egress::client(policy, &url)
                .await?
                .post(&url)
                .headers(headers)
                .header("accept", "application/json, text/event-stream")
                .json(msg)
                .timeout(Duration::from_mins(10));
            if let Some(s) = session.as_deref() {
                req = req.header("mcp-session-id", s);
            }
            self.check().await?;
            let res = req.send().await.map_err(|e| anyhow::anyhow!("can't reach {url}: {e}"))?;
            if msg["method"] == "tools/call" && !res.status().is_success() {
                anyhow::bail!("tool dispatch failed ({}); it will not be retried automatically", res.status());
            }
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
    async fn open_legacy(&self, tx: mpsc::Sender<Value>) -> Result<(Legacy, tokio::task::JoinHandle<()>)> {
        for stale in [false, true] {
            let (url, headers, policy) = self.target(stale).await?;
            let res = crate::egress::client(policy, &url)
                .await?
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
                    let base = reqwest::Url::parse(&url)?;
                    let endpoint = base.join(path.trim())?;
                    if endpoint.origin() != base.origin()
                        || !endpoint.username().is_empty()
                        || endpoint.password().is_some()
                    {
                        anyhow::bail!("the legacy MCP endpoint must use the connector's origin");
                    }
                    break endpoint.to_string();
                }
            };
            let connector = self.connector.clone();
            let closed = self.cancellation.clone();
            let reader = tokio::spawn(async move {
                while let Ok(Some(event)) = events.next().await {
                    if let Some(v) = sse_data(&event)
                        && tx.send(v).await.is_err()
                    {
                        closed.close();
                        return;
                    }
                }
                // ponytail: the session lives on this stream, so losing it ends the proxy and the
                // agent sees the server exit. Reconnect and re-initialize if that proves too blunt.
                eprintln!("{connector}: the server closed its event stream");
                closed.close();
            });
            return Ok((Legacy { endpoint, policy }, reader));
        }
        anyhow::bail!("{} still refuses the sign-in; sign in again in Extensions", self.connector)
    }

    /// Posts one message on the older transport. Answers normally arrive on the stream;
    /// a server that answers in the body is heard too.
    async fn post_legacy(&self, legacy: &Legacy, msg: &Value) -> Result<Vec<Value>> {
        let endpoint = legacy.endpoint.as_str();
        for stale in [false, true] {
            let (_, headers, _) = self.target(stale).await?;
            self.check().await?;
            let res = crate::egress::client(legacy.policy, endpoint)
                .await?
                .post(endpoint)
                .headers(headers)
                .json(msg)
                .timeout(Duration::from_mins(1))
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("can't reach {endpoint}: {e}"))?;
            if res.status() == reqwest::StatusCode::UNAUTHORIZED && !stale && msg["method"] != "tools/call" {
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
