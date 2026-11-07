//! A linked `WhatsApp` number for this computer: a child process speaks the `WhatsApp` Web protocol,
//! the host only starts it, keeps its state and turns its messages into chat and routine events.
//!
//! The number is not official `WhatsApp` Web: linking uses the phone's linked devices, and `WhatsApp`
//! can block numbers it sees as automated. The child process dies with the host.

use crate::hub::Hub;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::Mutex,
};

/// What the apps show about the link.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkState {
    /// `disconnected`, `starting`, `qr`, `connected`, `loggedOut` or `error`.
    pub state: String,
    pub qr: Option<String>,
    pub me: Option<String>,
    pub name: Option<String>,
    pub error: Option<String>,
}

impl LinkState {
    fn view(&self) -> Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

#[derive(Default)]
struct Link {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
}

#[derive(Default)]
pub struct WhatsApp {
    state: Mutex<LinkState>,
    link: Mutex<Link>,
    sender: std::sync::atomic::AtomicBool,
}

impl WhatsApp {
    pub async fn state(&self) -> LinkState {
        self.state.lock().await.clone()
    }

    /// Starts the bridge and the link. A running link stays as it is.
    pub fn start(self: &Arc<Self>, hub: &Arc<Hub>) {
        self.ensure_sender(hub);
        let app = self.clone();
        let hub = hub.clone();
        tokio::spawn(async move {
            if let Err(error) = app.begin(&hub).await {
                app.set(
                    &hub,
                    LinkState { state: "error".into(), error: Some(format!("{error:#}")), ..LinkState::default() },
                )
                .await;
            }
        });
    }

    /// Runs the reply sender once for the life of the host.
    fn ensure_sender(self: &Arc<Self>, hub: &Arc<Hub>) {
        if self.sender.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let app = self.clone();
        let hub = hub.clone();
        tokio::spawn(async move {
            loop {
                if app.state.lock().await.state == "connected"
                    && let Err(error) = app.replies(&hub).await
                {
                    tracing::warn!(error = format!("{error:#}"), "WhatsApp replies need attention");
                }
                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            }
        });
    }

    /// Sends the bot replies of this chat back to it. A file that left the chat follows its text.
    async fn replies(&self, hub: &Hub) -> Result<()> {
        for reply in hub.store.pending_whatsapp_replies()? {
            if !hub.store.claim_whatsapp_reply(&reply.entry)? {
                continue;
            }
            let mut sent = Ok(());
            if !reply.text.trim().is_empty() {
                sent = self.send_message(&reply.chat, &reply.text, None, None, None).await;
            }
            if sent.is_ok() {
                for file in &reply.files {
                    let path = file["path"].as_str().unwrap_or_default();
                    if path.is_empty() {
                        continue;
                    }
                    sent = self
                        .send_message(
                            &reply.chat,
                            "",
                            Some(Path::new(path)),
                            file["name"].as_str(),
                            file["mimetype"].as_str(),
                        )
                        .await;
                    if sent.is_err() {
                        break;
                    }
                }
            }
            hub.store.finish_whatsapp_reply(&reply.entry, if sent.is_ok() { "sent" } else { "failed" })?;
            if let Err(error) = sent {
                tracing::warn!(%error,entry=%reply.entry,"WhatsApp reply was not sent");
            }
        }
        Ok(())
    }

    async fn begin(&self, hub: &Arc<Hub>) -> Result<()> {
        {
            let state = self.state.lock().await;
            if matches!(state.state.as_str(), "starting" | "qr" | "connected") {
                return Ok(());
            }
        }
        {
            let mut link = self.link.lock().await;
            link.child = None;
            link.stdin = None;
        }
        self.set(hub, LinkState { state: "starting".into(), ..LinkState::default() }).await;
        let bridge = bridge_path().context("the WhatsApp bridge is missing from this build")?;
        let node = node_path().await?;
        let root = crate::service::data_dir().join("whatsapp");
        tokio::fs::create_dir_all(&root).await?;
        let mut child = Command::new(node)
            .arg(&bridge)
            .arg(&root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .context("Could not start the WhatsApp link")?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().context("the WhatsApp link has no output")?;
        {
            let mut link = self.link.lock().await;
            link.child = Some(child);
            link.stdin = stdin;
        }
        let hub = hub.clone();
        let app = hub.whatsapp.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(event) = serde_json::from_str::<Value>(&line) {
                    app.on_event(&hub, event).await;
                }
            }
            app.dropped(&hub).await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        self.send(&json!({"type": "start"})).await
    }

