//! Session search corresponding to Hermes' runtime_session_store.py/session_persistence.py.
//! Adaptation: FTS5 candidates over Codync entries; existing visibility and substring rules stay authoritative.

use super::entries::{ENTRY_COLS, row_entry};
use super::visibility::PUBLIC;
use super::{Entry, Store};
use crate::LockExt;
use anyhow::Result;
use rusqlite::Connection;
use std::fmt::Write as _;

/// Backfill once and maintain the index transactionally with entry inserts, edits and deletions.
pub(super) fn open(c: &Connection) -> Result<()> {
    let exists: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='session_search')",
        [],
        |r| r.get(0),
    )?;
    let tx = c.unchecked_transaction()?;
    tx.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS session_search USING fts5(text, tokenize='trigram');
         CREATE TRIGGER IF NOT EXISTS session_search_insert AFTER INSERT ON entries
         WHEN new.kind = 'user' OR (new.kind = 'agent' AND json_extract(new.data, '$.final') = 1) BEGIN
           INSERT INTO session_search(rowid, text) VALUES(new.seq, CAST(COALESCE(json_extract(new.data, '$.text'), '') AS TEXT));
         END;
         CREATE TRIGGER IF NOT EXISTS session_search_update AFTER UPDATE OF data, kind ON entries
         WHEN old.kind = 'user' OR (old.kind = 'agent' AND json_extract(old.data, '$.final') = 1)
           OR new.kind = 'user' OR (new.kind = 'agent' AND json_extract(new.data, '$.final') = 1) BEGIN
           DELETE FROM session_search WHERE rowid=old.seq;
           INSERT INTO session_search(rowid, text)
             SELECT new.seq, CAST(COALESCE(json_extract(new.data, '$.text'), '') AS TEXT)
             WHERE new.kind = 'user' OR (new.kind = 'agent' AND json_extract(new.data, '$.final') = 1);
         END;
         CREATE TRIGGER IF NOT EXISTS session_search_delete AFTER DELETE ON entries BEGIN
           DELETE FROM session_search WHERE rowid=old.seq;
         END;",
    )?;
    if !exists {
        tx.execute(
            "INSERT INTO session_search(rowid, text)
             SELECT seq, CAST(COALESCE(json_extract(data, '$.text'), '') AS TEXT)
             FROM entries WHERE kind = 'user' OR (kind = 'agent' AND json_extract(data, '$.final') = 1)",
            [],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// ASCII terms of at least three letters are safe trigram candidates. Short terms and Unicode
/// keep the existing Unicode-lowercase substring path, including CJK queries shorter than three glyphs.
fn candidates(terms: &[String]) -> Option<String> {
    let terms: Vec<String> = terms
        .iter()
        .filter(|s| s.len() >= 3 && s.bytes().all(|b| b.is_ascii_alphanumeric()))
        .map(|s| format!("\"{s}\""))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" AND "))
}

impl Store {
    /// Visible messages containing every term, newest first, from `from` (a seq) back.
    pub fn search_messages(&self, bot_id: &str, terms: &[String], from: Option<i64>, limit: i64) -> Result<Vec<Entry>> {
        self.search_messages_between(bot_id, terms, from, limit, i64::MIN..i64::MAX)
    }

