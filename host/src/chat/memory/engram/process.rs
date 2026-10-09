//! Serialize native MCP requests per bot. A failed write is never replayed automatically.

use super::{directory, install, project};
use anyhow::{Context, Result, anyhow, bail, ensure};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::Path,
    process::Stdio,
    sync::{Arc, LazyLock, Once},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout},
    sync::Mutex,
};

type Slot = Arc<Mutex<Option<Process>>>;
static PROCESSES: LazyLock<Mutex<HashMap<String, Slot>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);
const MAX_MESSAGE_BYTES: u64 = 8 * 1024 * 1024;

struct Process {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    last_used: Instant,
    next_id: u64,
}

impl Process {
    async fn start(binary: &Path, dir: &Path, project: &str) -> Result<Self> {
        tokio::fs::create_dir_all(dir).await?;
        let config_dir = dir.join(".engram");
        tokio::fs::create_dir_all(&config_dir).await?;
        let config = config_dir.join("config.json");
        if tokio::fs::try_exists(&config).await? {
            let saved: Value = serde_json::from_slice(&tokio::fs::read(&config).await?)?;
            ensure!(saved["project_name"] == project, "Engram project binding does not match this bot");
        } else {
            tokio::fs::write(&config, serde_json::to_vec(&json!({"project_name":project}))?).await?;
        }
        let mut child = tokio::process::Command::new(binary)
            .args(["mcp", "--project", project])
            .env("ENGRAM_DATA_DIR", dir)
            // Memory stays local even if the user's shell opts another Engram into cloud sync.
            .env("ENGRAM_CLOUD_AUTOSYNC", "0")
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .context("starting Engram")?;
        let input = child.stdin.take().context("Engram stdin unavailable")?;
        let output = BufReader::new(child.stdout.take().context("Engram stdout unavailable")?);
        let mut process = Self { child, input, output, next_id: 0, last_used: Instant::now() };
        process
            .request(
                "initialize",
                json!({
                    "protocolVersion": crate::mcp::PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name": "codync", "version": env!("CARGO_PKG_VERSION")},
                }),
            )
            .await?;
        process.send(&json!({"jsonrpc":"2.0", "method":"notifications/initialized"})).await?;
        let registered = process
            .request(
                "tools/call",
                json!({"name":"mem_session_start", "arguments":{
                    "id": format!("{project}:codync-background"), "directory": dir,
                }}),
            )
            .await?;
        ensure!(registered["isError"] != true, "Engram session registration failed: {registered}");
        Ok(process)
    }

    async fn send(&mut self, value: &Value) -> Result<()> {
        let mut bytes = serde_json::to_vec(value)?;
        bytes.push(b'\n');
        self.input.write_all(&bytes).await?;
        self.input.flush().await?;
        Ok(())
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params})).await?;
        loop {
            let mut line = Vec::new();
            let read = (&mut self.output).take(MAX_MESSAGE_BYTES + 1).read_until(b'\n', &mut line).await?;
            ensure!(read > 0, "Engram closed its output");
            ensure!(read as u64 <= MAX_MESSAGE_BYTES, "Engram response exceeds size limit");
            let value: Value = serde_json::from_slice(&line).context("invalid Engram response")?;
            if value.get("id") != Some(&json!(id)) {
                continue;
            }
            if let Some(error) = value.get("error") {
                bail!("Engram: {error}");
            }
            return value.get("result").cloned().context("Engram response has no result");
        }
    }
}

fn start_idle_cleanup() {
    static START: Once = Once::new();
    START.call_once(|| {
        tokio::spawn(async {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                let slots: Vec<Slot> = PROCESSES.lock().await.values().cloned().collect();
                for slot in slots {
                    if let Ok(mut process) = slot.try_lock()
                        && process.as_ref().is_some_and(|p| p.last_used.elapsed() >= Duration::from_secs(300))
                    {
                        *process = None;
                    }
                }
            }
        });
    });
}

