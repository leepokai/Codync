//! Bounded roster queries share the transcript's visible-message rules.

use super::entries::row_entry;
use super::visibility::PUBLIC;
use super::{Entry, Store};
use crate::LockExt;
use anyhow::Result;
use rusqlite::{params, params_from_iter};
use std::fmt::Write as _;

const COLS: &str = "seq, id, bot_id, rev, kind, turn, data, created_at, updated_at, thread_id";
const VISIBLE: &str = "(kind = 'user' OR (kind = 'agent' AND json_extract(data, '$.final') = 1))";

impl Store {
    pub fn roster_search(&self, bots: &[String], terms: &[String], before: i64, limit: i64) -> Result<Vec<Entry>> {
        self.roster_search_scoped(bots, terms, before, limit, super::SearchScope::All)
    }

    pub fn roster_search_scoped(
        &self,
        bots: &[String],
        terms: &[String],
        before: i64,
        limit: i64,
        scope: super::SearchScope,
    ) -> Result<Vec<Entry>> {
        let mut sql = format!(
            "SELECT {COLS} FROM entries WHERE bot_id IN (SELECT value FROM json_each(?1))
             AND seq < ?2 AND {PUBLIC} AND {VISIBLE}
             AND EXISTS (SELECT 1 FROM bots WHERE bots.id = entries.bot_id AND deleted = 0
                         AND COALESCE(json_extract(config, '$.hidden'), 0) = 0)"
        );
        let mut args: Vec<rusqlite::types::Value> = vec![serde_json::to_string(bots)?.into(), before.into()];
        match scope {
            super::SearchScope::All => {}
            super::SearchScope::Main => sql.push_str(" AND thread_id IS NULL"),
            super::SearchScope::Threads => sql.push_str(" AND thread_id IS NOT NULL"),
        }
        for term in terms {
            args.push(term.to_lowercase().into());
            write!(
                sql,
                " AND instr(codync_lower(CAST(COALESCE(json_extract(data, '$.text'), '') AS TEXT)), ?{}) > 0",
                args.len()
            )?;
        }
        args.push(limit.into());
        write!(sql, " ORDER BY seq DESC LIMIT ?{}", args.len())?;
        let c = self.db.locked();
        let mut statement = c.prepare(&sql)?;
        Ok(statement.query_map(params_from_iter(args), row_entry)?.collect::<rusqlite::Result<_>>()?)
    }

    /// Main-chat preview only; reading it never advances a read cursor.
    pub fn roster_preview(&self, bot: &str) -> Result<Vec<Entry>> {
        let c = self.db.locked();
        let mut statement = c.prepare(&format!(
            "SELECT {COLS} FROM entries WHERE bot_id = ?1 AND thread_id IS NULL AND {PUBLIC} AND {VISIBLE}
             ORDER BY seq DESC LIMIT 3"
        ))?;
        let mut rows = statement.query_map([bot], row_entry)?.collect::<rusqlite::Result<Vec<_>>>()?;
        rows.reverse();
        Ok(rows)
    }

