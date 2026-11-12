//! A real host and its built-in `chat` MCP server, with a scripted agent: a bot asks the user to choose, the card waits
//! like an approval card, and the user's press comes back to the bot as its answer and as their message.

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
    async fn post(&self, method: &str, body: Value) -> reqwest::Response {
        reqwest::Client::new()
            .post(format!("{}/api/{method}", self.base))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    async fn card(&self, bot: &str) -> Value {
        self.wait_for(bot, "notice", |e| e["data"]["decision"] == true && e["data"]["status"] == "pending").await
    }

    async fn status(&self, bot: &str, status: &str) {
        common::eventually("bot status", Duration::from_secs(20), async || {
            common::snapshot(&self.base, &self.token).await["bots"]
                .as_array()
                .unwrap()
                .iter()
                .any(|b| b["id"] == bot && b["status"] == status)
        })
        .await;
    }

    fn prompts(&self) -> Vec<String> {
        std::fs::read_to_string(self.home.join("ada/prompts.jsonl")).unwrap().lines().map(str::to_owned).collect()
    }
    async fn start() -> Self {
        let home = std::env::temp_dir().join(format!("codync-decision-e2e-{}", uuid::Uuid::new_v4()));
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
            .timeout(Duration::from_mins(2))
            .send()
            .await
            .unwrap();
        let status = response.status();
        let value: Value = response.json().await.unwrap();
        assert!(status.is_success(), "{method}: {value}");
        value
    }

    async fn bot(&self) -> String {
        let cwd = self.home.join("ada");
        std::fs::create_dir_all(&cwd).unwrap();
        let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
        self.call(
            "createBot",
            json!({
                "name": "Ada", "backend": "custom", "cwd": cwd, "permission": "ask", "notify": false,
                "command": common::python_agent(&agent),
            }),
        )
        .await["bot"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    async fn wait_for(&self, bot: &str, kind: &str, predicate: impl Fn(&Value) -> bool) -> Value {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let history = self.call("history", json!({"botId": bot})).await;
                if let Some(entry) =
                    history["entries"].as_array().unwrap().iter().find(|e| e["kind"] == kind && predicate(e))
                {
                    return entry.clone();
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("the decision card flow did not finish")
    }
}

#[tokio::test]
async fn a_choice_card_answers_the_bot_and_becomes_the_users_message() {
    let host = Host::start().await;
    let bot = host.bot().await;
    host.call("send", json!({"botId": bot, "text": "ASK Ship now or wait?|Ship now|Wait for Friday|Wait for Friday"}))
        .await;

    // The card: the question, the options in order, and the one the bot suggests.
    let card = host.card(&bot).await;
    assert_eq!(card["data"]["question"], "Ship now or wait?", "{card}");
    assert_eq!(card["data"]["choices"], json!(["Ship now", "Wait for Friday"]));
    assert!(card["data"].get("options").is_none(), "legacy permission decoders retain their shape");
    assert_eq!(card["data"]["recommended"], 1);
    assert_eq!(card["data"]["author"], bot);
    // It needs the user, as an approval card does, and its text stands in for it where no card is shown.
    let roster = common::snapshot(&host.base, &host.token).await;
    let listed = roster["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot).unwrap();
    assert_eq!(listed["status"], "needsInput", "{listed}");
    assert!(card["data"]["text"].as_str().unwrap().starts_with("Ship now or wait?"));

    // A press that names no option changes nothing; a real one is the bot's answer and the user's message.
    let refused = reqwest::Client::new()
        .post(format!("{}/api/respondDecision", host.base))
        .bearer_auth(&host.token)
        .json(&json!({"entryId": card["id"], "option": "Later"}))
        .send()
        .await
        .unwrap();
    assert!(!refused.status().is_success(), "no such option");
    let answered = host.call("respondDecision", json!({"entryId": card["id"], "option": "Wait for Friday"})).await;
    assert_eq!(answered["choice"], "Wait for Friday");

    let said = host.wait_for(&bot, "user", |e| e["data"]["from"] == "decision").await;
    assert_eq!(said["data"]["text"], "Wait for Friday", "the choice is the user's next message");
    let done = host.wait_for(&bot, "agent", |e| e["data"]["final"] == true).await;
    let text = done["data"]["text"].as_str().unwrap();
    assert!(text.starts_with("Asked: "), "{text}");
    assert!(text.contains("Wait for Friday"), "the tool hands the choice back: {text}");

    let card = host.wait_for(&bot, "notice", |e| e["id"] == card["id"] && e["data"]["status"] == "answered").await;
    assert_eq!(
        (card["data"]["status"].as_str(), card["data"]["selected"].as_str()),
        (Some("answered"), Some("Wait for Friday"))
    );
    // The same press again, or a stale one, isn't another answer.
    let twice = reqwest::Client::new()
        .post(format!("{}/api/respondDecision", host.base))
        .bearer_auth(&host.token)
        .json(&json!({"entryId": card["id"], "option": "Ship now"}))
        .send()
        .await
        .unwrap();
    assert!(!twice.status().is_success(), "a card answers once");
    let messages = host.call("history", json!({"botId": bot})).await;
    assert_eq!(messages["entries"].as_array().unwrap().iter().filter(|e| e["data"]["from"] == "decision").count(), 1);
    host.status(&bot, "idle").await;
    assert_eq!(host.prompts().len(), 1, "the answer must not prompt a second full turn");
    assert_eq!(said["turn"], card["turn"]);
    assert_eq!(said["data"]["status"], "sent");
}

#[tokio::test]
async fn a_card_refused_in_a_room_says_why() {
    let host = Host::start().await;
    let member = host.bot().await;
    let group =
        host.call("createBot", json!({"name": "Room", "kind": "group", "members": [member]})).await["bot"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
    // The member's room turn runs while the user's message is queued behind it: its answer is the room's, not a card.
    host.call("send", json!({"botId": group, "text": "ROOM_DECISION"})).await;
    let reply = host.wait_for(&group, "agent", |e| e["data"]["final"] == true).await;
    assert!(reply["data"]["text"].as_str().is_some_and(|e| e.contains("own chat")), "{reply}");
    let history = host.call("history", json!({"botId": member})).await;
    assert!(history["entries"].as_array().unwrap().iter().all(|e| e["data"]["decision"] != true));
}

#[tokio::test]
async fn actual_chat_mcp_waits_more_than_thirty_seconds_for_the_choice() {
    let host = Host::start().await;
    let bot = host.bot().await;
    host.call("send", json!({"botId":bot,"text":"ASK Delayed?|Now|Later"})).await;
    let card = host.card(&bot).await;
    tokio::time::sleep(Duration::from_secs(31)).await;
    host.status(&bot, "needsInput").await;
    let result = host.call("respondDecision", json!({"entryId":card["id"],"option":"Later"})).await;
    assert_eq!(result["choice"], "Later");
    let done = host.wait_for(&bot, "agent", |e| e["data"]["final"] == true).await;
    assert!(done["data"]["text"].as_str().unwrap().starts_with("Asked:"));
    host.status(&bot, "idle").await;
    assert_eq!(host.prompts().len(), 1);
}

#[tokio::test]
async fn numeric_button_labels_and_simultaneous_answers_are_delivered_once() {
    let host = Host::start().await;
    let bot = host.bot().await;
    host.call("send", json!({"botId":bot,"text":"ASK Number?|2|4|4"})).await;
    let card = host.card(&bot).await;
    let body = json!({"entryId":card["id"],"option":"2"});
    let (one, two) = tokio::join!(host.post("respondDecision", body.clone()), host.post("respondDecision", body));
    assert_ne!(one.status().is_success(), two.status().is_success());
    let winner = if one.status().is_success() { one } else { two };
    assert_eq!(winner.json::<Value>().await.unwrap()["choice"], "2");
    host.status(&bot, "idle").await;
    let history = host.call("history", json!({"botId":bot})).await;
    assert_eq!(history["entries"].as_array().unwrap().iter().filter(|e| e["data"]["from"] == "decision").count(), 1);
    assert_eq!(host.prompts().len(), 1);
}

#[tokio::test]
async fn a_legacy_text_reply_selects_the_twelfth_option_without_an_extra_turn() {
    let host = Host::start().await;
    let bot = host.bot().await;
    let choices: Vec<_> = (1..=12).map(|n| format!("Choice {n}")).collect();
    host.call("send", json!({"botId":bot,"text":format!("ASK Choose?|{}|Choice 12", choices.join("|"))})).await;
    let card = host.card(&bot).await;
    assert_eq!(card["kind"], "notice", "old clients show this kind in their chat");
    assert!(card["data"]["text"].as_str().unwrap().contains("12. Choice 12"));
    host.call("send", json!({"botId":bot,"text":"12"})).await;
    let answered = host.wait_for(&bot, "notice", |e| e["id"] == card["id"] && e["data"]["status"] == "answered").await;
    assert_eq!(answered["data"]["selected"], "Choice 12");
    host.status(&bot, "idle").await;
    assert_eq!(host.prompts().len(), 1);
    let message = host.wait_for(&bot, "user", |e| e["data"]["from"] == "decision").await;
    assert_eq!(message["data"]["text"], "12", "keep the user's exact text");
    assert_eq!(message["data"]["decisionChoice"], "Choice 12");
    assert_eq!(message["turn"], card["turn"]);
}

#[tokio::test]
async fn answering_clears_needs_input_while_the_same_turn_keeps_working() {
    let host = Host::start().await;
    let bot = host.bot().await;
    host.call("send", json!({"botId":bot,"text":"ASK_HOLD Which?|Left|Right"})).await;
    let card = host.card(&bot).await;
    host.call("respondDecision", json!({"entryId":card["id"],"option":"Right"})).await;
    host.status(&bot, "working").await;
    std::fs::write(host.home.join("ada/release"), "continue").unwrap();
    host.status(&bot, "idle").await;
    assert_eq!(host.prompts().len(), 1);
}

#[tokio::test]
async fn stopping_expires_a_choice_and_refuses_a_late_click() {
    let host = Host::start().await;
    let bot = host.bot().await;
    host.call("send", json!({"botId":bot,"text":"ASK Which?|Left|Right"})).await;
    let card = host.card(&bot).await;
    host.call("stop", json!({"botId":bot})).await;
    host.wait_for(&bot, "notice", |e| e["id"] == card["id"] && e["data"]["status"] == "expired").await;
    assert!(!host.post("respondDecision", json!({"entryId":card["id"],"option":"Left"})).await.status().is_success());
    let history = host.call("history", json!({"botId":bot})).await;
    assert!(history["entries"].as_array().unwrap().iter().all(|e| e["data"]["from"] != "decision"));
}

#[tokio::test]
async fn a_different_user_message_expires_the_card_and_runs_only_the_new_message() {
    let host = Host::start().await;
    let bot = host.bot().await;
    host.call("send", json!({"botId":bot,"text":"ASK Which?|Left|Right"})).await;
    let card = host.card(&bot).await;
    host.call("send", json!({"botId":bot,"text":"Changed direction"})).await;
    host.wait_for(&bot, "agent", |e| e["data"]["final"] == true && e["data"]["text"] == "reply: Changed direction")
        .await;
    host.wait_for(&bot, "notice", |e| e["id"] == card["id"] && e["data"]["status"] == "expired").await;
    host.status(&bot, "idle").await;
    assert_eq!(host.prompts().len(), 2);
}

#[tokio::test]
async fn host_restart_expires_the_old_card_even_if_the_turn_resumes() {
    let mut host = Host::start().await;
    let bot = host.bot().await;
    host.call("send", json!({"botId":bot,"text":"ASK Which?|Left|Right"})).await;
    let old = host.card(&bot).await;
    common::stop(&mut host.child);
    let (child, base) = common::serve(&host.home).await;
    host.child = child;
    host.base = base;
    host.wait_for(&bot, "notice", |e| e["id"] == old["id"] && e["data"]["status"] == "expired").await;
    assert!(!host.post("respondDecision", json!({"entryId":old["id"],"option":"Left"})).await.status().is_success());
}

#[tokio::test]
async fn a_choice_after_send_message_keeps_the_final_reply_and_reacts_to_the_choice() {
    let host = Host::start().await;
    let bot = host.bot().await;
    host.call("send", json!({"botId":bot,"text":"ASK_AFTER_MESSAGE Which?|Left|Right"})).await;
    let card = host.card(&bot).await;
    host.call("respondDecision", json!({"entryId":card["id"],"option":"Right"})).await;
    host.wait_for(&bot, "agent", |e| {
        e["data"]["final"] == true && e["data"]["text"].as_str().is_some_and(|t| t.starts_with("Asked:"))
    })
    .await;
    host.status(&bot, "idle").await;
    let choice = host.wait_for(&bot, "user", |e| e["data"]["from"] == "decision").await;
    assert_eq!(choice["data"]["botReactions"][0]["emoji"], "👍");
    assert_eq!(host.prompts().len(), 1);
}

#[tokio::test]
async fn parallel_mcp_questions_publish_only_one_live_card() {
    let host = Host::start().await;
    let bot = host.bot().await;
    host.call(
        "send",
        json!({"botId":bot,"text":r#"CHAT_TWICE request_decision {"question":"Which?","options":["Left","Right"]}"#}),
    )
    .await;
    let card = host.card(&bot).await;
    common::eventually("the second parallel call was refused", Duration::from_secs(20), async || {
        host.home.join("ada/second-decision-refused").exists()
    })
    .await;
    host.call("respondDecision", json!({"entryId":card["id"],"option":"Right"})).await;
    let done = host.wait_for(&bot, "agent", |e| e["data"]["final"] == true).await;
    assert!(done["data"]["text"].as_str().unwrap().contains("one question at a time"));
    let history = host.call("history", json!({"botId":bot})).await;
    assert_eq!(history["entries"].as_array().unwrap().iter().filter(|e| e["data"]["decision"] == true).count(), 1);
}

#[tokio::test]
async fn answering_a_choice_keeps_an_unanswered_permission_in_needs_input() {
    let host = Host::start().await;
    let bot = host.bot().await;
    host.call("send", json!({"botId":bot,"text":"ASK_PERMISSION Which?|Left|Right"})).await;
    let card = host.card(&bot).await;
    let permission = host.wait_for(&bot, "permission", |e| e["data"]["status"] == "pending").await;
    host.call("respondDecision", json!({"entryId":card["id"],"option":"Right"})).await;
    host.status(&bot, "needsInput").await;
    host.call("respondPermission", json!({"entryId":permission["id"],"optionId":"allow"})).await;
    host.status(&bot, "idle").await;
    assert_eq!(host.prompts().len(), 1);
}

#[tokio::test]
async fn natural_270_second_expiry_takes_no_answer_and_the_next_call_recovers() {
    let host = Host::start().await;
    let bot = host.bot().await;
    host.call("send", json!({"botId":bot,"text":"ASK_HOLD Which?|Left|Right"})).await;
    let card = host.card(&bot).await;
    let started = std::time::Instant::now();
    common::eventually("the full choice deadline", Duration::from_secs(290), async || {
        let history = host.call("history", json!({"botId":bot})).await;
        history["entries"].as_array().unwrap().iter().any(|e| e["id"] == card["id"] && e["data"]["status"] == "expired")
    })
    .await;
    assert!(started.elapsed() >= Duration::from_secs(265), "did not reach the natural deadline");
    host.status(&bot, "working").await;
    assert!(!host.post("respondDecision", json!({"entryId":card["id"],"option":"Left"})).await.status().is_success());
    let history = host.call("history", json!({"botId":bot})).await;
    assert!(history["entries"].as_array().unwrap().iter().all(|e| e["data"]["from"] != "decision"));
    std::fs::write(host.home.join("ada/release"), "continue").unwrap();
    host.status(&bot, "idle").await;
    host.call("send", json!({"botId":bot,"text":"ASK Recovery?|Left|Right"})).await;
    let next = host.card(&bot).await;
    assert_ne!(next["id"], card["id"]);
    host.call("respondDecision", json!({"entryId":next["id"],"option":"Right"})).await;
    host.status(&bot, "idle").await;
    assert_eq!(host.prompts().len(), 2);
}
