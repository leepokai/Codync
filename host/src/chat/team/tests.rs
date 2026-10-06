use super::*;
use crate::store::Store;
use std::path::PathBuf;

#[test]
fn wait_graph_rejects_self_duplicates_and_indirect_cycles() {
    let requests = Requests::default();
    for bot in ["a", "b", "c", "d"] {
        requests.start_turn(bot, None);
    }
    assert!(requests.begin("self", "a", "a").is_err());
    let (ab, _) = requests.begin("ab", "a", "b").unwrap();
    assert!(requests.begin("duplicate", "a", "b").is_err());
    let _bc = requests.begin("bc", "b", "c").unwrap();
    assert!(requests.begin("ca", "c", "a").is_err());
    let _dc = requests.begin("dc", "d", "c").unwrap();
    requests.cancel_from("a");
    assert!(*ab.borrow());
    assert!(requests.begin("late", "a", "d").is_err());
    requests.0.locked().pending.remove("ab");
    assert!(requests.begin("ca", "c", "a").is_ok());
}

struct Fixture {
    hub: Arc<Hub>,
    dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("codync-team-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("test.db")).unwrap();
        let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
        for id in ["a", "b", "c"] {
            let cwd = dir.join(id);
            std::fs::create_dir_all(&cwd).unwrap();
            let cfg: BotConfig = serde_json::from_value(json!({
                "id": id, "name": id, "backend": "fixture", "cwd": cwd,
                "command": crate::shell::python(&agent),
                "notify": false, "permission": "ask",
            }))
            .unwrap();
            store.save_bot(&cfg).unwrap();
        }
        let hub = Hub::new(
            store,
            "test-host".into(),
            crate::remote::identity::Identity::load_or_create(&dir).unwrap(),
            "test-token".into(),
            19222,
        );
        hub.start().unwrap();
        hub.set_runtime("a", |r| r.status = BotStatus::Working);
        hub.team.start_turn("a", None);
        Self { hub, dir }
    }

    fn request(&self, message: &str) -> tokio::task::JoinHandle<Result<Value>> {
        let hub = self.hub.clone();
        let message = message.to_owned();
        tokio::spawn(async move {
            crate::api::dispatch(
                &hub,
                &crate::api::devices::Caller::Local,
                "teamCall",
                json!({
                    "botId": "a", "name": "ask_bot", "arguments": {"botId": "b", "message": message},
                }),
            )
            .await
        })
    }

    async fn message(&self, message: &str) -> Result<Value> {
        call(&self.hub, "a", "message_bot", &json!({"botId": "b", "message": message})).await
    }

    fn notices(&self, request: &Value) -> Vec<crate::store::Entry> {
        ["a", "b"]
            .into_iter()
            .flat_map(|bot| self.hub.store.history(bot, i64::MAX, 200).unwrap())
            .filter(|e| e.data["delegationId"] == request["requestId"])
            .collect()
    }

    async fn send_user(&self, text: &str) {
        let result = crate::api::dispatch(
            &self.hub,
            &crate::api::devices::Caller::Local,
            "send",
            json!({"botId": "b", "text": text}),
        )
        .await;
        result.unwrap();
    }

    async fn until(&self, condition: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("fixture condition timed out");
    }