    pub async fn logout(&self, hub: &Arc<Hub>) -> Result<()> {
        self.send(&json!({"type": "logout"})).await.ok();
        {
            let mut link = self.link.lock().await;
            if let Some(child) = link.child.as_mut() {
                let _ = child.kill().await;
            }
            link.child = None;
            link.stdin = None;
        }
        self.set(hub, LinkState { state: "loggedOut".into(), ..LinkState::default() }).await;
        Ok(())
    }

    /// Sends text, or a file path with its name and type, to a chat.
    pub async fn send_message(
        &self,
        to: &str,
        text: &str,
        path: Option<&Path>,
        name: Option<&str>,
        mimetype: Option<&str>,
    ) -> Result<()> {
        let state = self.state.lock().await;
        if state.state != "connected" {
            bail!("WhatsApp is not connected");
        }
        let mut body = json!({"type": "send", "id": uuid::Uuid::new_v4().to_string(), "to": to, "text": text});
        if let Some(path) = path {
            body["path"] = json!(path);
            body["name"] = json!(name.unwrap_or("file"));
            body["mimetype"] = json!(mimetype.unwrap_or("application/octet-stream"));
        }
        drop(state);
        self.send(&body).await
    }

    async fn send(&self, body: &Value) -> Result<()> {
        let mut link = self.link.lock().await;
        let stdin = link.stdin.as_mut().context("WhatsApp is not running")?;
        stdin.write_all(format!("{body}\n").as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }

    async fn set(&self, hub: &Hub, next: LinkState) {
        let view = next.view();
        *self.state.lock().await = next;
        let _ = hub.events.send(json!({"type": "whatsapp", "whatsapp": view}));
    }

    async fn on_event(&self, hub: &Arc<Hub>, event: Value) {
        let state = event["type"].as_str().unwrap_or_default();
        match state {
            "qr" => {
                self.set(
                    hub,
                    LinkState {
                        state: "qr".into(),
                        qr: event["qr"].as_str().map(str::to_owned),
                        ..LinkState::default()
                    },
                )
                .await;
            }
            "state" => {
                let mut next = LinkState {
                    state: event["state"].as_str().unwrap_or("disconnected").into(),
                    me: event["me"].as_str().map(str::to_owned),
                    name: event["name"].as_str().map(str::to_owned),
                    error: event["error"].as_str().map(str::to_owned),
                    qr: None,
                };
                if next.state == "connected" {
                    self.set(hub, next).await;
                    return;
                }
                if next.state == "error" {
                    next.state = "error".into();
                }
                self.set(hub, next).await;
            }
            "message" => self.on_message(hub, &event).await,
            _ => {}
        }
    }

    async fn on_message(&self, hub: &Arc<Hub>, event: &Value) {
        let text = event["text"].as_str().unwrap_or_default().trim().to_owned();
        if text.is_empty() && event["media"].is_null() {
            return;
        }
        let Some(chat) = event["chat"].as_str().filter(|c| !c.is_empty()) else { return };
        let group = event["group"].as_bool().unwrap_or(false);
        if group && !event["mentioned"].as_bool().unwrap_or(false) && !event["quoted"].as_bool().unwrap_or(false) {
            return;
        }
        // The orchestrator: an `@name` in the message picks that bot, anything else the chosen one.
        let roster: Vec<crate::store::BotRow> = hub
            .store
            .bots()
            .unwrap_or_default()
            .into_iter()
            .filter(|bot| !bot.deleted && !bot.config.is_group() && !crate::external::is_external(&bot.config))
            .collect();
        let (chosen, text) = mentioned_bot(&roster, &text);
        let bot = chosen
            .map(|b| b.config.id.clone())
            .or_else(|| hub.store.kv_get("whatsapp.bot"))
            .filter(|id| roster.iter().any(|b| &b.config.id == id));
        let Some(bot) = bot else {
            let _ = self
                .send_message(
                    chat,
                    "No hay ningún bot asignado a este WhatsApp. Elige uno en Ajustes.",
                    None,
                    None,
                    None,
                )
                .await;
            return;
        };
        let media = event["media"].as_object();
        let mut attached = Vec::new();
        let note = if let Some(m) = media {
            if let Ok(Some(path)) = save_media(m).await {
                attached.push(path.clone());
                format!("[Attached: {path}]")
            } else {
                let kind = m.get("kind").and_then(Value::as_str).unwrap_or("file");
                format!("[{kind}] {}", m.get("name").and_then(Value::as_str).unwrap_or(&text))
            }
        } else {
            text.clone()
        };
        let routine_event = json!({
            "source": "whatsapp",
            "event": if group { "groupMention" } else { "message" },
            "chat": event["chat"],
            "from": event["sender"],
            "name": event["name"],
            "text": text,
            "group": event["group"],
            "media": media.cloned(),
        });
        hub.routines.receive_event(&hub.store, &routine_event, event["id"].as_str().map(str::to_owned));
        let Some(bot_row) = hub.store.bot(&bot).ok().flatten() else { return };
        let root = if let Some(root) = hub.store.whatsapp_root(chat) {
            root
        } else {
            let root = entry_id(&["root", chat]);
            hub.add_entry_once(
                &root,
                &crate::store::Lane::main(&bot),
                crate::store::EntryKind::User,
                hub.store.max_turn(&bot),
                &json!({"text":format!("WhatsApp conversation · {chat}"), "source":"whatsapp", "status":"sent", "whatsappChat": chat}),
            )
            .ok();
            hub.store.map_whatsapp_thread(chat, &root).ok();
            root
        };
        let id = entry_id(&["message", chat, event["id"].as_str().unwrap_or_default()]);
        let prompt = format!(
            "WhatsApp message from {}:\n\n{note}",
            event["name"].as_str().unwrap_or_else(|| event["sender"].as_str().unwrap_or("someone"))
        );
        let lane = crate::store::Lane::in_thread(&bot, &root);
        let Ok((entry, _)) = hub.add_entry_once(
            &id,
            &lane,
            crate::store::EntryKind::User,
            hub.store.max_turn(&bot) + 1,
            &json!({"text":prompt, "status":"queued", "source":"whatsapp", "whatsappChat": chat,
                "whatsappFiles": attached}),
        ) else {
            return;
        };
        if entry.data["status"] == "queued" {
            let text = crate::chat::prompt::of_entry(hub, &bot_row.config, &entry.data);
            if let Err(error) = hub.send_cmd(&bot, crate::agent::bot::Cmd::Send { lane, entry_id: entry.id, text }) {
                tracing::warn!(%error, "Could not deliver a WhatsApp message to a bot");
            }
        }
    }

    async fn dropped(&self, hub: &Hub) {
        let state = self.state.lock().await.state.clone();
        if state != "loggedOut" && state != "stopped" {
            self.set(
                hub,
                LinkState {
                    state: "error".into(),
                    error: Some("The WhatsApp link stopped".into()),
                    ..LinkState::default()
                },
            )
            .await;
        }
    }
}

/// A stable entry id from its parts (`whatsapp-<hash>`).
fn entry_id(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_le_bytes());
        digest.update(part.as_bytes());
    }
    format!("whatsapp-{}", crate::chat::files::digest_hex(&digest.finalize()))
}

