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
    /// `disconnected`, `starting`, `qr`, `pairing`, `connected`, `loggedOut` or `error`.
    pub state: String,
    pub qr: Option<String>,
    /// The code to type in `WhatsApp` → Linked devices instead of scanning a QR code.
    pub pair_code: Option<String>,
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
    /// Which link a read loop belongs to, so a replaced link's ending is not reported.
    generation: u64,
}

#[derive(Default)]
pub struct WhatsApp {
    state: Mutex<LinkState>,
    link: Mutex<Link>,
    /// The number a pairing code was asked for, until the link ends.
    pairing: std::sync::Mutex<Option<String>>,
    sender: std::sync::atomic::AtomicBool,
}

impl WhatsApp {
    pub async fn state(&self) -> LinkState {
        self.state.lock().await.clone()
    }

    /// The number a pairing code was asked for, while the link waits for it.
    fn pairing(&self) -> Option<String> {
        self.pairing.lock().map(|pairing| pairing.clone()).unwrap_or_default()
    }

    fn set_pairing(&self, phone: Option<String>) {
        if let Ok(mut pairing) = self.pairing.lock() {
            *pairing = phone;
        }
    }

    /// Starts the bridge and the link. A running link stays as it is.
    pub fn start(self: &Arc<Self>, hub: &Arc<Hub>) {
        self.ensure_sender(hub);
        let app = self.clone();
        let hub = hub.clone();
        tokio::spawn(async move {
            if let Err(error) = app.begin(&hub, None).await {
                app.set(
                    &hub,
                    LinkState { state: "error".into(), error: Some(format!("{error:#}")), ..LinkState::default() },
                )
                .await;
            }
        });
    }

    /// Links `phone` with a pairing code instead of a QR code scanned by that same phone. A running
    /// link stops first so the code is asked again for the number given.
    pub fn pair(self: &Arc<Self>, hub: &Arc<Hub>, phone: &str) -> Result<()> {
        let phone = normalize_phone(phone)?;
        self.ensure_sender(hub);
        let app = self.clone();
        let hub = hub.clone();
        tokio::spawn(async move {
            app.set_pairing(Some(phone.clone()));
            // The child kills itself when dropped; the new generation makes the old read loop's
            // ending leave the new link's state alone.
            let child = {
                let mut link = app.link.lock().await;
                link.generation += 1;
                link.stdin = None;
                link.child.take()
            };
            drop(child);
            if let Err(error) = app.begin(&hub, Some(&phone)).await {
                app.set(
                    &hub,
                    LinkState { state: "error".into(), error: Some(format!("{error:#}")), ..LinkState::default() },
                )
                .await;
            }
        });
        Ok(())
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
        self.cards(hub).await?;
        Ok(())
    }

    /// Shows a pending approval as a numbered request, once, to the person who wrote.
    async fn cards(&self, hub: &Hub) -> Result<()> {
        for (chat, entry) in hub.store.pending_whatsapp_cards()? {
            let Some(card) = hub.store.entry(&entry) else { continue };
            let Some(options) = card.data["options"].as_array().filter(|o| o.len() >= 2) else { continue };
            let sender = hub.store.whatsapp_sender(&chat).unwrap_or_default();
            let mut lines = vec![format!("{}\n\nResponde 1 o 2:", card.data["title"].as_str().unwrap_or("Aprobación"))];
            for (index, option) in options.iter().take(2).enumerate() {
                lines.push(format!("{}) {}", index + 1, option["name"].as_str().unwrap_or_default()));
            }
            self.send_message(&chat, &lines.join("\n"), None, None, None).await?;
            hub.store.mark_whatsapp_card(&chat, &entry, &sender)?;
        }
        Ok(())
    }