    fn prompts(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("b/prompts.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    async fn shutdown(self) {
        self.hub.shutdown().await;
        // Windows may still hold files of an agent that just exited.
        let _ = std::fs::remove_dir_all(self.dir);
    }
}

#[tokio::test]
async fn delegation_roundtrip_keeps_queued_user_messages_separate() {
    let f = Fixture::new();
    let request = f.request("BLOCK review these changes");
    f.until(|| f.prompts().len() == 1).await;
    let sent = crate::api::dispatch(
        &f.hub,
        &crate::api::devices::Caller::Local,
        "send",
        json!({"botId": "b", "text": "thanks"}),
    )
    .await
    .unwrap();
    let request_entry = f
        .hub
        .store
        .history("b", i64::MAX, 100)
        .unwrap()
        .into_iter()
        .find(|e| e.data["delegationId"].is_string())
        .unwrap();
    assert!(request_entry.data["text"].as_str().unwrap().contains("Request from a"));
    assert!(f.hub.store.kv_get("turn.inflight.b").is_none_or(|v| v.is_empty()));
    std::fs::write(f.dir.join("b/release"), "").unwrap();
    let result = request.await.unwrap().unwrap();
    assert_eq!(result["reply"], "reply: BLOCK review these changes");
    f.until(|| f.prompts().len() == 2 && f.hub.runtime("b").status == BotStatus::Idle).await;
    assert_eq!(f.prompts()[1], "thanks");
    assert_eq!(f.hub.store.entry(sent["entry"]["id"].as_str().unwrap()).unwrap().data["status"], "sent");
    assert_eq!(f.hub.store.entry(&request_entry.id).unwrap().data["status"], "completed");
    assert!(f.hub.team.0.locked().pending.is_empty());
    let servers: Value = serde_json::from_slice(&std::fs::read(f.dir.join("b/servers.json")).unwrap()).unwrap();
    assert!(servers.as_array().unwrap().iter().any(|s| s["name"] == "team"));
    f.shutdown().await;
}

#[tokio::test]
async fn permission_cards_still_require_the_recipients_approval() {
    let f = Fixture::new();
    let request = f.request("PERMISSION inspect the project");
    f.until(|| f.hub.runtime("b").status == BotStatus::NeedsInput).await;
    assert!(!request.is_finished());
    let cycle = call(&f.hub, "b", "ask_bot", &json!({"botId": "a", "message": "help me back"})).await;
    assert!(cycle.unwrap_err().to_string().contains("wait for each other"));
    let card = f.hub.store.history("b", i64::MAX, 100).unwrap().into_iter().find(|e| e.kind == "permission").unwrap();
    crate::api::dispatch(
        &f.hub,
        &crate::api::devices::Caller::Local,
        "respondPermission",
        json!({"entryId": card.id, "optionId": "allow"}),
    )
    .await
    .unwrap();
    assert!(request.await.unwrap().is_ok());
    f.shutdown().await;
}

#[tokio::test]
async fn stopping_requester_cancels_only_its_delegation() {
    let f = Fixture::new();
    let request = f.request("BLOCK waiting for cancellation");
    f.until(|| f.prompts().len() == 1).await;
    crate::api::dispatch(&f.hub, &crate::api::devices::Caller::Local, "send", json!({"botId": "b", "text": "thanks"}))
        .await
        .unwrap();
    f.hub.send_cmd("a", Cmd::Stop).unwrap();
    assert!(request.await.unwrap().unwrap_err().to_string().contains("cancelled"));
    f.until(|| f.prompts().len() == 2 && f.hub.runtime("b").status == BotStatus::Idle).await;
    assert_eq!(f.prompts()[1], "thanks");
    assert!(f.hub.team.0.locked().pending.is_empty());
    f.shutdown().await;
}

#[tokio::test]
async fn timeout_and_recipient_errors_release_waiters() {
    let f = Fixture::new();
    for message in ["FAIL requested failure", "EMPTY no text"] {
        assert!(f.request(message).await.unwrap().is_err());
        assert!(f.hub.team.0.locked().pending.is_empty());
    }
    let source = visible_bot(&f.hub, "a").unwrap();
    let error = ask(&f.hub, &source, "b", "BLOCK timed request", Duration::from_millis(100)).await.unwrap_err();
    assert!(error.to_string().contains("timed out"));
    f.until(|| f.hub.runtime("b").status == BotStatus::Idle).await;
    assert!(f.hub.team.0.locked().pending.is_empty());
    assert!(f.request("after timeout").await.unwrap().is_ok());
    f.shutdown().await;
}

#[tokio::test]
async fn invalid_hidden_and_deleted_targets_never_start() {
    let f = Fixture::new();
    let mut hidden = visible_bot(&f.hub, "c").unwrap();
    hidden.hidden = true;
    f.hub.store.save_bot(&hidden).unwrap();
    let list = call(&f.hub, "a", "list_bots", &json!({})).await.unwrap();
    assert_eq!(list["bots"].as_array().unwrap().len(), 1);
    assert_eq!(list["bots"][0]["id"], "b");
    for target in ["a", "c", "missing"] {
        assert!(call(&f.hub, "a", "ask_bot", &json!({"botId": target, "message": "do work"})).await.is_err());
    }
    assert!(f.request(" ").await.unwrap().is_err());
    f.hub.delete_bot("b").unwrap();
    assert!(f.request("deleted").await.unwrap().is_err());
    assert_eq!(f.prompts().len(), 0);
    f.shutdown().await;
}

#[tokio::test]
async fn queued_requests_are_not_merged_and_cancelled_requests_never_execute() {
    let f = Fixture::new();
    f.hub.set_runtime("c", |r| r.status = BotStatus::Working);
    f.hub.team.start_turn("c", None);
    let first = f.request("BLOCK first request");
    f.until(|| f.prompts().len() == 1).await;
    let hub = f.hub.clone();
    let second =
        tokio::spawn(
            async move { call(&hub, "c", "ask_bot", &json!({"botId": "b", "message": "second request"})).await },
        );
    f.until(|| f.hub.team.0.locked().pending.len() == 2).await;
    assert_eq!(f.prompts().len(), 1);
    std::fs::write(f.dir.join("b/release"), "").unwrap();
    assert_eq!(first.await.unwrap().unwrap()["reply"], "reply: BLOCK first request");
    assert_eq!(second.await.unwrap().unwrap()["reply"], "reply: second request");
    assert_eq!(f.prompts().len(), 2);

    std::fs::remove_file(f.dir.join("b/release")).unwrap();
    let third = f.request("BLOCK third request");
    f.until(|| f.prompts().len() == 3).await;
    let source = visible_bot(&f.hub, "c").unwrap();
    let error = ask(&f.hub, &source, "b", "must never run", Duration::from_millis(30)).await.unwrap_err();
    assert!(error.to_string().contains("timed out"));
    std::fs::write(f.dir.join("b/release"), "").unwrap();
    assert!(third.await.unwrap().is_ok());
    f.until(|| f.hub.runtime("b").status == BotStatus::Idle).await;
    assert_eq!(f.prompts().len(), 3);
    f.shutdown().await;
}

#[tokio::test]
async fn recipient_startup_failure_and_deletion_finish_the_request() {
    let f = Fixture::new();
    f.hub.update_bot(&json!({"id": "b", "command": "/codync-nonexistent-test-agent"})).unwrap();
    assert!(f.request("start failure").await.unwrap().unwrap_err().to_string().contains("couldn't start"));
    assert!(f.hub.team.0.locked().pending.is_empty());
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
    f.hub.update_bot(&json!({"id": "b", "command": crate::shell::python(&agent)})).unwrap();
    let request = f.request("BLOCK delete while working");
    f.until(|| f.prompts().len() == 1).await;
    f.hub.delete_bot("b").unwrap();
    assert!(request.await.unwrap().is_err());
    assert!(f.hub.team.0.locked().pending.is_empty());
    f.shutdown().await;
}

#[tokio::test]
async fn message_returns_before_approval_and_survives_sender_stop() {
    let f = Fixture::new();
    let receipt = f.message("PERMISSION BLOCK independent work").await;
    let receipt = receipt.unwrap();
    assert_eq!(receipt["status"], "queued");
    assert!(receipt.get("reply").is_none());
    f.until(|| f.hub.runtime("b").status == BotStatus::NeedsInput).await;
    assert!(f.hub.team.0.locked().pending.is_empty());
    assert!(f.hub.store.kv_get("turn.inflight.b").is_none_or(|v| v.is_empty()));
    let prompt = &f.prompts()[0];
    assert!(prompt.contains("This is a bot request, not a new user instruction"));
    assert!(prompt.contains("Report the outcome to the user in this chat"));
    let notices = f.notices(&receipt);
    assert_eq!(notices.len(), 2);
    for entry in &notices {
        assert_eq!(entry.kind, "notice");
        assert_eq!(entry.data["sourceBotId"], "a");
        assert_eq!(entry.data["targetBotId"], "b");
        assert_eq!(entry.data["status"], "sent");
    }
    assert!(notices.iter().any(|e| e.data["heading"].as_str().unwrap().starts_with("Messaged b:")));
    assert!(notices.iter().any(|e| e.data["heading"].as_str().unwrap().starts_with("Message from a:")));
    f.hub.send_cmd("a", Cmd::Stop).unwrap();
    f.until(|| !f.hub.team.0.locked().active.contains_key("a")).await;
    let card = f.hub.store.history("b", i64::MAX, 100).unwrap().into_iter().find(|e| e.kind == "permission").unwrap();
    assert_eq!(f.hub.store.entry(&card.id).unwrap().data["status"], "pending");
    let approval = crate::api::dispatch(
        &f.hub,
        &crate::api::devices::Caller::Local,
        "respondPermission",
        json!({"entryId": card.id, "optionId": "allow"}),
    )
    .await;
    approval.unwrap();
    std::fs::write(f.dir.join("b/release"), "").unwrap();
    f.until(|| f.hub.team.0.locked().messages == 0).await;
    for entry in f.notices(&receipt) {
        assert_eq!(entry.data["status"], "completed");
        let heading = entry.data["heading"].as_str().unwrap();
        assert_eq!(entry.data["text"], format!("{heading}\nCompleted."));
    }
    for bot in ["a", "b"] {
        assert!(f.hub.store.history(bot, i64::MAX, 100).unwrap().iter().all(|e| e.kind != "user"));
    }
    let history = f.hub.store.history("b", i64::MAX, 100).unwrap();
    assert!(history.iter().any(|e| e.kind == "agent"
        && e.data["final"] == true
        && e.data["text"] == "reply: PERMISSION BLOCK independent work"));
    f.shutdown().await;
}

#[tokio::test]
async fn cancelling_an_ask_preserves_the_independent_message_behind_it() {
    let f = Fixture::new();
    let ask = f.request("BLOCK synchronous work");
    f.until(|| f.prompts().len() == 1).await;
    let receipt = f.message("independent work").await;
    let receipt = receipt.unwrap();
    assert!(f.notices(&receipt).iter().all(|e| e.data["status"] == "queued"));
    assert_eq!(f.hub.team.0.locked().pending.len(), 1);
    f.hub.send_cmd("a", Cmd::Stop).unwrap();
    let result = ask.await;
    assert!(result.unwrap().unwrap_err().to_string().contains("cancelled"));
    f.until(|| f.hub.team.0.locked().messages == 0).await;
    assert_eq!(f.prompts().len(), 2);
    assert!(f.prompts()[1].ends_with("\n\nindependent work"));
    assert!(f.notices(&receipt).iter().all(|e| e.data["status"] == "completed"));
    f.shutdown().await;
}

#[tokio::test]
async fn recipient_stop_cancels_queued_messages_before_execution() {
    let f = Fixture::new();
    f.send_user("BLOCK user work").await;
    f.until(|| f.prompts().len() == 1).await;
    let first = f.message("must never run").await;
    let first = first.unwrap();
    let second = f.message("also must never run").await;
    let second = second.unwrap();
    f.hub.send_cmd("b", Cmd::Stop).unwrap();
    f.until(|| f.hub.team.0.locked().messages == 0 && f.hub.runtime("b").status == BotStatus::Idle).await;
    for receipt in [&first, &second] {
        let notices = f.notices(receipt);
        assert_eq!(notices.len(), 2);
        assert!(notices.iter().all(|e| e.data["status"] == "cancelled"
            && e.data["text"].as_str().unwrap().ends_with("Cancelled before execution.")));
    }
    let later = f.message("new work after Stop").await;
    let later = later.unwrap();
    f.until(|| f.hub.team.0.locked().messages == 0).await;
    assert_eq!(f.prompts().len(), 2);
    assert!(f.prompts()[1].ends_with("\n\nnew work after Stop"));
    assert!(f.notices(&later).iter().all(|e| e.data["status"] == "completed"));
    f.shutdown().await;
}

#[tokio::test]
async fn recipient_stop_cancels_running_message_and_releases_capacity() {
    let f = Fixture::new();
    let receipt = f.message("BLOCK running work").await;
    let receipt = receipt.unwrap();
    f.until(|| f.prompts().len() == 1).await;
    f.hub.send_cmd("b", Cmd::Stop).unwrap();
    f.until(|| f.hub.team.0.locked().messages == 0).await;
    assert!(f.notices(&receipt).iter().all(|e| e.data["status"] == "cancelled"
        && e.data["text"].as_str().unwrap().contains("Partial work may have happened")));
    f.shutdown().await;
}

#[tokio::test]
async fn mixed_requests_preserve_fifo_and_user_batch_boundaries() {
    let f = Fixture::new();
    f.send_user("BLOCK current turn").await;
    f.until(|| f.prompts().len() == 1).await;
    f.send_user("first user").await;
    f.send_user("second user").await;
    let first = f.message("first message").await;
    let first = first.unwrap();
    let ask = f.request("synchronous request");
    f.until(|| f.hub.team.0.locked().pending.len() == 1).await;
    let second = f.message("second message").await;
    let second = second.unwrap();
    f.send_user("third user").await;
    f.send_user("fourth user").await;
    std::fs::write(f.dir.join("b/release"), "").unwrap();
    let result = ask.await;
    assert_eq!(result.unwrap().unwrap()["reply"], "reply: synchronous request");
    f.until(|| f.prompts().len() == 6 && f.hub.runtime("b").status == BotStatus::Idle).await;
    let prompts = f.prompts();
    assert_eq!(prompts[1], "first user\n\nsecond user");
    assert!(prompts[2].ends_with("\n\nfirst message"));
    assert!(prompts[3].ends_with("\n\nsynchronous request"));
    assert!(prompts[4].ends_with("\n\nsecond message"));
    assert_eq!(prompts[5], "third user\n\nfourth user");
    for receipt in [&first, &second] {
        assert!(f.notices(receipt).iter().all(|e| e.data["status"] == "completed"));
    }
    f.shutdown().await;
}

#[tokio::test]
async fn message_failures_finish_notices_and_release_capacity() {
    let f = Fixture::new();
    for message in ["FAIL agent error", "REFUSE requested refusal", "EMPTY no report"] {
        let receipt = f.message(message).await;
        let receipt = receipt.unwrap();
        f.until(|| f.hub.team.0.locked().messages == 0).await;
        assert!(f.notices(&receipt).iter().all(|e| e.data["status"] == "failed"));
    }
    f.hub.update_bot(&json!({"id": "b", "command": "/codync-nonexistent-test-agent"})).unwrap();
    let receipt = f.message("startup failure").await;
    let receipt = receipt.unwrap();
    f.until(|| f.hub.team.0.locked().messages == 0).await;
    assert!(
        f.notices(&receipt)
            .iter()
            .all(|e| e.data["status"] == "failed"
                && e.data["text"].as_str().unwrap().contains("couldn't start recipient"))
    );
    f.shutdown().await;
}

#[tokio::test]
async fn failed_message_admission_never_executes_or_leaks_capacity() {
    let f = Fixture::new();
    let db = rusqlite::Connection::open(f.dir.join("test.db")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER reject_message BEFORE INSERT ON entries
        WHEN NEW.bot_id = 'b' AND NEW.kind = 'notice'
        BEGIN SELECT RAISE(ABORT, 'fixture save failure'); END;",
    )
    .unwrap();
    let result = f.message("cannot save recipient notice").await;
    assert!(result.is_err());
    assert_eq!(f.hub.team.0.locked().messages, 0);
    let history = f.hub.store.history("a", i64::MAX, 100).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].data["status"], "cancelled");
    db.execute_batch("DROP TRIGGER reject_message;").unwrap();
    f.hub.send_cmd("b", Cmd::Shutdown).unwrap();
    f.until(|| f.hub.send_cmd("b", Cmd::RefreshTools).is_err()).await;
    let result = f.message("recipient actor has exited").await;
    assert!(result.is_err());
    assert_eq!(f.hub.team.0.locked().messages, 0);
    assert_eq!(f.prompts().len(), 0);
    assert!(f.hub.store.history("a", i64::MAX, 100).unwrap().iter().all(|e| e.data["status"] == "cancelled"));
    drop(db);
    f.shutdown().await;
}

