//! A real host driving scripted ACP agents: the turn-end alert reaches the user only when the turn left
//! them something to read (#486) — a turn woken by a bot's reply that says nothing pushes nothing.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use futures::StreamExt;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
        let home = std::env::temp_dir().join(format!("codync-turn-alert-e2e-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let (child, base) = common::serve(&home).await;
        let token = std::fs::read_to_string(home.join("token")).unwrap().trim().to_owned();
        Self { child, base, token, home }
    }

    async fn call(&self, method: &str, body: Value) -> Value {
        let res = reqwest::Client::new()
            .post(format!("{}/api/{method}", self.base))
            .bearer_auth(&self.token)
            .json(&body)
            .timeout(Duration::from_mins(2))
            .send()
            .await
            .unwrap();
        let status = res.status();
        let v: Value = res.json().await.unwrap();
        assert!(status.is_success(), "{method} failed: {v}");
        v
    }

    /// A bot running the team fixture in its own folder, with its alerts on (the push under test).
    async fn bot(&self, name: &str) -> String {
        let cwd = self.home.join(name);
        std::fs::create_dir_all(&cwd).unwrap();
        let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
        let bot = json!({
            "name": name, "backend": "custom", "command": common::python_agent(&agent),
            "cwd": cwd, "permission": "ask",
        });
        self.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned()
    }

    async fn wait_for(&self, bot: &str, predicate: impl Fn(&Value) -> bool) -> Value {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let history = self.call("history", json!({"botId": bot})).await;
                if let Some(entry) = history["entries"].as_array().unwrap().iter().find(|e| predicate(e)) {
                    return entry.clone();
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("flow did not finish")
    }

    async fn wait_finished(&self) {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let sync = common::snapshot(&self.base, &self.token).await;
                let mut requests =
                    sync["entries"].as_array().unwrap().iter().filter(|e| e["data"]["delegationId"].is_string());
                if sync["bots"].as_array().unwrap().iter().all(|b| b["status"] == "idle")
                    && requests.all(|e| e["data"]["status"] == "completed")
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("flow did not release its turns and notices");
    }
}

/// The live `alert` events the host sent while the stream was open: the pushes a phone or desktop app shows.
fn alert_collector(base: String, token: String) -> Arc<Mutex<Vec<Value>>> {
    let alerts = Arc::new(Mutex::new(Vec::new()));
    let sink = alerts.clone();
    tokio::spawn(async move {
        let res = reqwest::Client::new()
            .get(format!("{base}/events?since=0&client=test"))
            .bearer_auth(token)
            .send()
            .await
            .expect("open the event stream");
        let mut body = res.bytes_stream();
        let mut buf = String::new();
        while let Some(Ok(chunk)) = body.next().await {
            buf.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(i) = buf.find('\n') {
                let line: String = buf.drain(..=i).collect();
                let Some(event) = line
                    .trim_end()
                    .strip_prefix("data:")
                    .and_then(|data| serde_json::from_str::<Value>(data.trim_start()).ok())
                else {
                    continue;
                };
                if event["type"] == "alert" {
                    sink.lock().unwrap().push(event["alert"].clone());
                }
            }
        }
    });
    alerts
}

/// The bodies of the alerts sent for `bot`, in order.
fn bodies(alerts: &Mutex<Vec<Value>>, bot: &str) -> Vec<String> {
    alerts
        .lock()
        .unwrap()
        .iter()
        .filter(|a| a["botId"] == bot)
        .map(|a| a["body"].as_str().unwrap_or_default().to_owned())
        .collect()
}

#[tokio::test]
async fn a_silent_turn_woken_by_a_bot_reply_does_not_alert_the_user() {
    let host = Host::start().await;
    let lead = host.bot("lead").await;
    let reviewer = host.bot("reviewer").await;
    host.call("updateBot", json!({"id": reviewer, "manageTeam": true})).await;
    let alerts = alert_collector(host.base.clone(), host.token.clone());

    // The lead delegates; its own turn answers the user ("Message queued"), which alerts once.
    host.call("send", json!({"botId": lead, "text": format!("DELEGATE {reviewer} BLOCK SILENT")})).await;
    host.wait_for(&lead, |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    // The reviewer holds its answer until the lead's alert is past the push throttle's 5 s, so a second
    // push for the lead would really go out were one sent.
    tokio::time::sleep(Duration::from_secs(6)).await;
    std::fs::write(host.home.join("reviewer/release"), "").unwrap();
    // The answer wakes the lead, whose turn answers with (pass): the user was sent nothing.
    host.wait_finished().await;
    let prompts: Vec<Value> = std::fs::read_to_string(host.home.join("lead/prompts.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(prompts.len(), 2, "the lead must have been woken by the reply");
    assert!(prompts[1].as_str().unwrap().contains("[Codync: reviewer replied to your message_bot request"));
    let history = host.call("history", json!({"botId": lead})).await;
    let finals: Vec<&str> = history["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "agent" && e["data"]["final"] == true)
        .map(|e| e["data"]["text"].as_str().unwrap())
        .collect();
    assert_eq!(finals, ["Message queued"], "the woken turn told the user nothing");

    // One push for the turn that answered the user; none for the silent one, none for the answering bot.
    assert_eq!(bodies(&alerts, &lead), ["Message queued"]);
    assert_eq!(bodies(&alerts, &reviewer), Vec::<String>::new());
}
