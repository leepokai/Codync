//! Artifacts, Grok Bot's way: a self-contained HTML document a bot publishes for the user to come back to (a report,
//! a tracker, a dashboard). The host keeps each artifact's latest HTML and title; publishing again with its id replaces
//! it, and the apps show the newest. A publish in the bot's own chat also posts an artifact card there, and the user's
//! replies to that card reach the bot as messages about the artifact (`chat::replies`).

use crate::agent::bot::Cmd;
use crate::chat::files;
use crate::hub::Hub;
use crate::store::{Store, now_ms};
use anyhow::{Context as _, Result, anyhow, bail, ensure};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;

/// Largest artifact, in bytes (images ride along as data URLs).
pub const MAX_ARTIFACT: u64 = 5 * 1024 * 1024;
const MAX_TITLE: usize = 200;

/// The `chat` server's artifact tools (when to use them: `outbox::INSTRUCTIONS`).
pub const TOOLS: [&str; 2] = ["publish_artifact", "list_artifacts"];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub id: String,
    pub bot_id: String,
    pub title: String,
    pub size: u64,
    pub sha256: String,
    pub created_at: i64,
    pub updated_at: i64,
    /// Its newest card in the bot's chat: what a message about the artifact replies to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_id: Option<String>,
}

impl Artifact {
    /// An artifact card entry's data: its text, and the artifact as published (`data.artifact`; the apps read the rest
    /// with `readArtifact`).
    fn card_entry(&self) -> Value {
        json!({
            "text": card_text(&self.title),
            "final": true,
            "artifact": {"id": self.id, "title": self.title, "updatedAt": self.updated_at},
        })
    }
}

/// An artifact card's text: what older apps show, and what a reply to the card quotes.
pub fn card_text(title: &str) -> String {
    format!("Artifact: {title}")
}

pub fn tools() -> [Value; 2] {
    [
        json!({
            "name": "publish_artifact",
            "description": "Publish an Artifact: one self-contained HTML file (up to 5 MiB) the user opens from a card in your chat and from its Library. Without artifact, it creates a new artifact (title required). With artifact (an id from an earlier publish or list_artifacts), it replaces that artifact's HTML, and its title when given. Paths are absolute or relative to your working directory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "The HTML file."},
                    "title": {"type": "string", "description": "The artifact's title, as the user should see it."},
                    "artifact": {"type": "string", "description": "The id of the artifact to update."},
                },
                "required": ["path"],
            },
            "annotations": {"readOnlyHint": false, "idempotentHint": false},
        }),
        json!({
            "name": "list_artifacts",
            "description": "Your artifacts, newest first: id, title, when each was last published, and the path of its current HTML (to make changes, edit a copy and publish that; never edit this file in place).",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": true},
        }),
    ]
}

fn key(bot: &str, id: &str) -> String {
    format!("artifact.{bot}.{id}")
}

/// Where a bot's artifacts live: `<id>.html` each.
pub fn root(bot: &str) -> PathBuf {
    crate::service::data_dir().join("bots").join(bot).join("artifacts")
}

/// The file holding an artifact's current HTML. Ids are UUIDs, so they never leave the folder.
pub fn html_path(bot: &str, id: &str) -> Result<PathBuf> {
    let id = uuid::Uuid::parse_str(id).context("invalid artifact id")?;
    Ok(root(bot).join(format!("{id}.html")))
}

pub fn get(store: &Store, bot: &str, id: &str) -> Option<Artifact> {
    serde_json::from_str(&store.kv_get(&key(bot, id))?).ok()
}

/// A bot's artifacts, the most recently published first.
pub fn list(store: &Store, bot: &str) -> Vec<Artifact> {
    let prefix = format!("artifact.{bot}.");
    let mut artifacts: Vec<Artifact> =
        store.kv_prefix(&prefix).iter().filter_map(|k| serde_json::from_str(&store.kv_get(k)?).ok()).collect();
    artifacts.sort_by_key(|artifact| std::cmp::Reverse(artifact.updated_at));
    artifacts
}

