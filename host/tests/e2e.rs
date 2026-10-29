//! End to end: a real `codync-host serve` driving a scripted ACP agent
//! (`fake_agent.py`) through a full turn with an approval in the middle.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: failing loudly is the point

mod common;

#[path = "e2e/wire_shape.rs"]
mod wire_shape;

/// The host's wire crypto doubles as this test's device implementation.
#[path = "../src/remote/crypto.rs"]
#[allow(dead_code)]
mod crypto;

use ed25519_dalek::SigningKey;
use futures::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

struct Host {
    child: Child,
    base: String,
    token: String,
    home: PathBuf,
}

impl Drop for Host {
    fn drop(&mut self) {
        common::stop(&mut self.child);
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

async fn start_host() -> Host {
    let home = std::env::temp_dir().join(format!("codync-e2e-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&home).unwrap();
    let (child, base) = common::serve(&home).await;
    let token = std::fs::read_to_string(home.join("token")).unwrap().trim().to_owned();
    Host { child, base, token, home }
}

impl Host {
    async fn call(&self, method: &str, body: Value) -> Value {
        let res = reqwest::Client::new()
            .post(format!("{}/api/{method}", self.base))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = res.status();
        let v: Value = res.json().await.unwrap();
        assert!(status.is_success(), "{method} failed: {v}");
        v
    }

    /// Polls the bot's history until `pred` matches an entry.
    async fn wait_for(&self, bot: &str, pred: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let h = self.call("history", json!({"botId": bot})).await;
            if let Some(e) = h["entries"].as_array().unwrap().iter().rev().find(|e| pred(e)) {
                return e.clone();
            }
            assert!(Instant::now() < deadline, "timed out; history: {h}");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

#[tokio::test]
async fn health_names_this_binary() {
    let host = start_host().await;
    let health: Value = reqwest::get(format!("{}/health", host.base)).await.unwrap().json().await.unwrap();
    assert!(health["binaryHash"].as_str().is_some_and(|hash| hash.len() == 64));
    // The apps compare it with the path they resolve, which on Windows has no `\\?\` prefix.
    let exe = std::fs::canonicalize(env!("CARGO_BIN_EXE_codync-host")).unwrap();
    let exe = exe.to_str().unwrap();
    assert_eq!(health["binaryPath"], exe.strip_prefix(r"\\?\").unwrap_or(exe));
}

#[tokio::test]
async fn readiness_needs_this_homes_token() {
    use axum::http::{HeaderMap, StatusCode};
    use axum::{Router, routing::get};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new().route(
        "/events",
        get(|headers: HeaderMap| async move {
            if headers.get("authorization").and_then(|v| v.to_str().ok()) == Some("Bearer listener-token") {
                StatusCode::OK
            } else {
                StatusCode::UNAUTHORIZED
            }
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let home = std::env::temp_dir().join(format!("codync-readiness-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&home).unwrap();
    assert!(common::ready(&base, &home).await.is_err(), "a foreign listener does not make a new home ready");
    std::fs::write(home.join("token"), "different-token").unwrap();
    assert!(common::ready(&base, &home).await.is_err(), "an existing token from a restart must match the listener");
    std::fs::write(home.join("token"), "listener-token").unwrap();
    assert!(common::ready(&base, &home).await.is_ok());
    server.abort();
    std::fs::remove_dir_all(home).unwrap();
}

// SIGTERM is Unix only; Windows stops the host by ending its process.
#[cfg(unix)]
#[tokio::test]
async fn termination_exits_with_an_open_event_stream() {
    let mut host = start_host().await;
    let stream =
        reqwest::Client::new().get(format!("{}/events", host.base)).bearer_auth(&host.token).send().await.unwrap();
    assert!(stream.status().is_success());
    assert!(Command::new("/bin/kill").args(["-TERM", &host.child.id().to_string()]).status().unwrap().success());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = host.child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline, "open SSE prevented host shutdown");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    drop(stream);
}

#[tokio::test]
async fn turn_with_approval_reaches_a_final_reply() {
    if Command::new(common::python()).arg("--version").output().is_err() {
        eprintln!("skipping: Python not available");
        return;
    }
    let host = start_host().await;
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fake_agent.py");
    let bot = host
        .call(
            "createBot",
            json!({
                "name": "Tester", "backend": "custom", "command": common::python_agent(&agent),
                "cwd": host.home.to_string_lossy(), "permission": "ask",
            }),
        )
        .await["bot"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    host.call("send", json!({"botId": bot, "text": "go", "clientNonce": "n1"})).await;
    // Retrying with the same nonce must not queue a second turn.
    host.call("send", json!({"botId": bot, "text": "go", "clientNonce": "n1"})).await;

    let card = host.wait_for(&bot, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await;
    assert_eq!(card["data"]["title"], "Edit a.txt");
    let bots = common::snapshot(&host.base, &host.token).await;
    assert_eq!(common::bot(&bots, &bot)["status"], "needsInput");

    host.call("respondPermission", json!({"entryId": card["id"], "optionId": "allow"})).await;
    let reply = host.wait_for(&bot, |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    assert!(reply["data"]["text"].as_str().unwrap().contains("\"allow\""), "{reply}");

    // The narration before the tool call stays out of the chat.
    let history = host.call("history", json!({"botId": bot})).await;
    let entries = history["entries"].as_array().unwrap();
    let finals = entries.iter().filter(|e| e["kind"] == "agent" && e["data"]["final"] == true).count();
    assert_eq!(finals, 1);
    assert_eq!(entries.iter().filter(|e| e["kind"] == "user").count(), 1);
    let tool = entries.iter().find(|e| e["kind"] == "tool").unwrap();
    assert_eq!(tool["data"]["status"], "completed");
    assert_eq!(tool["data"]["diffs"][0]["added"], 1);

    let synced = common::snapshot(&host.base, &host.token).await;
    assert_eq!(common::bot(&synced, &bot)["status"], "idle");
    assert_eq!(common::bot(&synced, &bot)["unread"], 1);
}

#[tokio::test]
async fn sent_messages_are_the_reply() {
    if Command::new(common::python()).arg("--version").output().is_err() {
        eprintln!("skipping: Python not available");
        return;
    }
    let host = start_host().await;
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fake_agent.py");
    let bot = host
        .call(
            "createBot",
            json!({
                "name": "Tester", "backend": "custom", "command": common::python_agent(&agent),
                "cwd": host.home.to_string_lossy(), "permission": "ask",
            }),
        )
        .await["bot"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let send = |text: &str| json!({"botId": bot, "name": "send_message", "arguments": {"text": text}});

    // Between turns there's no one to talk to.
    let idle = reqwest::Client::new()
        .post(format!("{}/api/chatCall", host.base))
        .bearer_auth(&host.token)
        .json(&send("hello?"))
        .send()
        .await
        .unwrap();
    assert!(!idle.status().is_success());

    host.call("send", json!({"botId": bot, "text": "go", "clientNonce": "n1"})).await;
    let card = host.wait_for(&bot, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await;
    // Mid-turn (the agent waits on the approval), each message is its own chat bubble at once.
    host.call("chatCall", send("Found it.")).await;
    host.call("chatCall", send("Fixing the typo now.")).await;
    let first = host.wait_for(&bot, |e| e["kind"] == "agent" && e["data"]["text"] == "Found it.").await;
    assert_eq!(first["data"]["final"], true);

    host.call("respondPermission", json!({"entryId": card["id"], "optionId": "allow"})).await;
    let deadline = Instant::now() + Duration::from_secs(20);
    while common::bot(&common::snapshot(&host.base, &host.token).await, &bot)["status"] != "idle" {
        assert!(Instant::now() < deadline, "turn didn't finish");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // What the agent wrote as its reply stays in the trace.
    let history = host.call("history", json!({"botId": bot})).await;
    let entries = history["entries"].as_array().unwrap();
    assert!(entries.iter().any(|e| e["data"]["text"].as_str().is_some_and(|t| t.starts_with("Done"))));
    let finals: Vec<&str> = entries
        .iter()
        .filter(|e| e["kind"] == "agent" && e["data"]["final"] == true)
        .map(|e| e["data"]["text"].as_str().unwrap())
        .collect();
    assert_eq!(finals, ["Found it.", "Fixing the typo now."]);
}

#[tokio::test]
async fn rejects_requests_without_the_token() {
    let host = start_host().await;
    let res = reqwest::Client::new().post(format!("{}/api/hello", host.base)).json(&json!({})).send().await.unwrap();
    assert_eq!(res.status(), 401);
    let res = reqwest::Client::new()
        .post(format!("{}/api/hello", host.base))
        .bearer_auth("wrong")
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// A phone talking to the host over the direct E2E channel.
struct Phone {
    ws: Ws,
    sealer: crypto::Sealer,
    opener: crypto::Opener,
}

enum Frame {
    Inner(Value),
    Closed(u16),
}

impl Phone {
    /// Connects and completes the handshake, verifying the host against the pinned `sk`.
    async fn connect(host: &Host, key: &SigningKey, sk: &[u8; 32], pair: bool) -> Self {
        let url = format!("{}/channel?v=1", host.base.replace("http://", "ws://"));
        let (mut ws, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let cid = crypto::computer_id_raw(sk);
        let dk = key.verifying_key().to_bytes();
        let ek = x25519_dalek::StaticSecret::from(crypto::random::<32>());
        let ek_pub = crypto::x25519_pub(&ek);
        let n = crypto::random::<32>();
        let sig = crypto::sign(key, &crypto::hs1_input(&cid, &dk, &ek_pub, &n));
        let hello = json!({"t": "hello", "v": 1, "dk": crypto::b64(&dk), "ek": crypto::b64(&ek_pub),
            "n": crypto::b64(&n), "sig": crypto::b64(&sig), "pair": pair});
        ws.send(hello.to_string().into()).await.unwrap();
        let welcome: Value = serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(welcome["t"], "welcome", "{welcome}");
        let ek_h: [u8; 32] = crypto::unb64_n(welcome["ek"].as_str().unwrap()).unwrap();
        let th = crypto::transcript_hash(&cid, &dk, &ek_pub, &n, &ek_h);
        crypto::verify(sk, &th, &crypto::unb64_n(welcome["sig"].as_str().unwrap()).unwrap()).unwrap();
        let keys = crypto::channel_keys(&crypto::x25519(&ek, &ek_h).unwrap(), &th);
        Self { ws, sealer: crypto::Sealer::new(keys.d2h), opener: crypto::Opener::new(keys.h2d, 16 << 20) }
    }

    async fn send(&mut self, v: Value) {
        for f in self.sealer.seal(v.to_string().as_bytes()).unwrap() {
            self.ws.send(f.to_string().into()).await.unwrap();
        }
    }

    async fn recv(&mut self) -> Frame {
        use tokio_tungstenite::tungstenite::Message;
        loop {
            let msg = tokio::time::timeout(Duration::from_secs(20), self.ws.next()).await.expect("host answers");
            match msg.unwrap().unwrap() {
                Message::Text(t) => {
                    let f: Value = serde_json::from_str(t.as_str()).unwrap();
                    if let Some(bytes) = self.opener.open(&f).unwrap() {
                        return Frame::Inner(serde_json::from_slice(&bytes).unwrap());
                    }
                }
                Message::Close(c) => return Frame::Closed(c.map_or(1005, |c| c.code.into())),
                _ => {}
            }
        }
    }

    async fn inner(&mut self) -> Value {
        match self.recv().await {
            Frame::Inner(v) => v,
            Frame::Closed(code) => panic!("closed with {code}"),
        }
    }

    async fn call(&mut self, id: u64, m: &str, b: Value) -> Value {
        self.send(json!({"id": id, "m": m, "b": b})).await;
        loop {
            let v = self.inner().await;
            if v["id"] == id && v.get("ev").is_none() {
                assert!(v["ok"].is_object(), "{m}: {v}");
                return v["ok"].clone();
            }
        }
    }
}

fn query_param(url: &str, name: &str) -> String {
    url.split(['?', '&']).find_map(|p| p.strip_prefix(&format!("{name}="))).unwrap().to_owned()
}

#[tokio::test]
async fn phone_pairs_and_chats_over_the_direct_channel() {
    if Command::new(common::python()).arg("--version").output().is_err() {
        eprintln!("skipping: Python not available");
        return;
    }
    let host = start_host().await;
    let old = tokio_tungstenite::connect_async(format!("{}/channel?v=2", host.base.replace("http://", "ws://"))).await;
    let Err(tokio_tungstenite::tungstenite::Error::Http(res)) = old else {
        panic!("an unknown version must be refused")
    };
    assert_eq!(res.status(), 426);

    let qr = host.call("pairing", json!({})).await;
    let url = qr["pairingUrl"].as_str().unwrap();
    let sk: [u8; 32] = crypto::unb64_n(&query_param(url, "sk")).unwrap();
    assert_eq!(query_param(url, "id"), crypto::computer_id(&sk));
    let key = SigningKey::from_bytes(&crypto::random());

    let mut phone = Phone::connect(&host, &key, &sk, true).await;
    let code = query_param(url, "code");
    phone.send(json!({"id": 1, "m": "pair", "b": {"code": code, "name": "Test iPhone", "platform": "ios"}})).await;
    assert_eq!(phone.inner().await["ok"]["computerId"], crypto::computer_id(&sk));
    assert!(matches!(phone.recv().await, Frame::Closed(4100)));

    let mut phone = Phone::connect(&host, &key, &sk, false).await;
    let hello = phone.call(1, "hello", json!({})).await;
    assert_eq!(hello["boxKey"], query_param(url, "bk"));
    // Phones check themselves against it (docs/reference/compatibility.md).
    assert!(hello["minApp"].as_str().is_some_and(|v| !v.is_empty()));
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fake_agent.py");
    let bot = phone
        .call(
            2,
            "createBot",
            json!({
                "name": "Tester", "backend": "custom", "command": common::python_agent(&agent),
                "cwd": host.home.to_string_lossy(), "permission": "auto",
            }),
        )
        .await["bot"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    phone.send(json!({"id": 3, "sub": "events", "b": {"since": 0, "client": "ios"}})).await;
    assert_eq!(phone.inner().await["ev"]["type"], "hello");
    phone.call(4, "send", json!({"botId": bot, "text": "go", "clientNonce": "n1"})).await;
    loop {
        let v = phone.inner().await;
        let e = &v["ev"]["entry"];
        if v["id"] == 3 && e["kind"] == "agent" && e["data"]["final"] == true {
            assert!(e["data"]["text"].as_str().unwrap().starts_with("Done"), "{e}");
            break;
        }
    }

    let devices = host.call("devices", json!({})).await;
    assert_eq!(devices["devices"][0]["name"], "Test iPhone");
    assert_eq!(devices["devices"][0]["connected"], true);
}

#[tokio::test]
async fn personal_workspaces_are_unique_persistent_and_optional() {
    let host = start_host().await;
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/workspace_agent.py");
    let first = host
        .call("createBot", json!({"name": "One", "backend": "custom", "command": common::python_agent(&agent)}))
        .await;
    let second = host.call("createBot", json!({"name": "Two", "backend": "claude", "cwd": ""})).await;
    let first = &first["bot"];
    let second = &second["bot"];
    let dir = PathBuf::from(first["cwd"].as_str().unwrap());
    assert_eq!(dir, host.home.join("bots").join(first["id"].as_str().unwrap()).join("workspace"));
    assert!(dir.is_dir());
    assert_ne!(first["cwd"], second["cwd"]);
    assert_eq!(first["managedWorkspace"], true);
    host.call("send", json!({"botId": first["id"], "text": "verify"})).await;
    host.wait_for(first["id"].as_str().unwrap(), |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    let execution: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("execution.json")).unwrap()).unwrap();
    assert_eq!(
        std::fs::canonicalize(execution["process"].as_str().unwrap()).unwrap(),
        std::fs::canonicalize(&dir).unwrap()
    );
    assert_eq!(execution["session"], first["cwd"]);
    std::fs::write(dir.join("keep.txt"), "persistent output").unwrap();
    let renamed = host.call("updateBot", json!({"id": first["id"], "name": "Renamed", "cwd": ""})).await;
    assert_eq!(renamed["bot"]["cwd"], first["cwd"]);
    let project = host.home.join("project");
    std::fs::create_dir(&project).unwrap();
    let explicit = host.call("updateBot", json!({"id": first["id"], "cwd": project})).await;
    assert_eq!(explicit["bot"]["managedWorkspace"], false);
    let restored = host.call("updateBot", json!({"id": first["id"], "cwd": ""})).await;
    assert_eq!(restored["bot"]["cwd"], first["cwd"]);
    assert_eq!(std::fs::read_to_string(dir.join("keep.txt")).unwrap(), "persistent output");
    host.call("deleteBot", json!({"botId": first["id"]})).await;
    assert!(dir.join("keep.txt").is_file(), "deleting a bot must retain its files");
    let group = host.call("createBot", json!({"name": "Group", "kind": "group", "members": [second["id"]]})).await;
    assert_eq!(group["bot"]["cwd"], "");
    assert_eq!(group["bot"]["managedWorkspace"], false);
}
