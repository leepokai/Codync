//! Loopback WebSocket bridge between the host and the Codync Browser extension.
//!
//! A small native-messaging helper connects Chrome's extension process to this authenticated,
//! loopback-only socket. The extension never receives the host bearer token.

use anyhow::{Context as _, Result, anyhow, bail, ensure};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt as _, StreamExt as _, future::BoxFuture};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{Mutex, mpsc, oneshot};

/// The public key in `apps/browser-extension/manifest.json` fixes this ID.
pub(super) const EXTENSION_ID: &str = "dgejialhpalhoigbkmgeabgpphlipiek";
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
type PendingReplies = std::sync::Arc<Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>>;

#[derive(Debug, Clone, Copy)]
pub(super) enum BridgeFailure {
    Offline,
    Send,
    Disconnected,
    Timeout,
}

impl std::fmt::Display for BridgeFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Offline => {
                "The Codync Browser extension isn't connected on this computer. Install and enable it in Chrome or Edge, then keep that browser running."
            }
            Self::Send => "The Codync Browser extension disconnected before the request could run",
            Self::Disconnected => "The Codync Browser extension disconnected before it answered",
            Self::Timeout => {
                "The Codync Browser extension did not answer in time. The action was not retried."
            }
        })
    }
}

impl std::error::Error for BridgeFailure {}

pub(super) fn failure_category(error: &anyhow::Error) -> &'static str {
    match error.downcast_ref::<BridgeFailure>() {
        Some(BridgeFailure::Offline) => "extension_offline",
        Some(BridgeFailure::Send | BridgeFailure::Disconnected) => "extension_disconnected",
        Some(BridgeFailure::Timeout) => "extension_timeout",
        None => {
            let message = format!("{error:#}").to_ascii_lowercase();
            if [
                "browser belongs to another bot",
                "stale browser operation",
                "browser tab changed",
                "browser target changed",
                "no longer running",
                "different user message",
            ]
            .iter()
            .any(|needle| message.contains(needle))
            {
                "stale_target"
            } else {
                "operation"
            }
        }
    }
}

#[derive(Clone)]
struct Connection {
    id: u64,
    out: mpsc::UnboundedSender<Value>,
    pending: PendingReplies,
}

/// One extension connection and the requests waiting for its replies.
#[derive(Default)]
pub(super) struct ExtensionBridge {
    connections: Mutex<Connections>,
    next_connection: AtomicU64,
}

#[derive(Default)]
struct Connections {
    active: Option<Connection>,
    waiting: VecDeque<Connection>,
}

impl ExtensionBridge {
    /// Runs one request in the extension. A missing connection is an explicit offline failure:
    /// callers must not fall back to another browser or computer.
    pub(super) fn call<'a>(&'a self, method: &'a str, params: Value) -> BoxFuture<'a, Result<Value>> {
        Box::pin(self.call_with_timeout(method, params, command_timeout()))
    }

    pub(super) async fn call_with_timeout(&self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        let connection = self.connections.lock().await.active.clone().ok_or_else(|| anyhow!(BridgeFailure::Offline))?;
        let id = uuid::Uuid::new_v4().to_string();
        let (reply, answer) = oneshot::channel();
        connection.pending.lock().await.insert(id.clone(), reply);
        if connection.out.send(json!({"id": id, "method": method, "params": params})).is_err() {
            connection.pending.lock().await.remove(&id);
            bail!(BridgeFailure::Send);
        }
        match tokio::time::timeout(timeout, answer).await {
            Ok(Ok(Ok(value))) => Ok(value),
            Ok(Ok(Err(message))) => bail!("{message}"),
            Ok(Err(_)) => bail!(BridgeFailure::Disconnected),
            Err(_) => {
                connection.pending.lock().await.remove(&id);
                // A timed-out start can leave a tab attached in Chrome. Ask the extension to cancel
                // the still-running request; if it already created a tab, its request guard removes it.
                let cancel_id = uuid::Uuid::new_v4().to_string();
                let _ = connection.out.send(json!({
                    "id": cancel_id,
                    "method": "cancel",
                    "params": {"requestId": id}
                }));
                bail!(BridgeFailure::Timeout);
            }
        }
    }

    /// Serves one authenticated loopback WebSocket, expecting a `hello` from our fixed extension.
    pub(super) async fn serve(&self, mut socket: WebSocket, host_id: &str) -> Result<()> {
        let hello = tokio::time::timeout(HELLO_TIMEOUT, socket.recv())
            .await
            .context("the Codync Browser extension did not send a hello")?
            .context("the Codync Browser extension closed before its hello")?
            .context("reading the Codync Browser extension hello")?;
        let hello = message_json(&hello)?;
        ensure!(hello["type"] == "hello", "the Codync Browser bridge expected a hello");
        ensure!(hello["extensionId"] == EXTENSION_ID, "the Codync Browser bridge only accepts the Codync extension");
        ensure!(hello["protocol"] == 1, "unsupported Codync Browser extension protocol");
        ensure!(
            hello["isolatedProfile"] == true,
            "Codync Browser must run in its dedicated profile; ordinary browser profiles are not accepted"
        );

        let (mut sink, mut stream) = socket.split();
        let (out, mut output) = mpsc::unbounded_channel();
        let connection = Connection {
            id: self.next_connection.fetch_add(1, Ordering::Relaxed) + 1,
            out,
            pending: std::sync::Arc::new(Mutex::new(HashMap::new())),
        };
        let became_active = {
            let mut connections = self.connections.lock().await;
            if connections.active.is_none() {
                connections.active = Some(connection.clone());
                true
            } else {
                connections.waiting.push_back(connection.clone());
                false
            }
        };
        let message = if became_active {
            json!({
                "type": "ready",
                "hostId": host_id,
                "protocol": 1,
                "isolatedProfile": true
            })
        } else {
            json!({
                "type": "queued",
                "detail": "Another Chrome or Edge window is connected to Codync. This window will connect when that one closes."
            })
        };
        if connection.out.send(message).is_err() {
            self.remove_and_promote(connection.id, host_id).await;
            bail!("the browser extension disconnected while joining the bridge");
        }

        let writer = tokio::spawn(async move {
            while let Some(value) = output.recv().await {
                if sink.send(Message::Text(value.to_string().into())).await.is_err() {
                    break;
                }
            }
        });

        let result = async {
            while let Some(message) = stream.next().await {
                let value = message_json(&message?)?;
                if value["type"] == "event" {
                    continue;
                }
                let Some(id) = value["id"].as_str() else {
                    continue;
                };
                if let Some(reply) = connection.pending.lock().await.remove(id) {
                    let answer = if value["ok"] == true {
                        Ok(value.get("result").cloned().unwrap_or(Value::Null))
                    } else {
                        let message =
                            value["error"].as_str().unwrap_or("the Codync Browser extension returned an invalid error");
                        match value["code"].as_str() {
                            Some(code) => Err(format!("{code}: {message}")),
                            None => Err(message.to_owned()),
                        }
                    };
                    let _ = reply.send(answer);
                }
            }
            Ok(())
        }
        .await;

        let _ = self.remove_and_promote(connection.id, host_id).await;
        fail_pending(connection.pending, "the browser extension disconnected").await;
        writer.abort();
        result
    }