async fn request(bot: &str, method: &str, params: Value) -> Result<Value> {
    start_idle_cleanup();
    let dir = directory(bot)?;
    let slot = PROCESSES.lock().await.entry(directory(bot)?.to_string_lossy().into_owned()).or_default().clone();
    let mut process = slot.lock().await;
    if let Some(p) = process.as_mut()
        && p.child.try_wait()?.is_some()
    {
        *process = None;
    }
    if process.is_none() {
        let binary = install::binary().await?;
        super::migrate::run(&binary, &dir, &project(bot)).await?;
        *process = Some(tokio::time::timeout(REQUEST_TIMEOUT, Process::start(&binary, &dir, &project(bot))).await??);
    }
    let result =
        tokio::time::timeout(REQUEST_TIMEOUT, process.as_mut().context("Engram not running")?.request(method, params))
            .await;
    match result {
        Ok(Ok(value)) => {
            if let Some(p) = process.as_mut() {
                p.last_used = Instant::now();
            }
            Ok(value)
        }
        failure => {
            // Dropping stdin and the child closes the session. The next request starts fresh;
            // replaying this request could duplicate a write whose acknowledgement was lost.
            *process = None;
            match failure {
                Ok(Err(error)) => Err(error),
                Err(error) => Err(anyhow!("Engram request timed out; check whether the change was saved: {error}")),
                Ok(Ok(_)) => unreachable!(),
            }
        }
    }
}

/// Stop the MCP writer while the native CLI removes all memory entities.
pub(in crate::chat::memory) async fn clear(bot: &str) -> Result<()> {
    prepare(bot).await?;
    let slot = PROCESSES.lock().await.entry(directory(bot)?.to_string_lossy().into_owned()).or_default().clone();
    let mut process = slot.lock().await;
    if let Some(mut p) = process.take() {
        p.child.kill().await?;
    }
    super::clear::run(bot).await
}

pub(in crate::chat::memory) async fn tools(bot: &str) -> Result<Value> {
    request(bot, "tools/list", json!({})).await
}

pub(in crate::chat::memory) async fn prepare(bot: &str) -> Result<()> {
    request(bot, "ping", json!({})).await?;
    Ok(())
}

pub(in crate::chat::memory) async fn call(bot: &str, name: &str, args: &Value) -> Result<Value> {
    ensure!(name.starts_with("mem_"), "unknown Engram tool");
    request(bot, "tools/call", json!({"name":name, "arguments":args})).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires the pinned release binary at CODYNC_TEST_ENGRAM"]
    async fn native_memory_survives_restart_and_cannot_read_another_bot() {
        let binary = std::env::var("CODYNC_TEST_ENGRAM").expect("set CODYNC_TEST_ENGRAM");
        let root = std::env::temp_dir().join(format!("codync-engram-test-{}", uuid::Uuid::new_v4()));
        let result = async {
            let mut a = Process::start(Path::new(&binary), &root.join("a"), "bot-a").await?;
            let saved = a
                .request(
                    "tools/call",
                    json!({"name":"mem_save", "arguments":{
                        "title":"使用者偏好", "type":"learning", "content":"使用者偏好繁體中文", "project":"bot-a"
                    }}),
                )
                .await?;
            ensure!(saved["isError"] != true, "save failed: {saved}");
            a.child.kill().await?;
            drop(a);
            let mut a = Process::start(Path::new(&binary), &root.join("a"), "bot-a").await?;
            let query = json!({"name":"mem_search", "arguments":{"query":"繁體中文", "all_projects":true}});
            let found = a.request("tools/call", query.clone()).await?;
            ensure!(found.to_string().contains("使用者偏好"), "restart lost memory: {found}");
            let mut b = Process::start(Path::new(&binary), &root.join("b"), "bot-b").await?;
            let isolated = b.request("tools/call", query).await?;
            ensure!(!isolated.to_string().contains("使用者偏好"), "cross-bot read: {isolated}");
            a.child.kill().await?;
            b.child.kill().await?;
            Ok::<_, anyhow::Error>(())
        }
        .await;
        std::fs::remove_dir_all(root).expect("cleanup test databases");
        result.expect("native Engram integration");
    }
}
