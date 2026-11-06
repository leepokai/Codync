//! Read marks: reading a chat, the roster's "Mark as read" and "Mark as Unread". The host alone decides what a read
//! does to a chat marked unread: a client's copy of `keptUnread` can be behind another device's mark.

use super::Hub;
use crate::LockExt;
use anyhow::{Result, bail};
use serde_json::Value;

impl Hub {
    /// The user read the chat: opened it, picked "Mark as read" or sent from it, which also ends a mark as unread.
    /// Nothing unread there is a no-op.
    pub fn mark_read(&self, id: &str) -> Result<()> {
        let _g = self.emit_lock.locked();
        self.read_locked(id)
    }

    /// The chat is on screen, so what's unread there is seen, unless it's marked unread: that waits for the user
    /// (`mark_read`). A no-op otherwise, so clients may call it whenever a chat is on screen.
    pub fn read_on_screen(&self, id: &str) -> Result<()> {
        let _g = self.emit_lock.locked();
        if self.store.kept_unread(id) {
            return Ok(());
        }
        self.read_locked(id)
    }

    /// Checked under `emit_lock`: a second concurrent read must see the first and no-op, not overwrite the read
    /// position `mark_unread` restores.
    fn read_locked(&self, id: &str) -> Result<()> {
        let Some(row) = self.store.bot(id)? else { bail!("unknown bot") };
        if self.store.unread(id, row.read_rev) == 0 && !self.store.kept_unread(id) {
            return Ok(());
        }
        self.store.mark_read(id)?;
        if let Some(v) = self.bot_value(id) {
            self.broadcast_bot(&v);
        }
        Ok(())
    }

    /// The user puts a chat back to unread (the roster's "Mark as Unread"). Returns the bot as it is now, also when
    /// there was nothing to mark and no event follows.
    pub fn mark_unread(&self, id: &str) -> Result<Value> {
        let _g = self.emit_lock.locked();
        let marked = self.store.mark_unread(id)?.is_some();
        let Some(v) = self.bot_value(id) else { bail!("unknown bot") };
        if marked {
            self.broadcast_bot(&v);
        }
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use crate::hub::Hub;
    use crate::remote::identity::Identity;
    use crate::store::{BotConfig, EntryKind, Lane, Store};
    use serde_json::json;

    /// A hub with one agent bot, "a".
    fn hub_with_bot() -> std::sync::Arc<Hub> {
        let dir = std::env::temp_dir().join(format!("codync-reads-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let cfg: BotConfig =
            serde_json::from_value(json!({"id": "a", "name": "Ann", "backend": "claude", "cwd": dir})).unwrap();
        store.save_bot(&cfg).unwrap();
        Hub::new(store, "reads-test".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0)
    }

    #[test]
    fn mark_unread_answers_with_the_bot_even_when_nothing_could_be_unread() {
        let hub = hub_with_bot();
        let mut events = hub.events.subscribe();

        let empty = hub.mark_unread("a").unwrap();
        assert_eq!(empty["keptUnread"], false);
        assert!(events.try_recv().is_err(), "no event for a no-op");

        hub.add_entry(&Lane::main("a"), EntryKind::Agent, 1, &json!({"text": "Done.", "final": true})).unwrap();
        hub.mark_read("a").unwrap();
        while events.try_recv().is_ok() {}
        let held = hub.mark_unread("a").unwrap();
        assert_eq!((held["keptUnread"].clone(), held["unread"].clone()), (json!(true), json!(1)));
        assert_eq!(events.try_recv().unwrap()["bot"]["keptUnread"], true);
        assert!(hub.mark_unread("missing").is_err());
    }

    #[test]
    fn a_chat_marked_unread_stays_so_on_screen_until_the_user_reads_it() {
        let hub = hub_with_bot();
        let reply = |turn| {
            hub.add_entry(&Lane::main("a"), EntryKind::Agent, turn, &json!({"text": "Done.", "final": true})).unwrap();
        };
        let unread = || hub.bot_value("a").unwrap()["unread"].clone();

        reply(1);
        hub.read_on_screen("a").unwrap();
        assert_eq!(unread(), 0, "on screen, what's unread is seen");

        hub.mark_unread("a").unwrap();
        let mut events = hub.events.subscribe();
        hub.read_on_screen("a").unwrap();
        assert_eq!(unread(), 1);
        assert!(events.try_recv().is_err(), "nothing changed, also for other devices");

        reply(2);
        hub.read_on_screen("a").unwrap();
        assert_eq!(unread(), 2, "a new reply stays unread with it");

        hub.mark_read("a").unwrap();
        assert_eq!((unread(), hub.bot_value("a").unwrap()["keptUnread"].clone()), (json!(0), json!(false)));

        reply(3);
        hub.read_on_screen("a").unwrap();
        assert_eq!(unread(), 0, "reading it ended the mark");
    }

    #[tokio::test]
    async fn only_an_automatic_mark_read_call_leaves_a_chat_marked_unread() {
        use crate::api::{devices::Caller, dispatch};
        let hub = hub_with_bot();
        hub.add_entry(&Lane::main("a"), EntryKind::Agent, 1, &json!({"text": "Done.", "final": true})).unwrap();
        hub.mark_read("a").unwrap();
        hub.mark_unread("a").unwrap();

        dispatch(&hub, &Caller::Local, "markRead", json!({"botId": "a", "automatic": true})).await.unwrap();
        assert_eq!(hub.bot_value("a").unwrap()["keptUnread"], true);

        // What an app from before `automatic` sends, and what opening a chat sends.
        dispatch(&hub, &Caller::Local, "markRead", json!({"botId": "a"})).await.unwrap();
        assert_eq!(hub.bot_value("a").unwrap()["keptUnread"], false);
    }

    #[test]
    fn concurrent_reads_keep_what_mark_unread_restores() {
        let hub = hub_with_bot();
        for turn in 1..=3 {
            hub.add_entry(&Lane::main("a"), EntryKind::Agent, turn, &json!({"text": "Done.", "final": true})).unwrap();
        }

        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| hub.mark_read("a").unwrap());
            }
        });

        assert_eq!(hub.mark_unread("a").unwrap()["unread"], 3);
    }
}
