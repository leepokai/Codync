//! A linked `WhatsApp` number for this computer: a child process speaks the `WhatsApp` Web protocol,
//! the host only starts it, keeps its state and turns its messages into chat and routine events.
//!
//! The number is not official `WhatsApp` Web: linking uses the phone's linked devices, and `WhatsApp`
//! can block numbers it sees as automated. The child process dies with the host.

mod approvals;
pub(crate) mod delivery;
mod incoming;
mod outgoing;
mod receipts;
mod settings;
mod transport;
mod voice_notes;
pub(crate) use settings::{set_allowed, set_bot};
mod inbox;
pub(crate) use inbox::claim_prompt;

use crate::hub::Hub;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::{path::PathBuf, process::Stdio, sync::Arc};
use tokio::{
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
}

#[derive(Default)]
pub struct WhatsApp {
    receipts: receipts::Receipts,
    lifecycle: Mutex<()>,
    transport_epoch: std::sync::atomic::AtomicU64,
    revocation_pending: std::sync::atomic::AtomicBool,
    incoming_worker: std::sync::atomic::AtomicBool,
    state: Mutex<LinkState>,
    link: Mutex<Link>,
    sender: std::sync::atomic::AtomicBool,
    /// The number a pairing code was asked for, until the link ends.
    pairing: std::sync::Mutex<Option<String>>,
}

impl WhatsApp {
    pub async fn state(&self) -> LinkState {
        self.state.lock().await.clone()
    }

    fn set_pairing(&self, phone: Option<String>) {
        if let Ok(mut pairing) = self.pairing.lock() {
            *pairing = phone;
        }
    }

    /// Starts the bridge and the link. A running link stays as it is.
    pub fn start(self: &Arc<Self>, hub: &Arc<Hub>) {
        self.ensure_sender(hub);
        inbox::start(self, hub);
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
        inbox::start(self, hub);
        let app = self.clone();
        let hub = hub.clone();
        tokio::spawn(async move {
            app.set_pairing(Some(phone.clone()));
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

    async fn begin(&self, hub: &Arc<Hub>, phone: Option<&str>) -> Result<()> {
        let _lifecycle = self.lifecycle.lock().await;
        if self.revocation_pending.load(std::sync::atomic::Ordering::SeqCst) {
            inbox::invalidate_link(hub)?;
        }
        {
            let state = self.state.lock().await;
            if phone.is_none() && matches!(state.state.as_str(), "starting" | "qr" | "pairing" | "connected") {
                return Ok(());
            }
        }
        let epoch = {
            let _configuration = crate::LockExt::locked(&hub.store.connector_lock);
            self.transport_epoch.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
        };
        self.receipts.disconnect();
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
        tokio::spawn(app.read(hub, stdout, epoch));
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        match phone {
            Some(phone) => self.send(&json!({"type": "pair", "phone": phone})).await,
            None => self.send(&json!({"type": "start"})).await,
        }
    }

    pub async fn logout(&self, hub: &Arc<Hub>) -> Result<()> {
        let _lifecycle = self.lifecycle.lock().await;
        {
            let _configuration = crate::LockExt::locked(&hub.store.connector_lock);
            self.transport_epoch.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        let invalidated = inbox::invalidate_link(hub);
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
        invalidated
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
                    pair_code: None,
                };
                if next.state == "connected" || next.state == "loggedOut" {
                    self.set_pairing(None);
                }
                if next.state == "connected" {
                    if let Err(error) = settings::record_account(hub, next.me.as_deref().unwrap_or_default()) {
                        self.set(
                            hub,
                            LinkState {
                                state: "error".into(),
                                error: Some(format!("{error:#}")),
                                ..LinkState::default()
                            },
                        )
                        .await;
                        return;
                    }
                    self.set(hub, next).await;
                    return;
                }
                if next.state == "loggedOut"
                    && let Err(error) = inbox::invalidate_link(hub)
                {
                    next.state = "error".into();
                    next.error = Some(format!("Could not revoke the previous WhatsApp link: {error:#}"));
                }
                if next.state == "error" {
                    next.state = "error".into();
                }
                self.set(hub, next).await;
            }
            "pairCode" => {
                self.set(
                    hub,
                    LinkState {
                        state: "pairing".into(),
                        pair_code: event["code"].as_str().map(str::to_owned),
                        ..LinkState::default()
                    },
                )
                .await;
            }
            "message" => {
                if let Err(error) = inbox::receive(self, hub, &event).await {
                    tracing::warn!(%error, "WhatsApp message could not be queued");
                }
            }
            _ => {}
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
    let Some(data) = media.get("data").and_then(Value::as_str) else {
        return Ok(voice_notes::stored_path(media)?.map(|path| path.to_string_lossy().into_owned()));
    };
    if data.len() > (32_usize * 1024 * 1024).div_ceil(3) * 4 {
        return Ok(None);
    }
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
