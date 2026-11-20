//! Loopback WebSocket bridge between the host and the Codync Browser extension.
//!
//! A small native-messaging helper connects Chrome's extension process to this authenticated,
//! loopback-only socket. The extension never receives the host bearer token.

use crate::LockExt;
use anyhow::{Context as _, Result, anyhow, bail, ensure};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt as _, StreamExt as _, future::BoxFuture};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, watch};

/// The public key in `apps/browser-extension/manifest.json` fixes this ID.
pub(super) const EXTENSION_ID: &str = "dgejialhpalhoigbkmgeabgpphlipiek";
pub(super) const TAKEOVER_ERROR: &str =
    "The user has taken over the screen from their phone. Wait until they hand it back, or ask them what to do.";
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
type PendingReplies = Arc<Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>>;

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
pub(super) struct ExtensionBridge {
    connections: Mutex<Connections>,
    next_connection: AtomicU64,
    takeover_epoch: watch::Sender<u64>,
}

impl Default for ExtensionBridge {
    fn default() -> Self {
        let (takeover_epoch, _) = watch::channel(0);
        Self { connections: Mutex::default(), next_connection: AtomicU64::new(0), takeover_epoch }
    }
}

#[derive(Default)]
struct Connections {
    active: Option<Connection>,
    waiting: VecDeque<Connection>,
}

impl ExtensionBridge {
    /// Synchronously revokes the active extension session when the user takes over the screen.
    pub(super) fn revoke_for_takeover(&self) {
        self.takeover_epoch.send_modify(|epoch| *epoch = epoch.saturating_add(1));
        if let Some(active) = &self.connections.locked().active {
            let _ = active.out.send(json!({
                "type": "takeover",
                "params": {"allForHost": true}
            }));
        }
    }

    /// Best-effort one-way revoke notification when turn cleanup gives up waiting for `stop`.
    pub(super) fn revoke_session(&self, target: &Value) {
        if let Some(active) = &self.connections.locked().active {
            let _ = active.out.send(json!({
                "type": "revoke",
                "params": {"target": target}
            }));
        }
    }

    #[cfg(test)]
    pub(super) fn attach_test_connection(&self, id: u64) -> mpsc::UnboundedReceiver<Value> {
        let (out, receiver) = mpsc::unbounded_channel();
        self.connections.locked().active = Some(Connection { id, out, pending: Arc::new(Mutex::new(HashMap::new())) });
        receiver
    }

    /// Runs one request in the extension. A missing connection is an explicit offline failure:
    /// callers must not fall back to another browser or computer.
    pub(super) fn call<'a>(&'a self, method: &'a str, params: Value) -> BoxFuture<'a, Result<Value>> {
        Box::pin(self.call_with_timeout(method, params, command_timeout()))
    }