    /// A window around a search hit in its own lane, plus its thread root.
    pub fn message_context(&self, entry: &Entry) -> Result<Vec<Entry>> {
        anyhow::ensure!(!super::is_internal(&entry.data), "message is internal");
        let c = self.db.locked();
        let mut statement = c.prepare(&format!(
            "SELECT {COLS} FROM entries WHERE {PUBLIC} AND id IN (
               SELECT id FROM (SELECT id FROM entries WHERE bot_id = ?1 AND {PUBLIC} AND thread_id IS ?2 AND seq <= ?3
                               ORDER BY seq DESC LIMIT 20)
               UNION SELECT id FROM (SELECT id FROM entries WHERE bot_id = ?1 AND {PUBLIC} AND thread_id IS ?2 AND seq > ?3
                                     ORDER BY seq LIMIT 20)
               UNION SELECT id FROM entries WHERE bot_id = ?1 AND id = ?2) ORDER BY seq"
        ))?;
        Ok(statement
            .query_map(params![entry.bot_id, entry.thread_id, entry.seq], row_entry)?
            .collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{BotConfig, EntryKind, Lane};
    use serde_json::json;

    #[test]
    fn search_pages_visible_messages_and_preserves_the_lane() {
        let s = Store::open(std::path::Path::new(":memory:")).unwrap();
        for id in ["a", "b"] {
            let config: BotConfig = serde_json::from_value(json!({"id":id,"name":id})).unwrap();
            s.save_bot(&config).unwrap();
        }
        let main = Lane { chat: "a".into(), thread: None };
        let root = s.insert_entry(&main, EntryKind::User, 0, &json!({"text":"needle root"})).unwrap();
        let lane = Lane { chat: "a".into(), thread: Some(root.id.clone()) };
        let reply = s.insert_entry(&lane, EntryKind::Agent, 0, &json!({"text":"needle reply","final":true})).unwrap();
        s.insert_entry(&main, EntryKind::Thought, 0, &json!({"text":"needle private"})).unwrap();
        s.insert_entry(&main, EntryKind::Agent, 0, &json!({"text":"needle streaming"})).unwrap();
        s.insert_entry(
            &Lane { chat: "b".into(), thread: None },
            EntryKind::User,
            0,
            &json!({"text":"needle elsewhere"}),
        )
        .unwrap();
        let ids = vec!["a".into()];
        let terms = vec!["NEEDLE".into()];
        let hits = s.roster_search(&ids, &terms, i64::MAX, 1).unwrap();
        assert_eq!(hits[0].id, reply.id);
        let main_hits = s.roster_search_scoped(&ids, &terms, i64::MAX, 1, super::super::SearchScope::Main).unwrap();
        assert_eq!(main_hits[0].id, root.id, "scope is applied before LIMIT, not after paging");
        let thread_hits =
            s.roster_search_scoped(&ids, &terms, i64::MAX, 1, super::super::SearchScope::Threads).unwrap();
        assert_eq!(thread_hits[0].id, reply.id);
        assert!(
            s.roster_search_scoped(&ids, &terms, reply.seq, 1, super::super::SearchScope::Threads).unwrap().is_empty()
        );
        assert_eq!(s.roster_search(&ids, &terms, reply.seq, 1).unwrap()[0].id, root.id);
        let context = s.message_context(&reply).unwrap();
        assert!(context.iter().any(|e| e.id == root.id));
        assert!(context.iter().any(|e| e.id == reply.id));
        assert_eq!(s.roster_preview("a").unwrap().len(), 1);
    }

    #[test]
    fn unicode_search_excludes_hidden_bots_and_preview_does_not_mark_read() {
        let s = Store::open(std::path::Path::new(":memory:")).unwrap();
        let mut config: BotConfig = serde_json::from_value(json!({"id":"a","name":"A"})).unwrap();
        s.save_bot(&config).unwrap();
        for text in ["first", "second", "ÁRBOL literal %_", "fourth"] {
            s.insert_entry(&Lane::main("a"), EntryKind::Agent, 0, &json!({"text":text,"final":true})).unwrap();
        }
        let before = s.bot("a").unwrap().unwrap().read_rev;
        assert_eq!(s.roster_preview("a").unwrap().len(), 3);
        assert_eq!(s.bot("a").unwrap().unwrap().read_rev, before);
        let terms = vec!["árbol".into(), "%_".into()];
        assert_eq!(s.roster_search(&["a".into()], &terms, i64::MAX, 10).unwrap().len(), 1);
        assert_eq!(s.search_messages("a", &terms, None, 10).unwrap().len(), 1);
        config.hidden = true;
        s.save_bot(&config).unwrap();
        assert_eq!(s.roster_search(&["a".into()], &terms, i64::MAX, 10).unwrap().len(), 0);
    }
}