    /// `1` approves and `2` denies the last card of the chat; true when the text was one of them.
    async fn answer_card(&self, hub: &Arc<Hub>, chat: &str, sender: &str, text: &str) -> bool {
        let choice = match text.trim() {
            "1" => 0,
            "2" => 1,
            _ => return false,
        };
        let Some(id) = hub.store.whatsapp_card(chat, sender) else { return false };
        let card = hub.store.entry(&id).filter(|e| e.kind == "permission" && e.data["status"] == "pending");
        let Some(card) = card else {
            hub.store.forget_whatsapp_card(chat, &id).ok();
            let _ = self.send_message(chat, "Esa aprobación ya no está pendiente.", None, None, None).await;
            return true;
        };
        let Some(option) = card.data["options"].as_array().and_then(|o| o.get(choice)) else { return true };
        let option_id = option["optionId"].as_str().unwrap_or_default().to_owned();
        let answered = crate::api::dispatch(
            hub,
            &crate::api::devices::Caller::Local,
            "respondPermission",
            json!({"entryId": id, "optionId": option_id}),
        )
        .await;
        hub.store.forget_whatsapp_card(chat, &id).ok();
        let line = match answered {
            Ok(_) => "Hecho.".to_owned(),
            Err(error) => format!("{error:#}").chars().take(180).collect(),
        };
        let _ = self.send_message(chat, &line, None, None, None).await;
        true
    }

    async fn begin(&self, hub: &Arc<Hub>, phone: Option<&str>) -> Result<()> {
        {
            let state = self.state.lock().await;
            if phone.is_none() && matches!(state.state.as_str(), "starting" | "qr" | "pairing" | "connected") {
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
        let generation = {
            let mut link = self.link.lock().await;
            link.generation += 1;
            link.child = Some(child);
            link.stdin = stdin;
            link.generation
        };
        let hub = hub.clone();
        let app = hub.whatsapp.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(event) = serde_json::from_str::<Value>(&line) {
                    app.on_event(&hub, event).await;
                }
            }
            app.dropped(&hub, generation).await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        match phone {
            Some(phone) => self.send(&json!({"type": "pair", "phone": phone})).await,
            None => self.send(&json!({"type": "start"})).await,
        }
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
        self.set_pairing(None);
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
                let next = LinkState {
                    state: event["state"].as_str().unwrap_or("disconnected").into(),
                    me: event["me"].as_str().map(str::to_owned),
                    name: event["name"].as_str().map(str::to_owned),
                    error: event["error"].as_str().map(str::to_owned),
                    qr: None,
                    pair_code: None,
                };
                if next.state == "connected" || next.state == "loggedOut" {
                    self.set_pairing(None);
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
        let sender = event["sender"].as_str().unwrap_or_default();
        let group = event["group"].as_bool().unwrap_or(false);
        if group && !event["mentioned"].as_bool().unwrap_or(false) && !event["quoted"].as_bool().unwrap_or(false) {
            return;
        }
        // With a list, only the owner, allowed people and allowed groups get through.
        let allowed = allowed(hub);
        let me = self.state.lock().await.me.clone().unwrap_or_default();
        let own = !me.is_empty() && (chat == me || sender == me);
        if !allowed.is_empty() && !own {
            let listed = if group { allowed.iter().any(|a| a == chat) } else { allowed.iter().any(|a| a == sender) };
            if !listed {
                return;
            }
        }
        // `1` or `2` answers the request card, from the person who wrote it.
        if self.answer_card(hub, chat, sender, &text).await {
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
            hub.store.map_whatsapp_thread(chat, &root, sender).ok();
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

    async fn dropped(&self, hub: &Hub, generation: u64) {
        if self.link.lock().await.generation != generation {
            return;
        }
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

/// The chats and people allowed to write, as `whatsapp.allowed` keeps them; empty means anyone.
pub fn allowed(hub: &Hub) -> Vec<String> {
    hub.store
        .kv_get("whatsapp.allowed")
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .unwrap_or_default()
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

/// A phone number in digits, with its country code, as `WhatsApp`'s pairing code takes it.
fn normalize_phone(value: &str) -> Result<String> {
    let digits: String = value.chars().filter(char::is_ascii_digit).collect();
    if !(8..=15).contains(&digits.len()) {
        bail!("Use a phone number with its country code.");
    }
    Ok(digits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_link_has_no_code_and_is_disconnected() {
        let state = LinkState::default();
        assert_eq!(state.state, "");
        assert!(state.qr.is_none());
        assert!(state.pair_code.is_none());
    }

    #[test]
    fn a_pairing_number_keeps_its_digits_and_needs_its_country_code() {
        assert_eq!(normalize_phone("+52 (555) 555-5555").unwrap(), "525555555555");
        assert_eq!(normalize_phone("525555555555").unwrap(), "525555555555");
        assert!(normalize_phone("55555555").is_ok());
        assert!(normalize_phone("5555 555").is_err());
        assert!(normalize_phone("1234567890123456").is_err());
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
