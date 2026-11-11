//! Keep bridge receipts flowing while incoming control messages wait for their own sends.

use super::{Hub, LinkState, Value, WhatsApp, json};
use anyhow::{Context, Result, ensure};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

impl WhatsApp {
    pub(super) async fn read(self: Arc<Self>, hub: Arc<Hub>, stdout: tokio::process::ChildStdout, epoch: u64) {
        let (incoming, mut receiver) = tokio::sync::mpsc::channel(4);
        let app = self.clone();
        let target = hub.clone();
        let mut worker = tokio::spawn(async move {
            while let Some(event) = receiver.recv().await {
                if app.transport_epoch.load(Ordering::SeqCst) != epoch {
                    break;
                }
                app.on_event(&target, event).await;
            }
        });
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(mut event) = serde_json::from_str::<Value>(&line) else { continue };
            if self.transport_epoch.load(Ordering::SeqCst) != epoch {
                break;
            }
            if self.receipts.accept(&event) {
                continue;
            }
            if event["type"] == "message" {
                let me = self.state.lock().await.me.clone();
                let Some(me) = me else { continue };
                event["_codyncTransportEpoch"] = epoch.into();
                event["_codyncTransportMe"] = me.into();
                // Never await a full queue here: the receipt of a command being handled may
                // be further down stdout. Raw media is already bounded by the bridge.
                if let Err(error) = incoming.try_send(event) {
                    tracing::warn!("WhatsApp bridge input queue is full; incoming message was not accepted");
                    let event = error.into_inner();
                    let chat = event["chat"].as_str().unwrap_or_default().to_owned();
                    let sender = event["sender"].as_str().unwrap_or_default().to_owned();
                    let group = event["group"] == true;
                    let app = self.clone();
                    let hub = hub.clone();
                    // Retain only identity strings, never the rejected raw media payload.
                    tokio::spawn(async move {
                        app.reject_full(&hub, &chat, &sender, group).await;
                    });
                }
            } else {
                let _lifecycle = self.lifecycle.lock().await;
                if self.transport_epoch.load(Ordering::SeqCst) == epoch {
                    self.on_event(&hub, event).await;
                }
            }
        }
        {
            let _lifecycle = self.lifecycle.lock().await;
            if self.transport_epoch.load(Ordering::SeqCst) == epoch {
                self.receipts.disconnect();
                self.dropped(&hub).await;
            }
        }
        // A socket close must not discard accepted messages still being saved to the inbox.
        drop(incoming);
        if tokio::time::timeout(std::time::Duration::from_secs(10), &mut worker).await.is_err() {
            worker.abort();
        }
    }

    async fn reject_full(&self, hub: &Hub, chat: &str, sender: &str, group: bool) {
        if chat.is_empty() || sender.is_empty() {
            return;
        }
        let state = self.state.lock().await.clone();
        let Ok(raw) = hub.store.kv_read("whatsapp.allowed") else { return };
        let Ok(allowed) = raw.as_deref().map(serde_json::from_str::<Vec<String>>).transpose() else { return };
        let allowed = allowed.unwrap_or_default();
        let own = state.me.as_deref().is_some_and(|me| me == chat || me == sender);
        if !own && !allowed.is_empty() && !allowed.iter().any(|id| id == if group { chat } else { sender }) {
            return;
        }
        if let Err(error) =
            self.send_message(chat, "The input queue is full. Please try again shortly.", None, None, None).await
        {
            tracing::warn!(%error, "WhatsApp could not report a full input queue");
        }
    }

    /// Sends text or a file, returning success only after the bridge confirms a message id.
    pub async fn send_message(
        &self,
        to: &str,
        text: &str,
        path: Option<&Path>,
        name: Option<&str>,
        mimetype: Option<&str>,
    ) -> Result<()> {
        let mut body = json!({"type":"send","to":to,"text":text});
        if let Some(path) = path {
            body["path"] = json!(path);
            body["name"] = json!(name.unwrap_or("file"));
            body["mimetype"] = json!(mimetype.unwrap_or("application/octet-stream"));
        }
        self.send_payload(body, None).await
    }

    pub(super) async fn send_bound_text(
        &self,
        store: &crate::store::Store,
        account: &str,
        binding: &Value,
        to: &str,
        text: &str,
    ) -> Result<()> {
        self.send_payload(json!({"type":"send","to":to,"text":text}), Some((store, account, binding))).await
    }

    pub(super) async fn send_bound_file(
        &self,
        store: &crate::store::Store,
        binding: &Value,
        to: &str,
        file: &Value,
    ) -> Result<()> {
        let account = binding[0].as_str().unwrap_or_default();
        let body = json!({"type":"send", "to":to,"text":"", "path":file["path"],
            "name":file["name"].as_str().unwrap_or("file"),
            "mimetype":file["mimetype"].as_str().unwrap_or("application/octet-stream")});
        self.send_payload(body, Some((store, account, binding))).await
    }

    async fn send_payload(&self, mut body: Value, binding: Option<(&crate::store::Store, &str, &Value)>) -> Result<()> {
        let epoch = self.transport_epoch.load(Ordering::SeqCst);
        let identity = self.state.lock().await.me.clone();
        let lifecycle = self.lifecycle.lock().await;
        let state = self.state.lock().await;
        if let Some((store, account, expected)) = binding {
            ensure!(
                super::delivery::matches(store, account, expected)
                    .map_err(|error| super::delivery::unsent(error, true))?,
                "WhatsApp routine connection was revoked or replaced"
            );
        }
        if self.transport_epoch.load(Ordering::SeqCst) != epoch || state.me != identity {
            return Err(super::delivery::unsent(
                anyhow::anyhow!("WhatsApp connection changed before sending"),
                binding.is_some(),
            ));
        }
        if state.state != "connected" {
            return Err(super::delivery::unsent(anyhow::anyhow!("WhatsApp is not connected"), binding.is_some()));
        }
        if let Some((_, account, _)) = binding {
            ensure!(state.me.as_deref() == Some(account), "WhatsApp routine connection was revoked or replaced");
        }
        drop(state);
        let receipt = self.receipts.reserve().map_err(|error| super::delivery::unsent(error, binding.is_some()))?;
        body["id"] = json!(receipt.id);
        self.send(&body).await?;
        drop(lifecycle);
        receipt.wait().await
    }

    pub(super) async fn send(&self, body: &Value) -> Result<()> {
        let operation = async {
            let mut link = self.link.lock().await;
            let stdin = link.stdin.as_mut().context("WhatsApp is not running")?;
            stdin.write_all(format!("{body}\n").as_bytes()).await?;
            stdin.flush().await?;
            anyhow::Ok(())
        };
        tokio::time::timeout(std::time::Duration::from_secs(10), operation)
            .await
            .context("WhatsApp bridge write timed out; its outcome may be unknown")?
    }

    pub(super) async fn set(&self, hub: &Hub, next: LinkState) {
        if next.state != "connected" {
            self.receipts.disconnect();
        }
        let view = next.view();
        *self.state.lock().await = next;
        let _ = hub.events.send(json!({"type":"whatsapp","whatsapp":view}));
    }
}
