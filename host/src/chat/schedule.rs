//! Scheduled sends (`scheduleSend`): the user's messages held for a future time, swept into the ordinary
//! queue when their time comes. There is no second queue and no second scheduler: a scheduled entry is a
//! user Entry with `data.status = "scheduled"` in the same transcript; the sweep only decides *when* it
//! becomes the ordinary queued send it already looks like (see `chat/messages::fire_scheduled`).
//!
//! Pause interplay: this crate has no paused field yet (it lands with PR #572). When it does, the sweep's
//! due-filter gains `!cfg.paused` so a fire while the target is paused is HELD (stays scheduled, zero
//! spend) until resumed; until then #572's pause gate refuses such a delivery — either way a paused bot is
//! never woken and never spends.

use crate::hub::Hub;
use std::sync::Arc;
use std::time::Duration;

/// How often the sweep looks for due messages: minute-level scheduling doesn't need better than this.
const SWEEP_EVERY: Duration = Duration::from_secs(15);

pub fn start(hub: &Arc<Hub>) {
    // A host that stopped after firing a message but before handing it to its bot left the durable queued
    // marker. The actors are fresh now, so handing these on once is safe: a message whose turn had started
    // is already `sent` and is not here.
    if let Err(error) = dispatch_pending(hub) {
        tracing::warn!(error = format!("{error:#}"), "couldn't recover scheduled sends");
        count_deliver_failure(hub, "lookup");
    }
    let hub = hub.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(SWEEP_EVERY);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            if let Err(error) = sweep(&hub) {
                tracing::warn!(error = format!("{error:#}"), "couldn't sweep scheduled sends");
                count_deliver_failure(&hub, "lookup");
            }
        }
    });
}

/// Hands on scheduled messages whose durable flip landed before the last host stopped. Called once at
/// start, while every actor's live queue is empty; the per-entry ID keeps a retry idempotent.
pub fn dispatch_pending(hub: &Arc<Hub>) -> anyhow::Result<()> {
    for entry in hub.store.scheduled_dispatched()? {
        let Some(row) = hub.store.bot(&entry.bot_id)? else { continue };
        if row.deleted || row.config.is_group() || !hub.has_actor(&entry.bot_id) {
            continue;
        }
        if let Err(error) = crate::chat::messages::hand_on(hub, &row.config, &entry) {
            tracing::warn!(
                bot = %entry.bot_id,
                entry = %entry.id,
                error = format!("{error:#}"),
                "couldn't recover a scheduled message; it stays held for the next start"
            );
            count_deliver_failure(hub, "recover");
        }
    }
    Ok(())
}

/// Fires every scheduled message whose time has come, oldest first. A bot that has an actor gets its
/// messages handed on (`hand_on`); a bot whose actor is gone (a stopped or removed bot) keeps its
/// scheduled messages for the next start, like any send into a dead chat would.
pub fn sweep(hub: &Arc<Hub>) -> anyhow::Result<()> {
    let now = crate::store::now_ms();
    for row in hub.store.bots()? {
        if row.deleted || row.config.is_group() {
            continue;
        }
        for entry in hub.store.scheduled_due(&row.config.id, now)? {
            if !hub.has_actor(&row.config.id) {
                continue;
            }
            if let Err(error) = crate::chat::messages::fire_scheduled(hub, &entry) {
                tracing::warn!(
                    bot = %row.config.id,
                    entry = %entry.id,
                    error = format!("{error:#}"),
                    "couldn't fire a scheduled message"
                );
                count_deliver_failure(hub, "fire");
            }
        }
    }
    Ok(())
}

