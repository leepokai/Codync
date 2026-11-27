//! Codex's own API (`codex app-server`, JSON-RPC on stdio), for what an app asks Codex itself without a model turn:
//! the account's plan windows (`usage::codex`), or where a session is listed in Codex's own apps ([`move_to_section`]).

use crate::acp::{Acp, METHOD_NOT_FOUND, RpcError};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

/// Who is asking, as `initialize` tells Codex (its `clientInfo`).
#[derive(Clone, Copy, Debug)]
pub struct ClientInfo {
    /// The app's id ("codync").
    pub name: &'static str,
    /// Its name for people ("Codync").
    pub title: &'static str,
    /// Its version.
    pub version: &'static str,
}

/// Starts the installed Codex CLI's API (blocking: looks the program up). `None`: Codex isn't installed.
/// `env` points it at another `CODEX_HOME`; empty for the computer's own.
pub fn start(env: &[(String, String)]) -> Result<Option<Arc<Acp>>> {
    let Some(bin) = crate::backends::which("codex") else { return Ok(None) };
    let home = dirs::home_dir().context("locating the user's home directory")?;
    let command = format!("{} app-server", crate::shell::quote(&bin.to_string_lossy()));
    let (client, _events) = Acp::spawn(&command, &home.to_string_lossy(), env)?;
    Ok(Some(client))
}

/// What `work` asks the API that `start` brings up, after the handshake. Handshake and `work` get `timeout`
/// together, and the process is stopped however they end. `None`: Codex isn't installed.
pub async fn session<T>(
    start: impl FnOnce() -> Result<Option<Arc<Acp>>> + Send + 'static,
    client: ClientInfo,
    timeout: Duration,
    work: impl AsyncFnOnce(&Acp) -> Result<T>,
) -> Result<Option<T>> {
    // Lookup and process creation can block; the transport's async reads stay on the runtime.
    let started = tokio::task::spawn_blocking(start).await;
    let Some(acp) = started.context("joining Codex startup")?.context("starting Codex")? else { return Ok(None) };
    let exchange = async {
        initialize(&acp, client).await.context("initializing Codex")?;
        work(&acp).await
    };
    let answered = tokio::time::timeout(timeout, exchange).await;
    acp.kill().await;
    answered.with_context(|| format!("Codex didn't answer within {}ms", timeout.as_millis()))?.map(Some)
}

/// The handshake every other request waits for.
async fn initialize(acp: &Acp, client: ClientInfo) -> Result<()> {
    let info = json!({"clientInfo": {"name": client.name, "title": client.title, "version": client.version}});
    acp.request("initialize", info).await?;
    acp.notify("initialized", json!({})).await
}

/// The request filing a thread in a section failed at, as context on its error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionStep {
    List,
    Create,
    Move,
}

impl SectionStep {
    pub fn word(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Create => "create",
            Self::Move => "move",
        }
    }
}

impl std::fmt::Display for SectionStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.word())
    }
}

/// JSON-RPC's "invalid request".
const INVALID_REQUEST: i64 = -32600;

/// Puts `thread` in the section of Codex's own apps named `section`, made when Codex has none. `false`: a Codex
/// from before sections. An error carries the [`SectionStep`] it failed at.
pub async fn move_to_section(client: &Acp, thread: &str, section: &str) -> Result<bool> {
    let Some(id) = section_id(client, section).await? else { return Ok(false) };
    let moved = client.request("thread/section/move", json!({"threadId": thread, "sectionId": id})).await;
    moved.context(SectionStep::Move)?;
    Ok(true)
}

/// The id of the section named `name`, made when Codex has none. `None`: a Codex from before sections.
async fn section_id(client: &Acp, name: &str) -> Result<Option<String>> {
    let mut cursor = Value::Null;
    loop {
        let page = match client.request("threadSection/list", json!({"cursor": cursor})).await {
            Ok(page) => page,
            Err(error) if unknown_method(&error) => return Ok(None),
            Err(error) => return Err(error.context(SectionStep::List)),
        };
        let found = page["data"].as_array().into_iter().flatten().find(|section| section["name"] == name);
        if let Some(id) = found.and_then(|section| section["id"].as_str()) {
            return Ok(Some(id.to_owned()));
        }
        cursor = page["nextCursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    let made = client.request("threadSection/create", json!({"name": name})).await;
    let made = made.context(SectionStep::Create)?;
    let id = made["section"]["id"].as_str().context("Codex returned no section id").context(SectionStep::Create)?;
    Ok(Some(id.to_owned()))
}

/// Whether Codex refused a request because it doesn't know the method. Its parser calls that an invalid request
/// with an "unknown variant".
fn unknown_method(error: &anyhow::Error) -> bool {
    error.downcast_ref::<RpcError>().is_some_and(|refusal| {
        refusal.code == METHOD_NOT_FOUND
            || (refusal.code == INVALID_REQUEST && refusal.message.contains("unknown variant"))
    })
}

/// A stand-in for [`start`]: the Python `script` answering as Codex's API, in `mode`, run in its own folder.
#[cfg(any(test, feature = "test-support"))]
pub fn fixture(script: &std::path::Path, mode: &str) -> impl FnOnce() -> Result<Option<Arc<Acp>>> + Send + 'static {
    let dir = script.parent().map(|dir| dir.to_string_lossy().into_owned()).unwrap_or_default();
    let command = format!("{} {}", crate::shell::python(script), crate::shell::quote(mode));
    move || {
        let (client, _events) = Acp::spawn(&command, &dir, &[])?;
        Ok(Some(client))
    }
}
