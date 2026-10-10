//! A real host and built-in MCP process, with deterministic ACP agents instead
//! of paid providers. Exercises discovery, delegation, approval and reply sync.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

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
        common::stop(&mut self.child);
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

impl Host {
    async fn start() -> Self {
        let home = std::env::temp_dir().join(format!("codync-team-e2e-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        common::seed_memory_runtime(&home);
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let child = Command::new(env!("CARGO_BIN_EXE_codync-host"))
            .args(["serve", "--bind", "127.0.0.1", "--port", &port.to_string()])
            .env("CODYNC_CLOUD", "off")
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
        let result = self
            .call(
                "createBot",
                json!({
                    "name": name, "backend": "custom", "cwd": cwd, "permission": "ask", "notify": false,
                    "command": common::python_agent(&agent),
                }),
            )
            .await;
        result["bot"]["id"].as_str().unwrap().to_owned()
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

    async fn wait_finished(&self) -> Value {
        let completed = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let sync = self.call("sync", json!({"since": 0})).await;
                let mut notices =
                    sync["entries"].as_array().unwrap().iter().filter(|e| e["data"]["delegationId"].is_string());
                if sync["bots"].as_array().unwrap().iter().all(|b| b["status"] == "idle")
                    && notices.all(|e| e["data"]["status"] == "completed")
                {
                    return sync;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await;
        completed.expect("team flow did not release its turns and notices")
    }

    fn fixture_log(&self, bot: &str, file: &str) -> Vec<Value> {
        std::fs::read_to_string(self.home.join(bot).join(file))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
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
    assert!(requests.iter().all(|e| e["data"]["botMessage"]["reply"] == "reply: PERMISSION review the changes"));
    let conversation = host.call("botConversation", json!({"botId": lead, "peerId": reviewer})).await;
    assert_eq!(conversation["entries"].as_array().unwrap().len(), 1);
    let recipient_history = host.call("history", json!({"botId": reviewer})).await;
    assert!(!recipient_history["entries"].as_array().unwrap().iter().any(|e| e["data"]["final"] == true));
    assert!(
        recipient_history["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| { e["kind"] == "agent" && e["data"]["text"] == "reply: PERMISSION review the changes" }),
        "the reply remains available in the trace"
    );
    assert!(sync["bots"].as_array().unwrap().iter().all(|b| b["status"] == "idle"));
}

#[tokio::test]
async fn agent_messages_another_bot_then_finishes_before_recipient_approval() {
    let host = Host::start().await;
    let lead = host.bot("lead").await;
    let reviewer = host.bot("reviewer").await;
    host.call("send", json!({"botId": lead, "text": format!("MESSAGE {reviewer}")})).await;
    let permission = host.wait_for(&reviewer, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await;
    let final_reply = host.wait_for(&lead, |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    assert_eq!(final_reply["data"]["text"], "Message queued");
    let history = host.call("history", json!({"botId": reviewer})).await;
    assert!(!history["entries"].as_array().unwrap().iter().any(|e| e["data"]["final"] == true));
    assert!(history["entries"].as_array().unwrap().iter().all(|e| e["kind"] != "user"));
    // The sender's turn and MCP subprocess have finished; Stop must still leave
    // the recipient's approval and admitted message intact.
    host.call("stop", json!({"botId": lead})).await;
    host.call("respondPermission", json!({"entryId": permission["id"], "optionId": "allow"})).await;
    let report = host.wait_for(&reviewer, |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    assert_eq!(report["data"]["text"], "reply: PERMISSION review the changes");
    host.wait_for(&reviewer, |e| e["kind"] == "notice" && e["data"]["status"] == "completed").await;
    let sync = host.call("sync", json!({"since": 0})).await;
    let notices: Vec<_> =
        sync["entries"].as_array().unwrap().iter().filter(|e| e["data"]["delegationId"].is_string()).collect();
    assert_eq!(notices.len(), 2);
    for entry in &notices {
        assert_eq!(entry["kind"], "notice");
        assert_eq!(entry["data"]["sourceBotId"], lead);
        assert_eq!(entry["data"]["targetBotId"], reviewer);
        assert_eq!(entry["data"]["status"], "completed");
        assert!(!entry["data"]["text"].as_str().unwrap().contains("reply: PERMISSION"));
    }
    assert_eq!(notices[0]["data"]["delegationId"], notices[1]["data"]["delegationId"]);
    let history = host.call("history", json!({"botId": lead})).await;
    assert_eq!(history["entries"].as_array().unwrap().iter().filter(|e| e["data"]["final"] == true).count(), 1);
}

#[tokio::test]
async fn ask_returns_only_the_bot_answer_and_does_not_publish_a_user_report() {
    let host = Host::start().await;
    let lead = host.bot("lead").await;
    let reviewer = host.bot("reviewer").await;
    host.call("send", json!({"botId": lead, "text": format!("ASK_REPORT {reviewer}")})).await;
    let answer = host.wait_for(&lead, |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    assert_eq!(answer["data"]["text"], "Team reply: Answer for the requesting bot");
    for bot in [&lead, &reviewer] {
        let history = host.call("history", json!({"botId": bot})).await;
        let notices: Vec<_> = history["entries"].as_array().unwrap().iter().filter(|e| e["kind"] == "notice").collect();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0]["data"]["status"], "completed");
        assert!(notices[0]["data"]["text"].as_str().unwrap().contains("Answer for the requesting bot"));
        assert!(!history["entries"].as_array().unwrap().iter().any(|e| e["data"]["text"] == "Report for the user"));
        if bot == &reviewer {
            assert!(
                history["entries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|e| e["kind"] == "agent")
                    .all(|e| e["data"]["final"] != true)
            );
        }
    }
}

#[tokio::test]
async fn independent_message_reports_to_the_user_without_a_bot_reply_or_duplicate_fallback() {
    let host = Host::start().await;
    let lead = host.bot("lead").await;
    let reviewer = host.bot("reviewer").await;
    host.call("send", json!({"botId": lead, "text": format!("MESSAGE_REPORT {reviewer}")})).await;
    host.wait_for(&reviewer, |e| e["kind"] == "notice" && e["data"]["status"] == "completed").await;
    let history = host.call("history", json!({"botId": reviewer})).await;
    let reports: Vec<_> = history["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "agent" && e["data"]["final"] == true)
        .map(|e| e["data"]["text"].as_str().unwrap())
        .collect();
    assert_eq!(reports, ["Report for the user", "Second report for the user"]);
    assert!(history["entries"].as_array().unwrap().iter().any(|e| {
        e["kind"] == "agent" && e["data"]["final"] != true && e["data"]["text"] == "Private completion trace"
    }));
    for bot in [&lead, &reviewer] {
        let history = host.call("history", json!({"botId": bot})).await;
        let notices: Vec<_> = history["entries"].as_array().unwrap().iter().filter(|e| e["kind"] == "notice").collect();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0]["data"]["status"], "completed");
        let text = notices[0]["data"]["text"].as_str().unwrap();
        assert!(!text.contains("Report for the user"));
        assert!(!text.contains("Private completion trace"));
    }
}

fn assert_recipient_reports(sync: &Value, recipient: &str) {
    let entries: Vec<_> = sync["entries"].as_array().unwrap().iter().filter(|e| e["botId"] == recipient).collect();
    let reports: Vec<_> = entries
        .iter()
        .filter(|e| e["kind"] == "agent" && e["data"]["final"] == true)
        .map(|e| e["data"]["text"].as_str().unwrap())
        .collect();
    assert_eq!(reports, ["Report for the user", "Second report for the user"]);
    assert!(entries.iter().any(|e| e["kind"] == "agent"
        && e["data"]["final"] != true
        && e["data"]["text"] == "reply: PERMISSION review the changes"));
    assert!(entries.iter().any(|e| {
        e["kind"] == "agent" && e["data"]["final"] != true && e["data"]["text"] == "Private completion trace"
    }));
}

#[tokio::test]
async fn user_report_queued_behind_an_ask_is_delivered_in_the_recipient_chat() {
    let host = Host::start().await;
    let lead = host.bot("lead").await;
    let reviewer = host.bot("reviewer").await;
    host.call("send", json!({"botId": lead, "text": format!("DELEGATE {reviewer}")})).await;
    let permission = host.wait_for(&reviewer, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await;
    host.call("send", json!({"botId": reviewer, "text": "REPORT"})).await;
    host.call("respondPermission", json!({"entryId": permission["id"], "optionId": "allow"})).await;
    host.wait_for(&reviewer, |e| {
        e["kind"] == "agent" && e["data"]["final"] == true && e["data"]["text"] == "Second report for the user"
    })
    .await;
    let sync = host.wait_finished().await;
    assert_recipient_reports(&sync, &reviewer);
    assert!(sync["entries"].as_array().unwrap().iter().any(|e| {
        e["botId"] == lead
            && e["data"]["final"] == true
            && e["data"]["text"] == "Team reply: reply: PERMISSION review the changes"
    }));
    let prompts = host.fixture_log("reviewer", "prompts.jsonl");
    assert_eq!(prompts.len(), 2);
    assert_eq!(prompts[1], "REPORT");
}

#[tokio::test]
async fn independent_report_queued_behind_an_ask_gets_guidance_in_the_reused_session() {
    let host = Host::start().await;
    let lead = host.bot("lead").await;
    let reviewer = host.bot("reviewer").await;
    let messenger = host.bot("messenger").await;
    host.call("send", json!({"botId": lead, "text": format!("DELEGATE {reviewer}")})).await;
    let permission = host.wait_for(&reviewer, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await;
    host.call("send", json!({"botId": messenger, "text": format!("MESSAGE_REPORT {reviewer}")})).await;
    host.wait_for(&reviewer, |e| {
        e["kind"] == "notice" && e["data"]["sourceBotId"] == messenger && e["data"]["status"] == "queued"
    })
    .await;
    host.wait_for(&messenger, |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    host.call("respondPermission", json!({"entryId": permission["id"], "optionId": "allow"})).await;
    host.wait_for(&reviewer, |e| {
        e["kind"] == "agent" && e["data"]["final"] == true && e["data"]["text"] == "Second report for the user"
    })
    .await;
    let sync = host.wait_finished().await;
    assert_recipient_reports(&sync, &reviewer);
    let prompts = host.fixture_log("reviewer", "prompts.jsonl");
    assert_eq!(prompts.len(), 2);
    assert!(prompts[0].as_str().unwrap().contains("Do not use send_message or message_bot"));
    assert!(prompts[1].as_str().unwrap().contains("send_message is available for this independent request"));
    assert!(prompts[1].as_str().unwrap().contains("even if an earlier ask or older session instructions"));
    assert_eq!(host.fixture_log("reviewer", "sessions.jsonl").len(), 1, "the existing session must be reused");
}

async fn exercise_bounded_chain(second_tool: &str) {
    let host = Host::start().await;
    let first = host.bot("first").await;
    let second = host.bot("second").await;
    for (name, target, tool) in [("first", &second, "message_bot"), ("second", &first, second_tool)] {
        std::fs::write(host.home.join(name).join("handoff.json"), json!({"target": target, "tool": tool}).to_string())
            .unwrap();
    }
    let mut previous_rev = 0;
    // The second user turn must start a fresh budget after the first chain ends.
    for chain in 1..=2 {
        host.call("send", json!({"botId": first, "text": "CHAIN"})).await;
        let stopped = host
            .wait_for(&first, |e| {
                e["rev"].as_i64().unwrap_or(0) > previous_rev
                    && e["kind"] == "agent"
                    && e["data"]["text"].as_str().is_some_and(|s| s.contains("hop limit (8)"))
            })
            .await;
        assert!(stopped["data"]["text"].as_str().unwrap().contains("Chain stopped"));
        let sync = host.wait_finished().await;
        // The streamed text can precede finish_turn. Read its final state only
        // after both bots and all request notices have completed.
        let entries = sync["entries"].as_array().unwrap();
        let completed = entries.iter().find(|e| e["id"] == stopped["id"]).expect("completed stop entry");
        assert_eq!(completed["data"]["final"] == true, second_tool == "message_bot");
        let notices = entries.iter().filter(|e| e["data"]["delegationId"].is_string()).count();
        assert_eq!(notices, chain * 8 * 2, "the ninth handoff must never be queued");
        previous_rev = sync["rev"].as_i64().unwrap();
    }
}

#[tokio::test]
async fn independent_message_ping_pong_stops_and_a_new_user_turn_can_delegate() {
    exercise_bounded_chain("message_bot").await;
}

#[tokio::test]
async fn asking_between_independent_handoffs_cannot_reset_the_hop_limit() {
    exercise_bounded_chain("ask_bot").await;
}
