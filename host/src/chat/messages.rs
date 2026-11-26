//! The user's messages in a chat: typed (`send`) or relayed from a voice call (`chat::calls`).

use crate::agent::bot::Cmd;
use crate::hub::Hub;
use crate::store::{BotConfig, Entry, Lane};
use anyhow::{Context, Result};
use serde_json::Value;
use std::sync::Arc;

/// Posts `data` (the message's text and what it carries) as the user's newest message in `cfg`'s chat and hands it
/// on: to the bot's agent, or to a group's members.
pub fn post(hub: &Arc<Hub>, cfg: &BotConfig, data: Value) -> Result<Entry> {
    let (e, added) = save_once(hub, cfg, data)?;
    if added {
        hand_on(hub, cfg, &e)?;
    }
    Ok(e)
}

/// [`post`]'s first half, `data` saved as the user's newest message in `cfg`'s chat: for a caller to whom a message
/// that was never saved differs from one that was saved and not taken.
pub fn save(hub: &Hub, cfg: &BotConfig, data: Value) -> Result<Entry> {
    save_once(hub, cfg, data).map(|(entry, _)| entry)
}

fn save_once(hub: &Hub, cfg: &BotConfig, mut data: Value) -> Result<(Entry, bool)> {
    // A group has no agent of its own to queue behind: its message is simply sent.
    data["status"] = (if cfg.is_group() { "sent" } else { "queued" }).into();
    let turn = hub.store.max_turn(&cfg.id) + 1;
    let (entry, added) = hub.add_user_entry(&Lane::main(&cfg.id), turn, &data).context("couldn't save the message")?;
    // Before the turn starts, so its reply lands unread.
    if added && let Err(error) = hub.mark_read(&cfg.id) {
        tracing::warn!(%error, bot = %cfg.id, "couldn't mark bot read after send");
    }
    Ok((entry, added))
}

/// [`post`]'s second half: the saved message `e` goes to the bot's agent, or to a group's members.
pub fn hand_on(hub: &Arc<Hub>, cfg: &BotConfig, e: &Entry) -> Result<()> {
    let lane = Lane::main(&cfg.id);
    if cfg.is_group() {
        // Each member reads it in its room prompt (`chat::group`).
        crate::chat::group::start(hub, cfg, lane);
    } else {
        // `/skill …` reaches the agent as "read this skill first"; the chat keeps what was typed.
        let prompt = crate::chat::prompt::of_entry(hub, cfg, e);
        hub.send_cmd(&cfg.id, Cmd::Send { lane, entry_id: e.id.clone(), text: prompt })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::identity::Identity;
    use crate::store::{EntryKind, Store};
    use serde_json::json;
    use std::sync::Barrier;

    #[tokio::test]
    async fn sends_that_both_missed_the_nonce_check_post_once() {
        let dir = std::env::temp_dir().join(format!("codync-send-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let hub = Hub::new(
            Store::open(std::path::Path::new(":memory:")).unwrap(),
            "test".into(),
            Identity::load_or_create(&dir).unwrap(),
            "token".into(),
            0,
        );
        std::fs::remove_dir_all(dir).unwrap();
        let bot = |id: &str| {
            let cfg =
                serde_json::from_value(json!({"id": id, "name": id, "cwd": "/tmp", "backend": "custom"})).unwrap();
            hub.store.save_bot(&cfg).unwrap();
            cfg
        };
        let a = bot("a");
        let b = bot("b");
        let mut actor = hub.attach_bot_for_test("a");
        let mut other_actor = hub.attach_bot_for_test("b");
        let mut events = hub.events.subscribe();
        let both_checked = Barrier::new(2);
        let send =
            |cfg: &BotConfig, nonce: &str| post(&hub, cfg, json!({"text": "hello", "clientNonce": nonce})).unwrap();
        let attempt = || {
            let previous = hub.store.find_by_nonce("a", "same");
            both_checked.wait();
            assert!(previous.is_none());
            send(&a, "same")
        };
        let (first, duplicate) = std::thread::scope(|scope| {
            let first = scope.spawn(attempt);
            let duplicate = attempt();
            (first.join().unwrap(), duplicate)
        });
        assert_eq!(first.id, duplicate.id);
        let different = send(&a, "unique");
        let other = send(&b, "same");
        let empty = send(&a, "");
        let also_empty = send(&a, "");
        assert_ne!(empty.id, also_empty.id);
        assert_ne!(first.id, other.id);

        let main = [&first.id, &different.id, &empty.id, &also_empty.id];
        let stored = hub.store.history("a", i64::MAX, 10).unwrap();
        assert!(stored.iter().map(|e| &e.id).eq(main));
        let broadcasts = std::iter::from_fn(|| events.try_recv().ok())
            .filter(|v| v["type"] == "entry")
            .map(|v| v["entry"]["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert!(broadcasts.iter().eq([&first.id, &different.id, &other.id, &empty.id, &also_empty.id]));
        let delivered = std::iter::from_fn(|| actor.try_recv().ok()).filter_map(|cmd| match cmd {
            Cmd::Send { entry_id, .. } => Some(entry_id),
            _ => None,
        });
        assert!(delivered.eq(main.into_iter().cloned()));
        assert!(matches!(other_actor.try_recv().unwrap(), Cmd::Send { entry_id, .. } if entry_id == other.id));
        assert!(other_actor.try_recv().is_err());
        let legacy = hub
            .store
            .insert_entry(&Lane::main("b"), EntryKind::User, 2, &json!({"text": "before", "clientNonce": "legacy"}))
            .unwrap();
        assert_eq!(send(&b, "legacy").id, legacy.id);
        assert!(other_actor.try_recv().is_err());
    }
}
