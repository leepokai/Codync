//! A real host and built-in MCP process, with deterministic ACP agents instead
//! of paid providers. Exercises discovery, messages, approval and the reply's way back.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Child;
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
        let (child, base) = common::serve(&home).await;
        let mut host = Self { child, home, base, token: String::new() };
        std::fs::read_to_string(host.home.join("token")).unwrap().trim().clone_into(&mut host.token);
        host
    }

    async fn call(&self, method: &str, body: Value) -> Value {
        let response = reqwest::Client::new()
            .post(format!("{}/api/{method}", self.base))
            .bearer_auth(&self.token)
            .json(&body)
            // As long as `common::host`'s: on a busy machine (parallel tests) `createBot` alone can take over 10 s.
            .timeout(Duration::from_mins(2))
            .send()
            .await
            .unwrap();
        let status = response.status();
        let value: Value = response.json().await.unwrap();
        assert!(status.is_success(), "{method}: {value}");
        value
    }

    async fn bot(&self, name: &str) -> String {
        let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
        let result = self
            .call(
                "createBot",
                json!({
                    "name": name, "backend": "custom", "permission": "ask", "notify": false,
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
                let sync = common::snapshot(&self.base, &self.token).await;
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
        std::fs::read_to_string(common::folder(&self.home, bot).join(file))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

fn finals(history: &Value) -> Vec<&str> {
    history["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "agent" && e["data"]["final"] == true)
        .map(|e| e["data"]["text"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn a_bot_messages_another_and_relays_its_reply_to_the_user() {
    let host = Host::start().await;
    let lead = host.bot("lead").await;
    let reviewer = host.bot("reviewer").await;
    // Answers in its chat, not in its box (`work_e2e` has that): these are about the chat's own queue and trace.
    host.call("updateBot", json!({"id": reviewer, "manageTeam": true})).await;
    host.call("send", json!({"botId": lead, "text": format!("DELEGATE {reviewer}")})).await;
    let permission = host.wait_for(&reviewer, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await;
    // The lead didn't wait: its turn ended as soon as the request was queued.
    let queued = host.wait_for(&lead, |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    assert_eq!(queued["data"]["text"], "Message queued");
    // Stopping the sender leaves the recipient's approval and the accepted request alone.
    host.call("stop", json!({"botId": lead})).await;
    host.call("respondPermission", json!({"entryId": permission["id"], "optionId": "allow"})).await;
    let relayed = host
        .wait_for(&lead, |e| {
            e["kind"] == "agent" && e["data"]["final"] == true && e["data"]["text"] != "Message queued"
        })
        .await;
    assert_eq!(relayed["data"]["text"], "Relayed: reply: PERMISSION review the changes");
    let sync = host.wait_finished().await;
    let requests: Vec<_> =
        sync["entries"].as_array().unwrap().iter().filter(|e| e["data"]["delegationId"].is_string()).collect();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["data"]["delegationId"], requests[1]["data"]["delegationId"]);
    for entry in &requests {
        assert_eq!(entry["kind"], "notice");
        assert_eq!(entry["data"]["sourceBotId"], lead);
        assert_eq!(entry["data"]["targetBotId"], reviewer);
        assert_eq!(entry["data"]["status"], "completed");
        assert_eq!(entry["data"]["botMessage"]["reply"], "reply: PERMISSION review the changes");
    }
    let conversation = host.call("botConversation", json!({"botId": lead, "peerId": reviewer})).await;
    assert_eq!(conversation["entries"].as_array().unwrap().len(), 1);
    let recipient_history = host.call("history", json!({"botId": reviewer})).await;
    assert_eq!(finals(&recipient_history), Vec::<&str>::new(), "the answer is the lead's, not the reviewer's chat's");
    assert!(recipient_history["entries"].as_array().unwrap().iter().all(|e| e["kind"] != "user"));
    assert!(
        recipient_history["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "agent" && e["data"]["text"] == "reply: PERMISSION review the changes"),
        "the reply remains available in the trace"
    );
    let lead_history = host.call("history", json!({"botId": lead})).await;
    assert_eq!(finals(&lead_history), ["Message queued", "Relayed: reply: PERMISSION review the changes"]);
    let prompts = host.fixture_log("lead", "prompts.jsonl");
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1].as_str().unwrap().contains("[Codync: reviewer replied to your message_bot request"));
}

#[tokio::test]
async fn a_bot_request_answers_the_sender_and_never_the_user() {
    let host = Host::start().await;
    let lead = host.bot("lead").await;
    let reviewer = host.bot("reviewer").await;
    // Answers in its chat, not in its box (`work_e2e` has that): these are about the chat's own queue and trace.
    host.call("updateBot", json!({"id": reviewer, "manageTeam": true})).await;
    host.call("send", json!({"botId": lead, "text": format!("DELEGATE_REPORT {reviewer}")})).await;
    host.wait_for(&lead, |e| e["kind"] == "agent" && e["data"]["text"] == "Relayed: Answer for the requesting bot")
        .await;
    host.wait_finished().await;
    for bot in [&lead, &reviewer] {
        let history = host.call("history", json!({"botId": bot})).await;
        let notices: Vec<_> = history["entries"].as_array().unwrap().iter().filter(|e| e["kind"] == "notice").collect();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0]["data"]["status"], "completed");
        assert!(notices[0]["data"]["text"].as_str().unwrap().contains("Answer for the requesting bot"));
        assert!(!history["entries"].as_array().unwrap().iter().any(|e| e["data"]["text"] == "Report for the user"));
        if bot == &reviewer {
            assert_eq!(finals(&history), Vec::<&str>::new());
        }
    }
}

#[tokio::test]
async fn user_report_queued_behind_a_request_is_delivered_in_the_recipient_chat() {
    let host = Host::start().await;
    let lead = host.bot("lead").await;
    let reviewer = host.bot("reviewer").await;
    // Answers in its chat, not in its box (`work_e2e` has that): these are about the chat's own queue and trace.
    host.call("updateBot", json!({"id": reviewer, "manageTeam": true})).await;
    host.call("send", json!({"botId": lead, "text": format!("DELEGATE {reviewer}")})).await;
    let permission = host.wait_for(&reviewer, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await;
    host.call("send", json!({"botId": reviewer, "text": "REPORT"})).await;
    host.call("respondPermission", json!({"entryId": permission["id"], "optionId": "allow"})).await;
    host.wait_for(&reviewer, |e| {
        e["kind"] == "agent" && e["data"]["final"] == true && e["data"]["text"] == "Second report for the user"
    })
    .await;
    host.wait_for(&lead, |e| e["data"]["text"] == "Relayed: reply: PERMISSION review the changes").await;
    let sync = host.wait_finished().await;
    // The request's limits were its turn's: the user's own turn after it reports as usual.
    let entries: Vec<_> = sync["entries"].as_array().unwrap().iter().filter(|e| e["botId"] == reviewer).collect();
    let reports: Vec<_> = entries
        .iter()
        .filter(|e| e["kind"] == "agent" && e["data"]["final"] == true)
        .map(|e| e["data"]["text"].as_str().unwrap())
        .collect();
    assert_eq!(reports, ["Report for the user", "Second report for the user"]);
    assert!(entries.iter().any(|e| {
        e["kind"] == "agent" && e["data"]["final"] != true && e["data"]["text"] == "Private completion trace"
    }));
    let prompts = host.fixture_log("reviewer", "prompts.jsonl");
    assert_eq!(prompts.len(), 2);
    assert!(prompts[0].as_str().unwrap().contains("Your final reply goes back to lead"));
    assert_eq!(prompts[1], "REPORT");
    assert_eq!(host.fixture_log("reviewer", "sessions.jsonl").len(), 1, "the existing session must be reused");
}

#[tokio::test]
async fn message_ping_pong_stops_at_the_hop_limit_and_a_new_user_turn_can_delegate() {
    let host = Host::start().await;
    let first = host.bot("first").await;
    let second = host.bot("second").await;
    for (name, target) in [("first", &second), ("second", &first)] {
        std::fs::write(common::folder(&host.home, name).join("handoff.json"), json!({"target": target}).to_string())
            .unwrap();
    }
    let mut previous_rev = 0;
    // `first` passes the chain on from each reply turn and `second` hands it back, so only the hops each reply
    // carries stop it. The second user turn must start a fresh budget after the first chain ends.
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
        assert_eq!(completed["data"]["final"], true, "a reply turn answers the user");
        let notices = entries.iter().filter(|e| e["data"]["delegationId"].is_string()).count();
        assert_eq!(notices, chain * 8 * 2, "the ninth handoff must never be queued");
        previous_rev = sync["rev"].as_i64().unwrap();
    }
    // Each stopped chain left the user one Continue notice, which sends `first` "Continue" as the user's message.
    let sync = common::snapshot(&host.base, &host.token).await;
    let entries = sync["entries"].as_array().unwrap();
    let continues: Vec<_> = entries.iter().filter(|e| e["data"]["continueChain"] == true).collect();
    assert_eq!(continues.len(), 2, "one notice a stopped chain");
    let sent = host.call("continueChain", json!({"entryId": continues[1]["id"]})).await;
    assert_eq!(sent["entry"]["kind"], "user");
    assert_eq!(sent["entry"]["data"]["text"], "Continue");
    host.wait_finished().await;
}

#[tokio::test]
async fn completion_identifies_the_sent_reply_and_is_empty_for_the_adjacent_request() {
    if common::python_missing() {
        return;
    }
    let host = common::host::Host::start().await;
    let recipient = host.fixture_bot("Recipient", "team_agent.py", "").await;
    host.works_in_chat(&recipient).await;
    let sender = host.fixture_bot("Sender", "team_agent.py", "").await;
    let mut events = host.events().await;

    host.reply(&recipient, "REPORT").await;
    common::eventually("the own-chat turn finished", Duration::from_secs(20), async || {
        common::bot(&host.snapshot().await, &recipient)["status"] == "idle"
    })
    .await;
    let snapshot = host.snapshot().await;
    let sent = snapshot["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["data"]["text"] == "Second report for the user")
        .unwrap();
    assert_eq!(common::bot(&snapshot, &recipient)["completionReplyId"], sent["id"]);

    host.call("send", json!({"botId": sender, "text": format!("DELEGATE {recipient} BLOCK")})).await;
    let working = tokio::time::timeout(Duration::from_secs(10), async {
        let mut own_completed = false;
        loop {
            let event = events.recv().await.unwrap();
            let bot = &event["bot"];
            if bot["id"] != recipient {
                continue;
            }
            if bot["status"] == "idle" && bot["completionReplyId"] == sent["id"] {
                own_completed = true;
            }
            if own_completed && bot["status"] == "working" {
                break bot.clone();
            }
        }
    })
    .await
    .expect("live snapshots show own-chat completion followed by delegated work");
    assert_eq!(working.get("completionReplyId"), Some(&Value::Null));

    let reacted = host.call("react", json!({"entryId": sent["id"], "emoji": "👍"})).await;
    assert_eq!(reacted["entry"]["id"], sent["id"]);
    let cwd = PathBuf::from(working["cwd"].as_str().unwrap());
    std::fs::write(cwd.join("release"), "").unwrap();
    common::eventually("the request finished and its reply was relayed", Duration::from_secs(20), async || {
        let snapshot = host.snapshot().await;
        common::bot(&snapshot, &recipient)["status"] == "idle"
            && snapshot["entries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["botId"] == sender && e["data"]["text"].as_str().is_some_and(|t| t.starts_with("Relayed:")))
    })
    .await;

    let snapshot = host.snapshot().await;
    let idle = common::bot(&snapshot, &recipient);
    assert_eq!(idle["status"], "idle");
    assert_eq!(idle["lastMessage"], "Second report for the user");
    assert!(idle["lastAt"].as_i64().unwrap() >= working["startedAt"].as_i64().unwrap());
    assert_eq!(idle.get("completionReplyId"), Some(&Value::Null));
}

#[tokio::test]
async fn delegated_request_emits_no_stale_alert_or_final_reply_for_recipient() {
    use futures::StreamExt as _;

    let host = Host::start().await;
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
    let make_bot = |name: &str, manage_team: bool| {
        let cwd = host.home.join(name);
        std::fs::create_dir_all(&cwd).unwrap();
        let body = json!({
            "name": name, "backend": "custom", "cwd": cwd, "permission": "auto", "notify": true,
            "manageTeam": manage_team, "command": common::python_agent(&agent),
        });
        async { host.call("createBot", body).await["bot"]["id"].as_str().unwrap().to_owned() }
    };
    // manageTeam keeps bot-to-bot requests on Bot A's own actor (`!boxed`), where `status` flips working → idle.
    let bot_a = make_bot("bot-a", true).await;
    let bot_b = make_bot("bot-b", false).await;

    // 1. User chats with Bot A so its own chat has an earlier reply "X".
    host.call("send", json!({"botId": bot_a, "text": "hello first", "clientNonce": "x1"})).await;
    let first_reply = host.wait_for(&bot_a, |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    host.wait_finished().await;
    let old_text = first_reply["data"]["text"].as_str().unwrap().to_owned();

    // 2. Listen to live desktop events while Bot B delegates a `message_bot` turn to Bot A.
    let (alert_tx, mut alert_rx) = tokio::sync::mpsc::unbounded_channel();
    let stream_url = format!("{}/events?since=999999&client=desktop", host.base);
    let token = host.token.clone();
    let reader = tokio::spawn(async move {
        let res = reqwest::Client::new().get(stream_url).bearer_auth(token).send().await.unwrap();
        let mut body = res.bytes_stream();
        let mut buf = String::new();
        while let Some(Ok(chunk)) = body.next().await {
            buf.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(i) = buf.find('\n') {
                let line: String = buf.drain(..=i).collect();
                if let Some(data) = line.trim_end().strip_prefix("data:")
                    && let Ok(v) = serde_json::from_str::<Value>(data.trim_start())
                    && v["type"] == "alert"
                {
                    let _ = alert_tx.send(v["alert"].clone());
                }
            }
        }
    });
    tokio::time::sleep(Duration::from_millis(150)).await;

    host.call("send", json!({"botId": bot_b, "text": format!("DELEGATE_REPORT {bot_a}"), "clientNonce": "d1"})).await;
    host.wait_for(&bot_b, |e| {
        e["kind"] == "agent"
            && e["data"]["final"] == true
            && e["data"]["text"].as_str().is_some_and(|t| t.starts_with("Relayed:"))
    })
    .await;
    host.wait_finished().await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    reader.abort();

    let mut alerts = Vec::new();
    while let Ok(a) = alert_rx.try_recv() {
        alerts.push(a);
    }
    assert!(
        alerts.iter().all(|a| a["botId"] != bot_a),
        "recipient Bot A must not emit a done/stale alert for a delegated message_bot turn: {alerts:?}"
    );
    let history_a = host.call("history", json!({"botId": bot_a})).await;
    assert_eq!(
        finals(&history_a),
        vec![old_text.as_str()],
        "Bot A's own chat must keep only its prior reply so clients know no new own-chat reply landed"
    );
}
