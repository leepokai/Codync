//! The screen helper's socket: accepting helpers, JSON-RPC requests to them, and the Linux helper process.
//! Windows has no screen helper yet, so nothing listens there.

#[cfg(unix)]
use super::protocol::HelperStatus;
use super::{HELPER_TIMEOUT, Screen};
use crate::LockExt;
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
#[cfg(unix)]
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};

pub(super) type Pending = Mutex<HashMap<i64, oneshot::Sender<Result<Value, String>>>>;

/// One connected screen helper.
pub(super) struct Link {
    #[cfg_attr(windows, expect(dead_code, reason = "Windows has no screen helper yet"))]
    pub(super) id: u64,
    pub(super) tx: mpsc::UnboundedSender<String>,
    pub(super) pending: Pending,
    pub(super) next_id: AtomicI64,
}

impl Link {
    pub(super) async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.locked().insert(id, tx);
        let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
        if self.tx.send(line).is_err() {
            self.pending.locked().remove(&id);
            bail!("the screen helper disconnected");
        }
        // Permission dialogs wait for a person; normal helper calls should still fail quickly.
        let timeout = if method == "requestPermission" { Duration::from_secs(120) } else { HELPER_TIMEOUT };
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(Ok(v))) => Ok(v),
            Ok(Ok(Err(message))) => Err(anyhow!(message)),
            Ok(Err(_)) => bail!("the screen helper disconnected"),
            Err(_) => {
                self.pending.locked().remove(&id);
                bail!("the screen helper didn't answer `{method}` in time")
            }
        }
    }
}

#[cfg(windows)]
pub async fn serve_helpers(_screen: Arc<Screen>) {}