#[path = "quota_tests.rs"]
mod quotas;

#[tokio::test]
async fn shutdown_discards_accepted_mailbox_messages_and_finishes_notices() {
    let f = Fixture::new();
    // Admission is synchronous: these commands enter the mailbox in order
    // before the actor runs, so shutdown disposes of the unread request.
    f.hub.send_cmd("b", Cmd::Shutdown).unwrap();
    let receipt = f.message("must never start").await;
    let receipt = receipt.unwrap();
    f.until(|| f.hub.team.0.locked().messages == 0).await;
    assert!(f.notices(&receipt).iter().all(|e| e.data["status"] == "cancelled"));
    assert_eq!(f.prompts().len(), 0);
    f.shutdown().await;
}

#[tokio::test]
async fn invalid_messages_never_start_and_stopped_senders_cannot_admit_more() {
    let f = Fixture::new();
    let mut hidden = visible_bot(&f.hub, "c").unwrap();
    hidden.hidden = true;
    f.hub.store.save_bot(&hidden).unwrap();
    for target in ["a", "c", "missing"] {
        let result = call(&f.hub, "a", "message_bot", &json!({"botId": target, "message": "work"})).await;
        assert!(result.is_err());
    }
    for message in [" ".to_owned(), "x".repeat(MAX_MESSAGE_BYTES + 1)] {
        let result = f.message(&message).await;
        assert!(result.is_err());
    }
    f.hub.team.cancel_from("a");
    let result = f.message("late request").await;
    assert!(result.unwrap_err().to_string().contains("no longer working"));
    f.hub.team.start_turn("a", None);
    f.hub.delete_bot("b").unwrap();
    let result = f.message("deleted recipient").await;
    assert!(result.is_err());
    assert_eq!(f.hub.team.0.locked().messages, 0);
    assert_eq!(f.prompts().len(), 0);
    f.shutdown().await;
}

