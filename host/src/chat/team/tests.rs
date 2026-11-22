use super::*;
use crate::LockExt;
use crate::hub::BotStatus;
use crate::store::Store;
use std::path::PathBuf;
use std::time::Duration;

pub(super) struct Fixture {
    pub(super) hub: Arc<Hub>,
    dir: PathBuf,
    /// b answers requests from its box, one thread at a time, as a bot does by default.
    boxed: bool,
}

impl Fixture {
    pub(super) fn new() -> Self {
        Self::create(false)
    }

    fn create(boxed: bool) -> Self {
        let dir = std::env::temp_dir().join(format!("codync-team-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Self::open(dir, boxed)
    }

    /// The host over `dir`'s store and bots, started as a host process starts.
    fn open(dir: PathBuf, boxed: bool) -> Self {
        let store = Store::open(&dir.join("test.db")).unwrap();
        let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
        for id in ["a", "b", "c"] {
            let cwd = dir.join(id);
            std::fs::create_dir_all(&cwd).unwrap();
            let cfg: BotConfig = serde_json::from_value(json!({
                "id": id, "name": id, "backend": "fixture", "cwd": cwd,
                "command": codync_agent::shell::python(&agent),
                "notify": false, "permission": "ask",
                // They answer requests in their chat, whose queue these tests are about (the box: `work_e2e`).
                "manageTeam": !(boxed && id == "b"), "maxThreads": 1,
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
        let f = Self { hub, dir, boxed };
        f.sending("a");
        f
    }

    /// `bot` is in a turn of its own, so it may send; its replies start real turns that end this.
    fn sending(&self, bot: &str) {
        self.hub.set_runtime(bot, |r| r.status = BotStatus::Working);
        self.hub.team.start_turn(bot, Turn::Chat(0));
    }

    fn message(&self, message: &str) -> Result<Value> {
        call(&self.hub, "a", "a", "message_bot", &json!({"botId": "b", "message": message}))
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

    pub(super) async fn until(&self, condition: impl Fn() -> bool) {
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

    /// The host stops and a new process starts over the same store, as an update does.
    async fn restart(self) -> Self {
        self.hub.shutdown().await;
        Self::open(self.dir, self.boxed)
    }

    pub(super) async fn shutdown(self) {
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
    let back = call(&f.hub, "b", "b", "message_bot", &json!({"botId": "c", "message": "pass it on"}));
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
    // The sender hears that both were cancelled, though the user stopped them (#534).
    let heard = |receipt: &Value| {
        let id = receipt["requestId"].as_str().unwrap().to_owned();
        move |prompts: &[String]| {
            prompts.iter().any(|p| p.contains(&format!("couldn't finish your message_bot request {id}")))
        }
    };
    let (first_heard, second_heard) = (heard(&first), heard(&second));
    f.until(|| f.idle("a") && first_heard(&f.prompts_of("a")) && second_heard(&f.prompts_of("a"))).await;
    assert!(f.prompts_of("a").iter().all(|p| !p.contains("[Codync: b replied")));
    let woken = f.prompts_of("a").len();
    f.sending("a");
    let later = f.message("new work after Stop").unwrap();
    f.until(|| f.took("a", woken + 1)).await;
    assert_eq!(f.prompts().len(), 2);
    assert!(f.prompts()[1].ends_with("\n\nnew work after Stop"));
    assert!(f.notices(&later).iter().all(|e| e.data["status"] == "completed"));
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
    // The sender hears it was stopped, though the user stopped it (#534).
    f.until(|| f.took("a", 1)).await;
    let id = receipt["requestId"].as_str().unwrap();
    assert!(f.prompts_of("a")[0].contains(&format!("b couldn't finish your message_bot request {id}")));
    assert!(f.prompts_of("a")[0].contains("Recipient stopped"));
    f.shutdown().await;
}

#[tokio::test]
async fn replies_that_arrive_while_the_sender_works_are_one_turn() {
    let f = Fixture::new();
    f.send_to("a", "BLOCK user work").await;
    f.until(|| f.prompts_of("a").len() == 1).await;
    f.message("first").unwrap();
    call(&f.hub, "a", "a", "message_bot", &json!({"botId": "c", "message": "second"})).unwrap();
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
    let from_c = call(&f.hub, "c", "c", "message_bot", &json!({"botId": "b", "message": "message from c"}));
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

/// #386: a chain stopped by the handoff limit leaves the user one notice in the stopped bot's chat, whose Continue
/// sends the bot "Continue" as the user (a fresh chain), once.
#[tokio::test]
async fn hop_limit_refusal_leaves_the_user_a_notice() {
    let f = Fixture::new();
    // 8: the handoff limit (`requests::MAX_REQUEST_HOPS`).
    f.hub.team.start_turn("a", Turn::Chat(8));
    let continues = || {
        let history = f.hub.store.history("a", i64::MAX, 100).unwrap();
        history.into_iter().filter(|e| e.kind == "notice" && !e.data["continueChain"].is_null()).collect::<Vec<_>>()
    };
    for _ in 0..2 {
        assert!(f.message("one more handoff").unwrap_err().to_string().contains("hop limit"));
    }
    let notices = continues();
    assert_eq!(notices.len(), 1, "one notice a turn, however often the bot tries");
    let press = |id: &str| {
        crate::api::dispatch(&f.hub, &crate::api::devices::Caller::Local, "continueChain", json!({"entryId": id}))
    };
    let sent = || {
        let history = f.hub.store.history("a", i64::MAX, 100).unwrap();
        history.iter().filter(|e| e.kind == "user" && e.data["text"] == "Continue").count()
    };
    // A second stopped turn's notice, for the bot whose actor is gone below.
    f.hub.team.start_turn("a", Turn::Chat(8));
    assert!(f.message("one more handoff").is_err());
    let notices = continues();
    assert_eq!(notices.len(), 2);
    // A notice that couldn't be updated refuses the press, and a Continue that couldn't be saved puts the button back:
    // neither sends anything.
    let db = rusqlite::Connection::open(f.dir.join("test.db")).unwrap();
    for trigger in
        ["BEFORE UPDATE ON entries WHEN OLD.kind = 'notice'", "BEFORE INSERT ON entries WHEN NEW.kind = 'user'"]
    {
        db.execute_batch(&format!(
            "CREATE TRIGGER reject_continue {trigger} BEGIN SELECT RAISE(ABORT, 'fixture save failure'); END;"
        ))
        .unwrap();
        assert!(press(&notices[0].id).await.is_err());
        db.execute_batch("DROP TRIGGER reject_continue;").unwrap();
        assert_eq!(continues()[0].data["continueChain"], true, "the button stays to press again");
        assert_eq!(sent(), 0);
    }
    drop(db);
    let entry = press(&notices[0].id).await.unwrap();
    assert_eq!(entry["entry"]["data"]["text"], "Continue");
    assert_eq!(entry["entry"]["kind"], "user", "the user's own message, which starts a fresh chain");
    assert_eq!(continues()[0].data["continueChain"], false, "the button goes");
    assert!(press(&notices[0].id).await.is_err(), "and works once");
    // The hub still lists the bot, but its actor is gone: the saved Continue stays the only one, its button off.
    drop(f.hub.attach_bot_for_test("a"));
    assert!(press(&notices[1].id).await.is_err());
    assert_eq!(continues()[1].data["continueChain"], false, "a saved message never brings the button back");
    assert!(press(&notices[1].id).await.is_err());
    assert_eq!(sent(), 2, "one for each notice");
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
    let list = call(&f.hub, "a", "a", "list_bots", &json!({})).unwrap();
    assert_eq!(list["bots"].as_array().unwrap().len(), 1);
    assert_eq!(list["bots"][0]["id"], "b");
    for target in ["a", "c", "missing"] {
        let result = call(&f.hub, "a", "a", "message_bot", &json!({"botId": target, "message": "work"}));
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
    f.hub.set_runtime("a", |r| r.lane = Some(Lane::main("crew")));
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
    // Deleting the recipient wakes the sender with both requests' end (#534).
    f.until(|| {
        let prompts = f.prompts_of("a").join("\n");
        f.idle("a")
            && [&running, &queued].iter().all(|r| prompts.contains(r["requestId"].as_str().unwrap()))
            && prompts.matches("couldn't finish your message_bot request").count() == 2
    })
    .await;
    f.shutdown().await;
}

/// #534: requests that returned `queued` across a host restart (an update) never reached the recipient and their
/// sender never heard. Now one that waited is sent again, and the one that ran fails and wakes its sender.
#[tokio::test]
async fn a_restart_sends_waiting_requests_again_and_wakes_the_sender_for_the_running_one() {
    let f = Fixture::new();
    let running = f.message("BLOCK active message").unwrap();
    f.until(|| f.prompts().len() == 1).await;
    let queued = f.message("queued message").unwrap();
    assert!(f.hub.store.kv_get("turn.inflight.b").is_none_or(|v| v.is_empty()));
    let f = f.restart().await;
    let id = |receipt: &Value| receipt["requestId"].as_str().unwrap().to_owned();
    let (running_id, queued_id) = (id(&running), id(&queued));
    f.until(|| {
        let prompts = f.prompts_of("a").join("\n");
        f.idle("a")
            && f.idle("b")
            && prompts.contains(&format!("b couldn't finish your message_bot request {running_id}"))
            && prompts.contains(&format!("b replied to your message_bot request {queued_id}"))
    })
    .await;
    // The waiting request ran once, after the restart; the running one never ran again.
    let prompts = f.prompts();
    assert_eq!(prompts.len(), 2, "{prompts:?}");
    assert!(prompts[1].ends_with("\n\nqueued message"));
    assert!(f.notices(&running).iter().all(|e| e.data["status"] == "failed"
        && e.data["botMessage"]["detail"].as_str().unwrap().contains("Interrupted by host restart")));
    assert!(f.notices(&queued).iter().all(|e| e.data["status"] == "completed"));
    assert!(f.prompts_of("a").join("\n").contains("Interrupted by host restart"));
    // Both slots went back: nothing is left waiting, and no notice still waits for a reply turn.
    assert_eq!(f.hub.team.0.locked().messages, 0);
    assert!(f.hub.store.unrelayed_replies().unwrap().is_empty());
    f.shutdown().await;
}

/// The same through b's box, where a bot that doesn't manage the team (the default) takes requests: the restart
/// dropped the waiting task and failed its notices, and the sender never heard.
#[tokio::test]
async fn a_restart_sends_a_request_waiting_in_the_box_again() {
    let f = Fixture::create(true);
    let running = f.message("BLOCK active message").unwrap();
    f.until(|| f.prompts().len() == 1).await;
    // One thread at a time: this one waits in the box behind the running one.
    let queued = f.message("queued message").unwrap();
    assert!(f.notices(&queued).iter().all(|e| e.data["status"] == "queued"));
    let f = f.restart().await;
    let id = |receipt: &Value| receipt["requestId"].as_str().unwrap().to_owned();
    let (running_id, queued_id) = (id(&running), id(&queued));
    f.until(|| {
        let prompts = f.prompts_of("a").join("\n");
        f.idle("a")
            && prompts.contains(&format!("b couldn't finish your message_bot request {running_id}"))
            && prompts.contains(&format!("b replied to your message_bot request {queued_id}"))
    })
    .await;
    let prompts = f.prompts();
    assert_eq!(prompts.len(), 2, "{prompts:?}");
    assert!(prompts[1].ends_with("\n\nqueued message"));
    assert!(f.notices(&queued).iter().all(|e| e.data["status"] == "completed"));
    f.shutdown().await;
}

/// A reply that reached a busy sender waited in memory only: a restart lost it. Now its notice holds it until the
/// sender's turn takes it, so the next host start relays it.
#[tokio::test]
async fn a_reply_waiting_for_a_busy_sender_survives_a_restart() {
    let f = Fixture::new();
    f.send_to("a", "BLOCK user work").await;
    f.until(|| f.prompts_of("a").len() == 1).await;
    let receipt = f.message("work").unwrap();
    f.until(|| f.took("b", 1)).await;
    assert_eq!(f.hub.store.unrelayed_replies().unwrap().len(), 1, "it waits for a's turn");
    let f = f.restart().await;
    let id = receipt["requestId"].as_str().unwrap().to_owned();
    let relayed = |p: &String| p.contains(&format!("b replied to your message_bot request {id}"));
    f.until(|| f.idle("a") && f.prompts_of("a").iter().any(relayed)).await;
    assert!(f.hub.store.unrelayed_replies().unwrap().is_empty());
    assert_eq!(f.prompts().len(), 1, "the request itself never ran again");
    f.shutdown().await;
}

/// The sender's reply turn takes its replies and is marked running in one write: a crash between the two would
/// otherwise neither relay nor resume them, or (the other way round) do both.
#[tokio::test]
async fn a_reply_turn_takes_its_replies_and_is_marked_running_in_one_write() {
    let f = Fixture::new();
    f.send_to("a", "BLOCK user work").await;
    f.until(|| f.prompts_of("a").len() == 1).await;
    f.message("work").unwrap();
    f.until(|| f.took("b", 1)).await;
    let notice = f.hub.store.unrelayed_replies().unwrap().remove(0);
    assert!(f.hub.store.kv_get(&relaying_key(&notice.id)).is_some(), "the whole reply waits with it");
    let reply = BotReply {
        request_id: "r".into(),
        quote: String::new(),
        from_name: "b".into(),
        outcome: Ok("reply".into()),
        hops: 0,
        access: Access::Allow,
        entry: Some(notice.id.clone()),
    };
    // Another bot's mark, so a's own running turn doesn't overwrite it.
    relayed(&f.hub, &[reply], &[("turn.inflight.other".into(), "42".into())]);
    assert!(f.hub.store.unrelayed_replies().unwrap().is_empty());
    assert!(f.hub.store.kv_get(&relaying_key(&notice.id)).is_none());
    assert_eq!(f.hub.store.kv_get("turn.inflight.other").as_deref(), Some("42"));
    f.shutdown().await;
}

/// The notice shows only the start of a long reply; a restart relays the whole of it, as the live path does.
#[tokio::test]
async fn a_long_reply_relayed_after_a_restart_is_whole() {
    let f = Fixture::new();
    f.send_to("a", "BLOCK user work").await;
    f.until(|| f.prompts_of("a").len() == 1).await;
    let long = format!("work {}", "x".repeat(NOTICE_REPLY_BYTES * 2));
    f.message(&long).unwrap();
    f.until(|| f.took("b", 1)).await;
    let f = f.restart().await;
    let whole = format!("reply: {long}");
    f.until(|| f.idle("a") && f.prompts_of("a").iter().any(|p| p.contains(&whole))).await;
    assert!(f.hub.store.unrelayed_replies().unwrap().is_empty());
    f.shutdown().await;
}

#[tokio::test]
async fn a_restart_between_a_requests_two_notices_keeps_the_outcome_saved_first() {
    let f = Fixture::new();
    f.send_to("a", "BLOCK user work").await;
    f.until(|| f.prompts_of("a").len() == 1).await;
    let receipt = f.message("work\non two lines").unwrap();
    f.until(|| f.took("b", 1)).await;
    // The last process saved the sender's notice and stopped before the recipient's.
    let recipient = f.notices(&receipt).into_iter().find(|e| e.bot_id == "b").unwrap();
    let mut data = recipient.data.clone();
    data["status"] = "sent".into();
    f.hub.store.update_entry(&recipient.id, &data).unwrap();
    let f = f.restart().await;
    let id = receipt["requestId"].as_str().unwrap().to_owned();
    let replied = |p: &String| p.contains(&format!("b replied to your message_bot request {id}"));
    f.until(|| f.idle("a") && f.prompts_of("a").iter().any(replied)).await;
    assert!(f.prompts_of("a").iter().all(|p| !p.contains(crate::store::INTERRUPTED)), "no second outcome");
    let notices = f.notices(&receipt);
    assert!(notices.iter().all(|e| e.data["status"] == "completed"));
    let answers: Vec<_> = notices.iter().map(|e| e.data["text"].as_str().unwrap().split("Reply from").nth(1)).collect();
    assert!(answers[0].is_some() && answers[0] == answers[1], "both say the same: {answers:?}");
    assert_eq!(f.prompts().len(), 1, "the request itself never ran again");
    f.shutdown().await;
}

#[test]
fn restart_marks_remote_requests_interrupted_and_leaves_local_ones_to_settle() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let notice = |bot: &str, id: &str, status: &str, message: Value| {
        let data = json!({"delegationId": id, "status": status, "heading": "Message from a", "botMessage": message});
        store.insert_entry(&Lane::main(bot), EntryKind::Notice, 1, &data).unwrap()
    };
    let remote = notice(
        "b",
        "d",
        "sent",
        json!({"sourceBotId": "a@laptop", "targetBotId": "b", "text": "Work", "peerName": "a"}),
    );
    let local = notice("b", "l", "queued", json!({"sourceBotId": "a", "targetBotId": "b", "text": "Work"}));
    let complete = notice("a", "done", "completed", json!({"sourceBotId": "a", "targetBotId": "b", "text": "x"}));
    store.expire_pending().unwrap();
    let interrupted = store.entry(&remote.id).unwrap();
    assert_eq!(interrupted.data["status"], "failed");
    assert!(interrupted.data["text"].as_str().unwrap().contains("Interrupted by host restart"));
    assert!(interrupted.rev > remote.rev);
    assert_eq!(interrupted.data["botMessage"]["text"], "Work");
    assert!(interrupted.data["botMessage"]["detail"].as_str().unwrap().contains("Interrupted by host restart"));
    assert_eq!(store.entry(&complete.id).unwrap().data["status"], "completed");
    // `restart` settles it once the bots run.
    assert_eq!(store.entry(&local.id).unwrap().data["status"], "queued");
    let open: Vec<String> = store.unfinished_requests().unwrap().into_iter().map(|e| e.id).collect();
    assert_eq!(open, [local.id]);
}
