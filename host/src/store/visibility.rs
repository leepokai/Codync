//! Public conversations exclude internal scheduling and provisional background output.

use super::entries::{ENTRY_COLS, row_entry};
use super::{Entry, Store};
use crate::LockExt;
use anyhow::Result;
use rusqlite::params;
use serde_json::Value;

/// Keep the SQL and wire predicates equivalent, including prompts saved before `internal` was introduced.
pub(super) const PUBLIC: &str =
    "COALESCE(json_extract(data, '$.internal'), 0) != 1 AND COALESCE(json_extract(data, '$.from'), '') != 'proactive'";

pub fn is_internal(data: &Value) -> bool {
    data["internal"] == true || data["from"] == "proactive"
}

impl Store {
    /// A scheduled turn's completed messages, still held back until its successful completion.
    pub(crate) fn proactive_outputs(&self, bot: &str, turn: i64) -> Result<Vec<Entry>> {
        let c = self.db.locked();
        let mut statement = c.prepare(&format!(
            "SELECT {ENTRY_COLS} FROM entries WHERE bot_id = ?1 AND turn = ?2 AND thread_id IS NULL
             AND kind = 'agent' AND json_extract(data, '$.internal') = 1 AND json_extract(data, '$.final') = 1
             ORDER BY seq"
        ))?;
        Ok(statement.query_map(params![bot, turn], row_entry)?.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{EntryKind, Lane, tests::temp_store};
    use serde_json::json;

    #[test]
    fn internal_prompts_and_output_stay_out_of_public_history_search_previews_and_unread() {
        let store = temp_store();
        let lane = Lane::main("b");
        store.insert_entry(&lane, EntryKind::User, 1, &json!({"text":"Keep this"})).unwrap();
        store.insert_entry(&lane, EntryKind::User, 2, &json!({"text":"Scheduled prompt", "from":"proactive"})).unwrap();
        store
            .insert_entry(
                &lane,
                EntryKind::Agent,
                2,
                &json!({
                    "text":"NO_UPDATE", "internal":true, "final":true, "call":"call",
                    "files":[{"id":"file"}],
                }),
            )
            .unwrap();
        assert_eq!(store.history("b", i64::MAX, 100).unwrap().len(), 1);
        assert_eq!(store.last_message("b").unwrap().text, "Keep this");
        assert_eq!(store.unread("b", 0), 0);
        assert_eq!(store.search_messages("b", &[], None, 100).unwrap().len(), 1);
        assert_eq!(store.messages_after(&lane, 0, 100).unwrap().len(), 1);
        assert_eq!(store.conversation_since("b", 0, 100).unwrap().len(), 1);
        assert_eq!(store.roster_preview("b").unwrap().len(), 1);
        assert!(store.file_messages("b", 100).unwrap().is_empty());
        assert!(store.call_entries("b", "call", 100).unwrap().is_empty());
        assert_eq!(
            store.entries_since(0, 100).unwrap().len(),
            3,
            "sync carries flags so clients can apply later promotions"
        );
    }

    #[test]
    fn an_internal_entry_cannot_be_quoted_bookmarked_or_opened_by_context() {
        let store = temp_store();
        let entry = store
            .insert_entry(&Lane::main("b"), EntryKind::Agent, 1, &json!({"text":"Private", "final":true}))
            .unwrap();
        store.save_bookmark("b", &entry.id, None, "b", "Private", "").unwrap();
        let mut data = entry.data.clone();
        data["internal"] = true.into();
        store.update_entry(&entry.id, &data).unwrap();
        let hidden = store.entry(&entry.id).unwrap();
        assert_eq!(store.list_bookmarks("b").unwrap().len(), 0);
        assert!(store.save_bookmark("b", &entry.id, None, "b", "Private", "").is_err());
        assert!(crate::chat::replies::quote(&store, "b", &entry.id).is_err());
        assert!(store.message_context(&hidden).is_err());
    }

    #[test]
    fn useful_scheduled_output_can_be_published_and_survives_a_store_reopen() {
        let dir = std::env::temp_dir().join(format!("codync-internal-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let store = Store::open(&path).unwrap();
        let lane = Lane::main("b");
        let prompt = store
            .insert_entry(
                &lane,
                EntryKind::User,
                1,
                &json!({"from":"proactive", "internal":true, "text":"Check", "status":"queued"}),
            )
            .unwrap();
        let output = store
            .insert_entry(&lane, EntryKind::Agent, 1, &json!({"text":"Build passed", "final":true, "internal":true}))
            .unwrap();
        assert_eq!(store.proactive_outputs("b", 1).unwrap().len(), 1);
        let mut data = output.data;
        data["internal"] = false.into();
        store.update_entry(&output.id, &data).unwrap();
        store.expire_pending().unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        assert!(is_internal(&store.entry(&prompt.id).unwrap().data));
        assert!(store.proactive_outputs("b", 1).unwrap().is_empty());
        assert_eq!(store.history("b", i64::MAX, 100).unwrap().len(), 1);
        assert_eq!(store.last_message("b").unwrap().text, "Build passed");
        assert_eq!(store.unread("b", 0), 1);
    }

    #[test]
    fn a_person_typing_the_sentinel_is_still_a_visible_message() {
        assert!(!is_internal(&json!({"text":"NO_UPDATE"})));
        assert!(is_internal(&json!({"internal":true})));
        assert!(is_internal(&json!({"from":"proactive"})));
    }
}