/// A due message that did not go out (`message_schedule_failed`). `error` is a reason code.
/// The log above keeps the sentence; the event does not, and it carries no id and no name.
fn count_deliver_failure(hub: &Hub, error: &'static str) {
    let properties = serde_json::json!({"stage": "deliver", "error": error});
    crate::analytics::capture(
        hub,
        crate::analytics::Event::MessageScheduleFailed,
        properties.as_object().cloned().unwrap_or_default(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::Hub;
    use crate::remote::identity::Identity;
    use crate::store::{BotConfig, EntryKind, Lane, Store};
    use serde_json::json;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    /// A store with `bot`'s row saved, for the store-level tests.
    fn store_with_bot(id: &str) -> Store {
        let store = Store::open(Path::new(":memory:")).unwrap();
        let cfg: BotConfig = serde_json::from_value(json!({
            "id": id, "name": id, "backend": "claude", "cwd": ".",
        }))
        .unwrap();
        store.save_bot(&cfg).unwrap();
        store
    }

    /// A host with one fixture agent: real actors, so a fired message reaches the agent the ordinary way.
    struct Fixture {
        hub: Arc<Hub>,
        dir: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("codync-schedule-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            let store = Store::open(&dir.join("test.db")).unwrap();
            let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
            let cwd = dir.join("b");
            std::fs::create_dir_all(&cwd).unwrap();
            let cfg: BotConfig = serde_json::from_value(json!({
                "id": "b", "name": "b", "backend": "fixture", "cwd": cwd, "skills": ["pdf"],
                "command": codync_agent::shell::python(&agent),
                "notify": false, "permission": "ask",
            }))
            .unwrap();
            store.save_bot(&cfg).unwrap();
            store.kv_set("skills", &json!([{"id": "pdf", "name": "PDF", "source": "custom"}]).to_string()).unwrap();
            let hub = Hub::new(
                store,
                "test-host".into(),
                Identity::load_or_create(&dir).unwrap(),
                "test-token".into(),
                19222,
            );
            hub.start().unwrap();
            Self { hub, dir }
        }

        /// What the agent was prompted with, in order (the fixture logs each prompt as one JSON line).
        fn prompts(&self) -> Vec<String> {
            std::fs::read_to_string(self.dir.join("b").join("prompts.jsonl"))
                .unwrap_or_default()
                .lines()
                .filter_map(|l| serde_json::from_str::<String>(l).ok())
                .collect()
        }

        /// Runs the sweep until `condition`, generously but bounded (awaiting, so the runtime's tasks run).
        async fn until(&self, condition: impl Fn() -> bool) {
            for _ in 0..400 {
                if condition() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
                sweep(&self.hub).unwrap();
            }
            let r = self.hub.runtime("b");
            panic!(
                "fixture condition timed out: prompts={:?} status={:?} activity={:?} has_actor={}",
                self.prompts(),
                r.status,
                r.activity,
                self.hub.has_actor("b")
            );
        }

        fn schedule(&self, text: &str, in_ms: i64) -> crate::store::Entry {
            let e = crate::chat::messages::save_scheduled(
                &self.hub,
                &self.hub.store.bot("b").unwrap().unwrap().config,
                json!({"text": text, "clientNonce": uuid::Uuid::new_v4().to_string()}),
                crate::store::now_ms() + 60_000,
            )
            .unwrap();
            // The sweep's clock is what these tests exercise, not the floor: move the fire time to where the
            // test wants it.
            let mut data = e.data.clone();
            data["notBefore"] = json!(crate::store::now_ms() + in_ms);
            self.hub.set_entry(&e.id, &data).unwrap()
        }

        async fn send(&self, text: &str) {
            crate::api::dispatch(
                &self.hub,
                &crate::api::devices::Caller::Local,
                "send",
                json!({"botId": "b", "text": text}),
            )
            .await
            .unwrap();
        }

        fn status_of(&self, id: &str) -> String {
            self.hub.store.entry(id).unwrap().data["status"].as_str().unwrap_or_default().to_owned()
        }
    }

    #[test]
    fn a_scheduled_message_is_saved_but_not_handed_on() {
        let hub = crate::chat::calls::context::tests::hub();
        let cfg: BotConfig = serde_json::from_value(json!({
            "id": "b", "name": "b", "backend": "claude", "cwd": ".",
        }))
        .unwrap();
        hub.store.save_bot(&cfg).unwrap();
        let e = crate::chat::messages::save_scheduled(
            &hub,
            &cfg,
            json!({"text": "later", "clientNonce": "n"}),
            crate::store::now_ms() + 60_000,
        )
        .unwrap();
        assert_eq!(e.data["status"], "scheduled");
        assert!(e.data["notBefore"].as_i64().unwrap() > crate::store::now_ms());
        assert_eq!(hub.store.queued_messages("b").unwrap().len(), 0, "a scheduled message is not queued yet");
    }

    #[test]
    fn a_scheduled_message_must_be_in_the_future_and_a_group_cannot_have_one() {
        let hub = crate::chat::calls::context::tests::hub();
        let cfg: BotConfig = serde_json::from_value(json!({
            "id": "b", "name": "b", "backend": "claude", "cwd": ".",
        }))
        .unwrap();
        hub.store.save_bot(&cfg).unwrap();
        assert!(
            crate::chat::messages::save_scheduled(&hub, &cfg, json!({"text": "later"}), crate::store::now_ms() - 1,)
                .is_err()
        );
        // The floor is exactly the constant: at it the save is refused (it is a send by then); a hair
        // past it lands.
        assert!(
            crate::chat::messages::save_scheduled(
                &hub,
                &cfg,
                json!({"text": "later"}),
                crate::store::now_ms() + crate::chat::messages::SOONEST,
            )
            .is_err()
        );
        assert!(
            crate::chat::messages::save_scheduled(
                &hub,
                &cfg,
                json!({"text": "later"}),
                crate::store::now_ms() + crate::chat::messages::SOONEST + 5_000,
            )
            .is_ok()
        );
        let group: BotConfig = serde_json::from_value(json!({
            "id": "g", "name": "g", "kind": "group", "members": ["b"], "backend": "claude", "cwd": ".",
        }))
        .unwrap();
        assert!(
            crate::chat::messages::save_scheduled(
                &hub,
                &group,
                json!({"text": "later"}),
                crate::store::now_ms() + 60_000,
            )
            .is_err()
        );
    }

    /// A restart expires the live queue (a crash's queued sends can't be honored) and keeps scheduled ones.
    #[test]
    fn a_restart_keeps_scheduled_messages_while_expiring_queued_ones() {
        let store = store_with_bot("b");
        let chat = Lane::main("b");
        let queued =
            store.insert_entry(&chat, EntryKind::User, 1, &json!({"text": "now", "status": "queued"})).unwrap();
        let scheduled = store
            .insert_entry(
                &chat,
                EntryKind::User,
                2,
                &json!({"text": "later", "status": "scheduled", "notBefore": crate::store::now_ms() + 60_000}),
            )
            .unwrap();
        store.expire_pending().unwrap();
        assert_eq!(store.entry(&queued.id).unwrap().data["status"], "failed");
        assert_eq!(store.entry(&scheduled.id).unwrap().data["status"], "scheduled", "it fires after the restart");
    }

    /// The sweep fires due messages oldest-time first, each taking its delivery-time place in the chat.
    #[tokio::test]
    async fn the_sweep_fires_due_messages_in_time_order() {
        let f = Fixture::new();
        let first = f.schedule("first", 150);
        let second = f.schedule("second", 700);
        f.until(|| f.prompts().len() == 2).await;
        let prompts = f.prompts();
        assert!(prompts[0].contains("first"), "earliest first: {prompts:?}");
        assert!(prompts[1].contains("second"), "earliest first: {prompts:?}");
        // Each took its delivery-time place, as a message typed then would.
        let turns = [first.id.clone(), second.id.clone()].map(|id| f.hub.store.entry(&id).unwrap().turn);
        assert!(turns[0] < turns[1], "the delivery-time turns, in fire order: {turns:?}");
        for e in [&first, &second] {
            assert_eq!(f.status_of(&e.id), "sent", "the turn started: it is no longer scheduled");
            assert!(f.hub.store.entry(&e.id).unwrap().data["notBefore"].is_null(), "firing clears the fields");
        }
        f.hub.shutdown().await;
        let _ = std::fs::remove_dir_all(&f.dir);
    }

    /// A scheduled message whose time comes while the bot's turn is running enters the ordinary queue: it
    /// waits behind the running turn instead of pre-empting it, and delivers after.
    #[tokio::test]
    async fn a_scheduled_message_enters_the_queue_behind_a_live_one() {
        let f = Fixture::new();
        // A live send starts a turn (the agent's start takes a moment): the bot is busy while it runs.
        f.send("live now").await;
        let live = f
            .hub
            .store
            .history("b", i64::MAX, 50)
            .unwrap()
            .into_iter()
            .find(|e| e.kind == "user")
            .expect("the live message is saved");
        f.until(|| f.prompts().len() == 1).await;
        // Its time comes while that turn runs: it queues instead of interrupting.
        let scheduled = f.schedule("later", 1);
        f.until(|| f.status_of(&scheduled.id) != "scheduled").await;
        f.until(|| f.prompts().len() == 2).await;
        assert!(f.prompts()[0].contains("live now"), "the running turn first: {:#?}", f.prompts());
        assert!(f.prompts()[1].contains("later"), "the scheduled one delivered after: {:#?}", f.prompts());
        assert!(f.hub.store.entry(&scheduled.id).unwrap().turn > live.turn, "its place is its delivery time");
        assert_eq!(f.status_of(&scheduled.id), "sent");
        f.hub.shutdown().await;
        let _ = std::fs::remove_dir_all(&f.dir);
    }

    /// `updateScheduled` on a message waiting in the live queue: its text changes where it waits, and a
    /// time holds it for later — it leaves the queue, so the ending turn delivers it only at its time,
    /// with the edited text.
    #[tokio::test]
    async fn a_waiting_message_can_be_edited_and_held_for_later() {
        let f = Fixture::new();
        // A live send starts a turn; a second message right behind it waits in the queue.
        f.send("live now").await;
        f.send("first draft").await;
        let waiting = f
            .hub
            .store
            .history("b", i64::MAX, 50)
            .unwrap()
            .into_iter()
            .find(|e| e.data["text"] == "first draft")
            .expect("the waiting message is saved");
        assert_eq!(f.status_of(&waiting.id), "queued");
        // The text changes where it waits…
        crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "updateScheduled",
            json!({"botId": "b", "entryId": waiting.id, "text": "held and edited"}),
        )
        .await
        .unwrap();
        // …and a time holds it out of the live queue.
        crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "updateScheduled",
            json!({"botId": "b", "entryId": waiting.id, "notBefore": crate::store::now_ms() + 60_000}),
        )
        .await
        .unwrap();
        f.until(|| f.status_of(&waiting.id) == "scheduled").await;
        // The floor is not what this test exercises: move the fire time to just about now.
        let mut held = f.hub.store.entry(&waiting.id).unwrap();
        held.data["notBefore"] = json!(crate::store::now_ms() + 150);
        f.hub.set_entry(&waiting.id, &held.data).unwrap();
        // The running turn ends with nothing new to deliver: the live queue no longer holds it.
        f.until(|| f.prompts().len() == 2 && f.prompts()[1].contains("held and edited")).await;
        assert!(!f.prompts()[1].contains("first draft"), "the queue's copy is the edited one");
        assert_eq!(f.status_of(&waiting.id), "sent");
        f.hub.shutdown().await;
        let _ = std::fs::remove_dir_all(&f.dir);
    }

    /// Editing a queued message rebuilds the prompt waiting in its actor: the new words still carry the
    /// quote, the expanded skill command and the attachment paths of the entry the user originally sent.
    #[tokio::test]
    async fn a_waiting_message_edit_keeps_its_whole_prompt_context() {
        let f = Fixture::new();
        // A turn that stays open (the fixture waits for `release`): the second message waits in the queue
        // while the edit lands, so the test is not racing the first turn's end.
        f.send("live now BLOCK").await;
        f.until(|| f.prompts().len() == 1).await;
        let live = f
            .hub
            .store
            .history("b", i64::MAX, 50)
            .unwrap()
            .into_iter()
            .find(|e| e.kind == "user" && e.data["text"] == "live now BLOCK")
            .expect("the live message is saved");
        let upload = uuid::Uuid::new_v4().to_string();
        crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "upload",
            json!({"botId": "b", "uploadId": upload, "name": "context.txt", "data": "Y29udGV4dA==", "done": true}),
        )
        .await
        .unwrap();
        crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "send",
            json!({"botId": "b", "text": "/pdf original draft", "replyTo": live.id,
                  "attachments": [upload], "clientNonce": "queued-context"}),
        )
        .await
        .unwrap();
        let waiting = f
            .hub
            .store
            .history("b", i64::MAX, 50)
            .unwrap()
            .into_iter()
            .find(|e| e.data["text"] == "/pdf original draft")
            .expect("the waiting message is saved");
        f.until(|| f.status_of(&waiting.id) == "queued").await;
        crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "updateScheduled",
            json!({"botId": "b", "entryId": waiting.id, "text": "/pdf edited draft"}),
        )
        .await
        .unwrap();
        // The first turn was held open by `BLOCK`: release it so the queued message starts.
        let root = PathBuf::from(f.hub.store.bot("b").unwrap().unwrap().config.cwd);
        std::fs::write(root.join("release"), "").unwrap();
        f.until(|| f.prompts().len() == 2).await;
        let delivered = &f.prompts()[1];
        assert!(delivered.contains("edited draft"), "the edit reaches the agent: {delivered}");
        assert!(!delivered.contains("original draft"), "the old words are gone: {delivered}");
        assert!(delivered.contains("this replies to"), "the quote survives: {delivered}");
        assert!(delivered.contains("the user picked the \"PDF\" skill"), "the skill expansion survives: {delivered}");
        assert!(delivered.contains("[Attached files"), "the attachment instruction survives: {delivered}");
        assert!(delivered.contains("context.txt"), "the attachment path survives: {delivered}");
        f.hub.shutdown().await;
        let _ = std::fs::remove_dir_all(&f.dir);
    }

    /// `updateScheduled` changes the text and the time; `cancelScheduled` takes it back for good; a fire
    /// after a cancel delivers nothing.
    #[tokio::test]
    async fn a_scheduled_message_can_be_edited_and_cancelled() {
        let f = Fixture::new();
        let e = f.schedule("first draft", 60_000);
        let edited = crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "updateScheduled",
            json!({"botId": "b", "entryId": e.id, "text": "edited", "notBefore": crate::store::now_ms() + 120_000}),
        )
        .await
        .unwrap();
        assert_eq!(edited["entry"]["data"]["text"], "edited");
        assert!(edited["entry"]["data"]["notBefore"].as_i64().unwrap() > crate::store::now_ms() + 100_000);
        crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "cancelScheduled",
            json!({"botId": "b", "entryId": e.id}),
        )
        .await
        .unwrap();
        assert_eq!(f.status_of(&e.id), "cancelled");
        // Past its (cancelled) time — the due query must leave it out — the sweep delivers nothing more.
        let mut cancelled = f.hub.store.entry(&e.id).unwrap();
        cancelled.data["notBefore"] = (crate::store::now_ms() - 1).into();
        f.hub.store.update_entry(&e.id, &cancelled.data).unwrap();
        sweep(&f.hub).unwrap();
        assert_eq!(f.status_of(&e.id), "cancelled");
        assert_eq!(f.prompts().len(), 0);
        // Editing something that already went out is refused (while it merely waited it was editable).
        let fired = f.schedule("will fire", 50);
        f.until(|| f.status_of(&fired.id) == "sent").await;
        let late_edit = crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "updateScheduled",
            json!({"botId": "b", "entryId": fired.id, "text": "too late"}),
        )
        .await;
        assert!(late_edit.is_err());
        f.hub.shutdown().await;
        let _ = std::fs::remove_dir_all(&f.dir);
    }

    /// Two windows hold the same sheet. After one saves newer words, the other's save is refused
    /// and leaves those words and the scheduled time alone.
    #[tokio::test]
    async fn a_stale_update_does_not_replace_a_newer_edit() {
        let f = Fixture::new();
        let e = f.schedule("the first window's old words", 60_000);
        let edited = crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "updateScheduled",
            json!({"botId": "b", "entryId": e.id, "text": "the second window's newer words"}),
        )
        .await
        .unwrap();
        assert!(edited["entry"]["rev"].as_i64().unwrap() > e.rev);
        let stale = crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "updateScheduled",
            json!({"botId": "b", "entryId": e.id, "text": "the first window's stale words",
                   "notBefore": crate::store::now_ms() + 120_000, "expectedRev": e.rev}),
        )
        .await;
        assert!(stale.is_err(), "the stale sheet is refused");
        let current = f.hub.store.entry(&e.id).unwrap();
        assert_eq!(current.data["text"], "the second window's newer words");
        assert_eq!(current.data["status"], "scheduled");
        assert!(f.prompts().is_empty(), "nothing reaches the agent: {:?}", f.prompts());
        let saved = crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "updateScheduled",
            json!({"botId": "b", "entryId": e.id, "text": "the current final words", "expectedRev": current.rev}),
        )
        .await;
        assert!(saved.is_ok(), "the current sheet can still save: {saved:?}");
        assert_eq!(f.hub.store.entry(&e.id).unwrap().data["text"], "the current final words");
        f.hub.shutdown().await;
        let _ = std::fs::remove_dir_all(&f.dir);
    }

    /// `sendScheduledNow` delivers at once: it enters the ordinary queue as the sweep would.
    #[tokio::test]
    async fn send_scheduled_now_delivers_immediately() {
        let f = Fixture::new();
        let e = f.schedule("now please", 60_000);
        crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "sendScheduledNow",
            json!({"botId": "b", "entryId": e.id}),
        )
        .await
        .unwrap();
        f.until(|| f.prompts().len() == 1).await;
        assert!(f.prompts()[0].contains("now please"));
        assert_eq!(f.status_of(&e.id), "sent");
        f.hub.shutdown().await;
        let _ = std::fs::remove_dir_all(&f.dir);
    }

    /// "Clear the time" in the edit sheet sends the text currently in the sheet, not the saved draft.
    #[tokio::test]
    async fn send_scheduled_now_carries_the_sheet_edit() {
        let f = Fixture::new();
        let e = f.schedule("the old draft", 60_000);
        crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "sendScheduledNow",
            json!({"botId": "b", "entryId": e.id, "text": "the edited words"}),
        )
        .await
        .unwrap();
        f.until(|| f.prompts().len() == 1).await;
        assert!(f.prompts()[0].contains("the edited words"), "{:?}", f.prompts());
        assert!(!f.prompts()[0].contains("the old draft"));
        assert_eq!(f.status_of(&e.id), "sent");
        f.hub.shutdown().await;
        let _ = std::fs::remove_dir_all(&f.dir);
    }

    /// Two windows hold the same sheet. After one saves newer words, the other's Clear-the-time action is
    /// rejected as one operation: it neither replaces those words nor fires the message.
    #[tokio::test]
    async fn a_stale_clear_time_neither_replaces_nor_fires_a_newer_edit() {
        let f = Fixture::new();
        let e = f.schedule("the first window's old words", 60_000);
        let edited = crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "updateScheduled",
            json!({"botId": "b", "entryId": e.id, "text": "the second window's newer words"}),
        )
        .await
        .unwrap();
        assert!(edited["entry"]["rev"].as_i64().unwrap() > e.rev);
        let stale = crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "sendScheduledNow",
            json!({"botId": "b", "entryId": e.id, "expectedRev": e.rev, "text": "the first window's stale words"}),
        )
        .await;
        assert!(stale.is_err(), "the stale sheet is refused");
        let current = f.hub.store.entry(&e.id).unwrap();
        assert_eq!(current.data["text"], "the second window's newer words");
        assert_eq!(current.data["status"], "scheduled", "a refused replacement does not fire");
        assert!(current.data["notBefore"].is_i64());
        assert!(f.prompts().is_empty(), "nothing reaches the agent: {:?}", f.prompts());

        let sent = crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "sendScheduledNow",
            json!({"botId": "b", "entryId": e.id, "expectedRev": current.rev, "text": "the current final words"}),
        )
        .await;
        assert!(sent.is_ok(), "the current sheet can still send: {sent:?}");
        f.until(|| f.prompts().len() == 1).await;
        assert!(f.prompts()[0].contains("the current final words"), "{:?}", f.prompts());
        f.hub.shutdown().await;
        let _ = std::fs::remove_dir_all(&f.dir);
    }

    /// An update and a clear-time fire that start from the same sheet rev cannot both land. Whichever wins,
    /// the loser's snapshot never overwrites it.
    #[tokio::test]
    async fn an_edit_and_clear_time_are_atomic_when_they_race() {
        let f = Fixture::new();
        let e = f.schedule("before the race", 60_000);
        let edit = crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "updateScheduled",
            json!({"botId": "b", "entryId": e.id, "text": "the edit that raced"}),
        );
        let fire = crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "sendScheduledNow",
            json!({"botId": "b", "entryId": e.id, "expectedRev": e.rev, "text": "the clear that raced"}),
        );
        let (edit, fire) = tokio::join!(edit, fire);
        assert_ne!(edit.is_ok(), fire.is_ok(), "exactly one transition lands: edit={edit:?} fire={fire:?}");
        let current = f.hub.store.entry(&e.id).unwrap();
        if edit.is_ok() {
            assert_eq!(current.data["status"], "scheduled");
            assert_eq!(current.data["text"], "the edit that raced");
            assert!(f.prompts().is_empty(), "the losing fire delivered nothing");
        } else {
            assert_ne!(current.data["status"], "scheduled");
            assert_eq!(current.data["text"], "the clear that raced");
            f.until(|| f.prompts().len() == 1).await;
        }
        f.hub.shutdown().await;
        let _ = std::fs::remove_dir_all(&f.dir);
    }

    /// The due sweep and Send-now racing on the same scheduled row deliver exactly one turn: both use the
    /// revision-checked one-way flip, and the loser finds the row is no longer scheduled.
    #[tokio::test]
    async fn a_due_sweep_and_send_now_deliver_once() {
        let f = Fixture::new();
        let e = f.schedule("only once", 1);
        let snapshot = f.hub.store.entry(&e.id).unwrap();
        let fired = crate::chat::messages::fire_scheduled(&f.hub, &snapshot).unwrap();
        assert!(fired.is_some(), "the sweep's flip wins");
        let late = crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "sendScheduledNow",
            json!({"botId": "b", "entryId": e.id}),
        )
        .await;
        assert!(late.is_err(), "Send-now loses to the message that already fired");
        f.until(|| f.prompts().len() == 1 && f.status_of(&e.id) == "sent").await;
        sweep(&f.hub).unwrap();
        assert_eq!(f.prompts().len(), 1, "{:?}", f.prompts());
        f.hub.shutdown().await;
        let _ = std::fs::remove_dir_all(&f.dir);
    }

    /// A crash after the durable flip but before hand-on leaves a queued marker; the next start hands it on
    /// once, and running recovery again cannot deliver it a second time.
    #[tokio::test]
    async fn a_restart_recovers_a_fired_message_once() {
        let f = Fixture::new();
        let e = f.schedule("after the restart", 60_000);
        let current = f.hub.store.entry(&e.id).unwrap();
        let mut data = current.data.clone();
        data["status"] = "queued".into();
        data["scheduledFired"] = true.into();
        if let Some(o) = data.as_object_mut() {
            o.remove("notBefore");
        }
        let fired = f.hub.store.flip_scheduled(&e.id, Some(2), &data, current.rev).unwrap().expect("the fire lands");
        assert_eq!(fired.data["status"], "queued");
        f.hub.store.expire_pending().unwrap();
        assert_eq!(f.status_of(&e.id), "queued", "restart does not expire a fired scheduled message");
        dispatch_pending(&f.hub).unwrap();
        f.until(|| f.prompts().len() == 1 && f.status_of(&e.id) == "sent").await;
        dispatch_pending(&f.hub).unwrap();
        sweep(&f.hub).unwrap();
        assert_eq!(f.prompts().len(), 1, "the entry id keeps recovery idempotent: {:?}", f.prompts());
        f.hub.shutdown().await;
        let _ = std::fs::remove_dir_all(&f.dir);
    }

    /// The due query: only due entries, soonest first.
    #[test]
    fn the_due_query_filters_and_orders_by_fire_time() {
        let store = store_with_bot("b");
        let chat = Lane::main("b");
        let now = crate::store::now_ms();
        let late = store
            .insert_entry(
                &chat,
                EntryKind::User,
                1,
                &json!({"text": "late", "status": "scheduled", "notBefore": now + 5_000}),
            )
            .unwrap();
        let due_second = store
            .insert_entry(
                &chat,
                EntryKind::User,
                2,
                &json!({"text": "due-second", "status": "scheduled", "notBefore": now}),
            )
            .unwrap();
        let due_first = store
            .insert_entry(
                &chat,
                EntryKind::User,
                3,
                &json!({"text": "due-first", "status": "scheduled", "notBefore": now - 1_000}),
            )
            .unwrap();
        let due = store.scheduled_due("b", now).unwrap();
        assert_eq!(due.iter().map(|e| e.id.clone()).collect::<Vec<_>>(), [due_first.id, due_second.id]);
        assert!(!due.iter().any(|e| e.id == late.id));
    }

    /// A scheduled message's moves are one-way: the first transition lands, and a later one — a cancel racing a
    /// fire, two fires — is refused, so a snapshot taken earlier can never overwrite it.
    #[test]
    fn a_scheduled_message_moves_one_way() {
        let store = store_with_bot("b");
        let e = store
            .insert_entry(
                &Lane::main("b"),
                EntryKind::User,
                1,
                &json!({"text": "later", "status": "scheduled", "notBefore": crate::store::now_ms() + 60_000}),
            )
            .unwrap();
        let fired = store.flip_scheduled(&e.id, Some(2), &json!({"text": "later", "status": "queued"}), e.rev).unwrap();
        assert!(fired.is_some(), "the first move lands");
        let raced = store
            .flip_scheduled(&e.id, None, &json!({"text": "later", "status": "cancelled", "removed": true}), e.rev)
            .unwrap();
        assert!(raced.is_none(), "a second mover finds it isn't scheduled any more");
        assert_eq!(store.entry(&e.id).unwrap().data["status"], "queued", "the first move stands");
        assert_eq!(store.entry(&e.id).unwrap().turn, 2, "the fire's turn is kept");
    }

    /// An edit that lands first keeps its text: a fire holding a snapshot from before the edit finds the rev
    /// moved and refuses, so the sweep re-reads and delivers what was actually said.
    #[test]
    fn a_stale_fire_loses_to_a_landed_edit() {
        let store = store_with_bot("b");
        let e = store
            .insert_entry(
                &Lane::main("b"),
                EntryKind::User,
                1,
                &json!({"text": "first words", "status": "scheduled", "notBefore": crate::store::now_ms() + 60_000}),
            )
            .unwrap();
        // The edit lands first, as `updateScheduled` does: new text, still scheduled.
        let edited = store
            .flip_scheduled(
                &e.id,
                None,
                &json!({"text": "edited words", "status": "scheduled", "notBefore": crate::store::now_ms() + 60_000}),
                e.rev,
            )
            .unwrap();
        assert!(edited.is_some(), "the edit lands");
        // A fire holding the pre-edit snapshot loses: the rev moved under it.
        let raced =
            store.flip_scheduled(&e.id, Some(2), &json!({"text": "first words", "status": "queued"}), e.rev).unwrap();
        assert!(raced.is_none(), "the stale fire is refused");
        assert_eq!(store.entry(&e.id).unwrap().data["text"], "edited words", "the edit's text stands");
    }

    /// The refusals the desktop relies on: files and calls can't be scheduled, and a time too soon to be
    /// anything but a send is refused on both paths — a save and a hold.
    #[tokio::test]
    async fn files_calls_and_a_too_soon_time_are_refused() {
        let f = Fixture::new();
        // Files and calls are immediate-only.
        let files = crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "scheduleSend",
            json!({"botId": "b", "text": "with a file", "notBefore": crate::store::now_ms() + 60_000,
                  "attachments": [{"id": "a1", "name": "a.txt"}], "clientNonce": "n-files"}),
        )
        .await
        .unwrap_err();
        assert!(format!("{files:#}").contains("files"), "files refused: {files:#}");
        let call = crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "scheduleSend",
            json!({"botId": "b", "text": "in a call", "notBefore": crate::store::now_ms() + 60_000,
                  "call": "c1", "clientNonce": "n-call"}),
        )
        .await
        .unwrap_err();
        assert!(format!("{call:#}").contains("call"), "calls refused: {call:#}");
        // A time at the floor is a send, not a schedule: refused on the save path...
        let floor = crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "scheduleSend",
            json!({"botId": "b", "text": "too soon", "notBefore": crate::store::now_ms() + 10_000,
                  "clientNonce": "n-floor"}),
        )
        .await
        .unwrap_err();
        assert!(format!("{floor:#}").contains("at least 30"), "the floor refuses a save: {floor:#}");
        // ...and on the hold path (a queued message asked to fire too soon).
        f.send("live now").await;
        f.send("waiting").await;
        let waiting = f
            .hub
            .store
            .history("b", i64::MAX, 50)
            .unwrap()
            .into_iter()
            .find(|e| e.data["text"] == "waiting")
            .expect("the waiting message is saved");
        let hold = crate::api::dispatch(
            &f.hub,
            &crate::api::devices::Caller::Local,
            "updateScheduled",
            json!({"botId": "b", "entryId": waiting.id, "notBefore": crate::store::now_ms() + 10_000}),
        )
        .await
        .unwrap_err();
        assert!(format!("{hold:#}").contains("at least 30"), "the floor refuses a hold: {hold:#}");
        f.hub.shutdown().await;
        let _ = std::fs::remove_dir_all(&f.dir);
    }
}
