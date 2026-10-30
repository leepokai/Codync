//! Opening the database: tables, pragmas and schema migrations.

use super::Store;
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

/// Current schema version (kv `schema`).
const SCHEMA: i64 = 6;

/// 3: authorized devices (keys, not push tickets) with their push and Live Activity
/// tickets. The old ticket-only `devices` table is dropped; phones re-register on connect.
/// 4: threads (`entries.thread_id`); instruction snapshots are kept per session, so the
/// per-bot ones go.
/// 5: threads merge into their chats (quote replies took their place, see [`migrate_5`]).
/// 6: the dividers that announced a fresh agent session go; the chat no longer shows session resets.
fn migrate(c: &Connection) -> Result<()> {
    let version: i64 =
        c.query_row("SELECT CAST(v AS INTEGER) FROM kv WHERE k = 'schema'", [], |r| r.get(0)).optional()?.unwrap_or(0);
    if version >= SCHEMA {
        return Ok(());
    }
    if version < 3 {
        migrate_3(c)?;
    }
    if version < 4 {
        migrate_4(c)?;
    }
    if version < 5 {
        migrate_5(c)?;
    }
    migrate_6(c)
}

fn migrate_3(c: &Connection) -> Result<()> {
    c.execute_batch(
        "BEGIN;
         DROP TABLE IF EXISTS devices;
         CREATE TABLE devices(
            key TEXT PRIMARY KEY, name TEXT NOT NULL, platform TEXT NOT NULL,
            source TEXT NOT NULL, grant_id TEXT, scopes TEXT NOT NULL,
            lease_until INTEGER, created_at INTEGER NOT NULL, last_seen_at INTEGER);
         CREATE TABLE IF NOT EXISTS push_tickets(
            ticket TEXT PRIMARY KEY, device_key TEXT NOT NULL, push_key TEXT, ctx TEXT, name TEXT,
            created_at INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS activity_tickets(
            ticket TEXT PRIMARY KEY, bot_id TEXT NOT NULL, device_key TEXT NOT NULL, created_at INTEGER NOT NULL);
         INSERT INTO kv(k, v) VALUES('schema', '3') ON CONFLICT(k) DO UPDATE SET v = '3';
         COMMIT;",
    )?;
    Ok(())
}

fn migrate_4(c: &Connection) -> Result<()> {
    c.execute_batch(
        "BEGIN;
         ALTER TABLE entries ADD COLUMN thread_id TEXT;
         CREATE INDEX IF NOT EXISTS entries_thread ON entries(bot_id, thread_id, seq);
         DELETE FROM kv WHERE k GLOB 'context.*';
         INSERT INTO kv(k, v) VALUES('schema', '4') ON CONFLICT(k) DO UPDATE SET v = '4';
         COMMIT;",
    )?;
    Ok(())
}

/// Thread replies join their chat in time order. Each thread's first user reply quotes the message the thread was
/// on; a routine run's own reply stays in its trace, since its result is already in the chat, and the result shows
/// the run's trace. Thread sessions and read marks go (a routine run's session stays for its recovery). Rows whose
/// chat changes get new revs so synced clients pick them up, and keep their old one as `contentRev`. Thread trace
/// (thoughts, tool calls) keeps its rev: only the dropped column changed, and resending every run's trace would
/// flood each client's first catch-up.
fn migrate_5(c: &Connection) -> Result<()> {
    let tx = c.unchecked_transaction()?;
    let changed: Vec<String> = tx
        .prepare(
            "SELECT id FROM entries
             WHERE (thread_id IS NOT NULL AND (kind IN ('user', 'permission', 'notice')
                                               OR (kind = 'agent' AND json_extract(data, '$.final') = 1)))
               OR json_extract(data, '$.thread') IS NOT NULL OR id GLOB 'routine-result-*' ORDER BY seq",
        )?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    tx.execute_batch(
        // Without it each subquery below scans the whole table, for every row it is asked about.
        "CREATE INDEX migrate5_thread ON entries(thread_id, turn, kind);
         UPDATE entries SET data = json_set(data, '$.final', json('false'))
           WHERE thread_id GLOB 'routine-*' AND kind = 'agent' AND json_extract(data, '$.final') = 1
             AND NOT EXISTS (SELECT 1 FROM entries u WHERE u.thread_id = entries.thread_id AND u.turn = entries.turn
                               AND u.kind = 'user');
         UPDATE entries SET turn = COALESCE((SELECT MIN(t.turn) FROM entries t
                                             WHERE t.thread_id = 'routine-' || substr(entries.id, 16) AND t.turn > 0), turn)
           WHERE id GLOB 'routine-result-*';
         UPDATE entries SET data = json_set(data, '$.replyTo', json((SELECT json_object(
               'id', r.id,
               'author', CASE WHEN r.kind = 'user' THEN 'user' ELSE COALESCE(json_extract(r.data, '$.author'), r.bot_id) END,
               -- `replies::quote`'s text: the message's, or its files' names.
               'text', substr(COALESCE(NULLIF(json_extract(r.data, '$.text'), ''),
                                       (SELECT group_concat(json_extract(a.value, '$.name'), ', ')
                                          FROM json_each(r.data, '$.attachments') a), ''), 1, 500))
             FROM entries r WHERE r.id = entries.thread_id)))
           WHERE kind = 'user' AND EXISTS (SELECT 1 FROM entries r WHERE r.id = entries.thread_id)
             AND seq = (SELECT MIN(f.seq) FROM entries f WHERE f.thread_id = entries.thread_id AND f.kind = 'user');
         UPDATE entries SET data = json_remove(data, '$.thread') WHERE json_extract(data, '$.thread') IS NOT NULL;
         DROP INDEX migrate5_thread;
         DROP INDEX IF EXISTS entries_thread;
         ALTER TABLE entries DROP COLUMN thread_id;
         DELETE FROM kv WHERE k GLOB 'read.*' OR (k GLOB 'lane.*' AND NOT k GLOB 'lane.session.*/routine-*');
         INSERT INTO kv(k, v) VALUES('schema', '5') ON CONFLICT(k) DO UPDATE SET v = '5';",
    )?;
    for id in changed {
        let rev = super::next_rev(&tx)?;
        // A message the user read stays read (`contentRev`, as for a bot's reaction).
        tx.execute(
            "UPDATE entries SET rev = ?2, data = CASE WHEN json_extract(data, '$.contentRev') IS NULL
               THEN json_set(data, '$.contentRev', rev) ELSE data END WHERE id = ?1",
            rusqlite::params![id, rev],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn migrate_6(c: &Connection) -> Result<()> {
    c.execute_batch(
        "BEGIN;
         DELETE FROM entries WHERE kind = 'notice' AND json_extract(data, '$.style') = 'divider';
         INSERT INTO kv(k, v) VALUES('schema', '6') ON CONFLICT(k) DO UPDATE SET v = '6';
         COMMIT;",
    )?;
    Ok(())
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let c = Connection::open(path)?;
        c.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA secure_delete=ON;
             CREATE TABLE IF NOT EXISTS kv(k TEXT PRIMARY KEY, v TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS bots(
                id TEXT PRIMARY KEY, rev INTEGER NOT NULL, deleted INTEGER NOT NULL DEFAULT 0,
                config TEXT NOT NULL, session_id TEXT, read_rev INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE IF NOT EXISTS entries(
                seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT UNIQUE NOT NULL, bot_id TEXT NOT NULL,
                rev INTEGER NOT NULL, kind TEXT NOT NULL, turn INTEGER NOT NULL, data TEXT NOT NULL,
                created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
             CREATE INDEX IF NOT EXISTS entries_rev ON entries(rev);
             CREATE INDEX IF NOT EXISTS entries_bot ON entries(bot_id, seq);",
        )?;
        migrate(&c)?;
        codync_mem::schema::migrate(&c)?;
        let secret_key = if path == Path::new(":memory:") {
            let mut key = vec![0; 32];
            getrandom::fill(&mut key).map_err(|_| anyhow::anyhow!("random key unavailable"))?;
            Some(zeroize::Zeroizing::new(key))
        } else {
            None
        };
        Ok(Self {
            icon_lock: tokio::sync::Mutex::new(()),
            db: Mutex::new(c),
            connector_lock: Mutex::new(()),
            secret_key: Mutex::new(secret_key),
            vault_unlock: std::sync::Arc::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("codync-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("t.db")
    }

    #[test]
    fn migrations_replace_the_old_ticket_table() {
        let db = temp_db();
        {
            let c = Connection::open(&db).unwrap();
            c.execute_batch(
                "CREATE TABLE kv(k TEXT PRIMARY KEY, v TEXT NOT NULL);
                 CREATE TABLE devices(ticket TEXT PRIMARY KEY, name TEXT, created_at INTEGER NOT NULL);
                 INSERT INTO devices VALUES('old', 'iPhone', 1);",
            )
            .unwrap();
        }
        let s = Store::open(&db).unwrap();
        assert!(s.devices().unwrap().is_empty());
        assert_eq!(s.kv_get("schema").as_deref(), Some("6"));
        drop(s);
        let s = Store::open(&db).unwrap();
        assert!(s.devices().unwrap().is_empty(), "reopening keeps the new table");
    }

    #[test]
    fn session_reset_dividers_leave_the_chat() {
        let db = temp_db();
        {
            let c = Connection::open(&db).unwrap();
            Store::open(&db).unwrap();
            c.execute_batch(
                "UPDATE kv SET v = '5' WHERE k = 'schema';
                 INSERT INTO entries(id, bot_id, rev, kind, turn, data, created_at, updated_at) VALUES
                   ('reset', 'b', 1, 'notice', 0, '{\"text\":\"New session\",\"style\":\"divider\"}', 0, 0),
                   ('info', 'b', 2, 'notice', 0, '{\"text\":\"Routine · News\"}', 0, 0);",
            )
            .unwrap();
        }
        let s = Store::open(&db).unwrap();
        let chat: Vec<String> = s.history("b", i64::MAX, 10).unwrap().into_iter().map(|e| e.id).collect();
        assert_eq!(chat, ["info"]);
        assert_eq!(s.kv_get("schema").as_deref(), Some("6"));
    }

    #[test]
    fn threads_merge_into_their_chat() {
        let db = temp_db();
        {
            let c = Connection::open(&db).unwrap();
            Store::open(&db).unwrap();
            c.execute_batch(
                "UPDATE kv SET v = '4' WHERE k = 'schema';
                 ALTER TABLE entries ADD COLUMN thread_id TEXT;
                 INSERT INTO kv(k, v) VALUES('rev', '10'), ('read.root', '9'), ('lane.session.b@b/root', 's1'),
                   ('lane.session.b@b/routine-r1', 's2');
                 INSERT INTO entries(id, bot_id, thread_id, rev, kind, turn, data, created_at, updated_at) VALUES
                   ('root', 'b', NULL, 1, 'agent', 1, '{\"text\":\"Plan ready\",\"final\":true,\"author\":\"b\",\"thread\":{\"count\":2}}', 0, 0),
                   ('q', 'b', 'root', 2, 'user', 2, '{\"text\":\"why?\"}', 0, 0),
                   ('a', 'b', 'root', 3, 'agent', 2, '{\"text\":\"because\",\"final\":true}', 0, 0),
                   ('routine-r1', 'b', NULL, 4, 'notice', 0, '{\"text\":\"Routine · News\"}', 0, 0),
                   ('run', 'b', 'routine-r1', 5, 'agent', 3, '{\"text\":\"Today…\",\"final\":true}', 0, 0),
                   ('routine-result-r1', 'b', NULL, 6, 'agent', 0, '{\"text\":\"Today…\",\"final\":true}', 0, 0),
                   ('tool', 'b', 'routine-r1', 7, 'tool', 3, '{\"title\":\"Read\"}', 0, 0),
                   ('shot', 'b', NULL, 8, 'user', 4, '{\"text\":\"\",\"attachments\":[{\"name\":\"a.png\"}],\"thread\":{\"count\":1}}', 0, 0),
                   ('see', 'b', 'shot', 9, 'user', 5, '{\"text\":\"see?\"}', 0, 0);",
            )
            .unwrap();
        }
        let s = Store::open(&db).unwrap();
        let chat: Vec<String> = s.history("b", i64::MAX, 10).unwrap().into_iter().map(|e| e.id).collect();
        assert_eq!(
            chat,
            ["root", "q", "a", "routine-r1", "run", "routine-result-r1", "tool", "shot", "see"],
            "one chat, in time order"
        );
        let quote = s.entry("q").unwrap().data["replyTo"].clone();
        assert_eq!(quote, serde_json::json!({"id": "root", "author": "b", "text": "Plan ready"}));
        assert!(s.entry("root").unwrap().data.get("thread").is_none());
        assert_eq!(s.entry("run").unwrap().data["final"], false, "the run's reply was already copied into the chat");
        assert_eq!(s.entry("routine-result-r1").unwrap().turn, 3, "the result shows the run's trace");
        assert_eq!(s.entry("see").unwrap().data["replyTo"]["text"], "a.png", "a files-only message quotes its files");
        assert!(s.entry("q").unwrap().rev > 10, "synced clients get the changes");
        assert_eq!(s.entry("tool").unwrap().rev, 7, "thread trace isn't sent again");
        assert_eq!(s.unread("b", 10), 0, "a read chat stays read");
        assert_eq!(s.kv_get("read.root"), None);
        assert_eq!(s.kv_get("lane.session.b@b/root"), None);
        assert_eq!(s.kv_get("lane.session.b@b/routine-r1").as_deref(), Some("s2"), "a run's session stays");
    }
}