#[tokio::test]
async fn sender_deletion_preserves_messages_and_recipient_deletion_closes_them() {
    let f = Fixture::new();
    let running = f.message("BLOCK active message").await;
    let running = running.unwrap();
    f.until(|| f.prompts().len() == 1).await;
    let queued = f.message("queued message").await;
    let queued = queued.unwrap();
    f.hub.delete_bot("a").unwrap();
    assert_eq!(f.hub.team.0.locked().messages, 2);
    assert_eq!(f.hub.runtime("b").status, BotStatus::Working);
    std::fs::write(f.dir.join("b/release"), "").unwrap();
    f.until(|| f.hub.team.0.locked().messages == 0).await;
    for receipt in [&running, &queued] {
        assert!(f.notices(receipt).iter().all(|e| e.data["status"] == "completed"));
    }
    f.shutdown().await;

    let f = Fixture::new();
    let running = f.message("BLOCK active message").await;
    let running = running.unwrap();
    f.until(|| f.prompts().len() == 1).await;
    let queued = f.message("queued message").await;
    let queued = queued.unwrap();
    f.hub.delete_bot("b").unwrap();
    f.until(|| f.hub.team.0.locked().messages == 0).await;
    for receipt in [&running, &queued] {
        assert!(f.notices(receipt).iter().all(|e| e.data["status"] == "cancelled"));
    }
    assert_eq!(f.prompts().len(), 1);
    f.shutdown().await;
}

