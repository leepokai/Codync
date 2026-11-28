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
    // The host auto-creates a lead with no runnable agent. Its hop-limit notice would never finish, and a deleted
    // lead is not made again.
    let (lead, _) = auto_lead(&host).await;
    host.call("deleteBot", json!({"botId": lead})).await;
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

/// The host's auto-made lead, once it exists: `(id, name)`.
async fn auto_lead(host: &Host) -> (String, String) {
    let found = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let sync = common::snapshot(&host.base, &host.token).await;
            if let Some(lead) =
                sync["bots"].as_array().unwrap().iter().find(|bot| bot["lead"] == true && bot["deleted"] != true)
            {
                return (lead["id"].as_str().unwrap().to_owned(), lead["name"].as_str().unwrap().to_owned());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    found.expect("the host did not make its lead")
}

fn refusal_in(text: &str) -> u64 {
    let rest = text.split("refusal ").nth(1).unwrap_or_else(|| panic!("no refusal id in {text}"));
    rest.chars().take_while(char::is_ascii_digit).collect::<String>().parse().expect("refusal id")
}

#[tokio::test]
async fn a_hop_limit_notice_reaches_the_lead_and_a_reset_reopens_the_chain() {
    let host = Host::start().await;
    host.call("setHopReview", json!({"enabled": true})).await;
    let (lead, lead_name) = auto_lead(&host).await;
    let cwd = host.home.join(&lead_name);
    std::fs::create_dir_all(&cwd).unwrap();
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
    let updated = host
        .call(
            "updateBot",
            json!({
                "id": lead,
                "backend": "custom",
                "cwd": cwd,
                "permission": "ask",
                "notify": false,
                "manageTeam": true,
                "command": common::python_agent(&agent),
            }),
        )
        .await;
    assert!(updated["bot"]["command"].as_str().unwrap().contains("team_agent.py"));

    let first = host.bot("first").await;
    let second = host.bot("second").await;
    for (name, target) in [("first", &second), ("second", &first)] {
        std::fs::write(common::folder(&host.home, name).join("handoff.json"), json!({"target": target}).to_string())
            .unwrap();
    }
    host.call("send", json!({"botId": first, "text": "CHAIN"})).await;
    let stopped = host
        .wait_for(&first, |e| {
            e["kind"] == "agent" && e["data"]["text"].as_str().is_some_and(|text| text.contains("hop limit (8)"))
        })
        .await;
    let stopped_text = stopped["data"]["text"].as_str().unwrap();
    assert!(stopped_text.contains("Chain stopped"), "{stopped_text}");
    let refusal = refusal_in(stopped_text);
    // The revision boundary the wake's delegation must sit after: nothing the wake does may predate it.
    let before_approval = host.call("history", json!({"botId": first})).await["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["rev"].as_i64().unwrap_or(0))
        .max()
        .unwrap_or(0);

    let sync = host.wait_finished().await;
    let notice = sync["entries"].as_array().unwrap().iter().find(|entry| {
        entry["botId"] == lead
            && entry["data"]["heading"]
                .as_str()
                .is_some_and(|heading| heading.contains(&format!("Hop limit refusal {refusal}")))
    });
    let notice = notice.expect("the lead did not get the hop-limit notice");
    assert_eq!(notice["data"]["status"], "completed");
    assert_eq!(notice["data"]["sourceBotId"], first);
    assert_eq!(notice["data"]["targetBotId"], lead);
    assert_eq!(notice["data"]["botMessage"]["text"], "CHAIN");
    assert!(notice["data"]["botMessage"]["reply"].as_str().unwrap().contains("reply:"));
    let lead_prompts = host.fixture_log(&lead_name, "prompts.jsonl");
    assert!(lead_prompts.iter().any(|prompt| prompt.as_str().unwrap().contains("Hop limit reached")));
    assert!(lead_prompts.iter().any(|prompt| prompt.as_str().unwrap().contains("Use only the hop-limit tools")));

    host.call(
        "send",
        json!({
            "botId": lead,
            "text": format!("TEAM reset_hop_count {{\"refusalId\":{refusal},\"botId\":\"{first}\"}}"),
        }),
    )
    .await;
    host.wait_for(&lead, |entry| {
        entry["kind"] == "agent"
            && entry["data"]["final"] == true
            && entry["data"]["text"]
                .as_str()
                .is_some_and(|text| text.starts_with("Done:") && text.contains("\"reset\""))
    })
    .await;
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let path = common::folder(&host.home, "first").join("prompts.jsonl");
            if std::fs::read_to_string(&path).unwrap_or_default().contains("starts at 0") {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the reset wake did not reach the blocked bot");
    // The wake's own turn runs on the grant: its handoff is admitted at hop 0 and the chain goes on. This
    // delegate happens on the wake, before any user message: only the reset could have admitted it. The entry
    // must also sit after the pre-approval boundary, so an earlier chain's "Chain forwarded" cannot match.
    host.wait_for(&first, |entry| {
        entry["kind"] == "agent"
            && entry["data"]["text"].as_str() == Some("Chain forwarded")
            && entry["rev"].as_i64().unwrap_or(0) > before_approval
    })
    .await;
    let wake_rev = host.call("history", json!({"botId": first})).await["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "agent" && e["data"]["text"] == "Chain forwarded")
        .map(|e| e["rev"].as_i64().unwrap())
        .max()
        .unwrap();
    let sync = host.wait_finished().await;
    let previous_rev = sync["rev"].as_i64().unwrap();
    assert!(previous_rev > wake_rev, "the wake's delegation was not its own turn");

    // The wake already proved the grant by delegating at hop 0. Another chain from here is admitted too.
    host.call("send", json!({"botId": first, "text": "CHAIN"})).await;
    host.wait_for(&first, |entry| {
        entry["rev"].as_i64().unwrap_or(0) > previous_rev
            && entry["kind"] == "agent"
            && entry["data"]["final"] == true
            && entry["data"]["text"] == "Chain forwarded"
    })
    .await;
}
