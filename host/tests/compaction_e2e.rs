//! Compaction and rollover on a real host with disposable data: the host compacting a full conversation at the
//! end of a turn, its loop guard turning to a rollover, the handoff card the user edits or turns down, and a queued
//! compaction or a card the conversation no longer calls for.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[allow(dead_code)]
mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// A fixture bot that Codync summarizes for: Early, since by default the agent decides.
async fn early_bot(host: &Host, name: &str, mode: &str) -> String {
    let bot = host.fixture_bot(name, "instructions_agent.py", mode).await;
    host.call("updateBot", json!({"id": bot, "compaction": "early"})).await;
    bot
}

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
    let claude = early_bot(&host, "Claude-like", "claude").await;
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

    let codex = early_bot(&host, "Codex-like", "codex").await;
    host.reply(&codex, "USAGE 150000 SIZE 200000").await;
    notice(&host, &codex, "compaction").await;
    assert!(host.prompts(&codex).iter().any(|p| p == "/compact"), "Codex's keep-list is in its config");
}

#[tokio::test]
async fn the_users_threshold_decides_when_the_host_compacts() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Roomy", "instructions_agent.py", "claude").await;
    let shown = common::bot(&host.snapshot().await, &bot).clone();
    assert!(shown["compaction"].is_null(), "a custom command shows it once it runs Claude");
    host.reply(&bot, "USAGE 10000 SIZE 1000000").await;
    let deadline = Instant::now() + Duration::from_secs(10);
    let shown = loop {
        let shown = common::bot(&host.snapshot().await, &bot).clone();
        if !shown["compactionTokens"].is_null() || Instant::now() > deadline {
            break shown;
        }
    };
    assert_eq!(shown["compaction"], "off", "by default the agent decides");
    assert_eq!(
        shown["compactionTokens"],
        json!({"min": 50_000, "max": 960_000, "step": 10_000, "presets": (1..=9).map(|n| n * 100_000).collect::<Vec<u64>>()})
    );

    host.call("updateBot", json!({"id": bot, "compaction": 300_000})).await;
    host.reply(&bot, "USAGE 210000 SIZE 1000000").await;
    host.reply(&bot, "still short of it").await;
    assert!(!host.prompts(&bot).iter().any(|p| p.starts_with("/compact")), "200k is short of the user's 300k");
    host.reply(&bot, "USAGE 300000 SIZE 1000000").await;
    notice(&host, &bot, "compaction").await;

    let auto = host.fixture_bot("Auto", "instructions_agent.py", "codex").await;
    host.reply(&auto, "USAGE 950000 SIZE 1000000").await;
    host.reply(&auto, "next").await;
    assert!(!host.prompts(&auto).iter().any(|p| p == "/compact"), "by default left to the agent's own compaction");
    assert!(host.refused("updateBot", json!({"id": auto, "compaction": 123_456})).await.contains("multiple of"));
}

#[tokio::test]
async fn compacting_too_often_turns_into_a_rollover_the_user_can_edit_or_turn_down() {
    let host = Host::start().await;
    let bot = early_bot(&host, "Busy", "codex").await;
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

#[tokio::test]
async fn turning_summaries_off_withdraws_a_fresh_conversation_already_drafted() {
    let host = Host::start().await;
    let bot = early_bot(&host, "Changed", "codex").await;
    host.reply(&bot, "USAGE 210000 SIZE 1000000").await;
    notice(&host, &bot, "compaction").await;
    host.reply(&bot, "USAGE 210000 SIZE 1000000").await;
    let card = notice(&host, &bot, "handoff").await;
    assert_eq!(card["data"]["handoff"]["status"], "draft");

    host.call("updateBot", json!({"id": bot, "compaction": "off"})).await;
    common::eventually("the card was withdrawn", Duration::from_secs(10), async || {
        common::steer::entry(&host, &bot, card["id"].as_str().unwrap()).await["data"]["handoff"]["status"] == "kept"
    })
    .await;
    host.reply(&bot, "carry on").await;
    assert_eq!(host.requests(&bot, "session/new").len(), 1, "same conversation");
    assert!(!host.prompts(&bot).last().unwrap().contains("<handoff>"));
}

/// Holds a turn that fills the conversation, queues "second" (held too) behind it and lets the first end: the
/// compaction it called for waits in the queue while "second" runs.
async fn compaction_queued_behind_a_turn(host: &Host, bot: &str) {
    host.call("send", json!({"botId": bot, "text": "USAGE 210000 SIZE 1000000 HOLD"})).await;
    prompted(host, bot, 1).await;
    let second = host.call("send", json!({"botId": bot, "text": "second HOLD"})).await;
    assert_eq!(second["entry"]["data"]["status"], "queued");
    release(host, bot);
    prompted(host, bot, 2).await;
}

async fn prompted(host: &Host, bot: &str, prompts: usize) {
    common::eventually("the agent got the prompt", Duration::from_secs(30), async || {
        host.prompts(bot).len() == prompts
    })
    .await;
}

/// Ends the bot's held turn (`HOLD` in `instructions_agent.py`).
fn release(host: &Host, bot: &str) {
    std::fs::write(host.home.join("bots").join(bot).join("workspace").join("release"), "").unwrap();
}

/// Sends `text` and waits until the agent has it, after everything queued before it.
async fn reaches_agent(host: &Host, bot: &str, text: &str) {
    host.call("send", json!({"botId": bot, "text": text})).await;
    common::eventually("the agent got the message", Duration::from_secs(30), async || {
        host.prompts(bot).last().is_some_and(|p| p.contains(text))
    })
    .await;
}

fn compactions(host: &Host, bot: &str) -> usize {
    host.prompts(bot).iter().filter(|p| p.starts_with("/compact")).count()
}

#[tokio::test]
async fn a_queued_compaction_runs_only_while_still_due() {
    let host = Host::start().await;
    // The second turn queues another compaction, but the first one makes room.
    let busy = early_bot(&host, "Busy", "codex").await;
    compaction_queued_behind_a_turn(&host, &busy).await;
    release(&host, &busy);
    notice(&host, &busy, "compaction").await;
    reaches_agent(&host, &busy, "third").await;
    assert_eq!(compactions(&host, &busy), 1);

    // Summaries turned off while it waits.
    let off = early_bot(&host, "Off", "codex").await;
    compaction_queued_behind_a_turn(&host, &off).await;
    host.call("updateBot", json!({"id": off, "compaction": "off"})).await;
    release(&host, &off);
    reaches_agent(&host, &off, "third").await;
    assert_eq!(compactions(&host, &off), 0);
}