    pub(super) async fn call_with_timeout(&self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        let mut takeover_rx = self.takeover_epoch.subscribe();
        let mut watch_takeover = !matches!(method, "stop" | "cancel" | "revoke");
        let connection = self.connections.locked().active.clone().ok_or_else(|| anyhow!(BridgeFailure::Offline))?;
        let id = uuid::Uuid::new_v4().to_string();
        let (reply, mut answer) = oneshot::channel();
        connection.pending.locked().insert(id.clone(), reply);
        if connection.out.send(json!({"id": id, "method": method, "params": params})).is_err() {
            connection.pending.locked().remove(&id);
            bail!(BridgeFailure::Send);
        }
        let deadline = tokio::time::sleep(timeout);
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                reply = &mut answer => {
                    return match reply {
                        Ok(Ok(value)) => Ok(value),
                        Ok(Err(message)) => bail!("{message}"),
                        Err(_) => bail!(BridgeFailure::Disconnected),
                    };
                }
                changed = takeover_rx.changed(), if watch_takeover => {
                    if changed.is_ok() {
                        connection.pending.locked().remove(&id);
                        let cancel_id = uuid::Uuid::new_v4().to_string();
                        let _ = connection.out.send(json!({
                            "id": cancel_id,
                            "method": "cancel",
                            "params": {"requestId": id, "revokeSession": true}
                        }));
                        bail!(TAKEOVER_ERROR);
                    }
                    watch_takeover = false;
                }
                () = &mut deadline => {
                    connection.pending.locked().remove(&id);
                    // A timed-out start or page action can leave a tab attached in Chrome. Ask the extension to
                    // cancel the still-running request; never cancel an in-flight cleanup request (`stop`/`cancel`/`revoke`).
                    if !matches!(method, "stop" | "cancel" | "revoke") {
                        let cancel_id = uuid::Uuid::new_v4().to_string();
                        let _ = connection.out.send(json!({
                            "id": cancel_id,
                            "method": "cancel",
                            "params": {"requestId": id}
                        }));
                    }
                    bail!(BridgeFailure::Timeout);
                }
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
            pending: Arc::new(Mutex::new(HashMap::new())),
        };
        let became_active = {
            let mut connections = self.connections.locked();
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
            self.remove_and_promote(connection.id, host_id);
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
                if let Some(reply) = connection.pending.locked().remove(id) {
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

        let _ = self.remove_and_promote(connection.id, host_id);
        fail_pending(&connection.pending, "the browser extension disconnected");
        writer.abort();
        result
    }

    fn remove_and_promote(&self, id: u64, host_id: &str) -> Option<Connection> {
        let mut connections = self.connections.locked();
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

fn fail_pending(pending: &PendingReplies, message: &str) {
    let waiting = pending.locked().drain().map(|(_, reply)| reply).collect::<Vec<_>>();
    for reply in waiting {
        let _ = reply.send(Err(message.to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection(id: u64) -> (Connection, mpsc::UnboundedReceiver<Value>) {
        let (out, receiver) = mpsc::unbounded_channel();
        (Connection { id, out, pending: Arc::new(Mutex::new(HashMap::new())) }, receiver)
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
            let mut connections = bridge.connections.locked();
            connections.active = Some(first.clone());
            connections.waiting.push_back(second.clone());
        }

        assert_eq!(first_out.try_recv(), Err(mpsc::error::TryRecvError::Empty));
        let promoted = bridge.remove_and_promote(1, "host-a").unwrap();
        assert_eq!(promoted.id, 2);
        assert_eq!(bridge.connections.locked().active.as_ref().unwrap().id, 2);
        assert_eq!(first_out.try_recv(), Err(mpsc::error::TryRecvError::Empty));
        assert_eq!(second_out.recv().await.unwrap()["type"], "ready");
    }

    #[tokio::test]
    async fn a_timed_out_command_is_sent_once_and_never_replayed() {
        let bridge = Arc::new(ExtensionBridge::default());
        let (active, mut output) = connection(1);
        let pending = active.pending.clone();
        bridge.connections.locked().active = Some(active);

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
        assert!(pending.locked().is_empty());
    }

    #[tokio::test]
    async fn takeover_revokes_active_extension_session_and_aborts_waiting_call() {
        let bridge = Arc::new(ExtensionBridge::default());
        let (active, mut output) = connection(1);
        let pending = active.pending.clone();
        bridge.connections.locked().active = Some(active);

        let task = {
            let bridge = bridge.clone();
            tokio::spawn(async move {
                bridge.call_with_timeout("click", json!({"selector": "#queued"}), Duration::from_secs(5)).await
            })
        };
        let sent = output.recv().await.unwrap();
        assert_eq!(sent["method"], "click");

        bridge.revoke_for_takeover();

        let takeover = output.recv().await.unwrap();
        assert_eq!(takeover["type"], "takeover");
        assert_eq!(takeover["params"]["allForHost"], true);

        let cancelled = output.recv().await.unwrap();
        assert_eq!(cancelled["method"], "cancel");
        assert_eq!(cancelled["params"]["requestId"], sent["id"]);
        assert_eq!(cancelled["params"]["revokeSession"], true);

        let error = task.await.unwrap().unwrap_err();
        assert!(error.to_string().contains("taken over the screen"));
        assert!(pending.locked().is_empty());

        let stop_task = {
            let bridge = bridge.clone();
            tokio::spawn(async move { bridge.call_with_timeout("stop", json!({}), Duration::from_secs(5)).await })
        };
        let stop_sent = output.recv().await.unwrap();
        assert_eq!(stop_sent["method"], "stop");
        let stop_id = stop_sent["id"].as_str().unwrap().to_owned();
        let reply = pending.locked().remove(&stop_id).unwrap();
        let _ = reply.send(Ok(json!({"closed": true})));
        assert_eq!(stop_task.await.unwrap().unwrap()["closed"], true);
    }

    #[tokio::test]
    async fn a_timed_out_stop_does_not_emit_cancel() {
        let bridge = Arc::new(ExtensionBridge::default());
        let mut output = bridge.attach_test_connection(1);

        let error = bridge
            .call_with_timeout("stop", json!({"target": {"browserId": "b1"}}), Duration::from_millis(20))
            .await
            .unwrap_err();
        assert!(matches!(error.downcast_ref::<BridgeFailure>(), Some(BridgeFailure::Timeout)));

        let sent = output.recv().await.unwrap();
        assert_eq!(sent["method"], "stop");
        assert_eq!(output.try_recv(), Err(mpsc::error::TryRecvError::Empty));
    }

    #[tokio::test]
    async fn revoke_session_emits_one_way_target_revoke() {
        let bridge = ExtensionBridge::default();
        let mut output = bridge.attach_test_connection(1);
        let target = json!({"hostId": "h1", "botId": "bot1", "browserId": "b1"});

        bridge.revoke_session(&target);

        let frame = output.recv().await.unwrap();
        assert_eq!(frame["type"], "revoke");
        assert!(frame.get("id").is_none(), "{frame}");
        assert_eq!(frame["params"]["target"], target);
        assert_eq!(output.try_recv(), Err(mpsc::error::TryRecvError::Empty));
    }
}
