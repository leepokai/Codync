#![allow(clippy::unwrap_used)]

use super::*;
use crate::screen::tests::retina;
use crate::screen::viewer::{IceConfig, IceServer};
use std::sync::mpsc as sync_mpsc;
use tokio::sync::broadcast;

fn screen() -> Arc<Screen> {
    let (events, _) = broadcast::channel(8);
    Arc::new(Screen::new(Some(true), events))
}

fn status_line(params: &Value) -> String {
    format!("{}\n", json!({"jsonrpc": "2.0", "method": "status", "params": params}))
}

/// Connects a fake helper that reports `status`, once the host has applied it.
async fn connect_helper(screen: &Arc<Screen>, status: &Value) -> UnixStream {
    let (link, mut peer) = UnixStream::pair().unwrap();
    tokio::spawn(run_link(screen.clone(), link));
    let sent = peer.write_all(status_line(status).as_bytes()).await;
    sent.unwrap();
    for _ in 0..50 {
        if screen.status.locked().capture {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    peer
}

/// Another thread holding the status lock, as a status update in progress does.
struct StatusLock {
    checked: sync_mpsc::Receiver<bool>,
    release: sync_mpsc::Sender<()>,
    thread: std::thread::JoinHandle<()>,
}

/// Returns once the lock is held; the thread then watches whether anyone holds the link lock.
fn hold_status_lock(screen: &Arc<Screen>) -> StatusLock {
    let (held, holding) = sync_mpsc::channel();
    let (checked, checking) = sync_mpsc::channel();
    let (release, releasing) = sync_mpsc::channel();
    let thread = std::thread::spawn({
        let screen = screen.clone();
        move || {
            let _status = screen.status.locked();
            held.send(()).unwrap();
            let protected = (0..100).any(|_| {
                let taken = screen.link.try_lock().is_err();
                if !taken {
                    std::thread::sleep(Duration::from_millis(5));
                }
                taken
            });
            checked.send(protected).unwrap();
            // A timeout also releases the lock if an assertion fails.
            let _ = releasing.recv_timeout(Duration::from_secs(3));
        }
    });
    holding.recv_timeout(Duration::from_secs(1)).unwrap();
    StatusLock { checked: checking, release, thread }
}

impl StatusLock {
    /// Whether the link lock was held while the status lock was.
    fn link_was_held(&self) -> bool {
        self.checked.recv_timeout(Duration::from_secs(1)).unwrap()
    }

    fn release(self) {
        self.release.send(()).unwrap();
        self.thread.join().unwrap();
    }
}

async fn wait_for_replacement(screen: &Screen, old_id: u64) {
    for _ in 0..50 {
        if screen.link().unwrap().id != old_id && screen.status.locked().capture {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn helper_roundtrip_over_the_socket() {
    let screen = screen();
    let peer = connect_helper(&screen, &json!({"platform": "test", "capture": true, "displays": [retina()]})).await;
    let (observed_tx, mut observed_rx) = mpsc::unbounded_channel();
    // Fake helper records the host's ICE configuration.
    tokio::spawn(async move {
        let (rd, mut wr) = peer.into_split();
        let mut lines = BufReader::new(rd).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            let m: Value = serde_json::from_str(&l).unwrap();
            observed_tx.send(m.clone()).unwrap();
            if m["method"] == "answer" {
                let connected = json!({"jsonrpc": "2.0", "method": "session", "params": {"session": m["params"]["session"], "state": "connected"}});
                let written = wr.write_all(format!("{connected}\n").as_bytes()).await;
                written.unwrap();
            }
            let res = json!({"jsonrpc": "2.0", "id": m["id"], "result": {"sdp": format!("answer to {}", m["params"]["sdp"].as_str().unwrap_or_default())}});
            wr.write_all(format!("{res}\n").as_bytes()).await.unwrap();
        }
    });
    let local = screen.prepare("local", IceConfig::default(), false).unwrap();
    let res = screen.offer("local", "offer", local["session"].as_str().unwrap(), None).await.unwrap();
    assert_eq!(res["sdp"], "answer to offer");
    assert_eq!(screen.state()["viewers"], 1);
    assert!(matches!(
        screen.sessions.locked()[local["session"].as_str().unwrap()].state,
        super::super::ViewerState::Connected
    ));
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
    assert!(screen.offer("other", "offer", id, None).await.is_err());
    screen.offer("phone", "offer", id, None).await.unwrap();
    let remote = observed_rx.recv().await.unwrap();
    assert_eq!(remote["params"]["iceServers"], prepared["iceServers"]);
    assert_eq!(remote["params"]["maxBitrateBps"], 4_000_000);
    assert_eq!(remote["params"]["maxFramerate"], 30);
    screen.sessions.locked().get_mut(id).unwrap().ice.expires_at = 0;
    assert!(screen.offer("phone", "offer", id, None).await.is_err());
}

#[tokio::test]
async fn replacing_a_helper_ends_its_socket_and_private_signaling() {
    use futures::StreamExt as _;
    use tokio::io::AsyncReadExt as _;
    let screen = screen();
    let status = json!({"capture": true, "trickle": true});
    let mut first_peer = connect_helper(&screen, &status).await;
    let prepared = screen.prepare("phone", IceConfig::default(), true).unwrap();
    let session = prepared["session"].as_str().unwrap();
    let mut candidates = screen.candidates(session, "phone").unwrap().boxed();
    let ready = candidates.next().await;
    assert_eq!(ready.unwrap()["type"], "ready");
    let old_id = screen.link().unwrap().id;
    let _second_peer = connect_helper(&screen, &status).await;
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
    let screen = screen();
    let first_peer = connect_helper(&screen, &json!({"capture": true, "trickle": true})).await;
    let old_id = screen.link().unwrap().id;
    let (second, mut second_peer) = UnixStream::pair().unwrap();
    let sent = second_peer.write_all(status_line(&json!({"capture": true, "trickle": true})).as_bytes()).await;
    sent.unwrap();
    let mut first_peer = first_peer.into_std().unwrap();
    first_peer.set_nonblocking(false).unwrap();
    let status_lock = hold_status_lock(&screen);
    first_peer.write_all(status_line(&json!({"capture": false, "trickle": false})).as_bytes()).unwrap();
    let protected = status_lock.link_was_held();
    tokio::spawn(run_link(screen.clone(), second));
    status_lock.release();
    assert!(protected, "the previous helper must hold its generation until the update completes");
    wait_for_replacement(&screen, old_id).await;
    assert_ne!(screen.link().unwrap().id, old_id);
    assert!(screen.status.locked().trickle, "the replacement's capability must win");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn preparing_a_viewer_uses_one_helper_generation_and_its_capabilities() {
    let screen = screen();
    let _first_peer = connect_helper(&screen, &json!({"capture": true, "trickle": true})).await;
    let old_id = screen.link().unwrap().id;
    let (second, mut second_peer) = UnixStream::pair().unwrap();
    let sent = second_peer.write_all(status_line(&json!({"capture": true, "trickle": false})).as_bytes()).await;
    sent.unwrap();
    let status_lock = hold_status_lock(&screen);
    let preparing = std::thread::spawn({
        let screen = screen.clone();
        move || screen.prepare("phone", IceConfig::default(), true)
    });
    let protected = status_lock.link_was_held();
    tokio::spawn(run_link(screen.clone(), second));
    status_lock.release();
    let prepared = preparing.join().unwrap().unwrap();
    assert!(protected, "replacement must wait until preparation stores the helper generation");
    assert_eq!(prepared["trickle"], true);
    wait_for_replacement(&screen, old_id).await;
    assert!(!screen.owns_session(prepared["session"].as_str().unwrap(), "phone"));
    let fresh = screen.prepare("phone", IceConfig::default(), true).unwrap();
    assert_eq!(fresh["trickle"], false);
}
