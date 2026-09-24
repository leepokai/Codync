//! Minimal ACP (Agent Client Protocol) client: JSON-RPC 2.0 over a child
//! process's stdio. Updates are kept as `serde_json::Value` on purpose so a
//! newer adapter adding a variant never breaks parsing.

use crate::LockExt;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, oneshot};

/// stderr lines kept to explain a crash.
const STDERR_TAIL_LINES: usize = 12;

pub enum Incoming {
    Notification { method: String, params: Value },
    Request { id: Value, method: String, params: Value },
    Closed { stderr_tail: String },
}

/// In-flight requests by JSON-RPC id. A std mutex: it's never held across an `.await`.
type Pending = Arc<std::sync::Mutex<HashMap<i64, oneshot::Sender<Result<Value, Value>>>>>;

pub struct Acp {
    stdin: tokio::sync::Mutex<ChildStdin>,
    next_id: AtomicI64,
    pending: Pending,
    child: tokio::sync::Mutex<Child>,
}

impl Acp {
    /// Spawns `command` through `sh -c` so user-provided commands (npx, env vars) just work.
    pub fn spawn(command: &str, cwd: &str) -> Result<(Arc<Self>, mpsc::UnboundedReceiver<Incoming>)> {
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg(format!("exec {command}"))
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("starting `{command}`"))?;
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
        // Deliberately unbounded: `session/load` replays a whole history as notifications
        // *before* its response, while the bot is still awaiting that response. A bounded
        // queue would stall this reader, and with it the response — a deadlock.
        let (tx, rx) = mpsc::unbounded_channel();
        let pending = Pending::default();

        let tail = Arc::new(std::sync::Mutex::new(VecDeque::<String>::with_capacity(STDERR_TAIL_LINES)));
        {
            let tail = tail.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!(target: "agent", stderr = %line);
                    let mut t = tail.locked();
                    if t.len() == STDERR_TAIL_LINES {
                        t.pop_front();
                    }
                    t.push_back(line);
                }
            });
        }
        {
            let pending = pending.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let Ok(msg) = serde_json::from_str::<Value>(&line) else {
                        tracing::debug!(target: "agent", stdout = %line, "ignoring non-JSON output");
                        continue;
                    };
                    let method = msg["method"].as_str().map(str::to_owned);
                    match (method, msg.get("id").cloned()) {
                        (Some(method), Some(id)) if !id.is_null() => {
                            let _ = tx.send(Incoming::Request { id, method, params: msg["params"].clone() });
                        }
                        (Some(method), _) => {
                            let _ = tx.send(Incoming::Notification { method, params: msg["params"].clone() });
                        }
                        (None, Some(id)) => {
                            if let Some(id) = id.as_i64()
                                && let Some(waiter) = pending.locked().remove(&id)
                            {
                                let res = match msg.get("error") {
                                    Some(err) => Err(err.clone()),
                                    None => Ok(msg["result"].clone()),
                                };
                                let _ = waiter.send(res);
                            }
                        }
                        _ => {}
                    }
                }
                // Process gone: fail everyone still waiting.
                for (_, w) in pending.locked().drain() {
                    let _ = w.send(Err(json!({"message": "agent process exited"})));
                }
                let stderr_tail = tail.locked().iter().cloned().collect::<Vec<_>>().join("\n");
                let _ = tx.send(Incoming::Closed { stderr_tail });
            });
        }

        let acp = Arc::new(Self {
            stdin: tokio::sync::Mutex::new(stdin),
            next_id: AtomicI64::new(1),
            pending,
            child: tokio::sync::Mutex::new(child),
        });
        Ok((acp, rx))
    }

    async fn write(&self, msg: Value) -> Result<()> {
        let mut line = serde_json::to_vec(&msg)?;
        line.push(b'\n');
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(&line).await?;
        stdin.flush().await?;
        Ok(())
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.locked().insert(id, tx);
        self.write(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).await?;
        match rx.await {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => {
                let msg = e["message"].as_str().map_or_else(|| e.to_string(), str::to_owned);
                match e["data"]["details"].as_str().or(e["data"].as_str()) {
                    Some(d) => bail!("{msg} ({d})"),
                    None => bail!("{msg}"),
                }
            }
            Err(_) => bail!("{method}: connection closed"),
        }
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<()> {
        self.write(json!({"jsonrpc": "2.0", "method": method, "params": params})).await
    }

    pub async fn respond(&self, id: Value, result: Value) -> Result<()> {
        self.write(json!({"jsonrpc": "2.0", "id": id, "result": result})).await
    }

    pub async fn respond_error(&self, id: Value, code: i64, message: &str) -> Result<()> {
        self.write(json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})).await
    }

    pub async fn kill(&self) {
        // Already exited is fine; anything else is worth a log line.
        if let Err(error) = self.child.lock().await.kill().await
            && error.kind() != std::io::ErrorKind::InvalidInput
        {
            tracing::debug!(%error, "couldn't kill agent process");
        }
    }
}

/// Plain-text rendering of an ACP content block list / tool-call content list.
pub fn content_text(v: &Value) -> String {
    match v {
        Value::Array(items) => items.iter().map(content_text).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n"),
        Value::Object(o) => match o.get("type").and_then(Value::as_str) {
            Some("text") => o.get("text").and_then(Value::as_str).unwrap_or_default().to_owned(),
            Some("content") => o.get("content").map(content_text).unwrap_or_default(),
            Some("resource_link") => o.get("uri").and_then(Value::as_str).unwrap_or_default().to_owned(),
            Some("resource") => {
                o.get("resource").and_then(|r| r.get("text")).and_then(Value::as_str).unwrap_or_default().to_owned()
            }
            _ => String::new(),
        },
        _ => String::new(),
    }
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_text_flattens_blocks() {
        let v = json!([
            {"type": "content", "content": {"type": "text", "text": "a"}},
            {"type": "diff", "path": "/x"},
            {"type": "text", "text": "b"}
        ]);
        assert_eq!(content_text(&v), "a\nb");
        assert_eq!(truncate("héllo", 2), "h…");
    }

    #[tokio::test]
    async fn request_response_roundtrip() {
        // A fake agent: answers `initialize`, then sends a notification and exits.
        let script = r#"read l; id=$(echo "$l" | sed 's/.*"id":\([0-9]*\).*/\1/'); echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"ok\":true}}"; echo '{"jsonrpc":"2.0","method":"session/update","params":{"x":1}}'"#;
        let (acp, mut rx) = Acp::spawn(&format!("sh -c '{}'", script.replace('\'', "'\\''")), "/tmp").unwrap();
        let res = acp.request("initialize", json!({})).await.unwrap();
        assert_eq!(res["ok"], true);
        match rx.recv().await.unwrap() {
            Incoming::Notification { method, .. } => assert_eq!(method, "session/update"),
            _ => panic!("expected notification"),
        }
        assert!(matches!(rx.recv().await.unwrap(), Incoming::Closed { .. }));
    }
}
