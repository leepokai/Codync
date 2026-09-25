//! End to end: a real `codync-host serve` driving a scripted ACP agent
//! (`fake_agent.py`) through a full turn with an approval in the middle.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: failing loudly is the point

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
        let _ = self.child.kill();
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

async fn start_host() -> Host {
    let home = std::env::temp_dir().join(format!("codync-e2e-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&home).unwrap();
    let port = free_port();
    let child = Command::new(env!("CARGO_BIN_EXE_codync-host"))
        .args(["serve", "--bind", "127.0.0.1", "--port", &port.to_string()])
        .env("CODYNC_HOME", &home)
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
async fn turn_with_approval_reaches_a_final_reply() {
    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("skipping: python3 not available");
        return;
    }
    let host = start_host().await;
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fake_agent.py");
    let bot = host
        .call(
            "createBot",
            json!({
                "name": "Tester", "backend": "custom", "command": format!("python3 '{}'", agent.display()),
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
