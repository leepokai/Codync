//! Minimal ACP (Agent Client Protocol) client: JSON-RPC 2.0 over a child
//! process's stdio. Updates are kept as `serde_json::Value` on purpose so a
//! newer adapter adding a variant never breaks parsing.

use super::process::Process;
use crate::LockExt;
use crate::agent::fallback::reason::{self, Reason};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::ChildStdin;
use tokio::sync::{mpsc, oneshot};

pub mod codebuddy;

/// stderr lines kept to explain a crash.
const STDERR_TAIL_LINES: usize = 12;
/// How long a stopped agent gets to exit by itself before what's left of it is killed.
const EXIT_GRACE: Duration = Duration::from_secs(10);

/// Agents [`Acp::kill`] is still stopping, for the host to wait for before it exits.
static STOPPING: std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>> = std::sync::Mutex::new(Vec::new());
/// Every agent started, for the host to stop whichever are left before it exits.
static STARTED: std::sync::Mutex<Vec<Weak<Acp>>> = std::sync::Mutex::new(Vec::new());

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
    /// The adapter's own category (`data.errorKind`: Claude's `rate_limit`, `overloaded`, …; `CodeBuddy`'s
    /// `data.category`: `auth`, `custom_model_auth`, …; codex-acp's `data.codexErrorInfo` as `usage_limit_exceeded`),
    /// kept only when it's an identifier, as it can go to usage analytics.
    pub kind: Option<String>,
}

/// ACP's "authentication required".
pub const AUTH_REQUIRED: i64 = -32000;
/// JSON-RPC's "internal error".
pub const INTERNAL_ERROR: i64 = -32603;

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RpcError {}

/// Whether an agent's error asks for signing in: ACP's code, the category of an adapter that names one, or the
/// words of one that names none. A named category is believed: `CodeBuddy`'s `custom_model_auth` (a bad key in
/// `models.json`) says "Authentication failed" but signing in doesn't fix it.
pub fn needs_sign_in(e: &anyhow::Error) -> bool {
    let says_auth = |text: &str| text.to_lowercase().contains("auth");
    match e.chain().find_map(|c| c.downcast_ref::<RpcError>()) {
        Some(r) if r.kind.is_some() => reason::of_rpc(r) == Some(Reason::SignedOut),
        Some(r) => r.code == AUTH_REQUIRED || says_auth(&r.message) || says_auth(&e.to_string()),
        None => says_auth(&e.to_string()),
    }
}

/// Whether a failed prompt may have left its session dead in the agent: an internal error the
/// adapter gave no category. Claude's adapter answers every later prompt on a session whose query
/// stream ended with one ("The Claude Agent session has ended") until the session is closed.
pub fn may_end_session(e: &anyhow::Error) -> bool {
    e.chain().filter_map(|c| c.downcast_ref::<RpcError>()).any(|r| r.code == INTERNAL_ERROR && r.kind.is_none())
}

/// A way an agent fails that the host itself recognizes, kept as a type so usage
/// analytics can count it (`turn_finished.error`) without its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// No way to start the agent on this computer.
    NotSetUp,
    /// The agent stopped answering.
    Timeout,
    /// The agent process went away mid-request.
    Exited,
    /// The agent wouldn't take the bot's model.
    NoModel,
}

#[derive(Debug)]
pub struct Failure {
    pub kind: FailureKind,
    message: String,
}

