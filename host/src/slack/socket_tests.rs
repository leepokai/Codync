//! The Slack channel against a fake Slack: signed events over HTTP and envelopes over Socket Mode, with retries and
//! duplicates. Nothing here proves Slack's real behavior; `docs/guides/slack-manual-checklist.md` is for that.

use super::*;
use crate::egress::Policy;
use crate::remote::identity::Identity;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use futures::{SinkExt as _, StreamExt as _};
use hmac::{KeyInit as _, Mac as _};
use serde_json::json;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio_tungstenite::tungstenite::Message;

const SIGNING_SECRET: &str = "0123456789abcdef0123456789abcdef";

struct Fixture {
    hub: Arc<Hub>,
    dir: std::path::PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A host with one bot bound to channel `C1` for person `U1`, its Slack settings in the vault.
fn fixture(app_token: &str, signing_secret: &str) -> Fixture {
    let dir = std::env::temp_dir().join(format!("codync-slack-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = Store::open(std::path::Path::new(":memory:")).expect("store");
    let hub = Hub::new(store, "slack-test".into(), Identity::load_or_create(&dir).expect("identity"), "t".into(), 0);
    *hub.store.secret_key.locked() = Some(zeroize::Zeroizing::new(vec![7; 32]));
    let bot =
        serde_json::from_value(json!({"id": "b1", "name": "Bot", "backend": "fixture", "command": "x", "cwd": dir}))
            .expect("bot");
    hub.store.save_bot(&bot).expect("saved");
    let config = Config {
        workspace_id: "T1".into(),
        bot_user_id: "UBOT".into(),
        bot_token: "xoxb-test".into(),
        signing_secret: signing_secret.into(),
        app_token: app_token.into(),
        bindings: vec![Binding { channel_id: "C1".into(), bot_id: "b1".into(), allowed_users: vec!["U1".into()] }],
        generation: "g1".into(),
    };
    crate::market::vault::write(&hub.store, SLOT, &serde_json::to_string(&config).expect("json")).expect("vault");
    Fixture { hub, dir }
}

/// An `event_callback` as Slack sends it: a mention of the bot.
fn mention(event_id: &str, user: &str, text: &str, ts: &str) -> Value {
    json!({"type": "event_callback", "team_id": "T1", "event_id": event_id,
        "event": {"type": "app_mention", "user": user, "channel": "C1", "text": text, "ts": ts}})
}

fn envelope(id: &str, payload: &Value) -> Value {
    json!({"type": "events_api", "envelope_id": id, "payload": payload})
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..1000 {
        if done() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

// MARK: a fake Slack for Socket Mode

struct FakeSlack {
    api: String,
    acks: Arc<Mutex<Vec<String>>>,
    connections: Arc<AtomicUsize>,
}

/// Serves `apps.connections.open` and a socket. The nth connection gets `scripts[n]` after `hello`, waits for every
/// envelope in it to be acknowledged, and is then told to `disconnect` if another script follows.
async fn fake_slack(scripts: Vec<Vec<Value>>) -> FakeSlack {
    let sockets = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("socket port");
    let ws_url = format!("ws://{}", sockets.local_addr().expect("addr"));
    let acks = Arc::new(Mutex::new(Vec::new()));
    let connections = Arc::new(AtomicUsize::new(0));
    tokio::spawn({
        let (acks, connections) = (acks.clone(), connections.clone());
        async move {
            while let Ok((stream, _)) = sockets.accept().await {
                let n = connections.fetch_add(1, Ordering::SeqCst);
                let script = scripts.get(n).cloned().unwrap_or_default();
                let more = n + 1 < scripts.len();
                let acks = acks.clone();
                tokio::spawn(async move {
                    let Ok(ws) = tokio_tungstenite::accept_async(stream).await else { return };
                    let (mut tx, mut rx) = ws.split();
                    let _ = tx.send(Message::text(json!({"type": "hello"}).to_string())).await;
                    for frame in &script {
                        let _ = tx.send(Message::text(frame.to_string())).await;
                    }
                    let mut acked = 0;
                    while acked < script.len() {
                        let Some(Ok(Message::Text(text))) = rx.next().await else { return };
                        let ack: Value = serde_json::from_str(&text).expect("an ack is JSON");
                        acks.locked().push(ack["envelope_id"].as_str().expect("envelope id").to_owned());
                        acked += 1;
                    }
                    if more {
                        let _ = tx.send(Message::text(json!({"type": "disconnect"}).to_string())).await;
                    }
                    std::future::pending::<()>().await;
                });
            }
        }
    });
    let http = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("api port");
    let api = format!("http://{}/api", http.local_addr().expect("addr"));
    let app = axum::Router::new().route(
        "/api/apps.connections.open",
        axum::routing::post(move |headers: HeaderMap| {
            let url = ws_url.clone();
            async move {
                let token = headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or_default();
                if token == "Bearer xapp-test" {
                    axum::Json(json!({"ok": true, "url": url}))
                } else {
                    axum::Json(json!({"ok": false, "error": "invalid_auth"}))
                }
            }
        }),
    );
    tokio::spawn(async move { axum::serve(http, app).await });
    FakeSlack { api, acks, connections }
}

fn endpoint(api: &str) -> socket::Endpoint {
    socket::Endpoint {
        api: api.into(),
        policy: Policy::UserChosen,
        first_wait: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(40),
    }
}

#[tokio::test]
async fn socket_mode_queues_each_event_once_acknowledges_every_envelope_and_reconnects() {
    let f = fixture("xapp-test", "");
    let first = vec![
        envelope("e1", &mention("Ev1", "U1", "<@UBOT> hello", "100.1")),
        // Slack redelivers an event it did not see acknowledged: new envelope, same event id.
        envelope("e2", &{
            let mut again = mention("Ev1", "U1", "<@UBOT> hello", "100.1");
            again["retry_attempt"] = 1.into();
            again
        }),
        // Not an allowed person: acknowledged, never queued.
        envelope("e3", &mention("Ev2", "U2", "<@UBOT> let me in", "100.2")),
    ];
    let second = vec![envelope("e4", &mention("Ev3", "U1", "<@UBOT> again", "100.3"))];
    let fake = fake_slack(vec![first, second]).await;
    let task = tokio::spawn(socket::run(f.hub.clone(), endpoint(&fake.api)));

    until("every envelope to be acknowledged", || fake.acks.locked().len() == 4).await;
    assert_eq!(*fake.acks.locked(), ["e1", "e2", "e3", "e4"]);
    assert_eq!(fake.connections.load(Ordering::SeqCst), 2, "Slack's disconnect reopened the socket");
    let queued = f.hub.store.pending_slack().expect("pending");
    let texts: Vec<_> = queued.iter().map(|q| (q.event.as_str(), q.text.as_str(), q.user.as_str())).collect();
    assert_eq!(texts.len(), 2, "the retry and the stranger queued nothing: {texts:?}");
    assert!(texts.contains(&("Ev1", "hello", "U1")), "the mention is not part of what was said: {texts:?}");
    assert!(texts.contains(&("Ev3", "again", "U1")));
    task.abort();
}

#[tokio::test]
async fn a_refused_app_token_connects_nothing_and_keeps_trying() {
    let f = fixture("xapp-wrong", "");
    let fake = fake_slack(vec![vec![]]).await;
    let task = tokio::spawn(socket::run(f.hub.clone(), endpoint(&fake.api)));
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert_eq!(fake.connections.load(Ordering::SeqCst), 0);
    task.abort();
}

#[test]
fn frames_ask_for_what_they_mean() {
    let f = fixture("xapp-test", "");
    let h = |text: &str| socket::handle(&f.hub, "g1", text);
    assert_eq!(h(r#"{"type":"hello"}"#).expect("hello"), socket::Frame::Hello);
    assert_eq!(h(r#"{"type":"disconnect"}"#).expect("disconnect"), socket::Frame::Reconnect);
    assert_eq!(h(r#"{"type":"slash_commands","envelope_id":"x"}"#).expect("ignored"), socket::Frame::Ignore);
    let event = envelope("e9", &mention("Ev9", "U1", "<@UBOT> hi", "200.1")).to_string();
    assert_eq!(h(&event).expect("event"), socket::Frame::Ack("e9".into()));
    // The configuration changed since the socket was opened: the socket is opened again.
    assert_eq!(socket::handle(&f.hub, "old-generation", &event).expect("stale"), socket::Frame::Reconnect);
    assert!(h(r#"{"type":"events_api","payload":{}}"#).is_err(), "no envelope id, no acknowledgement");
    assert!(h("not json").is_err());
}

#[test]
fn only_slacks_own_sockets_are_followed() {
    let slack = socket::Endpoint::slack();
    assert!(slack.socket_ok("wss://wss-primary.slack.com/link/?ticket=1"));
    for url in [
        "ws://wss-primary.slack.com/",
        "wss://evil.example/",
        "wss://slack.com.evil.example/",
        "wss://x.slack.com.evil.example/",
    ] {
        assert!(!slack.socket_ok(url), "{url}");
    }
    assert!(endpoint("http://127.0.0.1/api").socket_ok("ws://127.0.0.1:9/"));
}

// MARK: signed events over HTTP

fn sign(secret: &str, timestamp: i64, body: &[u8]) -> HeaderMap {
    let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(secret.as_bytes()).expect("key");
    mac.update(format!("v0:{timestamp}:").as_bytes());
    mac.update(body);
    let mut headers = HeaderMap::new();
    headers.insert("x-slack-request-timestamp", timestamp.to_string().parse().expect("header"));
    let signature = format!("v0={}", crate::chat::files::digest_hex(&mac.finalize().into_bytes()));
    headers.insert("x-slack-signature", signature.parse().expect("header"));
    headers
}

async fn post(f: &Fixture, headers: HeaderMap, body: &Value) -> StatusCode {
    ingress::receive(State(f.hub.clone()), headers, Bytes::from(body.to_string())).await.status()
}

#[tokio::test]
async fn signed_events_are_queued_once_and_bad_ones_are_refused() {
    let f = fixture("", SIGNING_SECRET);
    let now = crate::store::now_ms() / 1000;
    let event = mention("Ev1", "U1", "<@UBOT> deploy", "300.1");
    let body = event.to_string();

    assert_eq!(post(&f, sign(SIGNING_SECRET, now, body.as_bytes()), &event).await, StatusCode::OK);
    // Slack retries after a slow answer: same event, same signature scheme, accepted and not queued twice.
    assert_eq!(post(&f, sign(SIGNING_SECRET, now, body.as_bytes()), &event).await, StatusCode::OK);
    assert_eq!(f.hub.store.pending_slack().expect("pending").len(), 1);

    let other = mention("Ev2", "U1", "<@UBOT> more", "300.2");
    let stale = sign(SIGNING_SECRET, now - 600, other.to_string().as_bytes());
    assert_eq!(post(&f, stale, &other).await, StatusCode::UNAUTHORIZED, "an old timestamp is a replay");
    let wrong = sign("fedcba9876543210fedcba9876543210", now, other.to_string().as_bytes());
    assert_eq!(post(&f, wrong, &other).await, StatusCode::UNAUTHORIZED);
    assert_eq!(f.hub.store.pending_slack().expect("pending").len(), 1);
}

#[tokio::test]
async fn with_socket_mode_only_the_http_endpoint_is_closed() {
    let f = fixture("xapp-test", "");
    let now = crate::store::now_ms() / 1000;
    let event = mention("Ev1", "U1", "<@UBOT> hi", "400.1");
    // Signed with the empty key an unset secret would be: still refused.
    let forged = sign("", now, event.to_string().as_bytes());
    assert_eq!(post(&f, forged, &event).await, StatusCode::UNAUTHORIZED);
    assert!(f.hub.store.pending_slack().expect("pending").is_empty());
}