    /// The same search within a half-open range of Unix milliseconds.
    pub fn search_messages_between(
        &self,
        bot_id: &str,
        terms: &[String],
        from: Option<i64>,
        limit: i64,
        window: std::ops::Range<i64>,
    ) -> Result<Vec<Entry>> {
        let mut sql = format!(
            "SELECT {ENTRY_COLS} FROM entries WHERE bot_id = ?1 AND {PUBLIC} AND seq <= ?3 AND created_at >= ?4 AND created_at < ?5 AND
               (kind = 'user' OR (kind = 'agent' AND json_extract(data, '$.final') = 1))"
        );
        for i in 0..terms.len() {
            write!(
                sql,
                " AND instr(codync_lower(CAST(COALESCE(json_extract(data, '$.text'), '') AS TEXT)), ?{}) > 0",
                i + 6
            )?;
        }
        let mut args: Vec<rusqlite::types::Value> = vec![
            bot_id.to_owned().into(),
            limit.into(),
            from.unwrap_or(i64::MAX).into(),
            window.start.into(),
            window.end.into(),
        ];
        args.extend(terms.iter().map(|t| t.to_lowercase().into()));
        if let Some(query) = candidates(terms) {
            args.push(query.into());
            write!(sql, " AND seq IN (SELECT rowid FROM session_search WHERE session_search MATCH ?{})", args.len())?;
        }
        sql.push_str(" ORDER BY seq DESC LIMIT ?2");
        let c = self.db.locked();
        let mut st = c.prepare(&sql)?;
        Ok(st.query_map(rusqlite::params_from_iter(args), row_entry)?.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{EntryKind, Lane};
    use super::*;
    use rusqlite::params;
    use serde_json::json;

    #[test]
    fn hermes_fts_search_tracks_edits_and_deletions_without_exposing_internal_messages() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let lane = Lane::main("a");
        let first =
            store.insert_entry(&lane, EntryKind::User, 1, &json!({"text":"Yesterday we shipped Codync"})).unwrap();
        store.insert_entry(&Lane::main("b"), EntryKind::User, 1, &json!({"text":"shipped secret"})).unwrap();
        store.insert_entry(&lane, EntryKind::User, 2, &json!({"text":"shipped internal", "internal":true})).unwrap();
        store.insert_entry(&lane, EntryKind::Agent, 2, &json!({"text":"shipped draft", "final":false})).unwrap();
        let search = |term: &str| store.search_messages("a", &[term.to_owned()], None, 30).unwrap();
        assert_eq!(search("SHIPPED").iter().map(|e| &e.id).collect::<Vec<_>>(), [&first.id]);
        store.update_entry(&first.id, &json!({"text":"We released Codync"})).unwrap();
        assert!(search("shipped").is_empty());
        assert_eq!(search("released").len(), 1);
        store
            .with_connection(|c| {
                c.execute("DELETE FROM entries WHERE id=?1", params![first.id])?;
                Ok(())
            })
            .unwrap();
        assert!(search("released").is_empty());
    }

    #[test]
    fn hermes_search_preserves_short_unicode_and_literal_query_semantics() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        store
            .insert_entry(&Lane::main("a"), EntryKind::User, 1, &json!({"text":"你好 ÉCOLE alpha-beta OR done"}))
            .unwrap();
        for term in ["你好", "école", "alpha-beta", "OR", "alpha"] {
            assert_eq!(store.search_messages("a", &[term.to_owned()], None, 5).unwrap().len(), 1, "{term}");
        }
        assert!(store.search_messages("a", &["alpha OR missing".into()], None, 5).unwrap().is_empty());
    }

    #[test]
    fn hermes_fts_backfills_existing_entries_and_reopening_is_idempotent() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE entries(seq INTEGER PRIMARY KEY, kind TEXT, data TEXT); INSERT INTO entries VALUES(1, 'user', '{\"text\":\"before migration\"}');").unwrap();
        open(&c).unwrap();
        open(&c).unwrap();
        let found: i64 = c
            .query_row("SELECT count(*) FROM session_search WHERE session_search MATCH 'migration'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(found, 1);
    }

    #[test]
    fn hermes_history_date_windows_keep_both_boundaries_and_pagination() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        for (turn, at) in [(1, 10), (2, 20), (3, 30)] {
            let entry =
                store.insert_entry(&Lane::main("a"), EntryKind::User, turn, &json!({"text":"release"})).unwrap();
            store
                .with_connection(|c| {
                    c.execute("UPDATE entries SET created_at=?1 WHERE id=?2", params![at, entry.id])?;
                    Ok(())
                })
                .unwrap();
        }
        let entries = store.search_messages_between("a", &["release".into()], None, 10, 10..30).unwrap();
        assert_eq!(entries.iter().map(|e| e.created_at).collect::<Vec<_>>(), [20, 10]);
        let page = store.search_messages_between("a", &[], Some(entries[1].seq), 1, 10..30).unwrap();
        assert_eq!(page[0].created_at, 10);
    }
}
