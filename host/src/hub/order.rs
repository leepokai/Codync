//! The roster's order: where the user dragged each bot and, with automatic order on, the bot with
//! the newest message moved to the top.

use super::Hub;
use crate::LockExt;
use crate::store::{BotRow, Entry};
use anyhow::Result;

/// The `kv` key of the automatic order switch: "1" when on.
const AUTO_ORDER: &str = "autoOrderBots";

impl Hub {
    /// Puts `ids` (the roster as shown, top first) in that order. Unknown and deleted ids are
    /// skipped; bots left out (hidden ones) keep their place.
    pub fn reorder_bots(&self, ids: &[String]) -> Result<()> {
        for (position, id) in (0_i64..).zip(ids) {
            {
                // Read and save under one lock: an edit in between would otherwise be written back.
                let _g = self.emit_lock.locked();
                let Some(mut row) = self.store.bot(id)?.filter(|r| !r.deleted) else { continue };
                if row.config.position == position {
                    continue;
                }
                row.config.position = position;
                self.store.save_bot(&row.config)?;
            }
            self.emit_bot(id);
        }
        Ok(())
    }

    /// Whether a bot moves to the top of the roster when it gets a new message.
    pub fn auto_order(&self) -> bool {
        self.store.kv_get(AUTO_ORDER).as_deref() == Some("1")
    }

    pub fn set_auto_order(&self, on: bool) -> Result<()> {
        if on { self.store.kv_set(AUTO_ORDER, "1") } else { self.store.kv_delete(AUTO_ORDER) }
    }

    /// With automatic order on, puts the chat `entry` is in on top of the roster when `entry` is a message
    /// its preview shows (`Store::last_message`): the user's, or an agent's finished reply. Dragging still
    /// works; the chat's next message moves it again.
    pub(crate) fn order_on_message(&self, entry: &Entry) {
        if !entry.is_message() || !entry.data["call"].is_null() || !self.auto_order() {
            return;
        }
        match self.move_to_top(&entry.bot_id) {
            Ok(true) => self.emit_bot(&entry.bot_id),
            Ok(false) => {}
            Err(error) => {
                tracing::warn!(error = format!("{error:#}"), bot = entry.bot_id, "couldn't move the bot to the top");
            }
        }
    }

    /// Gives `id` a position above every other live bot; `false` when it already had one.
    fn move_to_top(&self, id: &str) -> Result<bool> {
        let _g = self.emit_lock.locked();
        let bots = self.store.bots()?;
        let top = top_position(&bots, Some(id));
        let Some(mut row) = bots.into_iter().find(|b| b.config.id == id && !b.deleted) else { return Ok(false) };
        let Some(top) = top.filter(|&t| row.config.position >= t) else { return Ok(false) };
        row.config.position = top - 1;
        self.store.save_bot(&row.config)?;
        Ok(true)
    }

    /// A new bot goes on top once the user has arranged the roster; until then it stays at 0,
    /// so recent activity keeps ordering everything.
    pub(super) fn new_bot_position(&self) -> Result<i64> {
        let bots = self.store.bots()?;
        let arranged = bots.iter().any(|b| !b.deleted && b.config.position != 0);
        Ok(if arranged { top_position(&bots, None).unwrap_or(0) - 1 } else { 0 })
    }
}

/// The lowest position among live bots other than `except`: the top of the roster.
fn top_position(bots: &[BotRow], except: Option<&str>) -> Option<i64> {
    bots.iter().filter(|b| !b.deleted && Some(b.config.id.as_str()) != except).map(|b| b.config.position).min()
}

#[cfg(test)]
mod tests {
    use crate::hub::Hub;
    use crate::remote::identity::Identity;
    use crate::store::{BotConfig, EntryKind, Lane, Store};

    #[tokio::test]
    async fn dragged_order_sticks_and_new_bots_go_on_top() {
        let dir = std::env::temp_dir().join(format!("codync-order-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let hub = Hub::new(store, "order-test".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0);
        for id in ["a", "b", "c"] {
            let cfg: BotConfig =
                serde_json::from_value(serde_json::json!({"id": id, "name": id, "backend": "claude", "cwd": "/tmp"}))
                    .unwrap();
            hub.store.save_bot(&cfg).unwrap();
        }
        assert_eq!(hub.new_bot_position().unwrap(), 0, "unarranged: recent activity orders new bots too");

        hub.reorder_bots(&["c".into(), "a".into(), "gone".into(), "b".into()]).unwrap();
        let position = |id: &str| hub.store.bot(id).unwrap().unwrap().config.position;
        assert!(position("c") < position("a") && position("a") < position("b"));
        assert_eq!(hub.new_bot_position().unwrap(), -1, "arranged: a new bot goes on top");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn automatic_order_puts_the_bot_with_a_new_message_on_top() {
        let dir = std::env::temp_dir().join(format!("codync-auto-order-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let hub = Hub::new(store, "auto-order-test".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0);
        for id in ["a", "b", "c"] {
            let cfg: BotConfig =
                serde_json::from_value(serde_json::json!({"id": id, "name": id, "backend": "claude", "cwd": "/tmp"}))
                    .unwrap();
            hub.store.save_bot(&cfg).unwrap();
        }
        hub.reorder_bots(&["a".into(), "b".into(), "c".into()]).unwrap();
        let position = |id: &str| hub.store.bot(id).unwrap().unwrap().config.position;
        let say = |id: &str, kind: EntryKind, data: serde_json::Value| {
            hub.add_entry(&Lane::main(id), kind, 1, &data).unwrap();
        };

        say("c", EntryKind::User, serde_json::json!({"text": "hi"}));
        assert!(position("a") < position("c"), "off: the dragged order stays");

        hub.set_auto_order(true).unwrap();
        assert!(hub.auto_order());
        say("c", EntryKind::User, serde_json::json!({"text": "hi"}));
        assert!(position("c") < position("a") && position("a") < position("b"), "a message puts the bot on top");
        say("b", EntryKind::Agent, serde_json::json!({"text": "working", "final": false}));
        assert!(position("c") < position("b"), "an unfinished reply doesn't");
        say("b", EntryKind::Agent, serde_json::json!({"text": "done", "final": true}));
        assert!(position("b") < position("c"), "a finished reply does");
        let top = position("b");
        say("b", EntryKind::User, serde_json::json!({"text": "again"}));
        assert_eq!(position("b"), top, "already on top: left alone");

        let reply = hub.add_entry(&Lane::main("a"), EntryKind::Agent, 1, &serde_json::json!({"text": "…"})).unwrap();
        hub.set_entry(&reply.id, &serde_json::json!({"text": "done", "final": true})).unwrap();
        assert!(position("a") < position("b"), "finishing a reply puts the bot on top");

        let sent = hub.add_entry(&Lane::main("c"), EntryKind::User, 1, &serde_json::json!({"text": "hi"})).unwrap();
        hub.reorder_bots(&["b".into(), "c".into(), "a".into()]).unwrap();
        hub.set_entry(&sent.id, &serde_json::json!({"text": "hi", "status": "sent"})).unwrap();
        hub.set_entry(&reply.id, &serde_json::json!({"text": "done!", "final": true})).unwrap();
        assert!(position("b") < position("c"), "a status change after a drag leaves the order alone");
        assert!(position("c") < position("a"), "so does editing a reply that was already finished");

        hub.set_auto_order(false).unwrap();
        assert!(!hub.auto_order());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
