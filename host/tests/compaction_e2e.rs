//! Compaction and rollover on a real host with disposable data: the host compacting a full conversation at the
//! end of a turn, its loop guard turning to a rollover, and the handoff card the user edits or turns down.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[allow(dead_code)]
mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// Waits until the bot's chat has a notice with this field, and returns the newest one.
async fn notice(host: &Host, bot: &str, field: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let history = host.call("history", json!({"botId": bot})).await;
        if let Some(e) = history["entries"].as_array().unwrap().iter().rev().find(|e| e["data"][field].is_object()) {
            return e.clone();
        }
        assert!(Instant::now() < deadline, "no {field} notice: {history}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn a_full_conversation_is_compacted_with_its_keep_list() {
    let host = Host::start().await;
    let claude = host.fixture_bot("Claude-like", "instructions_agent.py", "claude").await;
    host.reply(&claude, "hello").await;
    host.write_memory(Some(&claude), "MEMORY.md", "- Allergic to peanuts\n");
    host.reply(&claude, "USAGE 210000 SIZE 1000000").await;
    let line = notice(&host, &claude, "compaction").await;
    assert_eq!(line["data"]["compaction"]["byHost"], true);
    assert_eq!(line["data"]["compaction"]["summary"], "Summary 1: the user is planning a trip.");
    let prompts = host.prompts(&claude);
    let compact = prompts.iter().find(|p| p.starts_with("/compact")).unwrap();
    assert!(compact.starts_with("/compact When your context is summarized"), "Claude gets the keep-list: {compact}");
    // Memory changed since the session opened, so the compaction keeps an opening rendered just before it.
    let loads = host.requests(&claude, "session/load");
    let opening = loads.last().unwrap()["params"]["_meta"]["systemPrompt"]["append"].as_str().unwrap();
    assert!(opening.contains("Allergic to peanuts"), "{opening}");

    let codex = host.fixture_bot("Codex-like", "instructions_agent.py", "codex").await;
    host.reply(&codex, "USAGE 150000 SIZE 200000").await;
    notice(&host, &codex, "compaction").await;
    assert!(host.prompts(&codex).iter().any(|p| p == "/compact"), "Codex's keep-list is in its config");
}

#[tokio::test]
async fn compacting_too_often_turns_into_a_rollover_the_user_can_edit_or_turn_down() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Busy", "instructions_agent.py", "codex").await;
    host.reply(&bot, "USAGE 210000 SIZE 1000000").await;
    notice(&host, &bot, "compaction").await;
    // Full again right after a compaction: compacting again wouldn't help.
    host.reply(&bot, "USAGE 210000 SIZE 1000000").await;
    let card = notice(&host, &bot, "handoff").await;
    assert_eq!(card["data"]["handoff"]["status"], "draft");
    let text = card["data"]["handoff"]["text"].as_str().unwrap();
    assert!(text.starts_with("<handoff>") && text.contains("USAGE 210000"), "{text}");

    host.call("editHandoff", json!({"entryId": card["id"], "text": "<handoff>We were booking a train.</handoff>"}))
        .await;
    host.reply(&bot, "Which seat did we pick?").await;
    let news = host.requests(&bot, "session/new");
    assert_eq!(news.len(), 2, "a new conversation");
    let last = host.prompts(&bot).last().unwrap().clone();
    assert!(last.contains("We were booking a train.") && last.contains("Which seat did we pick?"), "{last}");
    let card = host.call("history", json!({"botId": bot})).await["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == card["id"])
        .unwrap()
        .clone();
    assert_eq!(card["data"]["handoff"]["status"], "sent");
    assert!(host.refused("editHandoff", json!({"entryId": card["id"], "text": "late"})).await.contains("moved on"));

    // Turned down: the conversation carries on.
    host.reply(&bot, "USAGE 210000 SIZE 1000000").await;
    notice(&host, &bot, "compaction").await;
    host.reply(&bot, "USAGE 210000 SIZE 1000000").await;
    let card = host.call("history", json!({"botId": bot})).await["entries"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|e| e["data"]["handoff"]["status"] == "draft")
        .unwrap()
        .clone();
    host.call("keepConversation", json!({"entryId": card["id"]})).await;
    host.reply(&bot, "carry on").await;
    assert_eq!(host.requests(&bot, "session/new").len(), 2, "same conversation");
    assert!(!host.prompts(&bot).last().unwrap().contains("<handoff>"));
}
