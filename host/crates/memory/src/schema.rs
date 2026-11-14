//! The crate's own tables and their migrations, versioned apart from the caller's schema.

use rusqlite::{Connection, OptionalExtension};

const MIGRATIONS: &[&str] = &[
    // 1: entries, tombstones and the dream log.
    "CREATE TABLE memory_entries (
        id INTEGER PRIMARY KEY,
        bot TEXT,
        state TEXT NOT NULL,
        text TEXT NOT NULL,
        author TEXT NOT NULL,
        author_bot TEXT,
        pinned INTEGER NOT NULL DEFAULT 0,
        superseded_by INTEGER,
        room TEXT,
        evidence TEXT NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        rev INTEGER NOT NULL
    );
    CREATE INDEX memory_entries_bot ON memory_entries (bot, state);
    CREATE TABLE memory_tombstones (
        scope TEXT NOT NULL,
        hash TEXT NOT NULL,
        PRIMARY KEY (scope, hash)
    ) WITHOUT ROWID;
    CREATE TABLE memory_dreams (
        id INTEGER PRIMARY KEY,
        bot TEXT NOT NULL,
        at INTEGER NOT NULL,
        undone INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX memory_dreams_bot ON memory_dreams (bot, at);
    CREATE TABLE memory_dream_changes (
        dream INTEGER NOT NULL,
        entry INTEGER NOT NULL,
        change TEXT NOT NULL
    );
    CREATE INDEX memory_dream_changes_dream ON memory_dream_changes (dream);",
    // 2: long-form files (the layer's key is '' for the shared one), their versions, and the ones dreaming wrote.
    "CREATE TABLE memory_files (
        bot TEXT NOT NULL,
        name TEXT NOT NULL,
        hash TEXT NOT NULL,
        size INTEGER NOT NULL,
        mtime INTEGER NOT NULL,
        description TEXT,
        tokens INTEGER NOT NULL,
        version INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        rev INTEGER NOT NULL,
        PRIMARY KEY (bot, name)
    ) WITHOUT ROWID;
    CREATE TABLE memory_file_versions (
        id INTEGER PRIMARY KEY,
        bot TEXT NOT NULL,
        name TEXT NOT NULL,
        content TEXT,
        author TEXT NOT NULL,
        author_bot TEXT,
        cause TEXT NOT NULL,
        at INTEGER NOT NULL,
        rev INTEGER NOT NULL
    );
    CREATE INDEX memory_file_versions_file ON memory_file_versions (bot, name, id);
    CREATE TABLE memory_dream_files (
        dream INTEGER NOT NULL,
        version INTEGER NOT NULL
    );
    CREATE INDEX memory_dream_files_dream ON memory_dream_files (dream);",
    // 3: entry ids are never reused, so an old undo line, a dream's undo or a bot's stale id can't reach a newer
    // entry. Ids the dream log still names count as used.
    "CREATE TABLE memory_entries_new (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        bot TEXT,
        state TEXT NOT NULL,
        text TEXT NOT NULL,
        author TEXT NOT NULL,
        author_bot TEXT,
        pinned INTEGER NOT NULL DEFAULT 0,
        superseded_by INTEGER,
        room TEXT,
        evidence TEXT NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        rev INTEGER NOT NULL
    );
    INSERT INTO memory_entries_new SELECT * FROM memory_entries;
    DROP TABLE memory_entries;
    ALTER TABLE memory_entries_new RENAME TO memory_entries;
    CREATE INDEX memory_entries_bot ON memory_entries (bot, state);
    DELETE FROM sqlite_sequence WHERE name = 'memory_entries';
    INSERT INTO sqlite_sequence (name, seq) SELECT 'memory_entries', max(
        coalesce((SELECT max(id) FROM memory_entries), 0),
        coalesce((SELECT max(entry) FROM memory_dream_changes), 0));",
    // 4: memory is files. Who wrote each fact of a layer's MEMORY.md and what it was learned from, and the lines a
    // dream added and removed. The entries move into the files once (`legacy`), which drops their tables.
    "CREATE TABLE memory_facts (
        bot TEXT NOT NULL,
        fingerprint TEXT NOT NULL,
        author TEXT NOT NULL,
        author_bot TEXT,
        message TEXT,
        at INTEGER NOT NULL,
        PRIMARY KEY (bot, fingerprint)
    ) WITHOUT ROWID;
    CREATE TABLE memory_dream_lines (
        id INTEGER PRIMARY KEY,
        dream INTEGER NOT NULL,
        change TEXT NOT NULL,
        text TEXT NOT NULL
    );
    CREATE INDEX memory_dream_lines_dream ON memory_dream_lines (dream);",
];

/// Brings the crate's tables up to date. Safe to call on every start.
pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("CREATE TABLE IF NOT EXISTS memory_meta (key TEXT PRIMARY KEY, value INTEGER NOT NULL)", [])?;
    let done: i64 = conn
        .query_row("SELECT value FROM memory_meta WHERE key = 'schema'", [], |row| row.get(0))
        .optional()?
        .unwrap_or(0);
    for (version, sql) in (1_i64..).zip(MIGRATIONS).filter(|(version, _)| *version > done) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO memory_meta (key, value) VALUES ('schema', ?1)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [version],
        )?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_once() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        let version: i64 =
            conn.query_row("SELECT value FROM memory_meta WHERE key = 'schema'", [], |row| row.get(0)).unwrap();
        assert_eq!(version, i64::try_from(MIGRATIONS.len()).unwrap());
    }
}
