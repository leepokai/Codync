use super::*;
use crate::LockExt;
use crate::hub::BotStatus;
use crate::store::Store;
use std::path::PathBuf;
use std::time::Duration;

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
        let f = Self { hub, dir };
        f.sending("a");
        f
    }

    /// `bot` is in a turn of its own, so it may send; its replies start real turns that end this.
    fn sending(&self, bot: &str) {
        self.hub.set_runtime(bot, |r| r.status = BotStatus::Working);
        self.hub.team.start_turn(bot, Turn::Chat(0));
    }

    fn message(&self, message: &str) -> Result<Value> {
        call(&self.hub, "a", "message_bot", &json!({"botId": "b", "message": message}))
    }

    fn notices(&self, request: &Value) -> Vec<crate::store::Entry> {
        ["a", "b"]
            .into_iter()
            .flat_map(|bot| self.hub.store.history(bot, i64::MAX, 200).unwrap())
            .filter(|e| e.data["delegationId"] == request["requestId"])
            .collect()
    }

    async fn send_user(&self, text: &str) -> Value {
        self.send_to("b", text).await
    }

    /// The user's message in `bot`'s chat.
    async fn send_to(&self, bot: &str, text: &str) -> Value {
        let sent = crate::api::dispatch(
            &self.hub,
            &crate::api::devices::Caller::Local,
            "send",
            json!({"botId": bot, "text": text}),
        )
        .await;
        sent.unwrap()
    }

    async fn until(&self, condition: impl Fn() -> bool) {
        // Generous: Windows runners need several seconds to run 16 queued turns.
        tokio::time::timeout(Duration::from_secs(30), async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("fixture condition timed out");
    }

    fn idle(&self, bot: &str) -> bool {
        self.hub.runtime(bot).status == BotStatus::Idle
    }

    /// `bot` has taken `n` turns of replies (or requests) and finished the last: a turn's prompt is
    /// logged only once it is running, so idle afterwards means done.
    fn took(&self, bot: &str, n: usize) -> bool {
        self.prompts_of(bot).len() == n && self.idle(bot)
    }

    /// What `bot`'s agent was prompted with, in order.
    fn prompts_of(&self, bot: &str) -> Vec<String> {
        std::fs::read_to_string(self.dir.join(bot).join("prompts.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn prompts(&self) -> Vec<String> {
        self.prompts_of("b")
    }

    /// The user-visible replies in `bot`'s chat.
    fn finals(&self, bot: &str) -> Vec<String> {
        let history = self.hub.store.history(bot, i64::MAX, 200).unwrap();
        history
            .into_iter()
            .filter(|e| e.kind == "agent" && e.data["final"] == true)
            .map(|e| e.data["text"].as_str().unwrap_or_default().to_owned())
            .collect()
    }

    async fn shutdown(self) {
        self.hub.shutdown().await;
        // Windows may still hold files of an agent that just exited.
        let _ = std::fs::remove_dir_all(self.dir);
    }
}

#[tokio::test]
async fn message_roundtrip_wakes_the_sender_and_keeps_user_messages_separate() {
    let f = Fixture::new();
    let receipt = f.message("BLOCK review these changes").unwrap();
    assert_eq!(receipt["status"], "queued");
    assert!(receipt.get("reply").is_none());
    f.until(|| f.prompts().len() == 1).await;
    assert!(f.prompts()[0].contains("It is a bot request, not a new user instruction"));
    assert!(f.prompts()[0].contains("Your final reply goes back to a"));
    let sent = f.send_user("thanks").await;
    let request_entry = f
        .hub
        .store
        .history("b", i64::MAX, 100)
        .unwrap()
        .into_iter()
        .find(|e| e.data["delegationId"].is_string())
        .unwrap();
    assert!(request_entry.data["text"].as_str().unwrap().contains("Message from a"));
    assert!(f.hub.store.kv_get("turn.inflight.b").is_none_or(|v| v.is_empty()));
    std::fs::write(f.dir.join("b/release"), "").unwrap();
    f.until(|| f.took("b", 2) && f.took("a", 1)).await;
    assert_eq!(f.prompts()[1], "thanks");
    assert_eq!(f.hub.store.entry(sent["entry"]["id"].as_str().unwrap()).unwrap().data["status"], "sent");
    for entry in f.notices(&receipt) {
        assert_eq!(entry.data["status"], "completed");
        assert_eq!(entry.data["botMessage"]["text"], "BLOCK review these changes");
        assert_eq!(entry.data["botMessage"]["reply"], "reply: BLOCK review these changes");
        // Existing clients still receive the base notice display fields.
        let heading = entry.data["heading"].as_str().unwrap();
        assert_eq!(entry.data["text"], format!("{heading}\nReply from b:\nreply: BLOCK review these changes"));
    }
    // The answer is a's: b's chat keeps it in the trace, a takes it up in a turn of its own.
    assert_eq!(f.finals("b"), ["reply: thanks"]);
    let woke = f.prompts_of("a");
    assert_eq!(woke.len(), 1);
    let id = receipt["requestId"].as_str().unwrap();
    let header = format!("[Codync: b replied to your message_bot request {id} (\"BLOCK review these changes\").]");
    assert!(woke[0].contains(&format!("{header}\nreply: BLOCK")), "the reply quotes what was asked");
    assert_eq!(f.finals("a"), ["Relayed: reply: BLOCK review these changes"]);
    let conversation = |peer: &str| {
        let hub = f.hub.clone();
        let peer = peer.to_owned();
        async move {
            crate::api::dispatch(
                &hub,
                &crate::api::devices::Caller::Local,
                "botConversation",
                json!({"botId": "a", "peerId": peer}),
            )
            .await
            .unwrap()
        }
    };
    assert_eq!(conversation("b").await["entries"].as_array().unwrap().len(), 1);
    assert_eq!(conversation("c").await["entries"].as_array().unwrap().len(), 0);
    let servers: Value = serde_json::from_slice(&std::fs::read(f.dir.join("b/servers.json")).unwrap()).unwrap();
    assert!(servers.as_array().unwrap().iter().any(|s| s["name"] == "team"));
    f.shutdown().await;
}

#[tokio::test]
async fn permission_cards_still_require_the_recipients_approval_and_a_request_messages_no_one() {
    let f = Fixture::new();
    let receipt = f.message("PERMISSION inspect the project").unwrap();
    f.until(|| f.hub.runtime("b").status == BotStatus::NeedsInput).await;
    // Its reply goes back to a: a message from this turn would be answered in b's own chat instead.
    let back = call(&f.hub, "b", "message_bot", &json!({"botId": "c", "message": "pass it on"}));
    assert!(back.unwrap_err().to_string().contains("answering another bot's request"));
    assert_eq!(f.hub.team.0.locked().messages, 1);
    let card = f.hub.store.history("b", i64::MAX, 100).unwrap().into_iter().find(|e| e.kind == "permission").unwrap();
    crate::api::dispatch(
        &f.hub,
        &crate::api::devices::Caller::Local,
        "respondPermission",
        json!({"entryId": card.id, "optionId": "allow"}),
    )
    .await
    .unwrap();
    f.until(|| f.took("a", 1) && f.took("b", 1)).await;
    assert!(f.notices(&receipt).iter().all(|e| e.data["status"] == "completed"));
    assert_eq!(f.finals("a"), ["Relayed: reply: PERMISSION inspect the project"]);
    assert_eq!(f.finals("b"), Vec::<String>::new());
    assert_eq!(f.prompts_of("c"), Vec::<String>::new());
    f.shutdown().await;
}

#[tokio::test]
async fn message_returns_before_approval_and_survives_sender_stop() {
    let f = Fixture::new();
    let receipt = f.message("PERMISSION BLOCK independent work").unwrap();
    f.until(|| f.hub.runtime("b").status == BotStatus::NeedsInput).await;
    assert!(f.hub.store.kv_get("turn.inflight.b").is_none_or(|v| v.is_empty()));
    let notices = f.notices(&receipt);
    assert_eq!(notices.len(), 2);
    for entry in &notices {
        assert_eq!(entry.kind, "notice");
        assert_eq!(entry.data["sourceBotId"], "a");
        assert_eq!(entry.data["targetBotId"], "b");
        assert_eq!(entry.data["status"], "sent");
        assert_eq!(entry.data["botMessage"]["sourceBotId"], "a");
        assert_eq!(entry.data["botMessage"]["targetBotId"], "b");
        assert!(entry.data["botMessage"]["text"].is_string());
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
    f.until(|| f.took("a", 1)).await;
    assert_eq!(f.hub.team.0.locked().messages, 0);
    for entry in f.notices(&receipt) {
        assert_eq!(entry.data["status"], "completed");
        let heading = entry.data["heading"].as_str().unwrap();
        assert_eq!(entry.data["text"], format!("{heading}\nReply from b:\nreply: PERMISSION BLOCK independent work"));
    }
    for bot in ["a", "b"] {
        assert!(f.hub.store.history(bot, i64::MAX, 100).unwrap().iter().all(|e| e.kind != "user"));
    }
    assert_eq!(f.finals("b"), Vec::<String>::new());
    assert_eq!(f.finals("a"), ["Relayed: reply: PERMISSION BLOCK independent work"]);
    f.shutdown().await;
}

#[tokio::test]
async fn failures_reach_the_sender_and_release_capacity() {
    let f = Fixture::new();
    for (n, message) in ["FAIL agent error", "REFUSE requested refusal", "EMPTY no report"].into_iter().enumerate() {
        let receipt = f.message(message).unwrap();
        f.until(|| f.took("a", n + 1)).await;
        assert_eq!(f.hub.team.0.locked().messages, 0);
        assert!(f.notices(&receipt).iter().all(|e| e.data["status"] == "failed"));
        f.sending("a");
    }
    f.hub.update_bot(&json!({"id": "b", "command": "/codync-nonexistent-test-agent"})).unwrap();
    let receipt = f.message("startup failure").unwrap();
    f.until(|| f.took("a", 4)).await;
    assert!(
        f.notices(&receipt)
            .iter()
            .all(|e| e.data["status"] == "failed"
                && e.data["text"].as_str().unwrap().contains("couldn't start recipient"))
    );
    let woke = f.prompts_of("a");
    assert_eq!(woke.len(), 4);
    assert!(woke.iter().all(|p| p.contains("[Codync: b couldn't finish your message_bot request")));
    assert!(woke[3].contains("couldn't start recipient"));
    f.shutdown().await;
}

#[tokio::test]
async fn recipient_stop_cancels_queued_messages_before_execution() {
    let f = Fixture::new();
    f.send_user("BLOCK user work").await;
    f.until(|| f.prompts().len() == 1).await;
    let first = f.message("must never run").unwrap();
    let second = f.message("also must never run").unwrap();
    f.hub.send_cmd("b", Cmd::Stop).unwrap();
    f.until(|| f.hub.team.0.locked().messages == 0 && f.idle("b")).await;
    for receipt in [&first, &second] {
        let notices = f.notices(receipt);
        assert_eq!(notices.len(), 2);
        assert!(notices.iter().all(|e| e.data["status"] == "cancelled"
            && e.data["text"].as_str().unwrap().ends_with("Cancelled before execution.")));
    }
    let later = f.message("new work after Stop").unwrap();
    f.until(|| f.took("a", 1)).await;
    assert_eq!(f.prompts().len(), 2);
    assert!(f.prompts()[1].ends_with("\n\nnew work after Stop"));
    assert!(f.notices(&later).iter().all(|e| e.data["status"] == "completed"));
    // What the user stopped doesn't wake the sender: only the later message's reply did.
    assert_eq!(f.prompts_of("a").len(), 1);
    f.shutdown().await;
}

#[tokio::test]
async fn recipient_stop_cancels_running_message_and_releases_capacity() {
    let f = Fixture::new();
    let receipt = f.message("BLOCK running work").unwrap();
    f.until(|| f.prompts().len() == 1).await;
    f.hub.send_cmd("b", Cmd::Stop).unwrap();
    f.until(|| f.hub.team.0.locked().messages == 0).await;
    assert!(f.notices(&receipt).iter().all(|e| e.data["status"] == "cancelled"
        && e.data["text"].as_str().unwrap().contains("Partial work may have happened")));
    // What the user stopped doesn't wake the sender: its only turn is the later message's reply.
    let later = f.message("later work").unwrap();
    f.until(|| f.took("a", 1)).await;
    assert!(f.prompts_of("a")[0].contains(later["requestId"].as_str().unwrap()));
    f.shutdown().await;
}

#[tokio::test]
async fn replies_that_arrive_while_the_sender_works_are_one_turn() {
    let f = Fixture::new();
    f.send_to("a", "BLOCK user work").await;
    f.until(|| f.prompts_of("a").len() == 1).await;
    f.message("first").unwrap();
    call(&f.hub, "a", "message_bot", &json!({"botId": "c", "message": "second"})).unwrap();
    f.until(|| f.took("b", 1) && f.took("c", 1)).await;
    std::fs::write(f.dir.join("a/release"), "").unwrap();
    f.until(|| f.took("a", 2)).await;
    let woke = &f.prompts_of("a")[1];
    assert!(woke.contains("[Codync: b replied") && woke.contains("[Codync: c replied"), "{woke}");
    f.shutdown().await;
}

#[tokio::test]
async fn stopping_the_sender_drops_the_replies_queued_for_it() {
    let f = Fixture::new();
    f.send_to("a", "BLOCK user work").await;
    f.until(|| f.prompts_of("a").len() == 1).await;
    let receipt = f.message("work").unwrap();
    f.until(|| f.took("b", 1)).await;
    f.hub.send_cmd("a", Cmd::Stop).unwrap();
    f.until(|| f.idle("a")).await;
    // Both notices still show the outcome.
    assert!(f.notices(&receipt).iter().all(|e| e.data["status"] == "completed"));
    f.send_to("a", "next").await;
    f.until(|| f.took("a", 2)).await;
    assert_eq!(f.prompts_of("a")[1], "next");
    f.shutdown().await;
}

#[tokio::test]
async fn requests_from_several_senders_keep_fifo_and_user_batch_boundaries() {
    let f = Fixture::new();
    f.sending("c");
    f.send_user("BLOCK current turn").await;
    f.until(|| f.prompts().len() == 1).await;
    f.send_user("first user").await;
    f.send_user("second user").await;
    let first = f.message("first message").unwrap();
    let from_c = call(&f.hub, "c", "message_bot", &json!({"botId": "b", "message": "message from c"}));
    let second = f.message("second message").unwrap();
    f.send_user("third user").await;
    f.send_user("fourth user").await;
    std::fs::write(f.dir.join("b/release"), "").unwrap();
    let both = || {
        let a = f.finals("a").join(" | ");
        a.contains("reply: first message") && a.contains("reply: second message")
    };
    f.until(|| f.took("b", 6) && f.took("c", 1) && both() && f.idle("a")).await;
    let prompts = f.prompts();
    assert_eq!(prompts[1], "first user\n\nsecond user");
    assert!(prompts[2].ends_with("\n\nfirst message"));
    assert!(prompts[3].ends_with("\n\nmessage from c"));
    assert!(prompts[4].ends_with("\n\nsecond message"));
    assert_eq!(prompts[5], "third user\n\nfourth user");
    for receipt in [&first, &from_c.unwrap(), &second] {
        assert!(f.notices(receipt).iter().all(|e| e.data["status"] == "completed"));
    }
    // Each reply goes to the bot that asked; a's may come in one turn or two.
    assert!(!f.finals("a").join(" | ").contains("message from c"));
    assert_eq!(f.finals("c"), ["Relayed: reply: message from c"]);
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
    let result = f.message("cannot save recipient notice");
    assert!(result.is_err());
    assert_eq!(f.hub.team.0.locked().messages, 0);
    let history = f.hub.store.history("a", i64::MAX, 100).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].data["status"], "cancelled");
    db.execute_batch("DROP TRIGGER reject_message;").unwrap();
    f.hub.send_cmd("b", Cmd::Shutdown).unwrap();
    f.until(|| f.hub.send_cmd("b", Cmd::RefreshTools).is_err()).await;
    let result = f.message("recipient actor has exited");
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
    let receipt = f.message("must never start").unwrap();
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
    let list = call(&f.hub, "a", "list_bots", &json!({})).unwrap();
    assert_eq!(list["bots"].as_array().unwrap().len(), 1);
    assert_eq!(list["bots"][0]["id"], "b");
    for target in ["a", "c", "missing"] {
        let result = call(&f.hub, "a", "message_bot", &json!({"botId": target, "message": "work"}));
        assert!(result.is_err());
    }
    for message in [" ".to_owned(), "x".repeat(MAX_MESSAGE_BYTES + 1)] {
        let result = f.message(&message);
        assert!(result.is_err());
    }
    f.hub.team.end_turn("a");
    let result = f.message("late request");
    assert!(result.unwrap_err().to_string().contains("no longer working"));
    f.hub.team.start_turn("a", Turn::Chat(0));
    f.hub.delete_bot("b").unwrap();
    let result = f.message("deleted recipient");
    assert!(result.is_err());
    assert_eq!(f.hub.team.0.locked().messages, 0);
    assert_eq!(f.prompts().len(), 0);
    f.shutdown().await;
}

#[tokio::test]
async fn a_member_in_a_room_turn_mentions_instead_of_messaging() {
    let f = Fixture::new();
    f.hub.set_runtime("a", |r| r.lane = Some(crate::store::Lane::main("crew")));
    let result = f.message("work");
    assert!(result.unwrap_err().to_string().contains("@-mention the room's members"));
    assert_eq!(f.hub.team.0.locked().messages, 0);
    f.shutdown().await;
}

#[tokio::test]
async fn sender_deletion_preserves_messages_and_recipient_deletion_closes_them() {
    let f = Fixture::new();
    let running = f.message("BLOCK active message").unwrap();
    f.until(|| f.prompts().len() == 1).await;
    let queued = f.message("queued message").unwrap();
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
    let running = f.message("BLOCK active message").unwrap();
    f.until(|| f.prompts().len() == 1).await;
    let queued = f.message("queued message").unwrap();
    f.hub.delete_bot("b").unwrap();
    f.until(|| f.hub.team.0.locked().messages == 0).await;
    for receipt in [&running, &queued] {
        assert!(f.notices(receipt).iter().all(|e| e.data["status"] == "cancelled"));
    }
    assert_eq!(f.prompts().len(), 1);
    // Deleting the recipient doesn't wake the sender: its only turn is a later message's reply.
    let later = call(&f.hub, "a", "message_bot", &json!({"botId": "c", "message": "later work"})).unwrap();
    f.until(|| f.took("a", 1)).await;
    assert!(f.prompts_of("a")[0].contains(later["requestId"].as_str().unwrap()));
    f.shutdown().await;
}

#[tokio::test]
async fn message_turns_have_no_resume_marker_and_restart_expires_both_notices() {
    let f = Fixture::new();
    let running = f.message("BLOCK active message").unwrap();
    f.until(|| f.prompts().len() == 1).await;
    let queued = f.message("queued message").unwrap();
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
                "delegationId": "d", "status": "sent", "heading": "Message from a",
                "botMessage": {"sourceBotId": "a", "targetBotId": "b", "text": "Work"},
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
    assert_eq!(interrupted.data["botMessage"]["text"], "Work");
    assert!(interrupted.data["botMessage"]["detail"].as_str().unwrap().contains("Interrupted by host restart"));
}