#[tokio::test]
async fn message_turns_have_no_resume_marker_and_restart_expires_both_notices() {
    let f = Fixture::new();
    let running = f.message("BLOCK active message").await;
    let running = running.unwrap();
    f.until(|| f.prompts().len() == 1).await;
    let queued = f.message("queued message").await;
    let queued = queued.unwrap();
    assert!(f.hub.store.kv_get("turn.inflight.b").is_none_or(|v| v.is_empty()));
    // A new store connection applies the same startup cleanup to the
    // persisted queued and running notices, without replaying a prompt.
    let store = Store::open(&f.dir.join("test.db")).unwrap();
    assert_eq!(store.expire_pending().unwrap(), 4);
    for receipt in [&running, &queued] {
        assert!(f.notices(receipt).iter().all(|e| e.data["status"] == "failed"
            && e.data["text"].as_str().unwrap().contains("Interrupted by host restart")));
    }
    assert_eq!(f.prompts().len(), 1);
    drop(store);
    f.shutdown().await;
}

#[test]
fn restart_marks_requests_interrupted_without_replaying_them() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let pending = store
        .insert_entry(
            &crate::store::Lane::main("b"),
            EntryKind::Notice,
            1,
            &json!({
                "delegationId": "d", "status": "sent", "heading": "Request from a",
            }),
        )
        .unwrap();
    let complete = store
        .insert_entry(
            &crate::store::Lane::main("a"),
            EntryKind::Notice,
            1,
            &json!({
                "delegationId": "done", "status": "completed",
            }),
        )
        .unwrap();
    store.expire_pending().unwrap();
    let interrupted = store.entry(&pending.id).unwrap();
    assert_eq!(interrupted.data["status"], "failed");
    assert!(interrupted.data["text"].as_str().unwrap().contains("Interrupted by host restart"));
    assert_eq!(store.entry(&complete.id).unwrap().data["status"], "completed");
    assert!(interrupted.rev > pending.rev);
}
