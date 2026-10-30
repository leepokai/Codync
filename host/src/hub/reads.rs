//! The roster's read marks: "Mark as read" and "Mark as Unread".

use super::Hub;
use crate::LockExt;
use anyhow::{Result, bail};
use serde_json::{Value, json};

impl Hub {
    /// Marks what the user has seen as read. Nothing unread there is a no-op, so clients
    /// may call it whenever a chat is on screen.
    pub fn mark_read(&self, id: &str) -> Result<()> {
        let Some(row) = self.store.bot(id)? else { bail!("unknown bot") };
        if self.store.unread(id, row.read_rev) == 0 && !self.store.kept_unread(id) {
            return Ok(());
        }
        let _g = self.emit_lock.locked();
        self.store.mark_read(id)?;
        if let Some(v) = self.bot_value(id) {
            let _ = self.events.send(json!({"type": "bot", "rev": v["rev"], "bot": v}));
        }
        Ok(())
    }

    /// The user puts a chat back to unread (the roster's "Mark as Unread"). Returns the bot as it is now, so a
    /// client can settle its pending hold even when there was nothing to mark and no event follows.
    pub fn mark_unread(&self, id: &str) -> Result<Value> {
        if self.store.bot(id)?.is_none() {
            bail!("unknown bot")
        }
        let _g = self.emit_lock.locked();
        let marked = self.store.mark_unread(id)?.is_some();
        let Some(v) = self.bot_value(id) else { bail!("unknown bot") };
        if marked {
            let _ = self.events.send(json!({"type": "bot", "rev": v["rev"], "bot": v}));
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

    #[test]
    fn mark_unread_answers_with_the_bot_even_when_nothing_could_be_unread() {
        let dir = std::env::temp_dir().join(format!("codync-reads-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let cfg: BotConfig =
            serde_json::from_value(json!({"id": "a", "name": "Ann", "backend": "claude", "cwd": dir})).unwrap();
        store.save_bot(&cfg).unwrap();
        let hub = Hub::new(store, "reads-test".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0);
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
}
