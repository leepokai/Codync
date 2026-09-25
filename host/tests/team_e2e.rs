//! A real host and built-in MCP process, with deterministic ACP agents instead
//! of paid providers. Exercises discovery, delegation, approval and reply sync.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

struct Host {
    child: Child,
    home: PathBuf,
    base: String,
    token: String,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

impl Host {
    async fn start() -> Self {
        let home = std::env::temp_dir().join(format!("codync-team-e2e-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let child = Command::new(env!("CARGO_BIN_EXE_codync-host"))
            .args(["serve", "--bind", "127.0.0.1", "--port", &port.to_string()])
            .env("CODYNC_HOME", &home)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut host = Self { child, home, base: format!("http://127.0.0.1:{port}"), token: String::new() };
        tokio::time::timeout(Duration::from_secs(20), async {
            while reqwest::get(format!("{}/health", host.base)).await.is_err() {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();
        std::fs::read_to_string(host.home.join("token")).unwrap().trim().clone_into(&mut host.token);
        host
    }

    async fn call(&self, method: &str, body: Value) -> Value {
        let response = reqwest::Client::new()
            .post(format!("{}/api/{method}", self.base))
            .bearer_auth(&self.token)
            .json(&body)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .unwrap();
        let status = response.status();
        let value: Value = response.json().await.unwrap();
        assert!(status.is_success(), "{method}: {value}");
        value
    }

    async fn bot(&self, name: &str) -> String {
        let cwd = self.home.join(name);
        std::fs::create_dir_all(&cwd).unwrap();
        let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
        self.call(
            "createBot",
            json!({
                "name": name, "backend": "custom", "cwd": cwd, "permission": "ask", "notify": false,
                "command": format!("python3 -u '{}'", agent.display().to_string().replace('\'', "'\\''")),
            }),
        )
        .await["bot"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
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
        .expect("team flow did not finish")
    }
}

#[tokio::test]
async fn agent_uses_team_mcp_and_returns_the_reviewers_answer() {
    let host = Host::start().await;
    let lead = host.bot("lead").await;
    let reviewer = host.bot("reviewer").await;
    host.call("send", json!({"botId": lead, "text": format!("DELEGATE {reviewer}")})).await;
    let permission = host.wait_for(&reviewer, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await;
    let lead_history = host.call("history", json!({"botId": lead})).await;
    assert!(!lead_history["entries"].as_array().unwrap().iter().any(|e| e["data"]["final"] == true));
    host.call("respondPermission", json!({"entryId": permission["id"], "optionId": "allow"})).await;
    let final_reply = host.wait_for(&lead, |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    assert_eq!(final_reply["data"]["text"], "Team reply: reply: PERMISSION review the changes");
    let sync = host.call("sync", json!({"since": 0})).await;
    let requests: Vec<_> =
        sync["entries"].as_array().unwrap().iter().filter(|e| e["data"]["delegationId"].is_string()).collect();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|e| e["data"]["status"] == "completed"));
    assert_eq!(requests[0]["data"]["delegationId"], requests[1]["data"]["delegationId"]);
    assert!(sync["bots"].as_array().unwrap().iter().all(|b| b["status"] == "idle"));
}
