//! A real host and Chrome: one user message binds an isolated tab, clicks, types, posts a screenshot
//! to the same chat, and resists stale targets, user takeover and Stop.

#![cfg(windows)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

#[cfg(windows)]
#[path = "common/phone.rs"]
mod phone;

#[cfg(windows)]
#[allow(dead_code)]
#[path = "../src/remote/crypto.rs"]
mod crypto;

use axum::{Router, response::Html, routing::get};
#[cfg(windows)]
use base64::Engine as _;
use common::host::Host;
#[cfg(windows)]
use ed25519_dalek::SigningKey;
use futures::{SinkExt as _, StreamExt as _};
use image::GenericImageView as _;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
#[cfg(windows)]
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
#[cfg(windows)]
use tokio_tungstenite::tungstenite::{Message, http::header};

const PAGE: &str = r#"<!doctype html>
<html><head><title>Codync isolated browser</title></head>
<body>
  <button id="count" onclick="document.querySelector('#state').textContent='Count: 1'">Count</button>
  <input id="name" aria-label="Name">
  <p id="state">Count: 0</p>
</body></html>"#;

async fn page_url() -> String {
    let app = Router::new().route("/", get(|| async { Html(PAGE) }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}/")
}

fn browser_available() -> bool {
    if std::env::var_os("CODYNC_BROWSER_BIN").is_some_and(|value| PathBuf::from(value).is_file()) {
        return true;
    }
    #[cfg(windows)]
    let candidates = [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    ];
    #[cfg(target_os = "macos")]
    let candidates = ["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"];
    #[cfg(all(unix, not(target_os = "macos")))]
    let candidates = ["/usr/bin/google-chrome", "/usr/bin/chromium", "/usr/bin/chromium-browser"];
    candidates.into_iter().any(|path| Path::new(path).is_file())
}

async fn browser_bot(host: &Host) -> String {
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/browser_agent.py");
    let command = common::python_agent(&agent);
    let bot = json!({
        "name": "Browser Bot",
        "backend": "custom",
        "command": command,
        "connectors": [],
        "computer": true,
        "notify": false
    });
    host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned()
}

async fn send(host: &Host, bot: &str, text: &str) -> String {
    host.call(
        "send",
        json!({"botId": bot, "text": text, "clientNonce": uuid::Uuid::new_v4().to_string()}),
    )
    .await["entry"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn results_path(host: &Host, bot: &str) -> PathBuf {
    host.home.join("bots").join(bot).join("workspace").join("browser-results.json")
}

fn active_path(host: &Host, bot: &str) -> PathBuf {
    host.home.join("bots").join(bot).join("workspace").join("browser-active.json")
}

fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut value = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(value, "{byte:02x}");
    }
    value
}

fn capture_failure(home: &Path) -> std::io::Result<PathBuf> {
    fn copy_evidence(source: &Path, target: &Path) -> std::io::Result<()> {
        let Ok(metadata) = std::fs::symlink_metadata(source) else {
            return Ok(());
        };
        if metadata.is_dir() {
            std::fs::create_dir_all(target)?;
            for child in std::fs::read_dir(source)? {
                let Ok(child) = child else {
                    continue;
                };
                let _ = copy_evidence(&child.path(), &target.join(child.file_name()));
            }
            return Ok(());
        }
        if !metadata.is_file() {
            return Ok(());
        }
        let interesting = source.file_name().and_then(|name| name.to_str()).is_some_and(|name| {
            name == "host.log"
                || name == "chrome.log"
                || name == "DevToolsActivePort"
                || Path::new(name).extension().is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        });
        if interesting {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(source, target)?;
        }
        Ok(())
    }

    let evidence_root =
        std::env::var_os("CODYNC_BROWSER_E2E_CAPTURE").map_or_else(|| home.with_extension("failure"), PathBuf::from);
    let target = evidence_root.join(home.file_name().unwrap_or_default());
    copy_evidence(home, &target)?;
    Ok(target)
}

async fn wait_json(home: &Path, path: &Path, within: Duration) -> Value {
    let deadline = Instant::now() + within;
    loop {
        if let Ok(text) = tokio::fs::read_to_string(path).await
            && let Ok(value) = serde_json::from_str(&text)
        {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {}; preserved evidence at {}",
            path.display(),
            capture_failure(home)
                .map_or_else(|error| format!("unavailable: {error}"), |path| path.display().to_string())
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn history(host: &Host, bot: &str) -> Vec<Value> {
    host.call("history", json!({"botId": bot})).await["entries"].as_array().unwrap().clone()
}

#[cfg(windows)]
async fn fake_extension(
    host: &Host,
    screenshot: Vec<u8>,
    start_delay: Duration,
    requests: Arc<StdMutex<Vec<Value>>>,
) -> tokio::task::JoinHandle<()> {
    let url = host.base.replacen("http://", "ws://", 1) + "/browser-extension";
    let mut request = url.into_client_request().unwrap();
    request.headers_mut().insert(header::AUTHORIZATION, format!("Bearer {}", host.token).parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    socket
        .send(Message::Text(
            json!({
                "type": "hello",
                "protocol": 1,
                "extensionId": "dgejialhpalhoigbkmgeabgpphlipiek",
                "version": "test",
                "isolatedProfile": true
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let ready = socket.next().await.unwrap().unwrap();
    assert!(ready.to_text().unwrap().contains("\"ready\""));
    tokio::spawn(async move {
        let mut closed_stop = false;
        let mut tab_sequence = 0;
        while let Some(Ok(message)) = socket.next().await {
            let request: Value = match message {
                Message::Text(text) => serde_json::from_str(text.as_str()).unwrap(),
                Message::Binary(bytes) => serde_json::from_slice(&bytes).unwrap(),
                Message::Close(_) => break,
                Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
            };
            let Some(id) = request["id"].as_str() else {
                continue;
            };
            requests.lock().unwrap().push(request.clone());
            let state = json!({
                "url": "http://127.0.0.1/",
                "title": "Codync extension fixture",
                "text": if request["method"] == "click" { "Count: 1" } else { "name=Codync" }
            });
            if request["method"] == "start" {
                tab_sequence += 1;
                closed_stop = tab_sequence == 1
                    && request["params"]["url"].as_str().is_some_and(|url| url.contains("closed-stop"));
            }
            if request["method"] == "start" && !start_delay.is_zero() {
                tokio::time::sleep(start_delay).await;
            }
            if request["method"] == "stop" && closed_stop {
                closed_stop = false;
                socket
                    .send(Message::Text(
                        json!({
                            "id": id,
                            "ok": false,
                            "code": "browser_session_closed",
                            "error": "Browser operation is no longer running in Codync Browser"
                        })
                        .to_string()
                        .into(),
                    ))
                    .await
                    .unwrap();
                continue;
            }
            let result = match request["method"].as_str().unwrap() {
                "start" => json!({
                    "tabId": if tab_sequence == 1 { "123".to_owned() } else { tab_sequence.to_string() },
                    "browserVersion": "Chrome test",
                    "browserExecutable": "chrome-extension://fixture",
                    "extensionVersion": "0.1.0",
                    "state": state
                }),
                "state" | "navigate" | "click" | "type" => state,
                "screenshot" => json!({
                    "data": base64::engine::general_purpose::STANDARD.encode(&screenshot),
                    "state": state
                }),
                "stop" => json!({"closed": true}),
                "cancel" => json!({"cancelled": true}),
                method => panic!("unexpected browser extension method {method}"),
            };
            socket
                .send(Message::Text(json!({"id": id, "ok": true, "result": result}).to_string().into()))
                .await
                .unwrap();
            if request["method"] == "stop" {
                break;
            }
        }
    })
}

#[cfg(windows)]
#[tokio::test]
async fn screenshot_returns_to_the_same_chat_with_guards() {
    if !browser_available() {
        eprintln!("skipping: Chrome/Chromium not available");
        return;
    }
    if common::python_missing() {
        return;
    }

    let host = Host::start_with(&[("CODYNC_BROWSER_HEADLESS", std::ffi::OsStr::new("1"))]).await;
    let bot = browser_bot(&host).await;
    let url = page_url().await;

    let message_id = send(&host, &bot, &format!("browser-happy {url}")).await;
    let result = wait_json(&host.home, &results_path(&host, &bot), Duration::from_secs(90)).await;
    assert_eq!(result["start"]["target"]["messageId"], message_id, "{result}");
    assert_ne!(result["start"]["target"]["operationId"], message_id, "{result}");
    assert!(!result["start"]["target"]["hostId"].as_str().unwrap().is_empty(), "{result}");
    assert_eq!(result["start"]["target"]["botId"], bot, "{result}");
    assert_eq!(result["reusedStart"]["reused"], true, "{result}");
    assert_eq!(result["reusedStart"]["target"], result["start"]["target"], "{result}");
    assert!(result["start"]["target"]["browserVersion"].as_str().unwrap().contains("Chrome"), "{result}");
    assert!(result["staleError"].as_str().unwrap().contains("browser target changed"), "{result}");
    assert!(result["click"]["state"]["text"].as_str().unwrap().contains("Count: 1"), "{result}");
    assert!(result["type"]["state"]["text"].as_str().unwrap().contains("name=Codync"), "{result}");
    assert_eq!(result["screenshot"]["postedToChat"], true, "{result}");
    assert_eq!(result["screenshot"]["screenshot"]["width"], 1280, "{result}");
    assert_eq!(result["screenshot"]["screenshot"]["height"], 720, "{result}");
    assert_eq!(result["stop"]["closed"], true, "{result}");
    assert!(result["postStopError"].as_str().unwrap().contains("no longer running"), "{result}");

    let entries = history(&host, &bot).await;
    let user = entries.iter().find(|entry| entry["id"] == message_id).unwrap();
    assert_eq!(user["kind"], "user");
    let file = entries
        .iter()
        .find(|entry| entry["data"]["files"].as_array().is_some_and(|files| !files.is_empty()))
        .expect("the screenshot file card is in the same chat");
    let shared = file["data"]["files"][0].clone();
    let operation_id = result["start"]["target"]["operationId"].as_str().unwrap();
    assert_eq!(shared["name"], format!("browser-{}-1.png", &operation_id[..8]));
    let stored = host.home.join("bots").join(&bot).join("files").join(shared["id"].as_str().unwrap());
    let bytes = tokio::fs::read(&stored).await.unwrap();
    let screenshot_sha256 = sha256_hex(&bytes);
    assert_eq!(screenshot_sha256, result["screenshot"]["screenshot"]["sha256"].as_str().unwrap());
    let image = image::load_from_memory(&bytes).unwrap();
    assert_eq!(image.dimensions(), (1280, 720));
    assert_ne!(file["id"], message_id, "the screenshot must arrive as its own same-chat entry");

    let target = &result["start"]["target"];
    let chrome_executable = PathBuf::from(target["browserExecutable"].as_str().unwrap());
    let chrome_sha256 = sha256_hex(&std::fs::read(&chrome_executable).unwrap());
    let profile = host.home.join("browsers").join(&bot).join(target["browserId"].as_str().unwrap()).join("profile");
    eprintln!(
        "BROWSER_EVIDENCE chrome_executable={} chrome_sha256={} chrome_pid={} browser_version={} profile={} host_id={} bot_id={} message_id={} operation_id={} browser_id={} tab_id={} generation={} url={} screenshot_path={} screenshot_sha256={} screenshot_size={} screenshot_width={} screenshot_height={}",
        chrome_executable.display(),
        chrome_sha256,
        target["browserPid"],
        target["browserVersion"].as_str().unwrap(),
        profile.display(),
        target["hostId"].as_str().unwrap(),
        target["botId"].as_str().unwrap(),
        target["messageId"].as_str().unwrap(),
        operation_id,
        target["browserId"].as_str().unwrap(),
        target["tabId"].as_str().unwrap(),
        target["generation"].as_str().unwrap(),
        result["screenshot"]["state"]["url"].as_str().unwrap(),
        result["screenshot"]["screenshot"]["path"].as_str().unwrap(),
        screenshot_sha256,
        result["screenshot"]["screenshot"]["size"],
        result["screenshot"]["screenshot"]["width"],
        result["screenshot"]["screenshot"]["height"],
    );
    eprintln!(
        "BROWSER_RECEIPT chat_bot_id={} user_message_id={} file_message_id={} file_id={} file_name={} stored_file={} stored_sha256={}",
        bot,
        message_id,
        file["id"].as_str().unwrap(),
        shared["id"].as_str().unwrap(),
        shared["name"].as_str().unwrap(),
        stored.display(),
        screenshot_sha256,
    );

    // User takeover happens after the tab is bound: no later browser action may run.
    let _ = tokio::fs::remove_file(results_path(&host, &bot)).await;
    let _ = tokio::fs::remove_file(active_path(&host, &bot)).await;
    let _takeover_message = send(&host, &bot, &format!("browser-takeover {url}")).await;
    let _ = wait_json(&host.home, &active_path(&host, &bot), Duration::from_secs(30)).await;
    let state = host.call("screenTakeover", json!({"on":true})).await;
    assert_eq!(state["userControl"], true, "{state}");
    let takeover = wait_json(&host.home, &results_path(&host, &bot), Duration::from_secs(30)).await;
    assert!(takeover["guardError"].as_str().unwrap().contains("taken over the screen"), "{takeover}");
    host.call("screenTakeover", json!({"on":false})).await;

    // Stop cancels the bound operation before its delayed first action.
    let _ = tokio::fs::remove_file(results_path(&host, &bot)).await;
    let _ = tokio::fs::remove_file(active_path(&host, &bot)).await;
    let _stop_message = send(&host, &bot, &format!("browser-stop {url}")).await;
    let _ = wait_json(&host.home, &active_path(&host, &bot), Duration::from_secs(30)).await;
    host.call("stop", json!({"botId": bot})).await;
    let stopped = wait_json(&host.home, &results_path(&host, &bot), Duration::from_secs(30)).await;
    assert!(stopped["cancelError"].as_str().unwrap().contains("no longer running"), "{stopped}");
}

#[tokio::test]
#[cfg(windows)]
async fn extension_bridge_posts_to_the_same_chat_without_a_headless_fallback() {
    if common::python_missing() {
        return;
    }
    let host = Host::start().await;
    let mut png = Vec::new();
    image::DynamicImage::new_rgba8(2, 2)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let requests = Arc::new(StdMutex::new(Vec::new()));
    let extension = fake_extension(&host, png.clone(), Duration::ZERO, requests).await;
    let bot = browser_bot(&host).await;

    let message_id = send(&host, &bot, "browser-happy http://127.0.0.1/").await;
    let result = wait_json(&host.home, &results_path(&host, &bot), Duration::from_secs(45)).await;
    assert_eq!(result["start"]["target"]["messageId"], message_id, "{result}");
    assert_eq!(result["start"]["target"]["tabId"], "123", "{result}");
    assert_eq!(result["reusedStart"]["reused"], true, "{result}");
    assert!(result["click"]["state"]["text"].as_str().unwrap().contains("Count: 1"), "{result}");
    assert!(result["type"]["state"]["text"].as_str().unwrap().contains("name=Codync"), "{result}");
    assert_eq!(result["screenshot"]["screenshot"]["width"], 2, "{result}");
    assert_eq!(result["screenshot"]["screenshot"]["height"], 2, "{result}");
    assert_eq!(result["screenshot"]["postedToChat"], true, "{result}");
    assert_eq!(result["stop"]["closed"], true, "{result}");

    let entries = history(&host, &bot).await;
    let file = entries
        .iter()
        .find(|entry| entry["data"]["files"].as_array().is_some_and(|files| !files.is_empty()))
        .expect("the extension screenshot file card is in the same chat");
    let shared = file["data"]["files"][0].clone();
    let stored = host.home.join("bots").join(&bot).join("files").join(shared["id"].as_str().unwrap());
    assert_eq!(tokio::fs::read(&stored).await.unwrap(), png);
    assert_eq!(result["screenshot"]["screenshot"]["sha256"].as_str().unwrap(), sha256_hex(&png));
    extension.await.unwrap();
}

#[tokio::test]
#[cfg(windows)]
async fn iphone_channel_receives_browser_progress_and_screenshot_in_the_same_chat() {
    if common::python_missing() {
        return;
    }
    let host = Host::start().await;
    let mut png = Vec::new();
    image::DynamicImage::new_rgba8(2, 2)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let extension = fake_extension(&host, png.clone(), Duration::ZERO, Arc::new(StdMutex::new(Vec::new()))).await;
    let bot = browser_bot(&host).await;

    // Pair an iPhone over the direct E2E channel. The loopback-only desktop is the approver.
    let pairing = host.call("pairing", json!({})).await;
    let pairing_url = pairing["pairingUrl"].as_str().unwrap();
    let host_key: [u8; 32] = crypto::unb64_n(&phone::query_param(pairing_url, "sk")).unwrap();
    let device_key = SigningKey::from_bytes(&crypto::random());
    let mut pairing_phone = phone::Phone::connect(&host.base, &device_key, &host_key, true).await;
    let certificate = phone::test_certificate(&crypto::b64(device_key.verifying_key().as_bytes()), "browser-user");
    pairing_phone
        .send(json!({
            "id": 1,
            "m": "pair",
            "b": {
                "code": phone::query_param(pairing_url, "code"),
                "name": "Browser Test iPhone",
                "platform": "ios",
                "certificate": certificate,
            }
        }))
        .await;
    let request = loop {
        let requests = host.call("pairingRequests", json!({})).await;
        if let Some(request) = requests["requests"].as_array().and_then(|items| items.first()) {
            break request.clone();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(request["deviceName"], "Browser Test iPhone", "{request}");
    host.call("decidePairing", json!({"requestId": request["requestId"], "approve": true})).await;
    assert_eq!(pairing_phone.inner().await["ok"]["computerId"], crypto::computer_id(&host_key));
    assert!(matches!(pairing_phone.recv().await, phone::Frame::Closed(4100)));

    let mut iphone = phone::Phone::connect(&host.base, &device_key, &host_key, false).await;
    let hello = iphone.call(1, "hello", json!({})).await;
    assert_eq!(hello["boxKey"], phone::query_param(pairing_url, "bk"));
    let current_rev = host.snapshot().await["rev"].as_i64().unwrap();
    iphone.send(json!({"id": 2, "sub": "events", "b": {"since": current_rev, "client": "ios"}})).await;
    assert_eq!(iphone.inner().await["ev"]["type"], "hello");

    // Send from the phone itself and retain every event, including the early working state.
    iphone
        .send(json!({
            "id": 3,
            "m": "send",
            "b": {
                "botId": bot,
                "text": "browser-happy http://127.0.0.1/",
                "clientNonce": "browser-iphone-e2e",
            }
        }))
        .await;

    let (mut message_id, mut saw_working, mut saw_idle, mut screenshot, mut final_reply) =
        (None, false, false, None, None);
    let deadline = Instant::now() + Duration::from_secs(60);
    while screenshot.is_none() || final_reply.is_none() || !saw_working || !saw_idle {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for the iPhone browser turn; preserved evidence at {}",
            capture_failure(&host.home)
                .map_or_else(|error| format!("unavailable: {error}"), |path| path.display().to_string())
        );
        let value = iphone.inner().await;
        if value["id"] == 3 && value.get("ev").is_none() {
            assert!(value["ok"].is_object(), "{value}");
            message_id = value["ok"]["entry"]["id"].as_str().map(str::to_owned);
            continue;
        }
        let event = &value["ev"];
        match event["type"].as_str() {
            Some("bot") if event["bot"]["id"] == bot => match event["bot"]["status"].as_str() {
                Some("working") => saw_working = true,
                Some("idle") => saw_idle = true,
                _ => {}
            },
            Some("entry") => {
                let entry = &event["entry"];
                if entry["botId"] != bot {
                    continue;
                }
                if entry["kind"] == "agent" && entry["data"]["files"].as_array().is_some_and(|files| !files.is_empty())
                {
                    screenshot = Some(entry.clone());
                }
                if entry["kind"] == "agent"
                    && entry["data"]["final"] == true
                    && entry["data"]["text"] == "Browser operation complete."
                {
                    final_reply = Some(entry.clone());
                }
            }
            _ => {}
        }
    }

    let result = wait_json(&host.home, &results_path(&host, &bot), Duration::from_secs(45)).await;
    let message_id = message_id.expect("the phone's send acknowledgement names its chat entry");
    assert_eq!(result["start"]["target"]["messageId"], message_id, "{result}");
    assert!(result["click"]["state"]["text"].as_str().unwrap().contains("Count: 1"), "{result}");
    assert!(result["type"]["state"]["text"].as_str().unwrap().contains("name=Codync"), "{result}");
    assert_eq!(result["screenshot"]["postedToChat"], true, "{result}");

    let screenshot = screenshot.expect("the iPhone receives the screenshot file card");
    let shared = &screenshot["data"]["files"][0];
    assert_eq!(
        shared["name"],
        format!("browser-{}-1.png", &result["start"]["target"]["operationId"].as_str().unwrap()[..8])
    );
    assert_ne!(screenshot["id"], message_id, "the screenshot is its own card in the same chat");
    let stored = host.home.join("bots").join(&bot).join("files").join(shared["id"].as_str().unwrap());
    assert_eq!(tokio::fs::read(stored).await.unwrap(), png);
    assert_eq!(shared["sha256"], sha256_hex(&png));
    assert_eq!(final_reply.unwrap()["data"]["final"], true);
    let history = history(&host, &bot).await;
    let sent = history.iter().find(|entry| entry["id"] == message_id).unwrap();
    assert_eq!(sent["data"]["text"], "browser-happy http://127.0.0.1/");

    eprintln!(
        "PHONE_BROWSER_RECEIPT platform=ios host_id={} bot_id={} message_id={} operation_id={} browser_id={} tab_id={} generation={} screenshot_entry_id={} screenshot_name={} screenshot_sha256={}",
        result["start"]["target"]["hostId"].as_str().unwrap(),
        bot,
        message_id,
        result["start"]["target"]["operationId"].as_str().unwrap(),
        result["start"]["target"]["browserId"].as_str().unwrap(),
        result["start"]["target"]["tabId"].as_str().unwrap(),
        result["start"]["target"]["generation"].as_str().unwrap(),
        screenshot["id"].as_str().unwrap(),
        shared["name"].as_str().unwrap(),
        shared["sha256"].as_str().unwrap(),
    );
    extension.await.unwrap();
}

#[tokio::test]
#[cfg(windows)]
async fn concurrent_start_requests_share_one_tab() {
    if common::python_missing() {
        return;
    }
    let host = Host::start().await;
    let mut png = Vec::new();
    image::DynamicImage::new_rgba8(2, 2)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let requests = Arc::new(StdMutex::new(Vec::new()));
    let extension = fake_extension(&host, png, Duration::ZERO, requests.clone()).await;
    let bot = browser_bot(&host).await;

    let _ = send(&host, &bot, "browser-concurrent http://127.0.0.1/").await;
    let result = wait_json(&host.home, &results_path(&host, &bot), Duration::from_secs(45)).await;
    let starts = result["concurrentStarts"].as_array().unwrap();
    assert_eq!(starts.len(), 2, "{result}");
    assert_ne!(starts[0]["reused"], starts[1]["reused"], "{result}");
    assert_eq!(starts[0]["target"], starts[1]["target"], "{result}");

    extension.await.unwrap();
    let methods = requests
        .lock()
        .unwrap()
        .iter()
        .filter_map(|request| request["method"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    assert_eq!(methods.iter().filter(|method| method.as_str() == "start").count(), 1, "{methods:?}");
    assert_eq!(methods.iter().filter(|method| method.as_str() == "state").count(), 1, "{methods:?}");
}

#[tokio::test]
#[cfg(windows)]
async fn stopping_during_a_delayed_start_closes_the_late_tab() {
    if common::python_missing() {
        return;
    }
    let host = Host::start().await;
    let mut png = Vec::new();
    image::DynamicImage::new_rgba8(2, 2)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let requests = Arc::new(StdMutex::new(Vec::new()));
    let extension = fake_extension(&host, png, Duration::from_secs(3), requests.clone()).await;
    let bot = browser_bot(&host).await;

    let _ = send(&host, &bot, "browser-start-stop http://127.0.0.1/").await;
    let deadline = Instant::now() + Duration::from_secs(30);
    while !requests.lock().unwrap().iter().any(|request| request["method"] == "start") {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for the browser start to reach the extension; preserved evidence at {}",
            capture_failure(&host.home)
                .map_or_else(|error| format!("unavailable: {error}"), |path| path.display().to_string())
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    host.call("stop", json!({"botId": bot})).await;
    let result = wait_json(&host.home, &results_path(&host, &bot), Duration::from_secs(45)).await;
    assert!(result["startError"].as_str().unwrap().contains("cancelled"), "{result}");

    extension.await.unwrap();
    let methods = requests
        .lock()
        .unwrap()
        .iter()
        .filter_map(|request| request["method"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    assert_eq!(methods.iter().filter(|method| method.as_str() == "start").count(), 1, "{methods:?}");
    assert_eq!(methods.iter().filter(|method| method.as_str() == "stop").count(), 1, "{methods:?}");
}

#[tokio::test]
#[cfg(windows)]
async fn closed_extension_tabs_do_not_block_new_starts() {
    if common::python_missing() {
        return;
    }
    let host = Host::start().await;
    let mut png = Vec::new();
    image::DynamicImage::new_rgba8(2, 2)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let requests = Arc::new(StdMutex::new(Vec::new()));
    let extension = fake_extension(&host, png, Duration::ZERO, requests.clone()).await;
    let bot = browser_bot(&host).await;
    let url = format!("{}?closed-stop=1", page_url().await);

    let _ = send(&host, &bot, &format!("browser-closed-stop {url}")).await;
    let result = wait_json(&host.home, &results_path(&host, &bot), Duration::from_secs(45)).await;
    assert_eq!(result["closedStop"]["alreadyClosed"], true, "{result}");
    assert_eq!(result["restarted"]["reused"], false, "{result}");
    assert_ne!(result["restarted"]["target"]["browserId"], result["start"]["target"]["browserId"], "{result}");
    assert_eq!(result["finalStop"]["alreadyClosed"], false, "{result}");

    extension.await.unwrap();
    let methods = requests
        .lock()
        .unwrap()
        .iter()
        .filter_map(|request| request["method"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    assert_eq!(methods.iter().filter(|method| method.as_str() == "start").count(), 2, "{methods:?}");
    assert_eq!(methods.iter().filter(|method| method.as_str() == "stop").count(), 2, "{methods:?}");
}

#[tokio::test]
#[cfg(windows)]
async fn browser_directory_failure_closes_the_started_tab() {
    if common::python_missing() {
        return;
    }
    let host = Host::start().await;
    let mut png = Vec::new();
    image::DynamicImage::new_rgba8(2, 2)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let requests = Arc::new(StdMutex::new(Vec::new()));
    let extension = fake_extension(&host, png, Duration::ZERO, requests.clone()).await;
    let bot = browser_bot(&host).await;
    let browsers = host.home.join("browsers");
    std::fs::create_dir_all(&browsers).unwrap();
    std::fs::write(browsers.join(&bot), b"blocked").unwrap();
    let url = format!("{}?setup-fail=1", page_url().await);

    let _ = send(&host, &bot, &format!("browser-setup-fail {url}")).await;
    let result = wait_json(&host.home, &results_path(&host, &bot), Duration::from_secs(45)).await;
    assert!(
        result["startError"].as_str().unwrap().contains("creating the extension browser evidence directory"),
        "{result}"
    );

    extension.await.unwrap();
    let methods = requests
        .lock()
        .unwrap()
        .iter()
        .filter_map(|request| request["method"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    assert_eq!(methods.iter().filter(|method| method.as_str() == "start").count(), 1, "{methods:?}");
    assert_eq!(methods.iter().filter(|method| method.as_str() == "stop").count(), 1, "{methods:?}");
}

#[tokio::test]
#[cfg(windows)]
async fn missing_extension_fails_instead_of_switching_browser_or_computer() {
    if common::python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = browser_bot(&host).await;
    let _ = send(&host, &bot, "browser-happy http://127.0.0.1/").await;
    let path = host.home.join("bots").join(&bot).join("workspace").join("browser-error.json");
    let failure = wait_json(&host.home, &path, Duration::from_secs(30)).await;
    assert!(failure["message"].as_str().unwrap().contains("Codync Browser extension isn't connected"), "{failure}");
}

#[tokio::test]
#[cfg(windows)]
async fn queued_click_aborts_after_slow_action_on_phone_takeover() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::sync::{Mutex as AsyncMutex, Notify, mpsc};

    if common::python_missing() {
        return;
    }
    let host = Host::start().await;

    let holding_navigate = Arc::new(AtomicBool::new(false));
    let saw_queued_click = Arc::new(AtomicBool::new(false));
    let saw_takeover = Arc::new(AtomicBool::new(false));
    let revoked = Arc::new(AtomicBool::new(false));
    let page_clicks = Arc::new(AtomicUsize::new(0));
    let release_slow_navigate = Arc::new(Notify::new());

    let url = host.base.replacen("http://", "ws://", 1) + "/browser-extension";
    let mut request = url.into_client_request().unwrap();
    request.headers_mut().insert(header::AUTHORIZATION, format!("Bearer {}", host.token).parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    socket
        .send(Message::Text(
            json!({
                "type": "hello",
                "protocol": 1,
                "extensionId": "dgejialhpalhoigbkmgeabgpphlipiek",
                "version": "test",
                "isolatedProfile": true
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let ready = socket.next().await.unwrap().unwrap();
    assert!(ready.to_text().unwrap().contains("\"ready\""));

    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Value>();
    let (mut sink, mut stream) = socket.split();
    let writer = tokio::spawn(async move {
        while let Some(value) = out_rx.recv().await {
            if sink.send(Message::Text(value.to_string().into())).await.is_err() {
                break;
            }
        }
    });

    let extension = {
        let holding_navigate = holding_navigate.clone();
        let saw_queued_click = saw_queued_click.clone();
        let saw_takeover = saw_takeover.clone();
        let revoked = revoked.clone();
        let page_clicks = page_clicks.clone();
        let release_slow_navigate = release_slow_navigate.clone();
        let tab_queue = Arc::new(AsyncMutex::new(()));
        tokio::spawn(async move {
            let mut tasks = Vec::new();
            while let Some(Ok(message)) = stream.next().await {
                let frame: Value = match message {
                    Message::Text(text) => serde_json::from_str(text.as_str()).unwrap(),
                    Message::Binary(bytes) => serde_json::from_slice(&bytes).unwrap(),
                    Message::Close(_) => break,
                    Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
                };
                if matches!(frame["type"].as_str(), Some("takeover" | "revoke")) {
                    revoked.store(true, Ordering::SeqCst);
                    saw_takeover.store(true, Ordering::SeqCst);
                    continue;
                }
                let Some(id) = frame["id"].as_str().map(str::to_owned) else {
                    continue;
                };
                let method = frame["method"].as_str().unwrap_or_default().to_owned();
                if method == "cancel" {
                    if frame["params"]["revokeSession"] == true {
                        revoked.store(true, Ordering::SeqCst);
                    }
                    let _ = out_tx.send(json!({"id": id, "ok": true, "result": {"cancelled": true}}));
                    continue;
                }
                if method == "click" {
                    saw_queued_click.store(true, Ordering::SeqCst);
                }
                let out_tx = out_tx.clone();
                let tab_queue = tab_queue.clone();
                let holding_navigate = holding_navigate.clone();
                let revoked = revoked.clone();
                let page_clicks = page_clicks.clone();
                let release_slow_navigate = release_slow_navigate.clone();
                let is_stop = method == "stop";
                tasks.push(tokio::spawn(async move {
                    let state = json!({
                        "url": "http://127.0.0.1/",
                        "title": "Codync extension fixture",
                        "text": "Count: 0"
                    });
                    let reply = match method.as_str() {
                        "start" => json!({
                            "id": id,
                            "ok": true,
                            "result": {
                                "tabId": "123",
                                "browserVersion": "Chrome test",
                                "browserExecutable": "chrome-extension://fixture",
                                "extensionVersion": "0.1.0",
                                "state": state
                            }
                        }),
                        "navigate" => {
                            let _guard = tab_queue.lock().await;
                            holding_navigate.store(true, Ordering::SeqCst);
                            release_slow_navigate.notified().await;
                            json!({"id": id, "ok": true, "result": state})
                        }
                        "click" => {
                            let _guard = tab_queue.lock().await;
                            if revoked.load(Ordering::SeqCst) {
                                json!({
                                    "id": id,
                                    "ok": false,
                                    "code": "browser_session_closed",
                                    "error": "Browser operation is no longer running in Codync Browser"
                                })
                            } else {
                                page_clicks.fetch_add(1, Ordering::SeqCst);
                                json!({"id": id, "ok": true, "result": state})
                            }
                        }
                        "stop" => {
                            let _guard = tab_queue.lock().await;
                            json!({"id": id, "ok": true, "result": {"closed": true}})
                        }
                        other => panic!("unexpected queued-takeover method {other}"),
                    };
                    let _ = out_tx.send(reply);
                }));
                if is_stop {
                    break;
                }
            }
            for task in tasks {
                let _ = task.await;
            }
        })
    };

    // Pair an iPhone to issue a real phone takeover while the click is queued in the extension.
    let pairing = host.call("pairing", json!({})).await;
    let pairing_url = pairing["pairingUrl"].as_str().unwrap();
    let host_key: [u8; 32] = crypto::unb64_n(&phone::query_param(pairing_url, "sk")).unwrap();
    let device_key = SigningKey::from_bytes(&crypto::random());
    let mut pairing_phone = phone::Phone::connect(&host.base, &device_key, &host_key, true).await;
    let certificate = phone::test_certificate(&crypto::b64(device_key.verifying_key().as_bytes()), "browser-user");
    pairing_phone
        .send(json!({
            "id": 1,
            "m": "pair",
            "b": {
                "code": phone::query_param(pairing_url, "code"),
                "name": "Takeover Test iPhone",
                "platform": "ios",
                "certificate": certificate,
            }
        }))
        .await;
    let request = loop {
        let requests = host.call("pairingRequests", json!({})).await;
        if let Some(request) = requests["requests"].as_array().and_then(|items| items.first()) {
            break request.clone();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    host.call("decidePairing", json!({"requestId": request["requestId"], "approve": true})).await;
    assert_eq!(pairing_phone.inner().await["ok"]["computerId"], crypto::computer_id(&host_key));
    assert!(matches!(pairing_phone.recv().await, phone::Frame::Closed(4100)));

    let mut iphone = phone::Phone::connect(&host.base, &device_key, &host_key, false).await;
    let _ = iphone.call(1, "hello", json!({})).await;

    let bot = browser_bot(&host).await;
    let _ = send(&host, &bot, "browser-queued-takeover http://127.0.0.1/").await;

    let deadline = Instant::now() + Duration::from_secs(30);
    while !(holding_navigate.load(Ordering::SeqCst) && saw_queued_click.load(Ordering::SeqCst)) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for slow navigate and queued click; preserved evidence at {}",
            capture_failure(&host.home)
                .map_or_else(|error| format!("unavailable: {error}"), |path| path.display().to_string())
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    let takeover_reply = iphone.call(2, "screenTakeover", json!({"on": true})).await;
    assert_eq!(takeover_reply["userControl"], true, "{takeover_reply}");

    let takeover_deadline = Instant::now() + Duration::from_secs(10);
    while !saw_takeover.load(Ordering::SeqCst) {
        assert!(Instant::now() < takeover_deadline, "extension never received host takeover signal");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    release_slow_navigate.notify_waiters();

    let result = wait_json(&host.home, &results_path(&host, &bot), Duration::from_secs(45)).await;
    assert_eq!(result["queuedClickIsError"], true, "{result}");
    assert!(
        result["queuedClickText"].as_str().unwrap().contains("taken over the screen")
            || result["queuedClickText"].as_str().unwrap().contains("no longer running"),
        "{result}"
    );
    assert_eq!(result["stop"]["closed"], true, "{result}");
    assert_eq!(page_clicks.load(Ordering::SeqCst), 0, "queued click executed page input after phone takeover");

    extension.await.unwrap();
    writer.abort();
}

#[tokio::test]
#[cfg(windows)]
async fn timed_out_stop_frees_bot_quota_and_never_cancels_stop() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::{Notify, mpsc};

    if common::python_missing() {
        return;
    }
    let host = Host::start().await;
    let mut png = Vec::new();
    image::DynamicImage::new_rgba8(2, 2)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();

    let holding_navigate = Arc::new(AtomicUsize::new(0));
    let revoke_count = Arc::new(AtomicUsize::new(0));
    let frames = Arc::new(StdMutex::new(Vec::<Value>::new()));
    let release_navigate = Arc::new(Notify::new());

    let url = host.base.replacen("http://", "ws://", 1) + "/browser-extension";
    let mut request = url.into_client_request().unwrap();
    request.headers_mut().insert(header::AUTHORIZATION, format!("Bearer {}", host.token).parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    socket
        .send(Message::Text(
            json!({
                "type": "hello",
                "protocol": 1,
                "extensionId": "dgejialhpalhoigbkmgeabgpphlipiek",
                "version": "test",
                "isolatedProfile": true
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let ready = socket.next().await.unwrap().unwrap();
    assert!(ready.to_text().unwrap().contains("\"ready\""));

    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Value>();
    let (mut sink, mut stream) = socket.split();
    let writer = tokio::spawn(async move {
        while let Some(value) = out_rx.recv().await {
            if sink.send(Message::Text(value.to_string().into())).await.is_err() {
                break;
            }
        }
    });

    let extension = {
        let holding_navigate = holding_navigate.clone();
        let revoke_count = revoke_count.clone();
        let frames = frames.clone();
        let release_navigate = release_navigate.clone();
        let png = png.clone();
        tokio::spawn(async move {
            let mut tab_seq = 0usize;
            let mut stop_seq = 0usize;
            let mut tasks = Vec::new();
            while let Some(Ok(message)) = stream.next().await {
                let frame: Value = match message {
                    Message::Text(text) => serde_json::from_str(text.as_str()).unwrap(),
                    Message::Binary(bytes) => serde_json::from_slice(&bytes).unwrap(),
                    Message::Close(_) => break,
                    Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
                };
                frames.lock().unwrap().push(frame.clone());
                if frame["type"] == "revoke" {
                    revoke_count.fetch_add(1, Ordering::SeqCst);
                    release_navigate.notify_waiters();
                    continue;
                }
                let Some(id) = frame["id"].as_str().map(str::to_owned) else {
                    continue;
                };
                let method = frame["method"].as_str().unwrap_or_default().to_owned();
                let state = json!({
                    "url": "http://127.0.0.1/",
                    "title": "Codync extension fixture",
                    "text": if method == "click" { "Count: 1" } else { "name=Codync" }
                });
                match method.as_str() {
                    "start" => {
                        tab_seq += 1;
                        let _ = out_tx.send(json!({
                            "id": id,
                            "ok": true,
                            "result": {
                                "tabId": (200 + tab_seq).to_string(),
                                "browserVersion": "Chrome test",
                                "browserExecutable": "chrome-extension://fixture",
                                "extensionVersion": "0.1.0",
                                "state": state
                            }
                        }));
                    }
                    "navigate" => {
                        holding_navigate.fetch_add(1, Ordering::SeqCst);
                        let out_tx = out_tx.clone();
                        let release_navigate = release_navigate.clone();
                        tasks.push(tokio::spawn(async move {
                            release_navigate.notified().await;
                            let _ = out_tx.send(json!({
                                "id": id,
                                "ok": false,
                                "code": "browser_session_closed",
                                "error": "Browser operation is no longer running in Codync Browser"
                            }));
                        }));
                    }
                    "stop" => {
                        stop_seq += 1;
                        if stop_seq <= 2 {
                            // Deliberately withhold the reply past host `cancel_bot`'s 2s timeout.
                            continue;
                        }
                        let _ = out_tx.send(json!({"id": id, "ok": true, "result": {"closed": true}}));
                        break;
                    }
                    "state" | "click" | "type" => {
                        let _ = out_tx.send(json!({"id": id, "ok": true, "result": state}));
                    }
                    "screenshot" => {
                        let _ = out_tx.send(json!({
                            "id": id,
                            "ok": true,
                            "result": {
                                "data": base64::engine::general_purpose::STANDARD.encode(&png),
                                "state": state
                            }
                        }));
                    }
                    "cancel" => {
                        let _ = out_tx.send(json!({"id": id, "ok": true, "result": {"cancelled": true}}));
                    }
                    other => panic!("unexpected extension method {other}"),
                }
            }
            for task in tasks {
                let _ = task.await;
            }
        })
    };

    let bot = browser_bot(&host).await;
    for expected_turn in 1..=2 {
        let _ = send(&host, &bot, "browser-blocked-stop http://127.0.0.1/").await;
        let deadline = Instant::now() + Duration::from_secs(30);
        while holding_navigate.load(Ordering::SeqCst) < expected_turn {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for blocked navigate #{expected_turn}; preserved evidence at {}",
                capture_failure(&host.home)
                    .map_or_else(|error| format!("unavailable: {error}"), |path| path.display().to_string())
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        host.call("stop", json!({"botId": bot})).await;
        common::eventually("stopped bot returns to idle", Duration::from_secs(30), || async {
            let sync = host.snapshot().await;
            common::bot(&sync, &bot)["status"] == "idle"
        })
        .await;
        let _ = tokio::fs::remove_file(active_path(&host, &bot)).await;
        let _ = tokio::fs::remove_file(results_path(&host, &bot)).await;
    }

    // Two timed-out stops must not exhaust the 2-tab per-bot limit on the third turn.
    let happy_message_id = send(&host, &bot, "browser-happy http://127.0.0.1/").await;
    let happy = wait_json(&host.home, &results_path(&host, &bot), Duration::from_secs(45)).await;
    assert_eq!(happy["start"]["target"]["messageId"], happy_message_id, "{happy}");
    assert_eq!(happy["start"]["target"]["tabId"], "203", "{happy}");
    assert_eq!(happy["screenshot"]["postedToChat"], true, "{happy}");
    assert_eq!(happy["stop"]["closed"], true, "{happy}");

    extension.await.unwrap();
    writer.abort();

    let recorded = frames.lock().unwrap().clone();
    let stop_ids = recorded
        .iter()
        .filter(|frame| frame["method"] == "stop")
        .filter_map(|frame| frame["id"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    assert_eq!(stop_ids.len(), 3, "{recorded:?}");
    let cancelled_ids = recorded
        .iter()
        .filter(|frame| frame["method"] == "cancel")
        .filter_map(|frame| frame["params"]["requestId"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    assert!(
        stop_ids.iter().all(|id| !cancelled_ids.contains(id)),
        "timed-out stop must never be cancelled by host: stop_ids={stop_ids:?} cancelled_ids={cancelled_ids:?}"
    );
    assert!(revoke_count.load(Ordering::SeqCst) >= 2, "expected revoke frames after timed-out stops");
}
