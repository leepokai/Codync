//! A linked `WhatsApp` number for this computer: a child process speaks the `WhatsApp` Web protocol,
//! the host only starts it, keeps its state and turns its messages into chat and routine events.
//!
//! The number is not official `WhatsApp` Web: linking uses the phone's linked devices, and `WhatsApp`
//! can block numbers it sees as automated. The child process dies with the host.

use crate::hub::Hub;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
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
}

impl WhatsApp {
    pub async fn state(&self) -> LinkState {
        self.state.lock().await.clone()
    }

    /// Starts the bridge and the link. A running link stays as it is.
    pub fn start(self: &Arc<Self>, hub: &Arc<Hub>) {
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
        let text = event["text"].as_str().unwrap_or_default();
        if text.is_empty() && event["media"].is_null() {
            return;
        }
        let media = event["media"].as_object();
        let note = media.map_or_else(
            || text.to_owned(),
            |m| {
                let kind = m.get("kind").and_then(Value::as_str).unwrap_or("file");
                format!("[{kind}] {}", m.get("name").and_then(Value::as_str).unwrap_or(text))
            },
        );
        let routine_event = json!({
            "source": "whatsapp",
            "event": if event["group"].as_bool().unwrap_or(false) { "groupMention" } else { "message" },
            "chat": event["chat"],
            "from": event["sender"],
            "name": event["name"],
            "text": text,
            "group": event["group"],
            "media": media.cloned(),
        });
        hub.routines.receive_event(&hub.store, &routine_event, None);
        let Some(bot) = hub.store.kv_get("whatsapp.bot") else { return };
        if hub.store.bot(&bot).ok().flatten().is_none() {
            return;
        }
        let prompt = format!(
            "WhatsApp message from {}:\n\n{note}",
            event["name"].as_str().unwrap_or_else(|| event["sender"].as_str().unwrap_or("someone"))
        );
        if let Err(error) = crate::api::dispatch(
            hub,
            &crate::api::devices::Caller::Local,
            "send",
            json!({"botId": bot, "text": prompt}),
        )
        .await
        {
            tracing::warn!(%error, "Could not deliver a WhatsApp message to a bot");
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
}
