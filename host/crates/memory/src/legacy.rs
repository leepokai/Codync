//! The memory entries Codync kept before memory became files (2.18 to 2.20): read once to move them into each
//! layer's `MEMORY.md`, then dropped.

use rusqlite::{Connection, OptionalExtension};

use crate::files::{Fact, parse_author};
use crate::store;

/// An old entry, as a fact of the layer it was in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// `None` for the shared layer.
    pub bot: Option<String>,
    /// Archived entries were out of the opening, still searchable.
    pub active: bool,
    pub fact: Fact,
}

/// The old entries, in the order the opening took them (pinned first, then the newest); `None` once they moved.
/// An entry a newer one replaced stays out: it is no longer true.
pub fn entries(conn: &Connection) -> store::Result<Option<Vec<Entry>>> {
    let exists = conn
        .query_row("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'memory_entries'", [], |_| Ok(()))
        .optional()?
        .is_some();
    if !exists {
        return Ok(None);
    }
    let mut stmt = conn.prepare(
        "SELECT bot, state, text, author, author_bot, evidence, created_at FROM memory_entries
         WHERE superseded_by IS NULL OR state = 'active'
         ORDER BY pinned DESC, updated_at DESC, id DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        let evidence: String = row.get(5)?;
        Ok(Entry {
            bot: row.get(0)?,
            active: row.get::<_, String>(1)? == "active",
            fact: Fact {
                text: row.get(2)?,
                author: parse_author(&row.get::<_, String>(3)?),
                author_bot: row.get(4)?,
                message: evidence.split(',').find(|id| !id.is_empty()).map(str::to_owned),
                at: row.get(6)?,
            },
        })
    })?;
    Ok(Some(rows.collect::<rusqlite::Result<_>>()?))
}

/// After the entries moved: old dream runs keep the lines they added and took out, then the entries' tables go.
pub fn finish(conn: &Connection) -> store::Result<()> {
    conn.execute_batch(
        "INSERT INTO memory_dream_lines (dream, change, text)
            SELECT c.dream, CASE c.change WHEN 'added' THEN 'added' ELSE 'removed' END, e.text
            FROM memory_dream_changes c JOIN memory_entries e ON e.id = c.entry ORDER BY c.rowid;
         DROP TABLE memory_dream_changes;
         DROP TABLE memory_entries;",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::Author;

    #[test]
    fn reads_the_old_entries_once_and_keeps_the_dream_log() {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO memory_entries (id, bot, state, text, author, author_bot, pinned, evidence, created_at, updated_at, rev)
                VALUES (1, 'a', 'active', 'Likes tea', 'dream', 'a', 0, 'm1', 10, 10, 1),
                       (2, 'a', 'archived', 'Lives in Taipei', 'user', NULL, 0, '', 5, 20, 2),
                       (3, NULL, 'active', 'Time zone is Asia/Taipei', 'bot', 'a', 1, '', 1, 1, 3);
             INSERT INTO memory_entries (id, bot, state, text, author, superseded_by, evidence, created_at, updated_at, rev)
                VALUES (4, 'a', 'archived', 'Lives in Kaohsiung', 'bot', 2, '', 1, 30, 4);
             INSERT INTO memory_dreams (id, bot, at) VALUES (7, 'a', 10);
             INSERT INTO memory_dream_changes (dream, entry, change) VALUES (7, 1, 'added'), (7, 2, 'archived');",
        )
        .unwrap();
        let old = entries(&conn).unwrap().unwrap();
        assert_eq!(
            old.iter().map(|e| e.fact.text.as_str()).collect::<Vec<_>>(),
            ["Time zone is Asia/Taipei", "Lives in Taipei", "Likes tea"],
            "a replaced entry stays out"
        );
        assert_eq!(
            (old[2].fact.author, old[2].fact.message.as_deref(), old[2].active),
            (Author::Dream, Some("m1"), true)
        );
        assert!(!old[1].active);
        finish(&conn).unwrap();
        assert_eq!(entries(&conn).unwrap(), None);
        let dream = crate::dream::get(&conn, 7).unwrap().unwrap();
        assert_eq!((dream.added[0].text.as_str(), dream.archived[0].text.as_str()), ("Likes tea", "Lives in Taipei"));
    }
}