/// Accepts screen helpers on `~/.codync/screen.sock`. The newest connection wins.
#[cfg(unix)]
pub async fn serve_helpers(screen: Arc<Screen>) {
    let path = crate::service::data_dir().join("screen.sock");
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(error) => {
            tracing::error!(%error, path = %path.display(), "can't listen for the screen helper");
            return;
        }
    };
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(error) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)) {
            tracing::warn!(%error, "couldn't restrict the screen helper socket");
        }
    }
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(run_link(screen.clone(), stream));
            }
            Err(error) => {
                tracing::warn!(%error, "screen helper accept failed");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub(super) fn graphical_session() -> bool {
    true
}

#[cfg(windows)]
pub(super) fn graphical_session() -> bool {
    false
}

/// A Wayland socket in this user's runtime dir, or a running X server.
#[cfg(target_os = "linux")]
pub(super) fn graphical_session() -> bool {
    let has = |dir: &std::path::Path, prefix: &str| {
        std::fs::read_dir(dir)
            .is_ok_and(|entries| entries.flatten().any(|e| e.file_name().to_string_lossy().starts_with(prefix)))
    };
    std::env::var_os("XDG_RUNTIME_DIR").is_some_and(|dir| has(dir.as_ref(), "wayland-"))
        || has(std::path::Path::new("/tmp/.X11-unix"), "X")
}

/// Linux: the host runs `codync-screen` itself while Remote screen is on (on macOS
/// launchd runs Codync Screen for the app). Restarts it if it exits; stops it when turned off.
#[cfg(target_os = "linux")]
pub async fn supervise_linux_helper(screen: Arc<Screen>) {
    let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("codync-screen")));
    let program = exe.filter(|p| p.is_file()).unwrap_or_else(|| "codync-screen".into());
    let mut child: Option<tokio::process::Child> = None;
    let mut backoff = Duration::from_secs(1);
    loop {
        let running = child.as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)));
        if screen.enabled() && !running {
            match tokio::process::Command::new(&program).kill_on_drop(true).spawn() {
                Ok(c) => child = Some(c),
                Err(error) => tracing::warn!(%error, program = %program.display(), "can't start codync-screen"),
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(30));
            continue;
        }
        if !screen.enabled()
            && let Some(mut c) = child.take()
        {
            let _ = c.kill().await;
        }
        if running && screen.link.locked().is_some() {
            backoff = Duration::from_secs(1);
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

#[cfg(unix)]
async fn run_link(screen: Arc<Screen>, stream: UnixStream) {
    let (rd, mut wr) = stream.into_split();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let link = Arc::new(Link {
        id: screen.next_link.fetch_add(1, Ordering::Relaxed),
        tx,
        pending: Mutex::default(),
        next_id: AtomicI64::new(1),
    });
    *screen.link.locked() = Some(link.clone());
    tracing::info!("screen helper connected");
    let writer = tokio::spawn(async move {
        while let Some(mut line) = rx.recv().await {
            line.push('\n');
            if wr.write_all(line.as_bytes()).await.is_err() {
                break;
            }
        }
    });
    let mut lines = BufReader::new(rd).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            tracing::debug!(line, "ignoring non-JSON from the screen helper");
            continue;
        };
        match (msg["method"].as_str(), msg["id"].as_i64()) {
            (None, Some(id)) => {
                if let Some(waiter) = link.pending.locked().remove(&id) {
                    let res = match msg.get("error") {
                        Some(e) => Err(e["message"].as_str().map_or_else(|| e.to_string(), str::to_owned)),
                        None => Ok(msg["result"].clone()),
                    };
                    let _ = waiter.send(res);
                }
            }
            (Some("status"), _) => match serde_json::from_value::<HelperStatus>(msg["params"].clone()) {
                Ok(st) => {
                    *screen.status.locked() = st;
                    screen.emit();
                }
                Err(error) => tracing::warn!(%error, "bad status from the screen helper"),
            },
            (Some("session"), _) => {
                if msg["params"]["state"] == "closed"
                    && let Some(s) = msg["params"]["session"].as_str()
                {
                    screen.session_closed(s);
                }
            }
            (Some(method), _) => tracing::debug!(method, "unknown screen helper notification"),
            _ => {}
        }
    }
    writer.abort();
    link.pending.locked().clear();
    let current = {
        let mut slot = screen.link.locked();
        let current = slot.as_ref().is_some_and(|l| l.id == link.id);
        if current {
            *slot = None;
        }
        current
    };
    if current {
        *screen.status.locked() = HelperStatus::default();
        screen.sessions.locked().clear();
        screen.control.locked().user = false;
        screen.emit();
    }
    tracing::info!("screen helper disconnected");
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::screen::tests::retina;
    use crate::screen::viewer::{IceConfig, IceServer};
    use tokio::sync::broadcast;

    #[tokio::test]
    async fn helper_roundtrip_over_the_socket() {
        let (tx, _) = broadcast::channel(8);
        let screen = Arc::new(Screen::new(Some(true), tx));
        let (a, b) = UnixStream::pair().unwrap();
        let (observed_tx, mut observed_rx) = mpsc::unbounded_channel();
        // Fake helper reports a display and records the host's ICE configuration.
        tokio::spawn(async move {
            let (rd, mut wr) = b.into_split();
            let status = json!({"jsonrpc": "2.0", "method": "status", "params": {"platform": "test", "capture": true, "displays": [retina()]}});
            wr.write_all(format!("{status}\n").as_bytes()).await.unwrap();
            let mut lines = BufReader::new(rd).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let m: Value = serde_json::from_str(&l).unwrap();
                observed_tx.send(m.clone()).unwrap();
                let res = json!({"jsonrpc": "2.0", "id": m["id"], "result": {"sdp": format!("answer to {}", m["params"]["sdp"].as_str().unwrap_or_default())}});
                wr.write_all(format!("{res}\n").as_bytes()).await.unwrap();
            }
        });
        tokio::spawn(run_link(screen.clone(), a));
        for _ in 0..50 {
            if screen.status.locked().capture {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let res = screen.offer("local", "offer", None, None).await.unwrap();
        assert_eq!(res["sdp"], "answer to offer");
        assert_eq!(screen.state()["viewers"], 1);
        assert_eq!(screen.state()["displays"][0]["name"], "Built-in");
        let direct = observed_rx.recv().await.unwrap();
        assert_eq!(direct["params"]["iceServers"], json!([]));
        assert_eq!(direct["params"]["maxBitrateBps"], 16_000_000);
        let config = IceConfig {
            ice_servers: vec![IceServer {
                urls: vec!["turn:turn.cloudflare.com:3478?transport=udp".into()],
                username: Some("u".into()),
                credential: Some("p".into()),
            }],
            ..IceConfig::default()
        };
        screen.takeover(true);
        let prepared = screen.prepare("phone", config).unwrap();
        screen.session_closed(res["session"].as_str().unwrap());
        assert_eq!(screen.state()["userControl"], true, "renewal keeps takeover while the replacement is pending");
        let id = prepared["session"].as_str().unwrap();
        assert!(screen.offer("other", "offer", Some(id), None).await.is_err());
        screen.offer("phone", "offer", Some(id), None).await.unwrap();
        let remote = observed_rx.recv().await.unwrap();
        assert_eq!(remote["params"]["iceServers"], prepared["iceServers"]);
        assert_eq!(remote["params"]["maxBitrateBps"], 4_000_000);
        assert_eq!(remote["params"]["maxFramerate"], 30);
        screen.sessions.locked().get_mut(id).unwrap().ice.expires_at = 0;
        assert!(screen.offer("phone", "offer", Some(id), None).await.is_err());
    }
}
