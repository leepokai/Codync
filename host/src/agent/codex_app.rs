//! Codex's own API (`codex app-server`, JSON-RPC on stdio), for what the host asks Codex itself without a model
//! turn: the account's plan windows (`usage::codex`) and where a bot's session is listed (`bot::codex_section`).

use anyhow::{Context, Result};
use codync_agent::acp::Acp;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

/// Starts the installed Codex CLI's API (blocking: looks the program up). `None`: Codex isn't installed.
/// `env` points it at another `CODEX_HOME`; empty for the computer's own.
pub fn start(env: &[(String, String)]) -> Result<Option<Arc<Acp>>> {
    let Some(bin) = codync_agent::backends::which("codex") else { return Ok(None) };
    let home = dirs::home_dir().context("locating the host user's home directory")?;
    let command = format!("{} app-server", codync_agent::shell::quote(&bin.to_string_lossy()));
    let (client, _events) = Acp::spawn(&command, &home.to_string_lossy(), env)?;
    Ok(Some(client))
}

/// What `work` asks the API that `start` brings up, after the handshake. Handshake and `work` get `timeout`
/// together, and the process is stopped however they end. `None`: Codex isn't installed.
pub async fn session<T>(
    start: impl FnOnce() -> Result<Option<Arc<Acp>>> + Send + 'static,
    timeout: Duration,
    work: impl AsyncFnOnce(&Acp) -> Result<T>,
) -> Result<Option<T>> {
    // Lookup and process creation can block; the transport's async reads stay on the runtime.
    let started = tokio::task::spawn_blocking(start).await;
    let Some(client) = started.context("joining Codex startup")?.context("starting Codex")? else { return Ok(None) };
    let exchange = async {
        initialize(&client).await.context("initializing Codex")?;
        work(&client).await
    };
    let answered = tokio::time::timeout(timeout, exchange).await;
    client.kill().await;
    answered.with_context(|| format!("Codex didn't answer within {}ms", timeout.as_millis()))?.map(Some)
}

/// The handshake every other request waits for.
async fn initialize(client: &Acp) -> Result<()> {
    let info = json!({"clientInfo": {"name": "codync", "title": "Codync", "version": env!("CARGO_PKG_VERSION")}});
    client.request("initialize", info).await?;
    client.notify("initialized", json!({})).await
}

/// A stand-in for [`start`]: `tests/fixtures/<script>` answering as Codex's API, in `mode`.
#[cfg(test)]
pub fn fixture(script: &str, mode: &str) -> impl FnOnce() -> Result<Option<Arc<Acp>>> + Send + 'static {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let script = root.join("tests/fixtures").join(script);
    let command = format!("{} {}", codync_agent::shell::python(&script), codync_agent::shell::quote(mode));
    move || {
        let (client, _events) = Acp::spawn(&command, &root.to_string_lossy(), &[])?;
        Ok(Some(client))
    }
}
