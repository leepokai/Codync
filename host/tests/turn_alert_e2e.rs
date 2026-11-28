//! A real host driving scripted ACP agents: the turn-end alert reaches the user only when the turn left
//! them something to read (#486), exactly once, and never on top of the alert a card or approval sent.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;

/// A bot running `fixture` in `home/<name>`, with its alerts on (the push under test).
async fn alerting_bot(host: &Host, name: &str, fixture: &str) -> String {
    let cwd = host.home.join(name);
    std::fs::create_dir_all(&cwd).unwrap();
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(fixture);
    let bot = json!({
        "name": name, "backend": "custom", "command": common::python_agent(&agent),
        "cwd": cwd, "permission": "ask",
    });
    host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned()
}

async fn wait_for(host: &Host, bot: &str, predicate: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let history = host.call("history", json!({"botId": bot})).await;
            if let Some(entry) = history["entries"].as_array().unwrap().iter().find(|e| predicate(e)) {
                return entry.clone();
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("flow did not finish")
}

async fn wait_finished(host: &Host) {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let sync = host.snapshot().await;
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

/// The live `alert` events from the host's event stream: the pushes a phone or desktop app shows.
struct Alerts {
    events: UnboundedReceiver<Value>,
    seen: Vec<Value>,
}

impl Alerts {
    /// Listens from now on (alerts are live only, never replayed).
    async fn listen(host: &Host) -> Self {
        Self { events: host.events().await, seen: Vec::new() }
    }

    /// The alerts sent for `bot` as (kind, body), in order, once the last one has had time to arrive.
    async fn of(&mut self, bot: &str) -> Vec<(String, String)> {
        tokio::time::sleep(Duration::from_millis(500)).await;
        while let Ok(event) = self.events.try_recv() {
            if event["type"] == "alert" {
                self.seen.push(event["alert"].clone());
            }
        }
        self.seen
            .iter()
            .filter(|a| a["botId"] == bot)
            .map(|a| (a["kind"].as_str().unwrap().to_owned(), a["body"].as_str().unwrap().to_owned()))
            .collect()
    }
}

fn alert(kind: &str, body: &str) -> (String, String) {
    (kind.into(), body.into())
}

#[tokio::test]
async fn a_silent_turn_woken_by_a_bot_reply_does_not_alert_the_user() {
    let host = Host::start().await;
    let lead = alerting_bot(&host, "lead", "team_agent.py").await;
    let reviewer = alerting_bot(&host, "reviewer", "team_agent.py").await;
    host.works_in_chat(&reviewer).await;
    let mut alerts = Alerts::listen(&host).await;

    // The lead delegates; its own turn answers the user ("Message queued"), which alerts once.
    host.call("send", json!({"botId": lead, "text": format!("DELEGATE {reviewer} BLOCK SILENT")})).await;
    wait_for(&host, &lead, |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    // The reviewer holds its answer until the lead's alert is past the push throttle's 5 s, so a second
    // push for the lead would really go out were one sent.
    tokio::time::sleep(Duration::from_secs(6)).await;
    std::fs::write(host.home.join("reviewer/release"), "").unwrap();
    // The answer wakes the lead, whose turn answers with (pass): the user was sent nothing.
    wait_finished(&host).await;
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
    assert_eq!(alerts.of(&lead).await, [alert("done", "Message queued")]);
    assert_eq!(alerts.of(&reviewer).await, []);
}

#[tokio::test]
async fn a_turn_that_talked_through_send_message_alerts_once_with_what_it_sent() {
    let host = Host::start().await;
    let bot = alerting_bot(&host, "writer", "team_agent.py").await;
    let mut alerts = Alerts::listen(&host).await;

    host.call("send", json!({"botId": bot, "text": "REPORT"})).await;
    wait_finished(&host).await;
    assert_eq!(alerts.of(&bot).await, [alert("done", "Report for the user\n\nSecond report for the user")]);
}

#[tokio::test]
async fn a_turn_that_only_shared_a_file_alerts_once_with_the_file() {
    let host = Host::start().await;
    let bot = alerting_bot(&host, "sharer", "file_agent.py").await;
    let mut alerts = Alerts::listen(&host).await;

    host.call("send", json!({"botId": bot, "text": "share-image files-only"})).await;
    wait_finished(&host).await;
    let size = std::fs::metadata(host.home.join("sharer/chart.png")).unwrap().len();
    assert_eq!(alerts.of(&bot).await, [alert("done", &format!("chart.png ({size} bytes)"))]);
}

#[tokio::test]
async fn a_reply_answered_while_the_next_message_waits_alerts_once_the_queue_is_done() {
    let host = Host::start().await;
    let bot = alerting_bot(&host, "worker", "team_agent.py").await;
    let mut alerts = Alerts::listen(&host).await;

    // The user's "ok" waits behind the real answer; its turn passes, so the answer is what the user has to read.
    host.call("send", json!({"botId": bot, "text": "BLOCK work"})).await;
    wait_for(&host, &bot, |e| e["kind"] == "user" && e["data"]["text"] == "BLOCK work").await;
    host.call("send", json!({"botId": bot, "text": "OK"})).await;
    std::fs::write(host.home.join("worker/release"), "").unwrap();
    wait_finished(&host).await;
    assert_eq!(alerts.of(&bot).await, [alert("done", "reply: BLOCK work")]);
}

#[tokio::test]
async fn a_reply_held_for_a_queued_message_is_dropped_when_the_user_stops() {
    let host = Host::start().await;
    let bot = alerting_bot(&host, "stopper", "team_agent.py").await;
    let mut alerts = Alerts::listen(&host).await;

    // The answer is held for the queued turn, which asks an approval; the user stops it, then says "ok".
    host.call("send", json!({"botId": bot, "text": "BLOCK work"})).await;
    wait_for(&host, &bot, |e| e["kind"] == "user" && e["data"]["text"] == "BLOCK work").await;
    host.call("send", json!({"botId": bot, "text": "PERMISSION wait"})).await;
    std::fs::write(host.home.join("stopper/release"), "").unwrap();
    wait_for(&host, &bot, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await;
    host.call("stop", json!({"botId": bot})).await;
    wait_finished(&host).await;
    host.call("send", json!({"botId": bot, "text": "OK"})).await;
    wait_finished(&host).await;
    assert_eq!(alerts.of(&bot).await, [alert("needsInput", "Edit a file")]);
}

#[tokio::test]
async fn a_turn_waiting_on_an_approval_alerts_only_for_the_approval() {
    let host = Host::start().await;
    let bot = alerting_bot(&host, "editor", "team_agent.py").await;
    let mut alerts = Alerts::listen(&host).await;

    // Asks for an approval, then ends with no text once it's answered: the card was what it left the user.
    host.call("send", json!({"botId": bot, "text": "PERMISSION EMPTY"})).await;
    let card = wait_for(&host, &bot, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await;
    host.call("respondPermission", json!({"entryId": card["id"], "optionId": "allow"})).await;
    wait_finished(&host).await;
    assert_eq!(alerts.of(&bot).await, [alert("needsInput", "Edit a file")]);
}

#[tokio::test]
async fn a_turn_that_ends_on_a_choice_card_alerts_only_for_the_card() {
    let host = Host::start().await;
    let bot = alerting_bot(&host, "chooser", "team_agent.py").await;
    let mut alerts = Alerts::listen(&host).await;

    // Puts a card up, then ends with no text while it still waits: the card was what it left the user.
    host.call("send", json!({"botId": bot, "text": "ASK_THEN_END Ship now?|Yes|No"})).await;
    wait_for(&host, &bot, |e| e["data"]["decision"] == true).await;
    std::fs::write(host.home.join("chooser/release"), "").unwrap();
    wait_finished(&host).await;
    assert_eq!(alerts.of(&bot).await, [alert("needsInput", "Ship now?")]);
}