/// Saves an artifact's details and tells the apps (`artifact` event; they upsert it by id).
fn put(hub: &Hub, artifact: &Artifact) -> Result<()> {
    hub.store.kv_set(&key(&artifact.bot_id, &artifact.id), &serde_json::to_string(artifact)?)?;
    let _ = hub.events.send(json!({"type": "artifact", "artifact": artifact}));
    Ok(())
}

pub fn title(value: &str) -> Result<String> {
    let value = value.trim();
    ensure!(!value.is_empty() && value.len() <= MAX_TITLE, "title must be 1–{MAX_TITLE} bytes");
    ensure!(!value.chars().any(char::is_control), "title can't hold control characters");
    Ok(value.to_owned())
}

/// Blocking: copies the bot's HTML file over the artifact's own, whole or not at all. Returns its size and SHA-256.
fn write(dest: &Path, source: &Path) -> Result<(u64, String)> {
    let input = files::open_regular(source)?;
    ensure!(input.metadata()?.len() <= MAX_ARTIFACT, "an artifact can be at most 5 MiB");
    let mut html = Vec::new();
    input.take(MAX_ARTIFACT + 1).read_to_end(&mut html)?;
    ensure!(u64::try_from(html.len())? <= MAX_ARTIFACT, "an artifact can be at most 5 MiB");
    ensure!(!html.is_empty(), "the artifact is empty");
    std::fs::create_dir_all(dest.parent().context("artifact folder")?).context("creating the artifacts folder")?;
    let partial = dest.with_extension("partial");
    std::fs::write(&partial, &html)?;
    std::fs::rename(&partial, dest)?;
    Ok((u64::try_from(html.len())?, files::digest_hex(&Sha256::digest(&html))))
}

/// The `chat` server's artifact tools.
pub async fn call(hub: &Arc<Hub>, bot: &str, name: &str, args: &Value) -> Result<Value> {
    match name {
        "publish_artifact" => publish(hub, bot, args).await,
        "list_artifacts" => {
            let artifacts: Vec<Value> = list(&hub.store, bot)
                .iter()
                .map(|artifact| {
                    let path = html_path(bot, &artifact.id).map(|path| path.display().to_string()).unwrap_or_default();
                    json!({"id": artifact.id, "title": artifact.title, "updatedAt": artifact.updated_at, "path": path})
                })
                .collect();
            Ok(json!({"artifacts": artifacts}))
        }
        _ => bail!("unknown artifact tool: {name}"),
    }
}

async fn publish(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value> {
    let row = hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
    let source = args["path"].as_str().filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("path is required"))?;
    let existing = match args["artifact"].as_str().filter(|s| !s.is_empty()) {
        Some(id) => {
            Some(get(&hub.store, bot, id).ok_or_else(|| anyhow!("no artifact {id}: list_artifacts shows yours"))?)
        }
        None => None,
    };
    let title = match (args["title"].as_str(), &existing) {
        (Some(value), _) => title(value)?,
        (None, Some(artifact)) => artifact.title.clone(),
        (None, None) => bail!("title is required for a new artifact"),
    };
    let id = existing.as_ref().map_or_else(|| uuid::Uuid::new_v4().to_string(), |artifact| artifact.id.clone());
    let (dest, source) = (html_path(bot, &id)?, Path::new(&row.config.cwd).join(source));
    let (size, sha256) = tokio::task::spawn_blocking(move || write(&dest, &source)).await??;
    let now = now_ms();
    let mut artifact = Artifact {
        id,
        bot_id: bot.to_owned(),
        title,
        size,
        sha256,
        created_at: existing.as_ref().map_or(now, |artifact| artifact.created_at),
        updated_at: now,
        entry_id: existing.and_then(|artifact| artifact.entry_id),
    };
    // Saved before its card goes out, so an app opening the card finds it.
    put(hub, &artifact)?;
    let mut answer = json!({"published": true, "artifact": {"id": artifact.id, "title": artifact.title}});
    let (reply, posted) = oneshot::channel();
    hub.send_cmd(bot, Cmd::PostArtifact { data: artifact.card_entry(), reply })?;
    let posted = tokio::time::timeout(Duration::from_secs(10), posted)
        .await
        .map_err(|_| anyhow!("the artifact was saved, but its card wasn't posted in time"))?
        .map_err(|_| anyhow!("the artifact was saved, but the bot shut down before posting its card"))?;
    match posted {
        Ok(entry) => {
            artifact.entry_id = Some(entry);
            put(hub, &artifact)?;
        }
        // A routine, a room or a bot request: the artifact is in the chat's Library, and its reply says so.
        Err(error) => answer["card"] = format!("not posted ({error:#}); mention the artifact in your reply").into(),
    }
    Ok(answer)
}