    async fn remove_and_promote(&self, id: u64, host_id: &str) -> Option<Connection> {
        let mut connections = self.connections.lock().await;
        if connections.active.as_ref().is_some_and(|active| active.id == id) {
            connections.active.take();
        } else {
            connections.waiting.retain(|connection| connection.id != id);
            return None;
        }
        while let Some(next) = connections.waiting.pop_front() {
            if next
                .out
                .send(json!({
                    "type": "ready",
                    "hostId": host_id,
                    "protocol": 1,
                    "isolatedProfile": true
                }))
                .is_ok()
            {
                connections.active = Some(next.clone());
                return Some(next);
            }
        }
        None
    }
}

fn command_timeout() -> Duration {
    if cfg!(debug_assertions)
        && let Some(milliseconds) = std::env::var("CODYNC_BROWSER_COMMAND_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
    {
        return Duration::from_millis(milliseconds);
    }
    DEFAULT_COMMAND_TIMEOUT
}

fn message_json(message: &Message) -> Result<Value> {
    match message {
        Message::Text(text) => serde_json::from_str(text.as_str()).context("invalid browser extension JSON"),
        Message::Binary(bytes) => serde_json::from_slice(bytes).context("invalid browser extension JSON"),
        Message::Close(_) => bail!("the browser extension closed the bridge"),
        Message::Ping(_) | Message::Pong(_) => Ok(Value::Null),
    }
}

async fn fail_pending(pending: PendingReplies, message: &str) {
    let waiting = pending.lock().await.drain().map(|(_, reply)| reply).collect::<Vec<_>>();
    for reply in waiting {
        let _ = reply.send(Err(message.to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection(id: u64) -> (Connection, mpsc::UnboundedReceiver<Value>) {
        let (out, receiver) = mpsc::unbounded_channel();
        (Connection { id, out, pending: std::sync::Arc::new(Mutex::new(HashMap::new())) }, receiver)
    }

    #[test]
    fn extension_id_is_a_chrome_id() {
        assert_eq!(EXTENSION_ID.len(), 32);
        assert!(EXTENSION_ID.bytes().all(|byte| (b'a'..=b'p').contains(&byte)));
    }

    #[tokio::test]
    async fn waiting_connections_are_promoted_in_order_without_disconnecting_the_active_browser() {
        let bridge = ExtensionBridge::default();
        let (first, mut first_out) = connection(1);
        let (second, mut second_out) = connection(2);
        {
            let mut connections = bridge.connections.lock().await;
            connections.active = Some(first.clone());
            connections.waiting.push_back(second.clone());
        }

        assert_eq!(first_out.try_recv(), Err(mpsc::error::TryRecvError::Empty));
        let promoted = bridge.remove_and_promote(1, "host-a").await.unwrap();
        assert_eq!(promoted.id, 2);
        assert_eq!(bridge.connections.lock().await.active.as_ref().unwrap().id, 2);
        assert_eq!(first_out.try_recv(), Err(mpsc::error::TryRecvError::Empty));
        assert_eq!(second_out.recv().await.unwrap()["type"], "ready");
    }

    #[tokio::test]
    async fn a_timed_out_command_is_sent_once_and_never_replayed() {
        let bridge = std::sync::Arc::new(ExtensionBridge::default());
        let (active, mut output) = connection(1);
        let pending = active.pending.clone();
        bridge.connections.lock().await.active = Some(active);

        let task = {
            let bridge = bridge.clone();
            tokio::spawn(async move {
                bridge.call_with_timeout("click", json!({"selector": "#once"}), Duration::from_millis(20)).await
            })
        };
        let sent = output.recv().await.unwrap();
        assert_eq!(sent["method"], "click");
        let error = task.await.unwrap().unwrap_err();
        assert!(matches!(error.downcast_ref::<BridgeFailure>(), Some(BridgeFailure::Timeout)));
        let cancelled = output.recv().await.unwrap();
        assert_eq!(cancelled["method"], "cancel");
        assert_eq!(cancelled["params"]["requestId"], sent["id"]);
        assert_eq!(output.try_recv(), Err(mpsc::error::TryRecvError::Empty));
        assert!(pending.lock().await.is_empty());
    }
}
