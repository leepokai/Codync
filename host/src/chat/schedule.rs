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
    let hub = hub.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(SWEEP_EVERY);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            if let Err(error) = sweep(&hub) {
                tracing::warn!(error = format!("{error:#}"), "couldn't sweep scheduled sends");
            }
        }
    });
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
            }
        }
    }
    Ok(())
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
                "id": "b", "name": "b", "backend": "fixture", "cwd": cwd,
                "command": codync_agent::shell::python(&agent),
                "notify": false, "permission": "ask",
            }))
            .unwrap();
            store.save_bot(&cfg).unwrap();
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
            crate::chat::messages::save_scheduled(
                &self.hub,
                &self.hub.store.bot("b").unwrap().unwrap().config,
                json!({"text": text, "clientNonce": uuid::Uuid::new_v4().to_string()}),
                crate::store::now_ms() + in_ms,
            )
            .unwrap()
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
        assert!(first.turn < second.turn);
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
        // A live send starts a turn; a second message waits behind it.
        f.send("live now").await;
        f.until(|| f.prompts().len() == 1).await;
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
            json!({"botId": "b", "entryId": waiting.id, "notBefore": crate::store::now_ms() + 150}),
        )
        .await
        .unwrap();
        f.until(|| f.status_of(&waiting.id) == "scheduled").await;
        // The running turn ends with nothing new to deliver: the live queue no longer holds it.
        f.until(|| f.prompts().len() == 2 && f.prompts()[1].contains("held and edited")).await;
        assert!(!f.prompts()[1].contains("first draft"), "the queue's copy is the edited one");
        assert_eq!(f.status_of(&waiting.id), "sent");
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
        // Past its (cancelled) time, the sweep delivers nothing more.
        sweep(&f.hub).unwrap();
        assert_eq!(f.status_of(&e.id), "cancelled");
        assert_eq!(f.prompts().len(), 0);
        // Editing something that already fired is refused.
        let fired = f.schedule("will fire", 50);
        f.until(|| f.status_of(&fired.id) != "scheduled").await;
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
}
