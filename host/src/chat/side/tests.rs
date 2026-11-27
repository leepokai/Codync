use super::*;
use crate::api::devices::Caller;
use crate::hub::{BotStatus, Hub};
use crate::store::Store;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn bot(cwd: &Path, id: &str) -> BotConfig {
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
    serde_json::from_value(json!({
        "id": id, "name": "Tester", "backend": "fixture", "cwd": cwd,
        "command": codync_agent::shell::python(&agent), "notify": false,
    }))
    .expect("valid bot")
}

/// A host with one fixture bot. `start()` is what lets the hub order entries; no bot turn is queued here.
fn hub(dir: &Path, cfg: &BotConfig) -> Arc<Hub> {
    let store = Store::open(&dir.join("test.db")).unwrap();
    store.save_bot(cfg).unwrap();
    let hub = Hub::new(
        store,
        "test-host".into(),
        crate::remote::identity::Identity::load_or_create(dir).unwrap(),
        "test-token".into(),
        19222,
    );
    hub.start().unwrap();
    hub
}

fn temp() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("codync-side-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn entry(hub: &Hub, bot_id: &str, kind: EntryKind, text: &str) {
    let data = json!({"text": text, "side": kind == EntryKind::Notice});
    hub.add_entry(&Lane::main(bot_id), kind, 1, &data).unwrap();
}

#[test]
fn parses_a_side_command_and_leaves_everything_else_alone() {
    assert_eq!(parse("/side what did we finish?"), Some("what did we finish?"));
    assert_eq!(parse("  /side   what changed?  "), Some("what changed?"));
    assert_eq!(parse("/side"), Some(""), "an empty question is still the command");
    assert_eq!(parse("/side    "), Some(""));
    assert_eq!(parse("/sidebar is a component"), None, "another slash command");
    assert_eq!(parse("/sideswipe"), None);
    assert_eq!(parse("what about /side?"), None, "only at the start");
    assert_eq!(parse("hello"), None);
}

#[tokio::test]
async fn a_snapshot_shows_recent_messages_and_skips_side_lines_and_steps() {
    let dir = temp();
    let cfg = bot(&dir.join("a"), "a");
    let hub = hub(&dir, &cfg);
    entry(&hub, "a", EntryKind::User, "start the release");
    entry(&hub, "a", EntryKind::Agent, "the build is green");
    entry(&hub, "a", EntryKind::Tool, "cargo test output");
    entry(&hub, "a", EntryKind::Notice, "a previous side answer");
    entry(&hub, "a", EntryKind::User, "and then?");

    let all = snapshot(&hub.store, "a", 24).unwrap();
    assert_eq!(all, "User: start the release\nTester: the build is green\nUser: and then?");
    let last = snapshot(&hub.store, "a", 2).unwrap();
    assert_eq!(last, "Tester: the build is green\nUser: and then?", "the limit takes the newest lines");
    assert_eq!(snapshot(&hub.store, "a", 24).unwrap(), all, "reading the snapshot changes nothing");
    hub.shutdown().await;
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn an_empty_chat_asks_from_an_empty_snapshot() {
    let dir = temp();
    let cfg = bot(&dir.join("a"), "a");
    let hub = hub(&dir, &cfg);
    assert_eq!(snapshot(&hub.store, "a", 24).unwrap(), "");
    let p = prompt("", "what's done?");
    assert!(p.contains("[codync/side]"));
    assert!(p.contains("No messages yet."));
    assert!(p.contains("Do not call any tool"));
    assert!(p.contains("Never claim to have done anything"));
    assert!(p.ends_with("Question: what's done?"));
    hub.shutdown().await;
    std::fs::remove_dir_all(&dir).unwrap();
}

/// `/side` never reaches the bot's actor: the bot stays idle, its own prompt log stays untouched, and the
/// throwaway agent's answer lands as a notice in the chat rather than as a turn in the bot's session.
#[tokio::test]
async fn a_side_question_runs_off_to_the_side_of_the_bot() {
    let dir = temp();
    let cwd = dir.join("a");
    std::fs::create_dir_all(&cwd).unwrap();
    let cfg = bot(&cwd, "a");
    let hub = hub(&dir, &cfg);
    let sent = crate::api::dispatch(&hub, &Caller::Local, "send", json!({"botId": "a", "text": "/side what's done?"}))
        .await
        .unwrap();
    let e = hub.store.entry(sent["entry"]["id"].as_str().unwrap()).unwrap();
    assert_eq!(e.data["status"], "sent", "a side question isn't queued behind a bot turn");
    assert_eq!(e.data["side"], true);

    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let entries = hub.store.history("a", i64::MAX, 100).unwrap();
            if entries.iter().any(|e| e.kind == "notice" && e.data["side"] == true) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the side answer lands in the chat");

    let entries = hub.store.history("a", i64::MAX, 100).unwrap();
    let answer = entries.iter().find(|e| e.kind == "notice" && e.data["side"] == true).unwrap();
    let text = answer.data["text"].as_str().unwrap();
    assert!(text.contains("reply:"), "the throwaway agent answered: {text}");
    assert_eq!(hub.runtime("a").status, BotStatus::Idle, "the bot's actor never got a turn");
    assert!(!cwd.join("prompts.jsonl").exists(), "nothing was written to the bot's own session");
    let entries = hub.store.history("a", i64::MAX, 100).unwrap();
    assert_eq!(entries.iter().filter(|e| e.kind == "agent").count(), 0, "no agent turn in the main transcript");
    hub.shutdown().await;
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn an_empty_side_question_and_a_group_get_a_notice_not_an_agent_turn() {
    let dir = temp();
    let cwd = dir.join("a");
    std::fs::create_dir_all(&cwd).unwrap();
    let cfg = bot(&cwd, "a");
    let hub = hub(&dir, &cfg);
    let group: BotConfig = serde_json::from_value(json!({
        "id": "g", "name": "Crew", "kind": "group", "members": ["a"], "backend": "fixture", "cwd": cwd,
    }))
    .unwrap();
    hub.store.save_bot(&group).unwrap();

    for (bot_id, text, expected) in [("a", "/side", "Write a question"), ("g", "/side what's done?", "bot's own chat")]
    {
        let sent: Value =
            crate::api::dispatch(&hub, &Caller::Local, "send", json!({"botId": bot_id, "text": text})).await.unwrap();
        assert_eq!(sent["entry"]["data"]["status"], "sent");
        let entries = hub.store.history(bot_id, i64::MAX, 100).unwrap();
        let notice = entries.iter().find(|e| e.kind == "notice" && e.data["side"] == true).unwrap();
        assert!(notice.data["text"].as_str().unwrap().contains(expected), "{}", notice.data["text"]);
    }
    assert_eq!(hub.runtime("a").status, BotStatus::Idle);
    assert!(!cwd.join("prompts.jsonl").exists());
    hub.shutdown().await;
    std::fs::remove_dir_all(&dir).unwrap();
}
