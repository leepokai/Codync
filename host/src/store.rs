//! SQLite persistence. Every mutation stamps a global, monotonically increasing
//! `rev`; clients sync with `since: rev` and never need a separate event log.

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::sync::Mutex;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BotConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_color")]
    pub avatar_color: String,
    /// Grok-Bot-style character shape: blob | pebble | squircle | tablet | wedge | hex | cloud | teardrop.
    #[serde(default = "default_shape")]
    pub avatar_shape: String,
    pub backend: String,
    #[serde(default)]
    pub command: Option<String>,
    pub cwd: String,
    /// `ask` = every permission request goes to the phone; `auto` = approve once automatically.
    #[serde(default = "default_permission")]
    pub permission: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub notify: Option<bool>,
    #[serde(default)]
    pub created_at: i64,
}

fn default_color() -> String {
    "blue".into()
}
fn default_shape() -> String {
    "blob".into()
}
fn default_permission() -> String {
    "ask".into()
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub seq: i64,
    pub bot_id: String,
    pub rev: i64,
    pub kind: String,
    pub turn: i64,
    pub data: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct BotRow {
    pub config: BotConfig,
    pub rev: i64,
    pub deleted: bool,
    pub session_id: Option<String>,
    pub read_rev: i64,
}

pub struct Store {
    db: Mutex<Connection>,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn next_rev(c: &Connection) -> Result<i64> {
    Ok(c.query_row(
        "INSERT INTO kv(k, v) VALUES('rev', '1') ON CONFLICT(k) DO UPDATE SET v = CAST(v AS INTEGER) + 1 RETURNING CAST(v AS INTEGER)",
        [],
        |r| r.get(0),
    )?)
}

fn row_entry(r: &rusqlite::Row) -> rusqlite::Result<Entry> {
    let data: String = r.get(6)?;
    Ok(Entry {
        seq: r.get(0)?,
        id: r.get(1)?,
        bot_id: r.get(2)?,
        rev: r.get(3)?,
        kind: r.get(4)?,
        turn: r.get(5)?,
        data: serde_json::from_str(&data).unwrap_or(Value::Null),
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
    })
}

const ENTRY_COLS: &str = "seq, id, bot_id, rev, kind, turn, data, created_at, updated_at";

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let c = Connection::open(path)?;
        c.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS kv(k TEXT PRIMARY KEY, v TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS bots(
                id TEXT PRIMARY KEY, rev INTEGER NOT NULL, deleted INTEGER NOT NULL DEFAULT 0,
                config TEXT NOT NULL, session_id TEXT, read_rev INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE IF NOT EXISTS entries(
                seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT UNIQUE NOT NULL, bot_id TEXT NOT NULL,
                rev INTEGER NOT NULL, kind TEXT NOT NULL, turn INTEGER NOT NULL, data TEXT NOT NULL,
                created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
             CREATE INDEX IF NOT EXISTS entries_rev ON entries(rev);
             CREATE INDEX IF NOT EXISTS entries_bot ON entries(bot_id, seq);
             CREATE TABLE IF NOT EXISTS devices(ticket TEXT PRIMARY KEY, name TEXT, created_at INTEGER NOT NULL);",
        )?;
        Ok(Self { db: Mutex::new(c) })
    }

    pub fn kv_get(&self, k: &str) -> Option<String> {
        let c = self.db.lock().unwrap();
        c.query_row("SELECT v FROM kv WHERE k = ?", [k], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
    }

    pub fn kv_set(&self, k: &str, v: &str) -> Result<()> {
        let c = self.db.lock().unwrap();
        c.execute(
            "INSERT INTO kv(k, v) VALUES(?1, ?2) ON CONFLICT(k) DO UPDATE SET v = ?2",
            params![k, v],
        )?;
        Ok(())
    }

    pub fn current_rev(&self) -> i64 {
        self.kv_get("rev").and_then(|v| v.parse().ok()).unwrap_or(0)
    }

    // MARK: bots

    pub fn bots(&self) -> Result<Vec<BotRow>> {
        let c = self.db.lock().unwrap();
        let mut st = c.prepare("SELECT config, rev, deleted, session_id, read_rev FROM bots")?;
        let rows = st
            .query_map([], |r| {
                let cfg: String = r.get(0)?;
                Ok((cfg, r.get(1)?, r.get::<_, i64>(2)? != 0, r.get(3)?, r.get(4)?))
            })?
            .filter_map(|r| r.ok())
            .filter_map(|(cfg, rev, deleted, session_id, read_rev)| {
                Some(BotRow {
                    config: serde_json::from_str(&cfg).ok()?,
                    rev,
                    deleted,
                    session_id,
                    read_rev,
                })
            })
            .collect();
        Ok(rows)
    }

    pub fn bot(&self, id: &str) -> Result<Option<BotRow>> {
        Ok(self.bots()?.into_iter().find(|b| b.config.id == id))
    }

    pub fn save_bot(&self, cfg: &BotConfig) -> Result<i64> {
        let c = self.db.lock().unwrap();
        let rev = next_rev(&c)?;
        c.execute(
            "INSERT INTO bots(id, rev, config) VALUES(?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET rev = ?2, config = ?3, deleted = 0",
            params![cfg.id, rev, serde_json::to_string(cfg)?],
        )?;
        Ok(rev)
    }

    pub fn touch_bot(&self, id: &str) -> Result<i64> {
        let c = self.db.lock().unwrap();
        let rev = next_rev(&c)?;
        c.execute("UPDATE bots SET rev = ?2 WHERE id = ?1", params![id, rev])?;
        Ok(rev)
    }

    pub fn delete_bot(&self, id: &str) -> Result<i64> {
        let c = self.db.lock().unwrap();
        let rev = next_rev(&c)?;
        c.execute(
            "UPDATE bots SET rev = ?2, deleted = 1, session_id = NULL WHERE id = ?1",
            params![id, rev],
        )?;
        c.execute("DELETE FROM entries WHERE bot_id = ?1", [id])?;
        Ok(rev)
    }

    pub fn set_session(&self, id: &str, session: Option<&str>) -> Result<()> {
        let c = self.db.lock().unwrap();
        c.execute("UPDATE bots SET session_id = ?2 WHERE id = ?1", params![id, session])?;
        Ok(())
    }

    pub fn mark_read(&self, id: &str) -> Result<i64> {
        let c = self.db.lock().unwrap();
        let rev = next_rev(&c)?;
        c.execute(
            "UPDATE bots SET read_rev = ?2, rev = ?2 WHERE id = ?1",
            params![id, rev],
        )?;
        Ok(rev)
    }

    /// Final agent messages + pending permission cards newer than what the user has read.
    pub fn unread(&self, id: &str, read_rev: i64) -> i64 {
        let c = self.db.lock().unwrap();
        c.query_row(
            "SELECT COUNT(*) FROM entries WHERE bot_id = ?1 AND rev > ?2 AND
               ((kind = 'agent' AND json_extract(data, '$.final') = 1) OR
                (kind = 'permission' AND json_extract(data, '$.status') = 'pending'))",
            params![id, read_rev],
            |r| r.get(0),
        )
        .unwrap_or(0)
    }

    /// Newest chat-visible line for the roster preview.
    pub fn last_message(&self, id: &str) -> Option<(String, i64)> {
        let c = self.db.lock().unwrap();
        c.query_row(
            "SELECT kind, data, updated_at FROM entries WHERE bot_id = ?1 AND
               (kind = 'user' OR (kind = 'agent' AND json_extract(data, '$.final') = 1))
             ORDER BY seq DESC LIMIT 1",
            [id],
            |r| {
                let kind: String = r.get(0)?;
                let data: String = r.get(1)?;
                let at: i64 = r.get(2)?;
                let text = serde_json::from_str::<Value>(&data)
                    .ok()
                    .and_then(|v| v["text"].as_str().map(str::to_owned))
                    .unwrap_or_default();
                let text = if kind == "user" { format!("You: {text}") } else { text };
                Ok((text, at))
            },
        )
        .optional()
        .ok()
        .flatten()
    }

    // MARK: entries

    pub fn insert_entry(&self, bot_id: &str, kind: &str, turn: i64, data: &Value) -> Result<Entry> {
        let c = self.db.lock().unwrap();
        let rev = next_rev(&c)?;
        let now = now_ms();
        let id = uuid::Uuid::new_v4().to_string();
        c.execute(
            "INSERT INTO entries(id, bot_id, rev, kind, turn, data, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![id, bot_id, rev, kind, turn, data.to_string(), now],
        )?;
        Ok(Entry {
            id,
            seq: c.last_insert_rowid(),
            bot_id: bot_id.into(),
            rev,
            kind: kind.into(),
            turn,
            data: data.clone(),
            created_at: now,
            updated_at: now,
        })
    }

    pub fn update_entry(&self, id: &str, data: &Value) -> Result<Option<Entry>> {
        let c = self.db.lock().unwrap();
        let rev = next_rev(&c)?;
        c.execute(
            "UPDATE entries SET rev = ?2, data = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, rev, data.to_string(), now_ms()],
        )?;
        Ok(c.query_row(
            &format!("SELECT {ENTRY_COLS} FROM entries WHERE id = ?1"),
            [id],
            row_entry,
        )
        .optional()?)
    }

    pub fn entry(&self, id: &str) -> Option<Entry> {
        let c = self.db.lock().unwrap();
        c.query_row(
            &format!("SELECT {ENTRY_COLS} FROM entries WHERE id = ?1"),
            [id],
            row_entry,
        )
        .optional()
        .ok()
        .flatten()
    }

    pub fn find_by_nonce(&self, bot_id: &str, nonce: &str) -> Option<Entry> {
        let c = self.db.lock().unwrap();
        c.query_row(
            &format!(
                "SELECT {ENTRY_COLS} FROM entries WHERE bot_id = ?1 AND kind = 'user' AND json_extract(data, '$.clientNonce') = ?2"
            ),
            params![bot_id, nonce],
            row_entry,
        )
        .optional()
        .ok()
        .flatten()
    }

    pub fn max_turn(&self, bot_id: &str) -> i64 {
        let c = self.db.lock().unwrap();
        c.query_row(
            "SELECT COALESCE(MAX(turn), 0) FROM entries WHERE bot_id = ?1",
            [bot_id],
            |r| r.get(0),
        )
        .unwrap_or(0)
    }

    /// Entries changed after `since`. A fresh client (`since == 0`) only gets the
    /// newest `cap` entries per bot and pages older ones in with `history`.
    pub fn entries_since(&self, since: i64, cap: i64) -> Result<Vec<Entry>> {
        let c = self.db.lock().unwrap();
        let sql = if since == 0 {
            format!(
                "SELECT {ENTRY_COLS} FROM (SELECT *, ROW_NUMBER() OVER (PARTITION BY bot_id ORDER BY seq DESC) AS rn FROM entries)
                 WHERE rn <= ?2 AND rev > ?1 ORDER BY seq"
            )
        } else {
            format!("SELECT {ENTRY_COLS} FROM entries WHERE rev > ?1 AND ?2 > 0 ORDER BY seq")
        };
        let mut st = c.prepare(&sql)?;
        let rows = st.query_map(params![since, cap], row_entry)?.filter_map(|r| r.ok()).collect();
        Ok(rows)
    }

    pub fn history(&self, bot_id: &str, before_seq: i64, limit: i64) -> Result<Vec<Entry>> {
        let c = self.db.lock().unwrap();
        let mut st = c.prepare(&format!(
            "SELECT {ENTRY_COLS} FROM entries WHERE bot_id = ?1 AND seq < ?2 ORDER BY seq DESC LIMIT ?3"
        ))?;
        let mut rows: Vec<Entry> = st
            .query_map(params![bot_id, before_seq, limit], row_entry)?
            .filter_map(|r| r.ok())
            .collect();
        rows.reverse();
        Ok(rows)
    }

    /// Permission cards left `pending` by a crash or restart can never be answered.
    pub fn expire_pending(&self) -> Result<Vec<Entry>> {
        let ids: Vec<String> = {
            let c = self.db.lock().unwrap();
            let mut st = c.prepare(
                "SELECT id FROM entries WHERE (kind = 'permission' AND json_extract(data, '$.status') = 'pending')
                  OR (kind = 'user' AND json_extract(data, '$.status') = 'queued')",
            )?;
            st.query_map([], |r| r.get(0))?.filter_map(|r| r.ok()).collect()
        };
        let mut out = vec![];
        for id in ids {
            if let Some(mut e) = self.entry(&id) {
                let status = if e.kind == "user" { "failed" } else { "expired" };
                e.data["status"] = status.into();
                if let Some(e) = self.update_entry(&id, &e.data)? {
                    out.push(e);
                }
            }
        }
        Ok(out)
    }

    // MARK: devices

    pub fn add_device(&self, ticket: &str, name: &str) -> Result<()> {
        let c = self.db.lock().unwrap();
        c.execute(
            "INSERT INTO devices(ticket, name, created_at) VALUES(?1, ?2, ?3)
             ON CONFLICT(ticket) DO UPDATE SET name = ?2",
            params![ticket, name, now_ms()],
        )?;
        Ok(())
    }

    pub fn remove_device(&self, ticket: &str) -> Result<()> {
        self.db.lock().unwrap().execute("DELETE FROM devices WHERE ticket = ?1", [ticket])?;
        Ok(())
    }

    pub fn devices(&self) -> Vec<String> {
        let c = self.db.lock().unwrap();
        let Ok(mut st) = c.prepare("SELECT ticket FROM devices") else { return vec![] };
        st.query_map([], |r| r.get(0))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rev_sync_and_unread() {
        let dir = std::env::temp_dir().join(format!("codync-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let s = Store::open(&dir.join("t.db")).unwrap();
        let cfg = BotConfig {
            id: "b1".into(),
            name: "Rev".into(),
            description: String::new(),
            avatar_color: "blue".into(),
            avatar_shape: "blob".into(),
            backend: "claude".into(),
            command: None,
            cwd: "/tmp".into(),
            permission: "ask".into(),
            model: None,
            pinned: false,
            hidden: false,
            notify: None,
            created_at: 0,
        };
        s.save_bot(&cfg).unwrap();
        let u = s.insert_entry("b1", "user", 1, &serde_json::json!({"text": "hi"})).unwrap();
        let a = s.insert_entry("b1", "agent", 1, &serde_json::json!({"text": "yo", "final": false})).unwrap();
        assert!(a.rev > u.rev);
        assert_eq!(s.unread("b1", 0), 0, "narration is not unread");
        let a2 = s.update_entry(&a.id, &serde_json::json!({"text": "yo!", "final": true})).unwrap().unwrap();
        assert_eq!(s.unread("b1", 0), 1);
        assert_eq!(s.entries_since(a2.rev - 1, 500).unwrap().len(), 1);
        assert_eq!(s.entries_since(0, 1).unwrap().len(), 1, "fresh sync is capped per bot");
        assert_eq!(s.last_message("b1").unwrap().0, "yo!");
        let r = s.mark_read("b1").unwrap();
        assert_eq!(s.unread("b1", r), 0);
        assert_eq!(s.history("b1", a.seq, 10).unwrap().len(), 1);
    }
}
