//! Anonymous inherited pipes avoid exposing a credential-bearing CDP server to local users.
use crate::agent::process::Process;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::path::Path;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, ChildStdout},
};

/// Request ids stay within JavaScript's safe integers (the broker is Node).
const MAX_SAFE_ID: u64 = (1 << 53) - 1;
/// Largest request or reply line, in bytes.
const MAX_MESSAGE: usize = 256 * 1024;

pub(crate) struct Pipe {
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next: u64,
    poisoned: bool,
}
pub(crate) fn spawn(browser: &Path, folder: &Path, proxy: Option<&str>) -> Result<(Process, Pipe)> {
    let node = crate::agent::backends::which("node")
        .or_else(|| {
            crate::agent::node::search_dirs().iter().map(|dir| dir.join("node.exe")).find(|path| path.is_file())
        })
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        })
        .context("Private browser transport requires Node.js; add it through the agent setup first")?;
    let mut command = tokio::process::Command::new(node);
    command
        .args(["-e", include_str!("../protected-browser-broker.mjs"), "--"])
        .arg(browser)
        .arg(folder)
        .args(proxy.map(|proxy| format!("--proxy={proxy}")))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .creation_flags(
            windows_sys::Win32::System::Threading::CREATE_SUSPENDED
                | windows_sys::Win32::System::Threading::CREATE_NO_WINDOW,
        )
        .kill_on_drop(true);
    let mut child = command.spawn().context("Could not start private browser transport")?;
    let input = child.stdin.take().context("Private browser input pipe missing")?;
    let output = BufReader::new(child.stdout.take().context("Private browser output pipe missing")?);
    let process = Process::new(child)?;
    Ok((process, Pipe { input, output, next: 0, poisoned: false }))
}
pub(crate) async fn rpc(pipe: &mut Pipe, method: &str, params: Value, session: Option<&str>) -> Result<Value> {
    ensure!(!pipe.poisoned, "Private browser transport was interrupted; start the browser again");
    pipe.next = pipe
        .next
        .checked_add(1)
        .filter(|id| *id <= MAX_SAFE_ID)
        .context("Private browser request identity exhausted")?;
    let id = pipe.next;
    let mut message = json!({"id":id,"method":method,"params":params});
    if let Some(session) = session {
        message["sessionId"] = json!(session);
    }
    let mut encoded = serde_json::to_vec(&message)?;
    encoded.push(b'\n');
    ensure!(encoded.len() <= MAX_MESSAGE, "Private browser request exceeded its limit");
    // A cancelled read_line can consume part of a reply. Never reuse that stream after cancellation.
    pipe.poisoned = true;
    pipe.input.write_all(&encoded).await.context("Private browser connection failed")?;
    loop {
        let mut line = String::new();
        ensure!(pipe.output.read_line(&mut line).await? > 0, "Private browser disconnected; outcome unknown");
        ensure!(line.len() <= MAX_MESSAGE, "Private browser reply exceeded its limit");
        let value: Value = serde_json::from_str(&line).context("Invalid private browser reply")?;
        if value["id"] != id {
            continue;
        }
        // The reply arrived whole, so the stream is usable even when the browser refused the operation.
        pipe.poisoned = false;
        ensure!(
            value.get("error").is_none() && value["result"].get("exceptionDetails").is_none(),
            "Private browser refused the operation; no secret detail is returned"
        );
        return Ok(value["result"].clone());
    }
}