/// The apps' artifact methods: `readArtifact {botId, artifactId, offset}` and `renameArtifact {botId, artifactId,
/// title}`. The list is the chat's Library (`chat::library`).
pub async fn api(hub: &Arc<Hub>, method: &str, b: &Value) -> Result<Value> {
    let bot = b["botId"].as_str().ok_or_else(|| anyhow!("`botId` is required"))?;
    ensure!(hub.store.bot(bot)?.is_some_and(|row| !row.deleted), "unknown bot");
    let id = b["artifactId"].as_str().ok_or_else(|| anyhow!("`artifactId` is required"))?;
    let mut artifact = get(&hub.store, bot, id).ok_or_else(|| anyhow!("unknown artifact"))?;
    match method {
        "readArtifact" => {
            let offset = b["offset"].as_u64().ok_or_else(|| anyhow!("offset must be a nonnegative integer"))?;
            let path = html_path(bot, id)?;
            let size = artifact.size;
            let data = tokio::task::spawn_blocking(move || read(&path, size, offset)).await??;
            Ok(
                json!({"data": base64::engine::general_purpose::STANDARD.encode(data), "size": size, "artifact": artifact}),
            )
        }
        "renameArtifact" => {
            artifact.title = title(b["title"].as_str().unwrap_or_default())?;
            put(hub, &artifact)?;
            Ok(json!({"artifact": artifact}))
        }
        _ => bail!("unknown artifact method: {method}"),
    }
}

/// One chunk of an artifact's HTML, from `offset`.
fn read(path: &Path, size: u64, offset: u64) -> Result<Vec<u8>> {
    ensure!(offset <= size, "invalid artifact offset");
    files::read_chunk(path, size, offset, "the artifact is missing or damaged")
}

/// The note a bot reads before a message about one of its artifacts (a reply to the artifact's card).
pub fn note(store: &Store, bot: &str, id: &str) -> Option<String> {
    let artifact = get(store, bot, id)?;
    let path = html_path(bot, id).ok()?;
    Some(format!(
        "[Codync: this is about your artifact \"{}\" (id {id}). Its current HTML is {}; to change the artifact, edit a copy \
         and publish_artifact with artifact: \"{id}\".]",
        artifact.title,
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_trimmed_and_bounded() {
        assert_eq!(title("  Weekly report ").unwrap(), "Weekly report");
        assert!(title("   ").is_err());
        assert!(title(&"x".repeat(MAX_TITLE + 1)).is_err());
        assert!(title("a\nb").is_err());
    }

    #[test]
    fn artifacts_are_written_whole_and_read_in_chunks() {
        let dir = std::env::temp_dir().join(format!("codync-artifacts-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let (source, dest) = (dir.join("artifact.html"), dir.join("artifacts/p.html"));
        let html = "<h1>Hi</h1>".repeat(50_000);
        std::fs::write(&source, &html).unwrap();
        let (size, sha) = write(&dest, &source).unwrap();
        assert_eq!(size, html.len() as u64);
        assert_eq!(sha, files::digest_hex(&Sha256::digest(html.as_bytes())));
        let first = read(&dest, size, 0).unwrap();
        assert_eq!(first.len(), files::CHUNK_SIZE);
        let rest = read(&dest, size, first.len() as u64).unwrap();
        assert_eq!([first, rest].concat(), html.as_bytes());
        assert!(read(&dest, size, size + 1).is_err(), "past the end");
        std::fs::write(&source, "").unwrap();
        assert!(write(&dest, &source).is_err(), "an empty artifact");
        assert!(html_path("b", "../x").is_err(), "ids are UUIDs");
        let _ = std::fs::remove_dir_all(dir);
    }
}
