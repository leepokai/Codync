//! A real host driving scripted ACP agents: Stop on an agent that ignores it, and room turns
//! that a newer group message replaced.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

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

impl Host {
    async fn start() -> Self {
        let home = std::env::temp_dir().join(format!("codync-turns-e2e-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        common::seed_memory_runtime(&home);
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let child = Command::new(env!("CARGO_BIN_EXE_codync-host"))
            .args(["serve", "--bind", "127.0.0.1", "--port", &port.to_string()])
            .env("CODYNC_HOME", &home)
            .env("CODYNC_CLOUD", "off")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let deadline = Instant::now() + Duration::from_secs(30);
        while reqwest::get(format!("{base}/health")).await.is_err() {
            assert!(Instant::now() < deadline, "host didn't start");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let token = std::fs::read_to_string(home.join("token")).unwrap().trim().to_owned();
        Self { child, base, token, home }
    }

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

    /// A bot running `fixture` in its own folder (`home/<name>`).
    async fn bot(&self, name: &str, fixture: &str) -> String {
        let cwd = self.home.join(name);
        std::fs::create_dir_all(&cwd).unwrap();
        let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fixture);
        let bot = json!({
            "name": name, "backend": "custom", "command": common::python_agent(&agent),
            "cwd": cwd, "permission": "ask", "notify": false,
        });
        self.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned()
    }

    async fn entries(&self, chat: &str) -> Vec<Value> {
        self.call("history", json!({"botId": chat})).await["entries"].as_array().unwrap().clone()
    }

    /// Polls `chat`'s history until `pred` matches an entry.
    async fn wait_for(&self, chat: &str, within: Duration, pred: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + within;
        loop {
            if let Some(e) = self.entries(chat).await.into_iter().rev().find(|e| pred(e)) {
                return e;
            }
            assert!(Instant::now() < deadline, "timed out; history: {:?}", self.entries(chat).await);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

fn python_missing() -> bool {
    let missing = Command::new(common::python()).arg("--version").output().is_err();
    if missing {
        eprintln!("skipping: Python not available");
    }
    missing
}

#[tokio::test]
async fn stop_ends_a_turn_whose_agent_ignores_the_cancel() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.bot("Stuck", "tests/fixtures/stubborn_agent.py").await;
    host.call("send", json!({"botId": bot, "text": "go"})).await;
    host.wait_for(&bot, Duration::from_secs(60), |e| e["data"]["text"] == "Working on it").await;
    host.call("stop", json!({"botId": bot})).await;
    // The agent never answers: after the grace period its process is stopped and the turn ends.
    host.wait_for(&bot, Duration::from_secs(40), |e| e["kind"] == "notice" && e["data"]["text"] == "Stopped.").await;
    let sync = host.call("sync", json!({"since": 0})).await;
    assert_eq!(sync["bots"][0]["status"], "idle");
}

#[tokio::test]
async fn a_member_busy_elsewhere_answers_only_the_latest_room_turn() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let member = host.bot("Member", "tests/fixtures/team_agent.py").await;
    let folder = host.home.join("Member");
    let group =
        host.call("createBot", json!({"name": "Room", "kind": "group", "members": [member]})).await["bot"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
    // Busy in its own chat until `release` exists.
    host.call("send", json!({"botId": member, "text": "BLOCK"})).await;
    let started = Instant::now();
    while !std::fs::read_to_string(folder.join("prompts.jsonl")).is_ok_and(|p| p.contains("BLOCK")) {
        assert!(started.elapsed() < Duration::from_secs(60), "the member never started");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    host.call("send", json!({"botId": group, "text": "first"})).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    host.call("send", json!({"botId": group, "text": "second"})).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    std::fs::write(folder.join("release"), "").unwrap();

    let reply =
        host.wait_for(&group, Duration::from_secs(30), |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    assert_eq!(reply["data"]["text"], "reply: group");
    tokio::time::sleep(Duration::from_secs(2)).await;
    // The room turn "second" replaced was skipped: no room prompt saw "first" without "second".
    let prompts = std::fs::read_to_string(folder.join("prompts.jsonl")).unwrap();
    let rooms: Vec<&str> = prompts.lines().filter(|p| p.contains("[Group chat")).collect();
    assert!(rooms.iter().any(|p| p.contains("User: second")), "{rooms:?}");
    assert!(!rooms.iter().any(|p| p.contains("User: first") && !p.contains("User: second")), "{rooms:?}");
    let finals = host.entries(&group).await.into_iter().filter(|e| e["kind"] == "agent" && e["data"]["final"] == true);
    assert_eq!(finals.count(), 1);
}
