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
        let timeout = if method == "requestPermission" { Duration::from_mins(2) } else { HELPER_TIMEOUT };
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
    let mut generation = screen.helper_generation.subscribe();
    let (rd, mut wr) = stream.into_split();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let link = Arc::new(Link {
        id: screen.next_link.fetch_add(1, Ordering::Relaxed),
        tx,
        pending: Mutex::default(),
        next_id: AtomicI64::new(1),
    });
    {
        let mut slot = screen.link.locked();
        *slot = Some(link.clone());
        screen.reset_helper_state();
        screen.helper_generation.send_replace(link.id);
    }
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
    loop {
        let packet = tokio::select! {
            packet = lines.next_line() => { let Ok(Some(packet)) = packet else { break }; packet }
            changed = generation.changed() => {
                if changed.is_err() || *generation.borrow() != link.id { break; }
                continue;
            }
        };
        let Ok(msg) = serde_json::from_str::<Value>(&packet) else {
            tracing::debug!(packet, "ignoring non-JSON from the screen helper");
            continue;
        };
        // Keep replacement atomic with every notification's state changes.
        let current = screen.link.locked();
        if current.as_ref().is_none_or(|current| current.id != link.id) {
            break;
        }
        let announce = handle_message(&screen, &link, &msg);
        drop(current);
        if announce {
            screen.emit();
        }
    }
    writer.abort();
    link.pending.locked().clear();
    let current = {
        let mut slot = screen.link.locked();
        let current = slot.as_ref().is_some_and(|l| l.id == link.id);
        if current {
            *slot = None;
            screen.reset_helper_state();
        }
        current
    };
    if current {
        screen.emit();
    }
    tracing::info!("screen helper disconnected");
}

/// Applies one message from the helper; true when clients should hear about the change.
#[cfg(unix)]
fn handle_message(screen: &Screen, link: &Link, msg: &Value) -> bool {
    match (msg["method"].as_str(), msg["id"].as_i64()) {
        (None, Some(id)) => {
            if let Some(waiter) = link.pending.locked().remove(&id) {
                let res = match msg.get("error") {
                    Some(e) => Err(e["message"].as_str().map_or_else(|| e.to_string(), str::to_owned)),
                    None => Ok(msg["result"].clone()),
                };
                let _ = waiter.send(res);
            }
            false
        }
        (Some("status"), _) => match serde_json::from_value::<HelperStatus>(msg["params"].clone()) {
            Ok(st) => {
                *screen.status.locked() = st;
                true
            }
            Err(error) => {
                tracing::warn!(%error, "bad status from the screen helper");
                false
            }
        },
        (Some("candidate"), _) => {
            if let Some(session) = msg["params"]["session"].as_str() {
                screen.helper_candidate(session, msg["params"]["candidate"].clone());
            }
            false
        }
        (Some("session"), _) => {
            let Some(session) = msg["params"]["session"].as_str() else { return false };
            match msg["params"]["state"].as_str() {
                Some("closed") => screen.remove_session(session),
                Some("connected") => {
                    screen.session_connected(session);
                    false
                }
                _ => false,
            }
        }
        (Some(method), _) => {
            tracing::debug!(method, "unknown screen helper notification");
            false
        }
        _ => false,
    }
}

#[cfg(unix)]
impl Screen {
    /// Nothing from a previous helper carries over to the next one.
    fn reset_helper_state(&self) {
        self.sessions.locked().clear();
        *self.status.locked() = HelperStatus::default();
        self.control.locked().user = false;
    }
}

#[cfg(all(test, unix))]
mod tests;