/// The bot an `@name` in `text` names, and the text without it; the first match wins.
fn mentioned_bot<'a>(roster: &'a [crate::store::BotRow], text: &str) -> (Option<&'a crate::store::BotRow>, String) {
    let lower = text.to_lowercase();
    let found = roster.iter().find(|bot| {
        let needle = format!("@{}", bot.config.name.to_lowercase());
        !bot.config.name.is_empty() && lower.contains(&needle)
    });
    match found {
        Some(bot) => {
            let needle = format!("@{}", bot.config.name.to_lowercase());
            let at = lower.find(&needle).unwrap_or(0);
            let mut stripped = text.to_owned();
            stripped.replace_range(at..at + needle.len(), "");
            (Some(bot), stripped.trim().to_owned())
        }
        None => (None, text.to_owned()),
    }
}

/// Saves attached media under the data folder, so the bot can open it; `None` when it is too big or wrong.
async fn save_media(media: &serde_json::Map<String, Value>) -> Result<Option<String>> {
    let Some(data) = media.get("data").and_then(Value::as_str) else { return Ok(None) };
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data)?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Ok(None);
    }
    let ext = match media.get("mimetype").and_then(Value::as_str) {
        Some("image/jpeg") => "jpg",
        Some("image/png") => "png",
        Some("image/webp") => "webp",
        Some("video/mp4") => "mp4",
        Some("audio/ogg") => "ogg",
        Some("application/pdf") => "pdf",
        _ => "bin",
    };
    let dir = crate::service::data_dir().join("whatsapp-media");
    tokio::fs::create_dir_all(&dir).await?;
    let name: String = media
        .get("name")
        .and_then(Value::as_str)
        .map(|n| n.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')).collect())
        .filter(|n: &String| !n.is_empty())
        .unwrap_or_else(|| format!("media.{ext}"));
    let path = dir.join(format!("{}-{name}", uuid::Uuid::new_v4().simple()));
    tokio::fs::write(&path, bytes).await?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendBody {
    pub to: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub path: Option<PathBuf>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub mimetype: Option<String>,
}

fn bridge_path() -> Option<PathBuf> {
    let mut beside = std::env::current_exe().ok()?.parent()?.to_path_buf();
    for candidate in [
        beside.join("whatsapp-bridge").join("bridge.mjs"),
        beside.join("whatsapp").join("bridge.mjs"),
        beside.join("resources").join("whatsapp").join("bridge.mjs"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("whatsapp-bridge").join("bridge.mjs"),
    ] {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    beside.pop();
    None
}

async fn node_path() -> Result<PathBuf> {
    if let Some(node) = crate::agent::backends::which("node") {
        return Ok(node);
    }
    crate::agent::node::ensure(&|line| tracing::info!(%line, "adding Node.js for WhatsApp")).await?;
    crate::agent::backends::which("node").ok_or_else(|| anyhow!("Node.js is needed for WhatsApp"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_link_has_no_code_and_is_disconnected() {
        let state = LinkState::default();
        assert_eq!(state.state, "");
        assert!(state.qr.is_none());
    }

    #[test]
    fn an_at_name_picks_that_bot_and_leaves_the_text() {
        let bot = |id: &str, name: &str| crate::store::BotRow {
            config: serde_json::from_value(
                json!({"id": id, "name": name, "backend": "fixture", "command": "x", "cwd": "/"}),
            )
            .unwrap(),
            rev: 0,
            deleted: false,
            session_id: None,
            read_rev: 0,
        };
        let roster = vec![bot("1", "Casa"), bot("2", "Trabajo")];
        let (chosen, text) = mentioned_bot(&roster, "@Trabajo resúmeme esto");
        assert_eq!(chosen.map(|b| b.config.id.as_str()), Some("2"));
        assert_eq!(text, "resúmeme esto");
        let (none, same) = mentioned_bot(&roster, "hola");
        assert!(none.is_none());
        assert_eq!(same, "hola");
        // A longer word that starts with a name still matches; the rest of it stays.
        let (partial, rest) = mentioned_bot(&roster, "@Casamiento ya");
        assert_eq!(partial.map(|b| b.config.id.as_str()), Some("1"));
        assert_eq!(rest, "miento ya");
    }
}
