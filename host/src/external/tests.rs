//! End-to-end: hook events in, a bot with a transcript and approval cards out.

use crate::hub::{BotStatus, Hub};
use crate::remote::identity::Identity;
use crate::store::{Entry, Store};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

fn hub() -> (Arc<Hub>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("codync-external-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = Store::open(std::path::Path::new(":memory:")).expect("store");
    let hub = Hub::new(store, "external-test".into(), Identity::load_or_create(&dir).expect("identity"), "t".into(), 0);
    (hub, dir)
}

/// One event of a Claude Code session in `/w/p` or, with `cwd`, another folder.
fn event(payload: &Value) -> Value {
    json!({"agent": "", "terminal": {"session": "w1"}, "payload": payload})
}

fn prompt(text: &str) -> Value {
    event(&json!({"hook_event_name": "UserPromptSubmit", "session_id": "s1", "cwd": "/w/p", "prompt": text}))
}

fn entries(hub: &Hub, kind: &str) -> Vec<Entry> {
    hub.store.entries_since(0, 200).expect("entries").into_iter().filter(|e| e.kind == kind).collect()
}

/// The only external bot, once one exists.
fn bot(hub: &Hub) -> Option<String> {
    hub.store.bots().ok()?.into_iter().filter(|r| !r.deleted).map(|r| r.config).find(super::is_external).map(|c| c.id)
}

async fn settle(hub: &Hub) {
    for _ in 0..200 {
        if !entries(hub, "permission").is_empty() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("no approval card appeared");
}

#[tokio::test]
async fn a_prompt_starts_a_bot_named_after_the_folder() {
    let (hub, dir) = hub();
    super::ingest(&hub, &prompt("ship it")).await;
    let id = bot(&hub).expect("a bot per session");
    let row = hub.store.bot(&id).unwrap().unwrap();
    assert_eq!(row.config.name, "Claude Code · p");
    assert_eq!(row.config.backend, super::BACKEND);
    assert_eq!(hub.runtime(&id).status, BotStatus::Working);
    let users = entries(&hub, "user");
    assert_eq!((users.len(), users[0].data["text"].as_str()), (1, Some("ship it")));
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_tool_call_is_shown_and_then_settled_with_its_diff() {
    let (hub, dir) = hub();
    super::ingest(&hub, &prompt("edit")).await;
    let start = json!({"hook_event_name": "PreToolUse", "session_id": "s1", "cwd": "/w/p",
        "tool_name": "Edit", "tool_use_id": "t1", "tool_input": {"file_path": "/w/p/a.rs"}});
    super::ingest(&hub, &event(&start)).await;
    let running = entries(&hub, "tool");
    assert_eq!((running.len(), running[0].data["status"].as_str()), (1, Some("in_progress")));
    assert_eq!(running[0].data["title"], "Edit a.rs");

    let done = json!({"hook_event_name": "PostToolUse", "session_id": "s1", "cwd": "/w/p", "tool_name": "Edit",
        "tool_use_id": "t1", "tool_input": {"file_path": "/w/p/a.rs", "old_string": "a\n", "new_string": "a\nb\n"}});
    super::ingest(&hub, &event(&done)).await;
    let settled = entries(&hub, "tool");
    assert_eq!(settled.len(), 1, "the same row is completed, not a second one added");
    assert_eq!(settled[0].data["status"], "completed");
    assert_eq!(settled[0].data["diffs"][0]["added"], 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_permission_request_waits_for_a_person_and_answers_the_hook() {
    let (hub, dir) = hub();
    super::ingest(&hub, &prompt("run")).await;
    let request = event(&json!({"hook_event_name": "PermissionRequest", "session_id": "s1", "cwd": "/w/p",
        "tool_name": "Bash", "tool_input": {"command": "cargo test"}}));
    let task = tokio::spawn({
        let hub = hub.clone();
        async move { super::ingest(&hub, &request).await }
    });
    settle(&hub).await;
    let card = entries(&hub, "permission").pop().expect("an approval card");
    assert_eq!((card.data["status"].as_str(), card.data["command"].as_str()), (Some("pending"), Some("cargo test")));
    let id = bot(&hub).unwrap();
    assert_eq!(hub.runtime(&id).status, BotStatus::NeedsInput);

    assert!(hub.external.answer(&hub, &card.id, Some("allow")), "our card");
    assert_eq!(task.await.unwrap()["decision"], "allow");
    let card = hub.store.entry(&card.id).unwrap();
    assert_eq!((card.data["status"].as_str(), card.data["selected"].as_str()), (Some("answered"), Some("allow")));
    assert_eq!(hub.runtime(&id).status, BotStatus::Working);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_hook_that_goes_away_never_leaves_a_card_pending() {
    let (hub, dir) = hub();
    super::ingest(&hub, &prompt("run")).await;
    let request = event(&json!({"hook_event_name": "PermissionRequest", "session_id": "s1", "cwd": "/w/p",
        "tool_name": "Bash", "tool_input": {"command": "cargo build"}}));
    let task = tokio::spawn({
        let hub = hub.clone();
        async move { super::ingest(&hub, &request).await }
    });
    settle(&hub).await;
    // The person answered in the terminal instead, so the hook is gone.
    task.abort();
    let _ = task.await;
    let card = entries(&hub, "permission").pop().expect("a card");
    assert_eq!(card.data["status"], "expired");
    // A late tap on it goes nowhere, but isn't an error either.
    assert!(hub.external.answer(&hub, &card.id, Some("allow")));
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn agents_whose_requests_cannot_be_answered_get_no_card() {
    let (hub, dir) = hub();
    let body = json!({"agent": "gemini", "payload": {"hook_event_name": "PermissionRequest", "session_id": "g",
        "cwd": "/w/p", "tool_name": "shell"}});
    let reply = super::ingest(&hub, &body).await;
    assert_eq!(reply["decision"], Value::Null);
    assert_eq!(entries(&hub, "permission").len(), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_finished_session_says_so_and_then_leaves_the_roster() {
    let (hub, dir) = hub();
    super::ingest(&hub, &prompt("go")).await;
    let id = bot(&hub).unwrap();
    let stop = json!({"hook_event_name": "Stop", "session_id": "s1", "cwd": "/w/p",
        "last_assistant_message": "Done."});
    super::ingest(&hub, &event(&stop)).await;
    let last = entries(&hub, "agent");
    assert_eq!((last.len(), last[0].data["final"].as_bool()), (1, Some(true)));
    assert_eq!(hub.runtime(&id).status, BotStatus::Idle);

    let end = json!({"hook_event_name": "SessionEnd", "session_id": "s1", "cwd": "/w/p"});
    super::ingest(&hub, &event(&end)).await;
    assert!(hub.store.bot(&id).unwrap().unwrap().deleted);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn one_session_is_one_bot_however_many_events_it_sends() {
    let (hub, dir) = hub();
    for text in ["one", "two"] {
        super::ingest(&hub, &prompt(text)).await;
    }
    assert_eq!(hub.store.bots().unwrap().iter().filter(|r| !r.deleted).count(), 1);
    let turns: Vec<i64> = entries(&hub, "user").iter().map(|e| e.turn).collect();
    assert_eq!(turns, vec![1, 2], "each prompt is its own turn");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn deleting_the_bot_stops_watching_that_session() {
    let (hub, dir) = hub();
    super::ingest(&hub, &prompt("go")).await;
    let id = bot(&hub).unwrap();
    hub.delete_bot(&id).unwrap();
    super::ingest(&hub, &prompt("go again")).await;
    assert!(bot(&hub).is_none(), "the deleted session isn't recreated by its next event");
    // A new session in the same terminal is watched again.
    let fresh = json!({"hook_event_name": "UserPromptSubmit", "session_id": "s2", "cwd": "/w/p", "prompt": "new"});
    super::ingest(&hub, &event(&fresh)).await;
    assert!(bot(&hub).is_some());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn deleting_a_bot_frees_the_hook_waiting_on_its_card() {
    let (hub, dir) = hub();
    super::ingest(&hub, &prompt("run")).await;
    let request = event(&json!({"hook_event_name": "PermissionRequest", "session_id": "s1", "cwd": "/w/p",
        "tool_name": "Bash", "tool_input": {"command": "npm publish"}}));
    let task = tokio::spawn({
        let hub = hub.clone();
        async move { super::ingest(&hub, &request).await }
    });
    settle(&hub).await;
    let id = bot(&hub).unwrap();
    hub.delete_bot(&id).unwrap();
    // Nobody decided: the terminal asks at once, instead of waiting out the 108 s.
    assert_eq!(task.await.unwrap()["decision"], Value::Null);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_session_start_only_announces_the_bot() {
    let (hub, dir) = hub();
    let start = json!({"hook_event_name": "SessionStart", "session_id": "s1", "cwd": "/w/p"});
    super::ingest(&hub, &event(&start)).await;
    assert!(bot(&hub).is_some(), "the roster shows the session before its first prompt");
    assert_eq!(entries(&hub, "user").len(), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_forbidden_command_from_a_terminal_agent_is_refused_without_a_card() {
    let (hub, dir) = hub();
    super::ingest(&hub, &prompt("run")).await;
    let request = event(&json!({"hook_event_name": "PermissionRequest", "session_id": "s1", "cwd": "/w/p",
        "tool_name": "Bash", "tool_input": {"command": "rm -rf /"}}));
    let reply = super::ingest(&hub, &request).await;
    assert!(entries(&hub, "permission").is_empty(), "no card asks about a command nobody may run");
    assert!(reply.to_string().contains("deny"), "{reply}");
    let _ = std::fs::remove_dir_all(dir);
}

/// A `PreToolUse` of `tool` from the hook installed to wait (`hook --ask`), or from the plain one.
fn asking(tool: &str, input: &Value, ask: bool) -> Value {
    let mut body = event(&json!({"hook_event_name": "PreToolUse", "session_id": "s1", "cwd": "/w/p",
        "tool_name": tool, "tool_use_id": "q1", "tool_input": input}));
    body["ask"] = ask.into();
    body
}

async fn cards(hub: &Hub, count: usize) -> Vec<Entry> {
    for _ in 0..400 {
        let found = entries(hub, "permission");
        if found.len() >= count {
            return found;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("expected {count} cards");
}

fn two_questions() -> Value {
    json!({"questions": [
        {"question": "Which database?", "header": "DB", "multiSelect": false,
         "options": [{"label": "SQLite", "description": "one file"}, {"label": "Postgres", "description": "a server"}]},
        {"question": "Add tests?", "header": "Tests", "multiSelect": false,
         "options": [{"label": "Yes"}, {"label": "No"}]},
    ]})
}

#[tokio::test]
async fn questions_are_asked_one_card_at_a_time_and_answered_by_label() {
    let (hub, dir) = hub();
    super::ingest(&hub, &prompt("plan")).await;
    let task = tokio::spawn({
        let hub = hub.clone();
        let body = asking("AskUserQuestion", &two_questions(), true);
        async move { super::ingest(&hub, &body).await }
    });
    let first = cards(&hub, 1).await.pop().unwrap();
    assert_eq!(first.data["title"], "Which database?");
    // An option the card never showed is ignored; the card keeps waiting.
    assert!(hub.external.answer(&hub, &first.id, Some("7")));
    assert_eq!(hub.store.entry(&first.id).unwrap().data["status"], "pending");
    assert!(hub.external.answer(&hub, &first.id, Some("1")));
    let second = cards(&hub, 2).await.pop().unwrap();
    assert_eq!(second.data["title"], "Add tests?");
    // A late second tap on the first card is a no-op.
    assert!(hub.external.answer(&hub, &first.id, Some("0")));
    assert!(hub.external.answer(&hub, &second.id, Some("0")));

    let reply = task.await.unwrap();
    let verdict = &reply["preToolUse"];
    assert_eq!(verdict["permissionDecision"], "allow");
    assert_eq!(verdict["updatedInput"]["answers"], json!({"Which database?": "Postgres", "Add tests?": "Yes"}));
    assert_eq!(verdict["updatedInput"]["questions"].as_array().unwrap().len(), 2, "the questions are echoed");
    assert_eq!(hub.store.entry(&first.id).unwrap().data["selected"], "1");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_cancelled_question_leaves_the_terminal_to_ask() {
    let (hub, dir) = hub();
    super::ingest(&hub, &prompt("plan")).await;
    let task = tokio::spawn({
        let hub = hub.clone();
        let body = asking("AskUserQuestion", &two_questions(), true);
        async move { super::ingest(&hub, &body).await }
    });
    let card = cards(&hub, 1).await.pop().unwrap();
    assert!(hub.external.answer(&hub, &card.id, None));
    let reply = task.await.unwrap();
    assert!(reply.get("preToolUse").is_none(), "{reply}");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_plan_is_approved_or_rejected_from_a_card() {
    let (hub, dir) = hub();
    super::ingest(&hub, &prompt("plan")).await;
    let plan = json!({"plan": "1. Add the table\n2. Migrate"});
    let task = tokio::spawn({
        let hub = hub.clone();
        let body = asking("ExitPlanMode", &plan, true);
        async move { super::ingest(&hub, &body).await }
    });
    let card = cards(&hub, 1).await.pop().unwrap();
    assert_eq!(card.data["detail"], "1. Add the table\n2. Migrate");
    assert!(hub.external.answer(&hub, &card.id, Some("deny")));
    let reply = task.await.unwrap();
    assert_eq!(reply["preToolUse"]["permissionDecision"], "deny");
    assert!(reply["preToolUse"]["reason"].as_str().unwrap().contains("rejected"));
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn the_plain_hook_and_other_agents_never_ask() {
    let (hub, dir) = hub();
    super::ingest(&hub, &prompt("plan")).await;
    // The plain PreToolUse hook only reports: no card, and it answers at once.
    let reply = super::ingest(&hub, &asking("AskUserQuestion", &two_questions(), false)).await;
    assert!(reply.get("preToolUse").is_none());
    // A tool the waiting hook was not installed for is not asked about.
    let reply = super::ingest(&hub, &asking("Bash", &json!({"command": "ls"}), true)).await;
    assert!(reply.get("preToolUse").is_none());
    assert!(entries(&hub, "permission").is_empty());
    let _ = std::fs::remove_dir_all(dir);
}
