#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use crate::remote::identity::Identity;
use crate::store::{BotConfig, Device, DeviceAccount, DeviceSource, Scope, Store, now_ms};
use axum::extract::ws::{Message as AxMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Request, State};
use axum::routing::{any, get, post};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use ed25519_dalek::SigningKey;
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::StaticSecret;

fn temp_hub() -> Arc<Hub> {
    let dir = std::env::temp_dir().join(format!("codync-relay-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    open_hub(&dir)
}

/// The host over `dir`'s database and identity, as `serve` opens it after a restart.
fn open_hub(dir: &std::path::Path) -> Arc<Hub> {
    let store = Store::open(&dir.join("t.db")).unwrap();
    Hub::new(store, "host".into(), Identity::load_or_create(dir).unwrap(), "token".into(), 0)
}

/// What the fake cloud saw and does.
#[derive(Default)]
struct Fake {
    /// ACL version the "DO" already stored (a previous database).
    stored_ver: std::sync::Mutex<i64>,
    /// Messages from the host, in order.
    from_host: std::sync::Mutex<Vec<Value>>,
    /// Messages to push to the host after `ready`.
    script: std::sync::Mutex<Vec<Value>>,
    state: std::sync::Mutex<Value>,
    /// State pulls to fail (500) before answering.
    state_failures: std::sync::Mutex<u32>,
    signatures_ok: std::sync::Mutex<Vec<bool>>,
    /// Each request's path and its `Codync-Relay-Token`, in order.
    relay_tokens: std::sync::Mutex<Vec<(String, Option<String>)>>,
}

fn note_token(f: &Fake, req: &Request) {
    let token = req.headers().get("codync-relay-token").and_then(|v| v.to_str().ok()).map(str::to_owned);
    f.relay_tokens.lock().unwrap().push((req.uri().path().to_owned(), token));
}

fn check_sig(req: &Request, sign_pub: &[u8; 32], authority: &str) -> bool {
    let h = req.headers().get("codync-sig").and_then(|v| v.to_str().ok()).unwrap_or_default();
    let field = |k: &str| h.split(',').find_map(|p| p.strip_prefix(&format!("{k}="))).unwrap_or_default();
    let ts: i64 = field("ts").parse().unwrap_or(0);
    let path = req.uri().path_and_query().unwrap().as_str();
    let input = crypto::request_sig_input(req.method().as_str(), authority, path, ts, field("nonce"), b"");
    field("kid") == b64(sign_pub)
        && crypto::verify(sign_pub, input.as_bytes(), &crypto::unb64_n(field("sig")).unwrap()).is_ok()
}

async fn fake_cloud(fake: Arc<Fake>, hub: Arc<Hub>) -> String {
    use axum::response::IntoResponse as _;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let authority = listener.local_addr().unwrap().to_string();
    let sign_pub = hub.identity.sign_pub();
    let cid = hub.identity.computer_id();
    let a1 = authority.clone();
    let app = axum::Router::new()
        .route(
            "/v1/host/register",
            post(move |State(f): State<Arc<Fake>>, req: Request| async move {
                note_token(&f, &req);
                let ok = req.headers().contains_key("codync-sig");
                f.signatures_ok.lock().unwrap().push(ok);
                axum::Json(json!({"computerId": cid, "owned": false}))
            }),
        )
        .route(
            "/v1/host/state",
            get(move |State(f): State<Arc<Fake>>, req: Request| async move {
                note_token(&f, &req);
                f.signatures_ok.lock().unwrap().push(check_sig(&req, &sign_pub, &a1));
                let mut failures = f.state_failures.lock().unwrap();
                if *failures > 0 {
                    *failures -= 1;
                    return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, axum::Json(json!({}))).into_response();
                }
                axum::Json(f.state.lock().unwrap().clone()).into_response()
            }),
        )
        .route(
            "/v1/relay/host",
            any(move |State(f): State<Arc<Fake>>, ws: WebSocketUpgrade, req: Request| async move {
                note_token(&f, &req);
                ws.on_upgrade(move |socket| fake_do(f, socket))
            }),
        )
        .with_state(fake);
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{authority}")
}

async fn fake_do(f: Arc<Fake>, mut ws: WebSocket) {
    while let Some(Ok(AxMessage::Text(t))) = ws.recv().await {
        let v: Value = serde_json::from_str(t.as_str()).unwrap();
        f.from_host.lock().unwrap().push(v.clone());
        let reply = match v["t"].as_str().unwrap() {
            "acl" => {
                let d = crypto::unb64(v["d"].as_str().unwrap()).unwrap();
                let acl: Value = serde_json::from_slice(&d).unwrap();
                let ver = acl["ver"].as_i64().unwrap();
                let mut stored = f.stored_ver.lock().unwrap();
                if ver <= *stored {
                    json!({"t": "error", "code": "staleVersion", "ver": *stored})
                } else {
                    *stored = ver;
                    json!({"t": "acl.ok", "ver": ver})
                }
            }
            "ready" => {
                let script: Vec<Value> = f.script.lock().unwrap().drain(..).collect();
                for m in script {
                    ws.send(AxMessage::Text(m.to_string().into())).await.unwrap();
                }
                continue;
            }
            _ => continue,
        };
        ws.send(AxMessage::Text(reply.to_string().into())).await.unwrap();
    }
}

/// Device side of §6.4, for the test.
fn seal_mailbox(hub: &Hub, device: &SigningKey, nonce: &str, plain: &[u8]) -> String {
    let cid = hub.identity.cid_raw();
    let dk = device.verifying_key().to_bytes();
    let eph = StaticSecret::from(crypto::random::<32>());
    let epk = crypto::x25519_pub(&eph);
    let ss = crypto::x25519(&eph, &hub.identity.box_pub()).unwrap();
    let salt = [b"codync/mbox/v1".as_slice(), &cid, &dk, &epk].concat();
    let mut key = [0u8; 32];
    Hkdf::<Sha256>::new(Some(&salt), &ss).expand(b"codync/mbox-key/v1", &mut key).unwrap();
    let aad = [dk.as_slice(), nonce.as_bytes()].concat();
    let ct = chacha20poly1305::ChaCha20Poly1305::new(&key.into())
        .encrypt(&[0u8; 12].into(), Payload { msg: plain, aad: &aad })
        .unwrap();
    let sig = crypto::sign(device, &[b"codync/mbox/v1".as_slice(), &cid, &epk, &ct].concat());
    b64(&[epk.as_slice(), &sig, &ct].concat())
}

fn test_owner() -> Value {
    json!({"userId": "user_1", "email": "k@example.com"})
}

/// The Codync relay is for a computer in an account.
fn in_an_account(hub: &Hub) {
    let owner = serde_json::from_value(test_owner()).unwrap();
    owner::remember(hub, Some(&owner)).unwrap();
}

async fn wait_for(f: &Fake, pred: impl Fn(&[Value]) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !pred(&f.from_host.lock().unwrap()) {
        assert!(Instant::now() < deadline, "timed out; host sent {:?}", f.from_host.lock().unwrap());
        sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_computer_in_no_account_connects_only_while_a_phone_may_pair() {
    let hub = temp_hub();
    let fake = Arc::new(Fake::default());
    *fake.state.lock().unwrap() = json!({"owner": null, "requests": [], "grants": []});
    let base = fake_cloud(fake.clone(), hub.clone()).await;
    hub.store.kv_set("test_cloud_url", &base).unwrap();
    tokio::spawn(run(hub.clone()));
    while !hub.cloud_synced.load(Ordering::Acquire) {
        sleep(Duration::from_millis(10)).await;
    }
    assert!(fake.from_host.lock().unwrap().is_empty(), "it read its state without opening a socket");
    assert!(!hub.cloud.status().connected);

    hub.pairing.locked().issue();
    hub.cloud.relay_wake.notify_one();
    wait_for(&fake, |m| m.iter().any(|v| v["t"] == "acl")).await;
}

#[tokio::test]
async fn connects_publishes_acl_past_a_stale_version_and_delivers_mail() {
    let hub = temp_hub();
    let fake = Arc::new(Fake::default());
    // The relay remembers version 7 from before this database was deleted.
    *fake.stored_ver.lock().unwrap() = 7;
    let grant_key = SigningKey::from_bytes(&crypto::random());
    let phone = SigningKey::from_bytes(&crypto::random());
    let dk = b64(phone.verifying_key().as_bytes());
    hub.store
        .put_device(&Device {
            key: dk.clone(),
            name: "Phone".into(),
            platform: "ios".into(),
            source: DeviceSource::Local,
            grant_id: None,
            scopes: vec![Scope::Control, Scope::Screen],
            lease_until: None,
            created_at: now_ms(),
            last_seen_at: None,
            account: DeviceAccount::Unrecorded,
        })
        .unwrap();
    let injected = b64(grant_key.verifying_key().as_bytes());
    *fake.state.lock().unwrap() = json!({"owner": test_owner(), "requests": [],
        "grants": [{"grantId": "grt_x", "deviceKey": injected, "deviceName": "x", "platform": "ios", "scopes": ["control"]}]});

    let cwd = std::env::temp_dir().to_string_lossy().into_owned();
    let cfg: BotConfig =
        serde_json::from_value(json!({"id": "", "name": "B", "cwd": cwd, "backend": "custom", "command": "true"}))
            .unwrap();
    let bot = hub.create_bot(cfg).unwrap();
    let bot_id = bot["id"].as_str().unwrap().to_owned();
    let nonce = "3F2504E0-4F89-11D3-9A0C-0305E82C3301";
    let inner = json!({"m": "send", "b": {"botId": bot_id, "text": "hi", "clientNonce": nonce}, "ts": now_ms()});
    let blob = seal_mailbox(&hub, &phone, nonce, inner.to_string().as_bytes());
    let stranger = SigningKey::from_bytes(&crypto::random());
    *fake.script.lock().unwrap() = vec![
        json!({"t": "mbox.item", "seq": 1, "from": dk, "nonce": nonce, "d": blob, "exp": now_ms() + 1000}),
        json!({"t": "mbox.item", "seq": 2, "from": dk, "nonce": nonce, "d": blob, "exp": now_ms() + 1000}),
        json!({"t": "mbox.item", "seq": 3, "from": b64(stranger.verifying_key().as_bytes()), "nonce": nonce, "d": blob, "exp": 0}),
        json!({"t": "open", "link": "L1", "dk": b64(stranger.verifying_key().as_bytes()), "pair": false}),
        json!({"t": "data", "link": "L1", "m": {"t": "hello", "v": 1, "dk": dk}}),
    ];

    let base = fake_cloud(fake.clone(), hub.clone()).await;
    hub.store.kv_set("test_cloud_url", &base).unwrap();
    in_an_account(&hub);
    tokio::spawn(run(hub.clone()));

    wait_for(&fake, |m| m.iter().filter(|v| v["t"] == "mbox.ack").count() == 3).await;
    wait_for(&fake, |m| m.iter().any(|v| v["t"] == "close" && v["link"] == "L1")).await;
    let sent = fake.from_host.lock().unwrap().clone();
    let kinds: Vec<&str> = sent.iter().map(|v| v["t"].as_str().unwrap()).collect();
    assert_eq!(&kinds[..3], ["acl", "acl", "ready"], "stale ACL is re-signed once, then ready");
    let acl: Value = serde_json::from_slice(&crypto::unb64(sent[1]["d"].as_str().unwrap()).unwrap()).unwrap();
    assert_eq!(acl["ver"], 8, "max(local, relay) + 1");
    assert_eq!(acl["devices"], json!([{"dk": dk, "grant": null}]), "a grant injected by the cloud isn't listed");
    crypto::verify(
        &hub.identity.sign_pub(),
        &crypto::unb64(sent[1]["d"].as_str().unwrap()).unwrap(),
        &crypto::unb64_n(sent[1]["sig"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    assert!(fake.signatures_ok.lock().unwrap().iter().all(|ok| *ok));
    assert!(hub.cloud_synced.load(Ordering::Acquire), "the state was applied on this connection");

    let acks: Vec<&Value> = sent.iter().filter(|v| v["t"] == "mbox.ack").collect();
    assert_eq!(acks[0]["ok"], true);
    assert_eq!(acks[1]["ok"], true, "a redelivery is acknowledged");
    assert_eq!((acks[2]["ok"].as_bool(), acks[2]["code"].as_str()), (Some(false), Some("unauthorized")));
    let history = hub.store.history(&bot_id, i64::MAX, 50).unwrap();
    assert_eq!(history.iter().filter(|e| e.data["clientNonce"] == nonce).count(), 1, "delivered exactly once");

    let close = sent.iter().find(|v| v["t"] == "close").unwrap();
    assert_eq!(close["code"], 4002, "hello.dk must match the link's open.dk");
    assert!(hub.cloud.status().connected);
}

#[tokio::test]
async fn registers_webhooks_and_runs_public_deliveries_once() {
    let hub = temp_hub();
    let fake = Arc::new(Fake::default());
    let cwd = std::env::temp_dir().to_string_lossy().into_owned();
    let cfg: BotConfig =
        serde_json::from_value(json!({"id": "", "name": "B", "cwd": cwd, "backend": "custom", "command": "true"}))
            .unwrap();
    let bot_id = hub.create_bot(cfg).unwrap()["id"].as_str().unwrap().to_owned();
    let saved = hub
        .routines
        .save(&hub, &bot_id, &json!({"name": "Hook", "instruction": "Report it", "triggers": [{"type": "webhook"}]}))
        .unwrap();
    let id = saved["routine"]["id"].as_str().unwrap().to_owned();
    let key = hub.routines.hooks()[0].key.clone();
    let body = b64(br#"{"text":"deploy failed"}"#);
    let item = |seq: i64, auth: &str, delivery: &str| {
        json!({"t": "hook.item", "seq": seq, "hook": id, "delivery": delivery,
            "headers": {"Authorization": format!("Bearer {auth}"), "X-Delivery-Id": delivery}, "body": body, "at": now_ms()})
    };
    *fake.script.lock().unwrap() =
        vec![item(1, &key, "d1"), item(2, &key, "d1"), item(3, "wrong", "d2"), item(4, &key, "d3")];

    *fake.state.lock().unwrap() = json!({"owner": test_owner(), "requests": [], "grants": []});
    let base = fake_cloud(fake.clone(), hub.clone()).await;
    hub.store.kv_set("test_cloud_url", &base).unwrap();
    in_an_account(&hub);
    tokio::spawn(run(hub.clone()));

    wait_for(&fake, |m| m.iter().filter(|v| v["t"] == "hook.ack").count() == 4).await;
    let sent = fake.from_host.lock().unwrap().clone();
    let hooks = sent.iter().find(|v| v["t"] == "hooks").unwrap();
    assert_eq!(hooks["hooks"], json!([{"id": id, "key": key, "enabled": true}]), "registered right after ready");
    let acks: Vec<(Option<bool>, Option<bool>, Option<&str>)> = sent
        .iter()
        .filter(|v| v["t"] == "hook.ack")
        .map(|v| (v["ok"].as_bool(), v["retry"].as_bool(), v["code"].as_str()))
        .collect();
    assert_eq!(
        acks,
        [
            (Some(true), None, None),
            (Some(true), None, None),
            (Some(false), Some(false), Some("unauthorized")),
            (Some(false), Some(true), Some("busy")),
        ],
        "a redelivery reuses its run; a wrong key is dropped; a busy routine is retried later"
    );
    let runs = hub.routines.list(&bot_id)["runs"].as_array().unwrap().clone();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["event"], json!({"text": "deploy failed"}));
    assert_eq!(runs[0]["deliveryId"], "d1");

    // Pausing changes what the relay should refuse: the set goes out again.
    hub.routines.set_enabled(&hub.store, &bot_id, &id, false).unwrap();
    wait_for(&fake, |m| m.iter().any(|v| v["t"] == "hooks" && v["hooks"][0]["enabled"] == false)).await;
    let creds = hub.routines.credentials(&hub, &bot_id, &id, true).unwrap();
    assert_ne!(creds["key"], key, "rotate replaces the key");
    assert_eq!(creds["url"], format!("{base}/v1/hooks/{}/{id}", hub.identity.computer_id()));
    wait_for(&fake, |m| m.iter().any(|v| v["t"] == "hooks" && v["hooks"][0]["key"] == creds["key"])).await;
}

#[tokio::test]
async fn account_mail_waits_for_a_state_pull_instead_of_failing() {
    let hub = temp_hub();
    let fake = Arc::new(Fake::default());
    let phone = SigningKey::from_bytes(&crypto::random());
    let dk = b64(phone.verifying_key().as_bytes());
    hub.store
        .put_device(&Device {
            key: dk.clone(),
            name: "Phone".into(),
            platform: "ios".into(),
            source: DeviceSource::Account,
            grant_id: Some("grt_a".into()),
            scopes: vec![Scope::Control, Scope::Screen],
            lease_until: Some(now_ms() + 60_000),
            created_at: now_ms(),
            last_seen_at: None,
            account: DeviceAccount::Unrecorded,
        })
        .unwrap();
    *fake.state.lock().unwrap() =
        json!({"owner": test_owner(), "requests": [], "grants": [{"grantId": "grt_a", "deviceKey": dk}]});
    // The pull on connect fails: without the fix the item below would be refused for good.
    *fake.state_failures.lock().unwrap() = 1;
    let cwd = std::env::temp_dir().to_string_lossy().into_owned();
    let cfg: BotConfig =
        serde_json::from_value(json!({"id": "", "name": "B", "cwd": cwd, "backend": "custom", "command": "true"}))
            .unwrap();
    let bot_id = hub.create_bot(cfg).unwrap()["id"].as_str().unwrap().to_owned();
    let nonce = "9A1B2C3D-4F89-11D3-9A0C-0305E82C3301";
    let inner = json!({"m": "send", "b": {"botId": bot_id, "text": "hi", "clientNonce": nonce}, "ts": now_ms()});
    let blob = seal_mailbox(&hub, &phone, nonce, inner.to_string().as_bytes());
    *fake.script.lock().unwrap() =
        vec![json!({"t": "mbox.item", "seq": 1, "from": dk, "nonce": nonce, "d": blob, "exp": now_ms() + 1000})];

    let base = fake_cloud(fake.clone(), hub.clone()).await;
    hub.store.kv_set("test_cloud_url", &base).unwrap();
    in_an_account(&hub);
    tokio::spawn(run(hub.clone()));

    wait_for(&fake, |m| m.iter().any(|v| v["t"] == "mbox.ack")).await;
    let sent = fake.from_host.lock().unwrap().clone();
    let ack = sent.iter().find(|v| v["t"] == "mbox.ack").unwrap();
    assert_eq!(ack["ok"], true, "delivered once the retried pull confirmed the grant: {ack}");
    assert!(hub.cloud_synced.load(Ordering::Acquire));
}

#[tokio::test]
async fn a_private_relay_gets_its_token_only_to_enroll_and_never_changes_account_devices() {
    let hub = temp_hub();
    let fake = Arc::new(Fake::default());
    let phone = SigningKey::from_bytes(&crypto::random());
    let dk = b64(phone.verifying_key().as_bytes());
    // Approved through the Codync cloud before this computer moved to its own relay.
    hub.store
        .put_device(&Device {
            key: dk.clone(),
            name: "Phone".into(),
            platform: "ios".into(),
            source: DeviceSource::Account,
            grant_id: Some("grt_a".into()),
            scopes: vec![Scope::Control, Scope::Screen],
            lease_until: Some(now_ms() + 60_000),
            created_at: now_ms(),
            last_seen_at: None,
            account: DeviceAccount::Unrecorded,
        })
        .unwrap();
    *fake.state.lock().unwrap() = json!({"owner": null, "grants": [], "requests": []});
    let cwd = std::env::temp_dir().to_string_lossy().into_owned();
    let cfg: BotConfig =
        serde_json::from_value(json!({"id": "", "name": "B", "cwd": cwd, "backend": "custom", "command": "true"}))
            .unwrap();
    let bot_id = hub.create_bot(cfg).unwrap()["id"].as_str().unwrap().to_owned();
    let nonce = "5D6E7F80-4F89-11D3-9A0C-0305E82C3301";
    let inner = json!({"m": "send", "b": {"botId": bot_id, "text": "hi", "clientNonce": nonce}, "ts": now_ms()});
    let blob = seal_mailbox(&hub, &phone, nonce, inner.to_string().as_bytes());
    *fake.script.lock().unwrap() =
        vec![json!({"t": "mbox.item", "seq": 1, "from": dk, "nonce": nonce, "d": blob, "exp": now_ms() + 1000})];
    let routine = hub
        .routines
        .save(&hub, &bot_id, &json!({"name": "Hook", "instruction": "Report it", "triggers": [{"type": "webhook"}]}))
        .unwrap();
    let routine_id = routine["routine"]["id"].as_str().unwrap().to_owned();

    let base = fake_cloud(fake.clone(), hub.clone()).await;
    hub.store.kv_set("test_cloud_url", &base).unwrap();
    hub.store.kv_set("test_cloud_token", "s3cret-token-value").unwrap();
    tokio::spawn(run(hub.clone()));

    wait_for(&fake, |m| m.iter().any(|v| v["t"] == "mbox.ack")).await;
    let sent = fake.from_host.lock().unwrap().clone();
    let ack = sent.iter().find(|v| v["t"] == "mbox.ack").unwrap();
    assert_eq!(
        (ack["ok"].as_bool(), ack["code"].as_str()),
        (Some(false), Some("unauthorized")),
        "refused, not held: {ack}"
    );
    assert_eq!(
        *fake.relay_tokens.lock().unwrap(),
        [("/v1/host/register".to_owned(), Some("s3cret-token-value".to_owned())), ("/v1/relay/host".to_owned(), None)],
        "only enrolling carries the token, and a private relay's state is never pulled"
    );
    assert!(hub.store.device(&dk).is_some(), "its empty grant list doesn't remove the account device");
    assert!(!hub.cloud_synced.load(Ordering::Acquire), "account devices stay out, as with the cloud off");
    assert_eq!(hub.cloud.status().relay, cloud::RelayKind::Private);
    let creds = hub.routines.credentials(&hub, &bot_id, &routine_id, false).unwrap();
    assert_eq!(creds["url"], Value::Null, "a private relay delivers no public webhook");
}

/// A session whose sink is a real WebSocket, so what it sends can be read.
async fn test_session(hub: Arc<Hub>) -> (Session, mpsc::UnboundedReceiver<Value>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (out_tx, out_rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        while let Some(Ok(Message::Text(t))) = ws.next().await {
            if out_tx.send(serde_json::from_str(t.as_str()).unwrap()).is_err() {
                break;
            }
        }
    });
    let (client, _) = tokio_tungstenite::connect_async(format!("ws://{addr}")).await.unwrap();
    let (sink, _stream) = client.split();
    let address = cloud::Address { url: "http://127.0.0.1".into(), token: None, kind: cloud::RelayKind::Codync };
    let session = Session {
        hub,
        address,
        sink,
        links: HashMap::new(),
        outs: StreamMap::new(),
        pulls: JoinSet::new(),
        assistant: JoinSet::new(),
        held: None,
        ready: true,
        retried: false,
    };
    (session, out_rx)
}

/// The device end of a relay link, doing the channel handshake over the relay's `Session`.
struct RelayPhone {
    key: SigningKey,
    session: Session,
    out: mpsc::UnboundedReceiver<Value>,
    sealer: Option<crypto::Sealer>,
    opener: Option<crypto::Opener>,
    link: String,
}

impl RelayPhone {
    async fn open(hub: &Arc<Hub>, key: SigningKey, link: &str) -> Self {
        let (mut session, out) = test_session(hub.clone()).await;
        let dk = b64(key.verifying_key().as_bytes());
        session.on_message(&json!({"t": "open", "link": link, "dk": dk, "pair": false}).to_string()).await.unwrap();
        Self { key, session, out, sealer: None, opener: None, link: link.to_owned() }
    }

    async fn send(&mut self, v: Value) {
        let frames = self.sealer.as_mut().map(|sealer| sealer.seal(v.to_string().as_bytes()).unwrap());
        for frame in frames.unwrap_or_else(|| vec![v]) {
            let msg = json!({"t": "data", "link": self.link, "m": frame});
            self.session.on_message(&msg.to_string()).await.unwrap();
        }
    }

    async fn wire(&mut self) -> Value {
        timeout(Duration::from_secs(5), async {
            loop {
                tokio::select! {
                    biased;
                    message = self.out.recv() => {
                        let message = message.unwrap();
                        assert_eq!(message["t"], "data", "link remains open: {message}");
                        return message["m"].clone();
                    }
                    Some((link, message)) = self.session.outs.next() => {
                        self.session.link_out(link, message).await.unwrap();
                    }
                }
            }
        })
        .await
        .expect("relay answers in time")
    }

    async fn hello(&mut self, hub: &Hub) {
        let ek = StaticSecret::from(crypto::random::<32>());
        let ek_pub = crypto::x25519_pub(&ek);
        let n = crypto::random::<32>();
        let dk = self.key.verifying_key().to_bytes();
        let cid = hub.identity.cid_raw();
        let sig = crypto::sign(&self.key, &crypto::hs1_input(&cid, &dk, &ek_pub, &n));
        self.send(json!({"t": "hello", "v": 1, "dk": b64(&dk), "ek": b64(&ek_pub),
            "n": b64(&n), "sig": b64(&sig), "pair": false}))
            .await;
        let w = self.wire().await;
        let ek_h: [u8; 32] = crypto::unb64_n(w["ek"].as_str().unwrap()).unwrap();
        let th = crypto::transcript_hash(&cid, &dk, &ek_pub, &n, &ek_h);
        crypto::verify(&hub.identity.sign_pub(), &th, &crypto::unb64_n(w["sig"].as_str().unwrap()).unwrap()).unwrap();
        let keys = crypto::channel_keys(&crypto::x25519(&ek, &ek_h).unwrap(), &th);
        self.sealer = Some(crypto::Sealer::new(keys.d2h));
        self.opener = Some(crypto::Opener::new(keys.h2d, 16 << 20));
    }

    async fn recv(&mut self) -> Value {
        loop {
            let v = self.wire().await;
            if v["t"] == "f"
                && let Some(bytes) = self.opener.as_mut().unwrap().open(&v).unwrap()
            {
                return serde_json::from_slice(&bytes).unwrap();
            }
        }
    }
}

/// A paired phone's key.
fn phone_key(hub: &Hub) -> SigningKey {
    let key = SigningKey::from_bytes(&crypto::random());
    hub.store
        .put_device(&Device {
            key: b64(key.verifying_key().as_bytes()),
            name: "Phone".into(),
            platform: "ios".into(),
            source: DeviceSource::Local,
            grant_id: None,
            scopes: vec![Scope::Control],
            lease_until: None,
            created_at: 0,
            last_seen_at: None,
            account: DeviceAccount::Unrecorded,
        })
        .unwrap();
    key
}

/// `bots` chats with `turns` messages each.
fn history(hub: &Hub, bots: usize, turns: i64) -> Vec<String> {
    (0..bots)
        .map(|i| {
            let id = format!("b{i}");
            hub.store
                .save_bot(&serde_json::from_value(json!({"id": id, "name": id, "createdAt": 0})).unwrap())
                .unwrap();
            for turn in 0..turns {
                hub.store
                    .insert_entry(
                        &crate::store::Lane::main(&id),
                        crate::store::EntryKind::User,
                        turn,
                        &json!({"text": format!("m{turn}")}),
                    )
                    .unwrap();
            }
            id
        })
        .collect()
}

impl RelayPhone {
    /// Subscribes to events from `since` and reads the whole catch-up; returns its `hello`.
    async fn catch_up(&mut self, id: u64, since: i64) -> (Value, Vec<Value>) {
        self.send(json!({"id": id, "sub": "events", "b": {"since": since, "client": "ios"}})).await;
        let hello = self.recv().await;
        let mut events = Vec::new();
        for _ in 0..hello["ev"]["catchUp"].as_u64().unwrap() {
            events.push(self.recv().await["ev"].clone());
        }
        (hello, events)
    }

    /// Waits, without forwarding anything, until this link's relay queue is full: its channel's
    /// writer is then stuck sending, so the channel reads nothing either.
    async fn fill_queues(&mut self) {
        timeout(Duration::from_secs(5), async {
            while self.session.outs.iter().find(|(link, _)| *link == self.link).unwrap().1.as_ref().len() < 64 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn an_overflowed_link_reconnects_and_resumes_from_its_rev() {
    let hub = temp_hub();
    let key = phone_key(&hub);
    history(&hub, 1, 1);
    let mut p = RelayPhone::open(&hub, key, "L1").await;
    p.hello(&hub).await;
    let (hello, _) = p.catch_up(1, 0).await;
    let rev = hello["ev"]["rev"].as_i64().unwrap();
    // A channel that never reads again: a full queue nobody drains, while the real channel's
    // input stays open (dropping it would end that channel normally instead).
    let (tx, _stalled) = mpsc::channel(1);
    tx.try_send(Value::Null).unwrap();
    let _input = p.session.links.insert("L1".into(), tx);
    tokio::time::pause();
    p.send(json!({"id": 2, "cancel": true})).await;
    tokio::time::resume();
    let RelayPhone { mut session, mut out, key, .. } = p;
    let close = timeout(Duration::from_secs(5), out.recv()).await.unwrap().unwrap();
    assert_eq!(
        (close["t"].as_str(), close["link"].as_str(), close["code"].as_u64()),
        (Some("close"), Some("L1"), Some(4008)),
        "{close}"
    );
    assert!(!session.links.contains_key("L1"), "the overflowed link is gone");
    let dk = b64(key.verifying_key().as_bytes());
    session.on_message(&json!({"t": "open", "link": "L2", "dk": dk, "pair": false}).to_string()).await.unwrap();
    let mut p = RelayPhone { key, session, out, sealer: None, opener: None, link: "L2".into() };
    p.hello(&hub).await;
    let new = hub
        .store
        .insert_entry(&crate::store::Lane::main("b0"), crate::store::EntryKind::User, 1, &json!({"text": "new"}))
        .unwrap();
    let (hello, events) = p.catch_up(1, rev).await;
    assert_eq!(hello["ev"]["catchUp"], 2, "the icons snapshot and the new message");
    assert_eq!(events[0]["type"], "icons");
    assert_eq!(events[1]["entry"]["id"], new.id);
}

#[tokio::test]
async fn a_link_whose_channel_waits_on_its_answers_stays_open() {
    let hub = temp_hub();
    let key = phone_key(&hub);
    history(&hub, 30, 250);
    let mut phone = RelayPhone::open(&hub, key, "busy").await;
    phone.hello(&hub).await;
    phone.send(json!({"id": 1, "sub": "events", "b": {"since": 0, "client": "ios"}})).await;
    phone.fill_queues().await;
    // A burst read off the socket at once, each answered: the channel can't read the next one until
    // its answer is out, so the relay must keep forwarding while it waits.
    let burst = LINK_BUFFER as u64 * 3;
    for _ in 0..burst {
        phone.send(json!({"id": 1, "sub": "events", "b": {"since": 0, "client": "ios"}})).await;
        assert!(phone.session.links.contains_key("busy"), "a busy link must stay open");
    }
    let hello = phone.recv().await;
    let mut catch_up = hello["ev"]["catchUp"].as_u64().unwrap();
    let mut refused = 0;
    while catch_up > 0 || refused < burst {
        let v = phone.recv().await;
        if v["err"]["status"] == 400 {
            refused += 1;
        } else {
            catch_up -= 1;
        }
    }
}

#[tokio::test]
async fn after_a_host_restart_a_reconnecting_iphone_catches_up_only_new_messages() {
    let dir = std::env::temp_dir().join(format!("codync-relay-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let hub = open_hub(&dir);
    let key = phone_key(&hub);
    let bots = history(&hub, 30, 250);
    let mut phone = RelayPhone::open(&hub, key.clone(), "before").await;
    phone.hello(&hub).await;
    let (hello, _) = phone.catch_up(1, 0).await;
    assert_eq!(hello["ev"]["catchUp"], 30 * 201 + 1, "a first sync holds 200 entries and the bot per chat, plus icons");
    let rev = hello["ev"]["rev"].as_i64().unwrap();
    drop(phone);
    drop(hub);

    let hub = open_hub(&dir);
    let new: Vec<_> = bots[..2]
        .iter()
        .map(|id| {
            hub.store
                .insert_entry(
                    &crate::store::Lane::main(id),
                    crate::store::EntryKind::User,
                    250,
                    &json!({"text": "new"}),
                )
                .unwrap()
                .id
        })
        .collect();
    let mut phone = RelayPhone::open(&hub, key, "after").await;
    phone.hello(&hub).await;
    let (hello, events) = phone.catch_up(1, rev).await;
    assert_eq!(hello["ev"]["catchUp"], 3, "the icons snapshot and the two new messages");
    assert_eq!(events[0]["type"], "icons");
    let caught: Vec<_> = events[1..].iter().map(|e| e["entry"]["id"].as_str().unwrap().to_owned()).collect();
    assert_eq!(caught, new);
    assert!(phone.session.links.contains_key("after"), "no link overflowed");
}