impl Failure {
    pub fn error(kind: FailureKind, message: impl Into<String>) -> anyhow::Error {
        Self { kind, message: message.into() }.into()
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Failure {}

/// A request already written to the agent, waiting for its response.
pub struct Answer {
    id: i64,
    pending: Pending,
    rx: Option<oneshot::Receiver<std::result::Result<Value, Value>>>,
}

impl Answer {
    pub async fn response(mut self) -> Result<Value> {
        let receiver = self.rx.take().expect("a response is consumed once");
        match receiver.await {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => Err(rpc_error(&e).into()),
            // The reader drops every waiter when the process goes away.
            Err(_) => Err(Failure::error(FailureKind::Exited, "agent process exited")),
        }
    }
}

/// An abandoned request (timed out, cancelled) no longer waits for its response.
impl Drop for Answer {
    fn drop(&mut self) {
        self.pending.locked().remove(&self.id);
    }
}

/// A JSON-RPC error object as an [`RpcError`].
fn rpc_error(e: &Value) -> RpcError {
    let msg = e["message"].as_str().map_or_else(|| e.to_string(), str::to_owned);
    let message = match e["data"]["details"].as_str().or(e["data"].as_str()) {
        Some(d) => format!("{msg} ({d})"),
        None => msg,
    };
    let kind = e["data"]["errorKind"]
        .as_str()
        .or(e["data"]["category"].as_str())
        .map(str::to_owned)
        .or_else(|| codex_kind(&e["data"]["codexErrorInfo"]))
        .filter(|k| !k.is_empty() && k.len() <= 40 && k.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'));
    RpcError { code: e["code"].as_i64().unwrap_or_default(), message, kind }
}

/// codex-acp's category (`data.codexErrorInfo`: `"usageLimitExceeded"`, or `{"httpConnectionFailed": {…}}`) in the
/// same form as Claude's `errorKind`: `usage_limit_exceeded`.
fn codex_kind(info: &Value) -> Option<String> {
    let name = info.as_str().or_else(|| info.as_object().and_then(|o| o.keys().next()).map(String::as_str))?;
    let mut kind = String::with_capacity(name.len() + 4);
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            kind.push('_');
        }
        kind.push(c.to_ascii_lowercase());
    }
    Some(kind)
}

pub struct Acp {
    /// `None` once stopped: the closed pipe tells the agent to exit.
    stdin: tokio::sync::Mutex<Option<ChildStdin>>,
    next_id: AtomicI64,
    pending: Pending,
    /// Owns the agent's whole process tree: dropping it kills what the agent started.
    child: Arc<tokio::sync::Mutex<Process>>,
    /// The process and process group leader.
    #[cfg(unix)]
    pid: Option<u32>,
    /// A cloud computer's keeper grants this process started with: revoked once it is stopped, or gone.
    grants: std::sync::Mutex<crate::keeper::client::Grants>,
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
        // So `kill` can stop everything it started.
        super::process::own_tree(&mut process);
        let mut child = process.spawn().with_context(|| format!("starting `{command}`"))?;
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
        #[cfg(unix)]
        let pid = child.id();
        let child = Arc::new(tokio::sync::Mutex::new(Process::new(child)?));
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
                // Process gone: dropping the waiters fails them as `FailureKind::Exited`.
                pending.locked().clear();
                let stderr_tail = tail.locked().iter().cloned().collect::<Vec<_>>().join("\n");
                let _ = tx.send(Incoming::Closed { stderr_tail });
            });
        }

        let acp = Arc::new(Self {
            stdin: tokio::sync::Mutex::new(Some(stdin)),
            next_id: AtomicI64::new(1),
            pending,
            child,
            #[cfg(unix)]
            pid,
            grants: std::sync::Mutex::default(),
        });
        note_started(&acp);
        Ok((acp, rx))
    }

    #[cfg(unix)]
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Keeps the grants in this process's environment for as long as it runs (`stop` or drop revokes them).
    pub fn hold(&self, grants: crate::keeper::client::Grants) {
        *self.grants.locked() = grants;
    }

    /// Whether it was started with keeper grants (which a restarted keeper no longer knows).
    pub fn holds_grants(&self) -> bool {
        !self.grants.locked().is_empty()
    }

    /// Everything still running in the agent's job.
    #[cfg(windows)]
    pub(crate) async fn job_processes(&self) -> Result<Vec<super::process::JobProcess>> {
        self.child.lock().await.processes().await
    }

    async fn write(&self, msg: Value) -> Result<()> {
        let mut line = serde_json::to_vec(&msg)?;
        line.push(b'\n');
        let mut stdin = self.stdin.lock().await;
        let stdin = stdin.as_mut().ok_or_else(|| Failure::error(FailureKind::Exited, "agent stopped"))?;
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
        let answer = Answer { id, pending: self.pending.clone(), rx: Some(rx) };
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

    /// Stops the agent and everything it started, without waiting for it. Closing its stdin
    /// lets it exit by itself first: Claude Code killed during an OAuth refresh loses the new
    /// refresh token, and the user's login with it.
    pub async fn kill(&self) {
        let mut child = self.child.lock().await;
        if !child.begin_stop() {
            return;
        }
        let grants = std::mem::take(&mut *self.grants.locked());
        drop(grants);
        self.stdin.lock().await.take();
        let owned = self.child.clone();
        let mut stopping = STOPPING.locked();
        stopping.retain(|task| !task.is_finished());
        stopping.push(tokio::spawn(async move { owned.lock().await.stop_within(EXIT_GRACE).await }));
    }
}

/// Waits until every agent being stopped has exited or been killed: at most [`EXIT_GRACE`], then
/// [`UNWIND`](super::process::UNWIND) for its tree to end from its leaves up.
pub async fn stopped() {
    let tasks = std::mem::take(&mut *STOPPING.locked());
    futures::future::join_all(tasks).await;
}

/// Stops every agent left and waits for them, before the host exits: `process::exit` drops no process, and Cursor's agent
/// outlives its closed stdin. The bots stop their own, so this is for a sign-in check or a model list still under way.
pub async fn stop_all() {
    let started: Vec<_> = std::mem::take(&mut *STARTED.locked()).iter().filter_map(Weak::upgrade).collect();
    futures::future::join_all(started.iter().map(|acp| acp.kill())).await;
    stopped().await;
}

fn note_started(acp: &Arc<Acp>) {
    let mut started = STARTED.locked();
    started.retain(|acp| acp.strong_count() > 0);
    started.push(Arc::downgrade(acp));
}

/// The result for a `session/request_permission`: the option picked, or `cancelled` without one.
pub fn permission_answer(option_id: Option<&str>) -> Value {
    match option_id {
        Some(id) => json!({"outcome": {"outcome": "selected", "optionId": id}}),
        None => json!({"outcome": {"outcome": "cancelled"}}),
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
mod tests;
