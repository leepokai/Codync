//! End to end: a real `codync-host serve` driving a scripted ACP agent
//! (`fake_agent.py`) through a full turn with an approval in the middle.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: failing loudly is the point

mod common;

/// The host's wire crypto doubles as this test's device implementation.
#[path = "../src/remote/crypto.rs"]
#[allow(dead_code)]
mod crypto;

use ed25519_dalek::SigningKey;
use futures::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
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

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

async fn start_host() -> Host {
    let home = std::env::temp_dir().join(format!("codync-e2e-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&home).unwrap();
    common::seed_memory_runtime(&home);
    let port = free_port();
    let child = Command::new(env!("CARGO_BIN_EXE_codync-host"))
        .args(["serve", "--bind", "127.0.0.1", "--port", &port.to_string()])
        .env("CODYNC_HOME", &home)
        .env("CODYNC_CLOUD", "off")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let base = format!("http://127.0.0.1:{port}");
    let deadline = Instant::now() + Duration::from_secs(20);
    while reqwest::get(format!("{base}/health")).await.is_err() {
        assert!(Instant::now() < deadline, "host didn't start");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
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
    let bots = host.call("sync", json!({"since": 0})).await;
    assert_eq!(bots["bots"][0]["status"], "needsInput");

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

    let synced = host.call("sync", json!({"since": 0})).await;
    assert_eq!(synced["bots"][0]["status"], "idle");
    assert_eq!(synced["bots"][0]["unread"], 1);
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
    while host.call("sync", json!({"since": 0})).await["bots"][0]["status"] != "idle" {
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

// ---------- wire shape snapshot (docs/reference/compatibility.md) ----------

/// Snapshot of what clients decode: field paths and JSON types of `hello` and the event
/// stream after a full turn. Values aren't kept, only the shape.
const WIRE_SHAPE: &str = "tests/fixtures/wire-shape.json";
/// Re-records the snapshot (refused while a field is removed or retyped without a new `minApp`).
const UPDATE_ENV: &str = "CODYNC_UPDATE_WIRE_SHAPE";
/// Depends on the machine (installed harnesses, displays, addresses, local usage), not the code.
const MACHINE_DEPENDENT: [&str; 4] = ["backends", "screen", "urls", "usage"];

fn json_type(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Every field path under `v` with its type; array elements merge under `[]`.
fn shape(v: &Value, path: &str, out: &mut std::collections::BTreeMap<String, String>) {
    out.insert(path.to_owned(), json_type(v).to_owned());
    match v {
        Value::Object(map) => {
            for (k, child) in map {
                if !MACHINE_DEPENDENT.contains(&k.as_str()) {
                    shape(child, &format!("{path}.{k}"), out);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                shape(item, &format!("{path}[]"), out);
            }
        }
        _ => {}
    }
}

/// The shape of a stream event, keyed by its type (and an entry's kind: their data differ).
fn event_shape(ev: &Value, out: &mut std::collections::BTreeMap<String, String>) {
    let ty = ev["type"].as_str().unwrap_or("?");
    let key = match ev["entry"]["kind"].as_str() {
        Some(kind) if ty == "entry" => format!("event:entry({kind})"),
        _ => format!("event:{ty}"),
    };
    shape(ev, &key, out);
}

/// What changed between the recorded and the current shape: (breaking, additions).
/// A field that was `null` on either side can't be judged by its type.
fn shape_diff(
    recorded: &std::collections::BTreeMap<String, String>,
    current: &std::collections::BTreeMap<String, String>,
) -> (Vec<String>, Vec<String>) {
    let mut breaking = vec![];
    for (path, ty) in recorded {
        match current.get(path) {
            None => breaking.push(format!("removed {path} ({ty})")),
            Some(now) if now != ty && now != "null" && ty != "null" => {
                breaking.push(format!("retyped {path}: {ty} → {now}"));
            }
            Some(_) => {}
        }
    }
    let added = current
        .iter()
        .filter(|(path, ty)| recorded.get(*path).is_none_or(|old| old == "null" && *ty != "null"))
        .map(|(path, ty)| format!("{path} ({ty})"))
        .collect();
    (breaking, added)
}

#[test]
fn shape_diff_tells_breaking_from_additive() {
    let map = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
    let recorded = map(&[("a", "object"), ("a.x", "string"), ("a.y", "null"), ("a.z", "number")]);
    let current = map(&[("a", "object"), ("a.x", "number"), ("a.y", "string"), ("a.w", "bool")]);
    let (breaking, added) = shape_diff(&recorded, &current);
    assert_eq!(breaking, ["retyped a.x: string → number", "removed a.z (number)"]);
    assert_eq!(added, ["a.w (bool)", "a.y (string)"]);
}

/// Reads the event stream's catch-up until it goes quiet.
async fn catch_up(host: &Host) -> Vec<Value> {
    let res = reqwest::Client::new()
        .get(format!("{}/events?since=0&client=test", host.base))
        .bearer_auth(&host.token)
        .send()
        .await
        .unwrap();
    let mut body = res.bytes_stream();
    let (mut buf, mut events) = (String::new(), vec![]);
    while let Ok(Some(Ok(chunk))) = tokio::time::timeout(Duration::from_millis(1500), body.next()).await {
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(i) = buf.find('\n') {
            let line: String = buf.drain(..=i).collect();
            if let Some(data) = line.trim_end().strip_prefix("data:")
                && let Ok(v) = serde_json::from_str::<Value>(data.trim_start())
            {
                events.push(v);
            }
        }
    }
    events
}

/// Older apps read what the host sends: a removed or retyped field breaks them. This fails
/// until the field is kept, or `MIN_APP` is raised the two-release way and the snapshot
/// re-recorded. New fields only need recording (`CODYNC_UPDATE_WIRE_SHAPE=1`).
#[tokio::test]
async fn wire_shape_matches_the_snapshot() {
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
    let card = host.wait_for(&bot, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await;
    host.call("respondPermission", json!({"entryId": card["id"], "optionId": "allow"})).await;
    host.wait_for(&bot, |e| e["kind"] == "agent" && e["data"]["final"] == true).await;

    let hello = host.call("hello", json!({})).await;
    let min_app = hello["minApp"].as_str().unwrap().to_owned();
    let mut current = std::collections::BTreeMap::new();
    shape(&hello, "hello", &mut current);
    for ev in catch_up(&host).await {
        event_shape(&ev, &mut current);
    }
    assert!(current.keys().any(|k| k.starts_with("event:entry(tool)")), "the turn's entries are missing");

    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(WIRE_SHAPE);
    let recorded: Value = std::fs::read_to_string(&file).map_or(Value::Null, |s| serde_json::from_str(&s).unwrap());
    let recorded_shapes: std::collections::BTreeMap<String, String> =
        serde_json::from_value(recorded["shapes"].clone()).unwrap_or_default();
    let (breaking, added) = shape_diff(&recorded_shapes, &current);
    let floor_raised = recorded["minApp"].as_str() != Some(min_app.as_str());
    assert!(
        breaking.is_empty() || floor_raised,
        "Older apps would break: {breaking:#?}\nKeep these fields, or follow the two-release rule \
         in docs/reference/compatibility.md: raise MIN_APP (host/src/compat.rs) once that iPhone \
         version is live, then re-record with {UPDATE_ENV}=1 cargo test --test e2e wire_shape"
    );
    if std::env::var_os(UPDATE_ENV).is_some() {
        let json = json!({"minApp": min_app, "shapes": current});
        std::fs::write(&file, serde_json::to_string_pretty(&json).unwrap() + "\n").unwrap();
        return;
    }
    assert!(
        breaking.is_empty() && added.is_empty() && !floor_raised,
        "The wire shape changed.\nAdded: {added:#?}\nRemoved or retyped (minApp was raised): {breaking:#?}\n\
         Record it with {UPDATE_ENV}=1 cargo test --test e2e wire_shape"
    );
}
