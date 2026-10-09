//! Minimal ACP (Agent Client Protocol) client: JSON-RPC 2.0 over a child
//! process's stdio. Updates are kept as `serde_json::Value` on purpose so a
//! newer adapter adding a variant never breaks parsing.

use super::process::Process;
use crate::LockExt;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::ChildStdin;
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

/// A JSON-RPC error answer (downcast from `request`'s error to read the code).
#[derive(Debug)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

/// ACP's "authentication required".
pub const AUTH_REQUIRED: i64 = -32000;

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RpcError {}

/// A request already written to the agent, waiting for its response.
pub struct Answer {
    method: String,
    id: i64,
    pending: Pending,
    rx: Option<oneshot::Receiver<std::result::Result<Value, Value>>>,
}

impl Answer {
    pub async fn response(mut self) -> Result<Value> {
        let receiver = self.rx.take().expect("a response is consumed once");
        match receiver.await {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => {
                let msg = e["message"].as_str().map_or_else(|| e.to_string(), str::to_owned);
                let message = match e["data"]["details"].as_str().or(e["data"].as_str()) {
                    Some(d) => format!("{msg} ({d})"),
                    None => msg,
                };
                Err(RpcError { code: e["code"].as_i64().unwrap_or_default(), message }.into())
            }
            Err(_) => bail!("{}: connection closed", self.method),
        }
    }
}

impl Drop for Answer {
    fn drop(&mut self) {
        self.pending.locked().remove(&self.id);
    }
}

pub struct Acp {
    stdin: tokio::sync::Mutex<ChildStdin>,
    next_id: AtomicI64,
    pending: Pending,
    child: Arc<tokio::sync::Mutex<Process>>,
}

impl Acp {
    /// Spawns `command` through the shell so user-provided commands (npx, env vars) just work.
    /// `env` stays out of the command line (it can hold API keys).
    pub fn spawn(
        command: &str,
        cwd: &str,
        env: &[(String, String)],
    ) -> Result<(Arc<Self>, mpsc::UnboundedReceiver<Incoming>)> {
        let mut process = crate::shell::exec(command).tokio();
        process
            .envs(env.iter().map(|(k, v)| (k, v)))
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // Its own group, so `kill` can stop everything it started.
        #[cfg(unix)]
        process.process_group(0);
        // Establish job ownership before the shell can spawn any descendants.
        #[cfg(windows)]
        process.creation_flags(windows_sys::Win32::System::Threading::CREATE_SUSPENDED);
        let mut child = process.spawn().with_context(|| format!("starting `{command}`"))?;
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
        let child = Process::new(child)?;
        let child = Arc::new(tokio::sync::Mutex::new(child));
        Process::monitor(&child);
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

        let acp = Arc::new(Self { stdin: tokio::sync::Mutex::new(stdin), next_id: AtomicI64::new(1), pending, child });
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
        let answer = self.send(method, params).await?;
        answer.response().await
    }

    /// Writes a request and returns its pending answer: anything written after this
    /// (a `session/cancel`) reaches the agent after the request itself.
    pub async fn send(&self, method: &str, params: Value) -> Result<Answer> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.locked().insert(id, tx);
        let answer = Answer { method: method.to_owned(), id, pending: self.pending.clone(), rx: Some(rx) };
        self.write(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).await?;
        Ok(answer)
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
        let mut child = self.child.lock().await;
        child.kill().await;
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

    async fn descendant_agent() -> (Arc<Acp>, mpsc::UnboundedReceiver<Incoming>) {
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/process_agent.py");
        let python = if cfg!(windows) { "python" } else { "python3" };
        let command = format!("{python} -u {}", crate::shell::quote(&script.display().to_string()));
        let (acp, mut rx) = Acp::spawn(&command, env!("CARGO_MANIFEST_DIR"), &[]).unwrap();
        let ready = tokio::time::timeout(std::time::Duration::from_secs(15), rx.recv()).await.unwrap();
        assert!(matches!(ready, Some(Incoming::Notification { method, .. }) if method == "ready"));
        (acp, rx)
    }

    async fn assert_descendant_pipes_close(mut rx: mpsc::UnboundedReceiver<Incoming>) {
        let closed = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await.unwrap();
        assert!(matches!(closed, Some(Incoming::Closed { .. })));
    }

    #[tokio::test]
    async fn leader_exit_releases_descendants_without_waiting_for_stdout_eof() {
        let (acp, rx) = descendant_agent().await;
        acp.notify("exit", json!({})).await.unwrap();
        assert_descendant_pipes_close(rx).await;
    }

    #[tokio::test]
    async fn dropping_an_agent_releases_its_process_tree() {
        let (acp, rx) = descendant_agent().await;
        drop(acp);
        assert_descendant_pipes_close(rx).await;
    }

    #[tokio::test]
    async fn actor_unwinding_releases_its_process_tree() {
        let (acp, rx) = descendant_agent().await;
        let task = tokio::spawn(async move {
            let _owned = acp;
            panic!("simulated actor panic");
        });
        assert!(task.await.unwrap_err().is_panic());
        assert_descendant_pipes_close(rx).await;
    }

    #[tokio::test]
    async fn timed_out_requests_do_not_accumulate_pending_answers() {
        let (acp, rx) = descendant_agent().await;
        for _ in 0..10 {
            let response =
                tokio::time::timeout(std::time::Duration::from_millis(10), acp.request("never", json!({}))).await;
            assert!(response.is_err());
            assert_eq!(acp.pending.locked().len(), 0);
        }
        acp.kill().await;
        assert_descendant_pipes_close(rx).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stopping_an_agent_closes_its_descendants_pipes() {
        let (acp, mut rx) = Acp::spawn(
            r#"sh -c 'sleep 60 & echo "{\"jsonrpc\":\"2.0\",\"method\":\"ready\"}"; read line'"#,
            "/tmp",
            &[],
        )
        .unwrap();
        let ready = tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv()).await.unwrap();
        assert!(matches!(ready, Some(Incoming::Notification { .. })));
        acp.kill().await;
        // Killing only the shell leaves sleep holding stdout for 60 seconds.
        let closed = tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv()).await.unwrap();
        assert!(matches!(closed, Some(Incoming::Closed { .. })));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn request_response_roundtrip() {
        // A fake agent: answers `initialize`, then sends a notification and exits.
        let script = r#"read l; id=$(echo "$l" | sed 's/.*"id":\([0-9]*\).*/\1/'); echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"ok\":true}}"; echo '{"jsonrpc":"2.0","method":"session/update","params":{"x":1}}'"#;
        let (acp, mut rx) = Acp::spawn(&format!("sh -c '{}'", script.replace('\'', "'\\''")), "/tmp", &[]).unwrap();
        let res = acp.request("initialize", json!({})).await.unwrap();
        assert_eq!(res["ok"], true);
        match rx.recv().await.unwrap() {
            Incoming::Notification { method, .. } => assert_eq!(method, "session/update"),
            _ => panic!("expected notification"),
        }
        assert!(matches!(rx.recv().await.unwrap(), Incoming::Closed { .. }));
    }
}
