//! A real host and a scripted agent: a bot made "Read-only" asks before editing a file and refuses commands; a profile can
//! be changed later, and editing its rules away from it drops its name.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

async fn final_replies(host: &Host, bot: &str) -> Vec<String> {
    let history = host.call("history", json!({"botId": bot})).await;
    history["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "agent" && e["data"]["final"] == true)
        .filter_map(|e| e["data"]["text"].as_str().map(str::to_owned))
        .collect()
}

async fn reply_number(host: &Host, bot: &str, n: usize) -> String {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let replies = final_replies(host, bot).await;
        if replies.len() > n {
            return replies[n].clone();
        }
        assert!(Instant::now() < deadline, "no reply number {n}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn pending_card(host: &Host, bot: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let history = host.call("history", json!({"botId": bot})).await;
        if let Some(card) = history["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == "permission" && e["data"]["status"] == "pending")
        {
            return card.clone();
        }
        assert!(Instant::now() < deadline, "no approval card");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn say(host: &Host, bot: &str, text: &str) {
    host.call("send", json!({"botId": bot, "text": text, "clientNonce": uuid::Uuid::new_v4().to_string()})).await;
}

#[tokio::test]
async fn a_read_only_bot_asks_before_editing_and_refuses_commands() {
    if Command::new(common::python()).arg("--version").output().is_err() {
        eprintln!("skipping: Python not available");
        return;
    }
    let host = Host::start().await;

    // The choices a client shows come from the host.
    let profiles = host.call("securityProfiles", json!({})).await;
    let ids: Vec<&str> = profiles["profiles"].as_array().unwrap().iter().filter_map(|p| p["id"].as_str()).collect();
    assert_eq!(ids, ["readOnly", "builder", "operator", "full"]);

    // Made read-only: the Approvals setting asks, and the rules come with it.
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/command_agent.py");
    let spec = json!({"name": "Reader", "backend": "custom", "command": common::python_agent(&script),
        "connectors": [], "notify": false, "securityProfile": "readOnly"});
    let created = host.call("createBot", spec).await["bot"].clone();
    let bot = created["id"].as_str().unwrap().to_owned();
    assert_eq!(
        (created["permission"].as_str(), created["securityProfile"].as_str()),
        (Some("ask"), Some("readOnly")),
        "{created}"
    );
    assert!(created["approvalRules"].as_array().is_some_and(|r| !r.is_empty()));

    // A file edit asks; a command is refused without asking.
    say(&host, &bot, "edit:src/main.rs").await;
    let card = pending_card(&host, &bot).await;
    assert_eq!(card["data"]["toolKind"], "edit");
    host.call("respondPermission", json!({"entryId": card["id"], "optionId": "no"})).await;
    assert!(reply_number(&host, &bot, 0).await.contains("\"no\""));
    say(&host, &bot, "run:ls").await;
    assert!(reply_number(&host, &bot, 1).await.contains("\"no\""), "the command is refused with no card");

    // Raised to Full: commands are approved, and what no bot may run is still refused.
    let full = host.call("updateBot", json!({"id": bot, "securityProfile": "full"})).await["bot"].clone();
    assert_eq!((full["permission"].as_str(), full["securityProfile"].as_str()), (Some("auto"), Some("full")), "{full}");
    say(&host, &bot, "run:make").await;
    assert!(reply_number(&host, &bot, 2).await.contains("\"yes\""));
    say(&host, &bot, "run:rm -rf /").await;
    assert!(reply_number(&host, &bot, 3).await.contains("\"no\""), "forbidden under Full too");

    // Saving the same profile again keeps the person's changes; changing a rule drops the profile's name.
    let again = host
        .call(
            "updateBot",
            json!({"id": bot, "securityProfile": "full", "approvalRules": [{"pattern": "make*", "decision": "deny"}]}),
        )
        .await["bot"]
        .clone();
    assert!(again["securityProfile"].is_null(), "a bot with its own rules no longer claims the profile: {again}");
}
