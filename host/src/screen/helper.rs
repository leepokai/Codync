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
#[cfg(unix)]
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
        match tokio::time::timeout(HELPER_TIMEOUT, rx).await {
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
        screen.sessions.locked().clear();
        *screen.status.locked() = HelperStatus::default();
        screen.control.locked().user = false;
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
        let mut announce = false;
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
                    announce = true;
                }
                Err(error) => tracing::warn!(%error, "bad status from the screen helper"),
            },
            (Some("candidate"), _) => {
                if let Some(session) = msg["params"]["session"].as_str() {
                    screen.helper_candidate(session, msg["params"]["candidate"].clone());
                }
            }
            (Some("session"), _) => {
                if msg["params"]["state"] == "closed"
                    && let Some(s) = msg["params"]["session"].as_str()
                {
                    announce = screen.remove_session(s);
                }
            }
            (Some(method), _) => tracing::debug!(method, "unknown screen helper notification"),
            _ => {}
        }
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
            *screen.status.locked() = HelperStatus::default();
            screen.sessions.locked().clear();
            screen.control.locked().user = false;
        }
        current
    };
    if current {
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
        let prepared = screen.prepare("phone", config, false).unwrap();
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

    #[tokio::test]
    async fn replacing_a_helper_ends_its_socket_and_private_signaling() {
        use futures::StreamExt as _;
        use tokio::io::AsyncReadExt as _;
        let (events, _) = broadcast::channel(8);
        let screen = Arc::new(Screen::new(Some(true), events));
        let (first, mut first_peer) = UnixStream::pair().unwrap();
        tokio::spawn(run_link(screen.clone(), first));
        let status = json!({"jsonrpc": "2.0", "method": "status", "params": {"capture": true, "trickle": true}});
        let sent = first_peer.write_all(format!("{status}\n").as_bytes()).await;
        sent.unwrap();
        for _ in 0..50 {
            if screen.status.locked().capture {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let prepared = screen.prepare("phone", IceConfig::default(), true).unwrap();
        let session = prepared["session"].as_str().unwrap();
        let mut candidates = screen.candidates(session, "phone").unwrap().boxed();
        let ready = candidates.next().await;
        assert_eq!(ready.unwrap()["type"], "ready");
        let old_id = screen.link().unwrap().id;
        let (second, mut second_peer) = UnixStream::pair().unwrap();
        tokio::spawn(run_link(screen.clone(), second));
        let sent = second_peer.write_all(format!("{status}\n").as_bytes()).await;
        sent.unwrap();
        let mut buffer = [0; 1];
        let closed = tokio::time::timeout(Duration::from_secs(1), first_peer.read(&mut buffer)).await;
        assert_eq!(closed.unwrap().unwrap(), 0, "the old helper must stop media when its socket ends");
        let ended = tokio::time::timeout(Duration::from_secs(1), candidates.next()).await;
        assert!(ended.unwrap().is_none());
        assert!(!screen.owns_session(session, "phone"));
        assert_ne!(screen.link().unwrap().id, old_id);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn helper_replacement_cannot_overtake_an_in_progress_status_update() {
        use std::io::Write as _;
        let (events, _) = broadcast::channel(8);
        let screen = Arc::new(Screen::new(Some(true), events));
        let (first, mut first_peer) = UnixStream::pair().unwrap();
        tokio::spawn(run_link(screen.clone(), first));
        let status = json!({"jsonrpc": "2.0", "method": "status", "params": {"capture": true, "trickle": true}});
        let sent = first_peer.write_all(format!("{status}\n").as_bytes()).await;
        sent.unwrap();
        for _ in 0..50 {
            if screen.status.locked().capture {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let old_id = screen.link().unwrap().id;
        let (second, mut second_peer) = UnixStream::pair().unwrap();
        let sent = second_peer.write_all(format!("{status}\n").as_bytes()).await;
        sent.unwrap();
        let mut first_peer = first_peer.into_std().unwrap();
        first_peer.set_nonblocking(false).unwrap();
        let (held, holding) = std::sync::mpsc::channel();
        let (checked, checking) = std::sync::mpsc::channel();
        let (release, releasing) = std::sync::mpsc::channel();
        let blocker = std::thread::spawn({
            let screen = screen.clone();
            move || {
                let _status = screen.status.locked();
                held.send(()).unwrap();
                let mut protected = false;
                for _ in 0..100 {
                    if screen.link.try_lock().is_err() {
                        protected = true;
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                checked.send(protected).unwrap();
                // A timeout also releases the lock if an assertion fails.
                let _ = releasing.recv_timeout(Duration::from_secs(3));
            }
        });
        holding.recv_timeout(Duration::from_secs(1)).unwrap();
        let stale = json!({"jsonrpc": "2.0", "method": "status", "params": {"capture": false, "trickle": false}});
        first_peer.write_all(format!("{stale}\n").as_bytes()).unwrap();
        let protected = checking.recv_timeout(Duration::from_secs(1)).unwrap();
        tokio::spawn(run_link(screen.clone(), second));
        release.send(()).unwrap();
        blocker.join().unwrap();
        assert!(protected, "the previous helper must hold its generation until the update completes");
        for _ in 0..50 {
            if screen.link().unwrap().id != old_id && screen.status.locked().capture {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_ne!(screen.link().unwrap().id, old_id);
        assert!(screen.status.locked().trickle, "the replacement's capability must win");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn preparing_a_viewer_uses_one_helper_generation_and_its_capabilities() {
        let (events, _) = broadcast::channel(8);
        let screen = Arc::new(Screen::new(Some(true), events));
        let (first, mut first_peer) = UnixStream::pair().unwrap();
        tokio::spawn(run_link(screen.clone(), first));
        let status = json!({"jsonrpc": "2.0", "method": "status", "params": {"capture": true, "trickle": true}});
        let sent = first_peer.write_all(format!("{status}\n").as_bytes()).await;
        sent.unwrap();
        for _ in 0..50 {
            if screen.status.locked().capture {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let old_id = screen.link().unwrap().id;
        let (second, mut second_peer) = UnixStream::pair().unwrap();
        let replacement = json!({"jsonrpc": "2.0", "method": "status", "params": {"capture": true, "trickle": false}});
        let sent = second_peer.write_all(format!("{replacement}\n").as_bytes()).await;
        sent.unwrap();
        let (held, holding) = std::sync::mpsc::channel();
        let (checked, checking) = std::sync::mpsc::channel();
        let (release, releasing) = std::sync::mpsc::channel();
        let blocker = std::thread::spawn({
            let screen = screen.clone();
            move || {
                let _status = screen.status.locked();
                held.send(()).unwrap();
                let mut protected = false;
                for _ in 0..100 {
                    if screen.link.try_lock().is_err() {
                        protected = true;
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                checked.send(protected).unwrap();
                let _ = releasing.recv_timeout(Duration::from_secs(3));
            }
        });
        holding.recv_timeout(Duration::from_secs(1)).unwrap();
        let preparing = std::thread::spawn({
            let screen = screen.clone();
            move || screen.prepare("phone", IceConfig::default(), true)
        });
        let protected = checking.recv_timeout(Duration::from_secs(1)).unwrap();
        tokio::spawn(run_link(screen.clone(), second));
        release.send(()).unwrap();
        blocker.join().unwrap();
        let prepared = preparing.join().unwrap().unwrap();
        assert!(protected, "replacement must wait until preparation stores the helper generation");
        assert_eq!(prepared["trickle"], true);
        for _ in 0..50 {
            if screen.link().unwrap().id != old_id && screen.status.locked().capture {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!screen.owns_session(prepared["session"].as_str().unwrap(), "phone"));
        let fresh = screen.prepare("phone", IceConfig::default(), true).unwrap();
        assert_eq!(fresh["trickle"], false);
    }
}
