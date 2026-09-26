//! SQLite persistence. Every mutation stamps a global, monotonically increasing
//! `rev`; clients sync with `since: rev` and never need a separate event log.
//!
//! Calls are synchronous. They're single-row or indexed queries measured in
//! microseconds, so async callers use them directly; the one bulk read (a fresh
//! client's catch-up) is capped per bot.

use crate::LockExt;
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::sync::Mutex;

/// How a bot handles the agent's permission requests (wire values `ask` / `auto`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    /// Every request becomes an approval card on the phone.
    #[default]
    Ask,
    /// Requests are approved once, automatically.
    Auto,
}

/// Transcript entry kinds (the `kind` column / wire field).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    User,
    Agent,
    Thought,
    Tool,
    Plan,
    Permission,
    Notice,
}

impl EntryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Agent => "agent",
            Self::Thought => "thought",
            Self::Tool => "tool",
            Self::Plan => "plan",
            Self::Permission => "permission",
            Self::Notice => "notice",
        }
    }
}

/// A roster row is an agent bot or a group chat of bots (wire values `agent` / `group`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BotKind {
    #[default]
    Agent,
    /// Several bots and the user in one conversation; it has no agent of its own (see `group`).
    Group,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BotConfig {
    pub id: String,
    #[serde(default)]
    pub kind: BotKind,
    /// A group's bots, in the order they were added.
    #[serde(default)]
    pub members: Vec<String>,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_color")]
    pub avatar_color: String,
    /// Grok-Bot-style character shape: blob | pebble | squircle | tablet | wedge | hex | cloud | teardrop.
    #[serde(default = "default_shape")]
    pub avatar_shape: String,
    /// Empty for a group.
    #[serde(default)]
    pub backend: String,
    #[serde(default)]
    pub command: Option<String>,
    /// Empty for a group.
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub permission: Permission,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub notify: Option<bool>,
    /// Connector ids (MCP servers) handed to the agent; see `market`.
    #[serde(default)]
    pub connectors: Vec<String>,
    /// Skill ids the bot is told about; see `market`.
    #[serde(default)]
    pub skills: Vec<String>,
    /// The bot gets the built-in `computer` MCP server (see `screen`).
    #[serde(default)]
    pub computer: bool,
    #[serde(default)]
    pub created_at: i64,
}

impl BotConfig {
    pub fn is_group(&self) -> bool {
        self.kind == BotKind::Group
    }
}

fn default_color() -> String {
    "blue".into()
}
fn default_shape() -> String {
    "blob".into()
}

/// kv key for per-bot state in one lane (`what`: `session`, `seen`).
pub fn lane_key(what: &str, bot_id: &str, lane: &Lane) -> String {
    format!("lane.{what}.{bot_id}@{}", lane.key())
}

/// Where an entry lives: a bot's or group's chat (`bot_id`), and optionally a thread in it,
/// named by its root entry. Every lane a bot talks in has its own ACP session.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Lane {
    pub chat: String,
    pub thread: Option<String>,
}

impl Lane {
    pub fn main(chat: &str) -> Self {
        Self { chat: chat.to_owned(), thread: None }
    }

    pub fn in_thread(chat: &str, root: &str) -> Self {
        Self { chat: chat.to_owned(), thread: Some(root.to_owned()) }
    }

    /// Stable text form for kv keys: `chat` or `chat/root`.
    pub fn key(&self) -> String {
        match &self.thread {
            Some(t) => format!("{}/{t}", self.chat),
            None => self.chat.clone(),
        }
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub seq: i64,
    pub bot_id: String,
    /// The thread's root entry; `None` in the main chat.
    pub thread_id: Option<String>,
    pub rev: i64,
    pub kind: String,
    pub turn: i64,
    pub data: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct LastMessage {
    pub text: String,
    pub at: i64,
    /// The bot that wrote it; `None` when the user did.
    pub author: Option<String>,
}

pub struct BotRow {
    pub config: BotConfig,
    pub rev: i64,
    pub deleted: bool,
    pub session_id: Option<String>,
    pub read_rev: i64,
}

/// Raw `bots` row before the config JSON is parsed.
type RawBot = (String, String, i64, bool, Option<String>, i64);

pub struct Store {
    db: Mutex<Connection>,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Logs a failed read and treats it as "nothing there" — for the read paths that feed
/// best-effort details (previews, unread badges), where one bad query mustn't fail a request.
fn logged<T>(what: &str, r: rusqlite::Result<T>) -> Option<T> {
    r.map_err(|error| tracing::warn!(%error, what, "database read failed")).ok()
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
        thread_id: r.get(9)?,
    })
}

fn row_bot(r: &rusqlite::Row) -> rusqlite::Result<RawBot> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)? != 0, r.get(4)?, r.get(5)?))
}

/// A stored config that no longer parses is skipped (and logged) rather than failing every listing.
fn parse_bot((id, cfg, rev, deleted, session_id, read_rev): RawBot) -> Option<BotRow> {
    match serde_json::from_str(&cfg) {
        Ok(config) => Some(BotRow { config, rev, deleted, session_id, read_rev }),
        Err(error) => {
            tracing::warn!(bot = %id, %error, "skipping unreadable bot config");
            None
        }
    }
}

/// Current schema version (kv `schema`).
const SCHEMA: i64 = 4;

/// 3: authorized devices (keys, not push tickets) with their push and Live Activity
/// tickets. The old ticket-only `devices` table is dropped; phones re-register on connect.
/// 4: threads (`entries.thread_id`); instruction snapshots are kept per session, so the
/// per-bot ones go.
fn migrate(c: &Connection) -> Result<()> {
    let version: i64 =
        c.query_row("SELECT CAST(v AS INTEGER) FROM kv WHERE k = 'schema'", [], |r| r.get(0)).optional()?.unwrap_or(0);
    if version >= SCHEMA {
        return Ok(());
    }
    if version < 3 {
        migrate_3(c)?;
    }
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

/// Where an authorized device came from (wire values `local` / `account`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DeviceSource {
    /// Paired with the QR code shown on this computer.
    Local,
    /// Approved here for a signed-in account; kept alive by a lease the cloud renews.
    Account,
}

/// What a device may do (wire values `control` / `screen`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    Control,
    Screen,
}

/// One row of the authorized-device table (spec §4.3).
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub key: String,
    pub name: String,
    pub platform: String,
    pub source: DeviceSource,
    #[serde(skip)]
    pub grant_id: Option<String>,
    pub scopes: Vec<Scope>,
    pub lease_until: Option<i64>,
    pub created_at: i64,
    pub last_seen_at: Option<i64>,
}

/// A push ticket with the device's push key (§6.7) and account context.
pub struct PushTicket {
    pub ticket: String,
    pub push_key: Option<String>,
    pub ctx: Option<String>,
}

const DEVICE_COLS: &str = "key, name, platform, source, grant_id, scopes, lease_until, created_at, last_seen_at";

fn row_device(r: &rusqlite::Row) -> rusqlite::Result<Device> {
    let source: String = r.get(3)?;
    let scopes: String = r.get(5)?;
    Ok(Device {
        key: r.get(0)?,
        name: r.get(1)?,
        platform: r.get(2)?,
        // Unknown values grant nothing: an unreadable source is an account device (leased),
        // unreadable scopes are none.
        source: serde_json::from_value(Value::String(source)).unwrap_or(DeviceSource::Account),
        grant_id: r.get(4)?,
        scopes: serde_json::from_str(&scopes).unwrap_or_default(),
        lease_until: r.get(6)?,
        created_at: r.get(7)?,
        last_seen_at: r.get(8)?,
    })
}

const ENTRY_COLS: &str = "seq, id, bot_id, rev, kind, turn, data, created_at, updated_at, thread_id";
const BOT_COLS: &str = "id, config, rev, deleted, session_id, read_rev";

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
             CREATE INDEX IF NOT EXISTS entries_bot ON entries(bot_id, seq);",
        )?;
        migrate(&c)?;
        Ok(Self { db: Mutex::new(c) })
    }

    pub fn kv_get(&self, k: &str) -> Option<String> {
        let c = self.db.locked();
        logged("kv", c.query_row("SELECT v FROM kv WHERE k = ?", [k], |r| r.get(0)).optional()).flatten()
    }

    pub fn kv_set(&self, k: &str, v: &str) -> Result<()> {
        let c = self.db.locked();
        c.execute("INSERT INTO kv(k, v) VALUES(?1, ?2) ON CONFLICT(k) DO UPDATE SET v = ?2", params![k, v])?;
        Ok(())
    }

    /// Keys starting with `prefix`.
    pub fn kv_prefix(&self, prefix: &str) -> Vec<String> {
        let c = self.db.locked();
        let rows = c
            .prepare("SELECT k FROM kv WHERE substr(k, 1, length(?1)) = ?1")
            .and_then(|mut st| st.query_map([prefix], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>());
        logged("kv prefix", rows).unwrap_or_default()
    }

    pub fn current_rev(&self) -> i64 {
        self.kv_get("rev").and_then(|v| v.parse().ok()).unwrap_or(0)
    }

    // MARK: bots

    pub fn bots(&self) -> Result<Vec<BotRow>> {
        let c = self.db.locked();
        let mut st = c.prepare(&format!("SELECT {BOT_COLS} FROM bots"))?;
        let rows = st.query_map([], row_bot)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().filter_map(parse_bot).collect())
    }

    pub fn bot(&self, id: &str) -> Result<Option<BotRow>> {
        let c = self.db.locked();
        let row = c.query_row(&format!("SELECT {BOT_COLS} FROM bots WHERE id = ?1"), [id], row_bot).optional()?;
        Ok(row.and_then(parse_bot))
    }

    pub fn save_bot(&self, cfg: &BotConfig) -> Result<i64> {
        let c = self.db.locked();
        let rev = next_rev(&c)?;
        c.execute(
            "INSERT INTO bots(id, rev, config) VALUES(?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET rev = ?2, config = ?3, deleted = 0",
            params![cfg.id, rev, serde_json::to_string(cfg)?],
        )?;
        Ok(rev)
    }

    pub fn touch_bot(&self, id: &str) -> Result<i64> {
        let c = self.db.locked();
        let rev = next_rev(&c)?;
        c.execute("UPDATE bots SET rev = ?2 WHERE id = ?1", params![id, rev])?;
        Ok(rev)
    }

    pub fn delete_bot(&self, id: &str) -> Result<i64> {
        let c = self.db.locked();
        let rev = next_rev(&c)?;
        c.execute("UPDATE bots SET rev = ?2, deleted = 1, session_id = NULL WHERE id = ?1", params![id, rev])?;
        c.execute("DELETE FROM entries WHERE bot_id = ?1", [id])?;
        // Lane state (see `lane_key`) it owned, or that bots kept in it.
        c.execute("DELETE FROM kv WHERE k GLOB ?1 OR k GLOB ?2", [format!("lane.*.{id}@*"), format!("lane.*@{id}*")])?;
        // ponytail: snapshots of replaced sessions stay until the bot goes; prune by session if kv grows.
        c.execute(
            "DELETE FROM kv WHERE k GLOB ?1 OR k GLOB ?2",
            [format!("context.{id}.*"), format!("context.epoch.{id}.*")],
        )?;
        Ok(rev)
    }

    pub fn set_session(&self, id: &str, session: Option<&str>) -> Result<()> {
        let c = self.db.locked();
        c.execute("UPDATE bots SET session_id = ?2 WHERE id = ?1", params![id, session])?;
        Ok(())
    }

    pub fn mark_read(&self, id: &str) -> Result<i64> {
        let c = self.db.locked();
        let rev = next_rev(&c)?;
        c.execute("UPDATE bots SET read_rev = ?2, rev = ?2 WHERE id = ?1", params![id, rev])?;
        Ok(rev)
    }

    /// Final agent messages + pending permission cards newer than what the user has read.
    pub fn unread(&self, id: &str, read_rev: i64) -> i64 {
        let c = self.db.locked();
        let n = c.query_row(
            "SELECT COUNT(*) FROM entries WHERE bot_id = ?1 AND rev > ?2 AND
               ((kind = 'agent' AND json_extract(data, '$.final') = 1) OR
                (kind = 'permission' AND json_extract(data, '$.status') = 'pending'))",
            params![id, read_rev],
            |r| r.get(0),
        );
        logged("unread", n).unwrap_or(0)
    }

    /// Newest chat-visible line in the main chat for the roster preview, with its author
    /// (`None` for the user; a group reply names the bot that wrote it).
    pub fn last_message(&self, id: &str) -> Option<LastMessage> {
        let c = self.db.locked();
        let row = c.query_row(
            "SELECT kind, data, updated_at FROM entries WHERE bot_id = ?1 AND thread_id IS NULL AND
               (kind = 'user' OR (kind = 'agent' AND json_extract(data, '$.final') = 1))
             ORDER BY seq DESC LIMIT 1",
            [id],
            |r| {
                let kind: String = r.get(0)?;
                let data: String = r.get(1)?;
                let at: i64 = r.get(2)?;
                let data = serde_json::from_str::<Value>(&data).unwrap_or_default();
                let text = data["text"].as_str().unwrap_or_default().to_owned();
                let author =
                    (kind != EntryKind::User.as_str()).then(|| data["author"].as_str().unwrap_or(id).to_owned());
                Ok(LastMessage { text, at, author })
            },
        );
        logged("last message", row.optional()).flatten()
    }

    // MARK: entries

    pub fn insert_entry(&self, lane: &Lane, kind: EntryKind, turn: i64, data: &Value) -> Result<Entry> {
        let c = self.db.locked();
        let rev = next_rev(&c)?;
        let now = now_ms();
        let id = uuid::Uuid::new_v4().to_string();
        c.execute(
            "INSERT INTO entries(id, bot_id, thread_id, rev, kind, turn, data, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![id, lane.chat, lane.thread, rev, kind.as_str(), turn, data.to_string(), now],
        )?;
        Ok(Entry {
            id,
            seq: c.last_insert_rowid(),
            bot_id: lane.chat.clone(),
            thread_id: lane.thread.clone(),
            rev,
            kind: kind.as_str().to_owned(),
            turn,
            data: data.clone(),
            created_at: now,
            updated_at: now,
        })
    }

    pub fn update_entry(&self, id: &str, data: &Value) -> Result<Option<Entry>> {
        let c = self.db.locked();
        let rev = next_rev(&c)?;
        c.execute(
            "UPDATE entries SET rev = ?2, data = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, rev, data.to_string(), now_ms()],
        )?;
        Ok(c.query_row(&format!("SELECT {ENTRY_COLS} FROM entries WHERE id = ?1"), [id], row_entry).optional()?)
    }

    pub fn entry(&self, id: &str) -> Option<Entry> {
        let c = self.db.locked();
        let row = c.query_row(&format!("SELECT {ENTRY_COLS} FROM entries WHERE id = ?1"), [id], row_entry);
        logged("entry", row.optional()).flatten()
    }

    pub fn find_by_nonce(&self, bot_id: &str, nonce: &str) -> Option<Entry> {
        let c = self.db.locked();
        let row = c.query_row(
            &format!(
                "SELECT {ENTRY_COLS} FROM entries WHERE bot_id = ?1 AND kind = 'user' AND json_extract(data, '$.clientNonce') = ?2"
            ),
            params![bot_id, nonce],
            row_entry,
        );
        logged("entry by nonce", row.optional()).flatten()
    }

    pub fn max_turn(&self, bot_id: &str) -> i64 {
        let c = self.db.locked();
        let n = c.query_row("SELECT COALESCE(MAX(turn), 0) FROM entries WHERE bot_id = ?1", [bot_id], |r| r.get(0));
        logged("max turn", n).unwrap_or(0)
    }

    /// Entries changed after `since`. A fresh client (`since == 0`) only gets the
    /// newest `cap` entries per bot and pages older ones in with `history`.
    pub fn entries_since(&self, since: i64, cap: i64) -> Result<Vec<Entry>> {
        let c = self.db.locked();
        let sql = if since == 0 {
            format!(
                "SELECT {ENTRY_COLS} FROM (SELECT *, ROW_NUMBER() OVER (PARTITION BY bot_id ORDER BY seq DESC) AS rn FROM entries)
                 WHERE rn <= ?2 AND rev > ?1 ORDER BY seq"
            )
        } else {
            format!("SELECT {ENTRY_COLS} FROM entries WHERE rev > ?1 AND ?2 > 0 ORDER BY seq")
        };
        let mut st = c.prepare(&sql)?;
        Ok(st.query_map(params![since, cap], row_entry)?.collect::<rusqlite::Result<_>>()?)
    }

    /// Older main-chat entries (threads load whole with [`Self::thread`]).
    pub fn history(&self, bot_id: &str, before_seq: i64, limit: i64) -> Result<Vec<Entry>> {
        let c = self.db.locked();
        let mut st = c.prepare(&format!(
            "SELECT {ENTRY_COLS} FROM entries WHERE bot_id = ?1 AND thread_id IS NULL AND seq < ?2
             ORDER BY seq DESC LIMIT ?3"
        ))?;
        let mut rows: Vec<Entry> =
            st.query_map(params![bot_id, before_seq, limit], row_entry)?.collect::<rusqlite::Result<_>>()?;
        rows.reverse();
        Ok(rows)
    }

    /// A thread's newest `limit` entries, oldest first.
    pub fn thread(&self, bot_id: &str, root: &str, limit: i64) -> Result<Vec<Entry>> {
        let c = self.db.locked();
        let mut st = c.prepare(&format!(
            "SELECT {ENTRY_COLS} FROM entries WHERE bot_id = ?1 AND thread_id = ?2 ORDER BY seq DESC LIMIT ?3"
        ))?;
        let mut rows: Vec<Entry> =
            st.query_map(params![bot_id, root, limit], row_entry)?.collect::<rusqlite::Result<_>>()?;
        rows.reverse();
        Ok(rows)
    }

    /// Chat-visible messages (the user's, final replies) in `lane` after `after_seq`, oldest first.
    pub fn messages_after(&self, lane: &Lane, after_seq: i64, limit: i64) -> Result<Vec<Entry>> {
        let c = self.db.locked();
        let mut st = c.prepare(&format!(
            "SELECT {ENTRY_COLS} FROM entries WHERE bot_id = ?1 AND thread_id IS ?2 AND seq > ?3 AND
               (kind = 'user' OR (kind = 'agent' AND json_extract(data, '$.final') = 1))
             ORDER BY seq DESC LIMIT ?4"
        ))?;
        let mut rows: Vec<Entry> = st
            .query_map(params![lane.chat, lane.thread, after_seq, limit], row_entry)?
            .collect::<rusqlite::Result<_>>()?;
        rows.reverse();
        Ok(rows)
    }

    /// The `seq` of `bot`'s last reply in `lane` (0 if it hasn't spoken there).
    pub fn last_spoke(&self, lane: &Lane, bot: &str) -> i64 {
        let c = self.db.locked();
        let n = c.query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM entries WHERE bot_id = ?1 AND thread_id IS ?2 AND kind = 'agent'
               AND json_extract(data, '$.final') = 1 AND json_extract(data, '$.author') = ?3",
            params![lane.chat, lane.thread, bot],
            |r| r.get(0),
        );
        logged("last spoke", n).unwrap_or(0)
    }

    /// A thread's reply count, newest reply time and the bots that replied (first reply first).
    pub fn thread_summary(&self, chat: &str, root: &str) -> Result<Value> {
        let c = self.db.locked();
        let mut st = c.prepare(
            "SELECT kind, json_extract(data, '$.author'), created_at FROM entries WHERE bot_id = ?1 AND thread_id = ?2
               AND (kind = 'user' OR (kind = 'agent' AND json_extract(data, '$.final') = 1)) ORDER BY seq",
        )?;
        let rows = st.query_map(params![chat, root], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, i64>(2)?))
        })?;
        let (mut count, mut last_at, mut authors) = (0, 0, Vec::<String>::new());
        for row in rows {
            let (kind, author, at) = row?;
            count += 1;
            last_at = at;
            let author = if kind == EntryKind::User.as_str() { "user".to_owned() } else { author.unwrap_or_default() };
            if !author.is_empty() && !authors.contains(&author) {
                authors.push(author);
            }
        }
        Ok(serde_json::json!({"count": count, "lastAt": last_at, "authors": authors}))
    }

    /// Permission cards left `pending` (and sends left `queued`) by a crash or restart
    /// can never be honored; returns how many were closed out.
    pub fn expire_pending(&self) -> Result<usize> {
        let ids: Vec<String> = {
            let c = self.db.locked();
            let mut st = c.prepare(
                "SELECT id FROM entries WHERE (kind = 'permission' AND json_extract(data, '$.status') = 'pending')
                  OR (kind = 'user' AND json_extract(data, '$.status') = 'queued')
                  OR (kind = 'notice' AND json_extract(data, '$.delegationId') IS NOT NULL
                      AND json_extract(data, '$.status') IN ('queued', 'sent'))",
            )?;
            st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
        };
        for id in &ids {
            if let Some(mut e) = self.entry(id) {
                let status = if e.kind == EntryKind::Notice.as_str() {
                    let heading = e.data["heading"].as_str().unwrap_or("Bot request");
                    e.data["text"] = format!(
                        "{heading}\nInterrupted by host restart. Partial work may have happened; check before retrying."
                    )
                    .into();
                    e.data["style"] = "error".into();
                    "failed"
                } else if e.kind == EntryKind::User.as_str() {
                    "failed"
                } else {
                    "expired"
                };
                e.data["status"] = status.into();
                self.update_entry(id, &e.data)?;
            }
        }
        Ok(ids.len())
    }

    // MARK: devices

    pub fn device(&self, key: &str) -> Option<Device> {
        let c = self.db.locked();
        let row = c.query_row(&format!("SELECT {DEVICE_COLS} FROM devices WHERE key = ?1"), [key], row_device);
        logged("device", row.optional()).flatten()
    }

    pub fn devices(&self) -> Result<Vec<Device>> {
        let c = self.db.locked();
        let mut st = c.prepare(&format!("SELECT {DEVICE_COLS} FROM devices ORDER BY created_at"))?;
        Ok(st.query_map([], row_device)?.collect::<rusqlite::Result<_>>()?)
    }

    /// Adds (or, for a key paired again, refreshes) an authorized device.
    pub fn put_device(&self, d: &Device) -> Result<()> {
        let c = self.db.locked();
        c.execute(
            "INSERT INTO devices(key, name, platform, source, grant_id, scopes, lease_until, created_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(key) DO UPDATE SET name = ?2, platform = ?3, source = ?4, grant_id = ?5,
               scopes = ?6, lease_until = ?7",
            params![
                d.key,
                d.name,
                d.platform,
                serde_json::to_value(d.source)?.as_str(),
                d.grant_id,
                serde_json::to_string(&d.scopes)?,
                d.lease_until,
                d.created_at
            ],
        )?;
        Ok(())
    }

    /// Removes a device with its push and Live Activity tickets; false when it wasn't there.
    pub fn remove_device(&self, key: &str) -> Result<bool> {
        let mut c = self.db.locked();
        let tx = c.transaction()?;
        let n = tx.execute("DELETE FROM devices WHERE key = ?1", [key])?;
        tx.execute("DELETE FROM push_tickets WHERE device_key = ?1", [key])?;
        tx.execute("DELETE FROM activity_tickets WHERE device_key = ?1", [key])?;
        tx.commit()?;
        Ok(n > 0)
    }

    /// Extends an account device's lease (the cloud still lists its grant).
    pub fn set_lease(&self, key: &str, until: i64) -> Result<()> {
        let c = self.db.locked();
        c.execute("UPDATE devices SET lease_until = ?2 WHERE key = ?1", params![key, until])?;
        Ok(())
    }

    pub fn touch_device(&self, key: &str) -> Result<()> {
        let c = self.db.locked();
        c.execute("UPDATE devices SET last_seen_at = ?2 WHERE key = ?1", params![key, now_ms()])?;
        Ok(())
    }

    pub fn add_push_ticket(
        &self,
        ticket: &str,
        device_key: &str,
        push_key: Option<&str>,
        ctx: Option<&str>,
        name: &str,
    ) -> Result<()> {
        let c = self.db.locked();
        c.execute(
            "INSERT INTO push_tickets(ticket, device_key, push_key, ctx, name, created_at) VALUES(?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(ticket) DO UPDATE SET device_key = ?2, push_key = ?3, ctx = ?4, name = ?5",
            params![ticket, device_key, push_key, ctx, name, now_ms()],
        )?;
        Ok(())
    }

    pub fn remove_push_ticket(&self, ticket: &str) -> Result<()> {
        self.db.locked().execute("DELETE FROM push_tickets WHERE ticket = ?1", [ticket])?;
        Ok(())
    }

    pub fn push_tickets(&self) -> Vec<PushTicket> {
        let c = self.db.locked();
        let rows = c.prepare("SELECT ticket, push_key, ctx FROM push_tickets").and_then(|mut st| {
            st.query_map([], |r| Ok(PushTicket { ticket: r.get(0)?, push_key: r.get(1)?, ctx: r.get(2)? }))?
                .collect::<rusqlite::Result<Vec<_>>>()
        });
        logged("push tickets", rows).unwrap_or_default()
    }

    pub fn add_activity_ticket(&self, ticket: &str, bot_id: &str, device_key: &str) -> Result<()> {
        let c = self.db.locked();
        c.execute(
            "INSERT INTO activity_tickets(ticket, bot_id, device_key, created_at) VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(ticket) DO UPDATE SET bot_id = ?2, device_key = ?3",
            params![ticket, bot_id, device_key, now_ms()],
        )?;
        Ok(())
    }

    pub fn activity_tickets(&self, bot_id: &str) -> Vec<String> {
        let c = self.db.locked();
        let rows = c
            .prepare("SELECT ticket FROM activity_tickets WHERE bot_id = ?1")
            .and_then(|mut st| st.query_map([bot_id], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>());
        logged("activity tickets", rows).unwrap_or_default()
    }

    /// Forgets a bot's Live Activity tickets (its activity ended); returns them.
    pub fn take_activity_tickets(&self, bot_id: &str) -> Vec<String> {
        let tickets = self.activity_tickets(bot_id);
        if let Err(error) = self.db.locked().execute("DELETE FROM activity_tickets WHERE bot_id = ?1", [bot_id]) {
            tracing::warn!(%error, bot = bot_id, "couldn't forget Live Activity tickets");
        }
        tickets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> Store {
        let dir = std::env::temp_dir().join(format!("codync-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Store::open(&dir.join("t.db")).unwrap()
    }

    #[test]
    fn rev_sync_and_unread() {
        let s = temp_store();
        let cfg = BotConfig {
            id: "b1".into(),
            kind: BotKind::Agent,
            members: vec![],
            name: "Rev".into(),
            description: String::new(),
            avatar_color: "blue".into(),
            avatar_shape: "blob".into(),
            backend: "claude".into(),
            command: None,
            cwd: "/tmp".into(),
            permission: Permission::Ask,
            model: None,
            pinned: false,
            hidden: false,
            notify: None,
            connectors: vec![],
            skills: vec![],
            computer: false,
            created_at: 0,
        };
        s.save_bot(&cfg).unwrap();
        let u = s.insert_entry(&Lane::main("b1"), EntryKind::User, 1, &serde_json::json!({"text": "hi"})).unwrap();
        let a = s
            .insert_entry(&Lane::main("b1"), EntryKind::Agent, 1, &serde_json::json!({"text": "yo", "final": false}))
            .unwrap();
        assert!(a.rev > u.rev);
        assert_eq!(s.unread("b1", 0), 0, "narration is not unread");
        let a2 = s.update_entry(&a.id, &serde_json::json!({"text": "yo!", "final": true})).unwrap().unwrap();
        assert_eq!(s.unread("b1", 0), 1);
        assert_eq!(s.entries_since(a2.rev - 1, 500).unwrap().len(), 1);
        assert_eq!(s.entries_since(0, 1).unwrap().len(), 1, "fresh sync is capped per bot");
        assert_eq!(s.last_message("b1").unwrap().text, "yo!");
        let r = s.mark_read("b1").unwrap();
        assert_eq!(s.unread("b1", r), 0);
        assert_eq!(s.history("b1", a.seq, 10).unwrap().len(), 1);
        assert_eq!(s.bot("b1").unwrap().unwrap().config.permission, Permission::Ask);
        assert!(s.bot("missing").unwrap().is_none());
    }

    #[test]
    fn restart_expires_pending_cards_and_queued_sends() {
        let s = temp_store();
        s.insert_entry(&Lane::main("b1"), EntryKind::Permission, 1, &serde_json::json!({"status": "pending"})).unwrap();
        let q =
            s.insert_entry(&Lane::main("b1"), EntryKind::User, 2, &serde_json::json!({"status": "queued"})).unwrap();
        s.insert_entry(&Lane::main("b1"), EntryKind::User, 1, &serde_json::json!({"status": "sent"})).unwrap();
        assert_eq!(s.expire_pending().unwrap(), 2);
        assert_eq!(s.entry(&q.id).unwrap().data["status"], "failed");
        assert_eq!(s.expire_pending().unwrap(), 0, "idempotent");
    }

    #[test]
    fn removing_a_device_drops_its_tickets() {
        let s = temp_store();
        let d = Device {
            key: "dk1".into(),
            name: "Phone".into(),
            platform: "ios".into(),
            source: DeviceSource::Local,
            grant_id: None,
            scopes: vec![Scope::Control, Scope::Screen],
            lease_until: None,
            created_at: 1,
            last_seen_at: None,
        };
        s.put_device(&d).unwrap();
        s.put_device(&Device { key: "dk2".into(), ..d.clone() }).unwrap();
        s.add_push_ticket("t1", "dk1", Some("pk"), Some("local"), "Phone").unwrap();
        s.add_push_ticket("t2", "dk2", None, None, "Phone").unwrap();
        s.add_activity_ticket("a1", "b1", "dk1").unwrap();
        s.add_activity_ticket("a2", "b1", "dk2").unwrap();
        let got = s.device("dk1").unwrap();
        assert_eq!((got.source, got.scopes), (DeviceSource::Local, vec![Scope::Control, Scope::Screen]));

        assert!(s.remove_device("dk1").unwrap());
        assert!(!s.remove_device("dk1").unwrap());
        assert!(s.device("dk1").is_none());
        assert_eq!(s.push_tickets().iter().map(|t| t.ticket.as_str()).collect::<Vec<_>>(), ["t2"]);
        assert_eq!(s.activity_tickets("b1"), ["a2"]);
        assert_eq!(s.take_activity_tickets("b1"), ["a2"]);
        assert!(s.activity_tickets("b1").is_empty());
    }

    #[test]
    fn migrations_replace_the_old_ticket_table_and_add_threads() {
        let dir = std::env::temp_dir().join(format!("codync-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("t.db");
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
        assert_eq!(s.kv_get("schema").as_deref(), Some("4"));
        assert!(s.thread("b", "root", 10).unwrap().is_empty(), "entries have a thread column");
        drop(s);
        let s = Store::open(&db).unwrap();
        assert!(s.devices().unwrap().is_empty(), "reopening keeps the new table");
    }

    #[test]
    fn permission_wire_values_round_trip() {
        let p: Permission = serde_json::from_str("\"auto\"").unwrap();
        assert_eq!(p, Permission::Auto);
        assert_eq!(serde_json::to_string(&Permission::Ask).unwrap(), "\"ask\"");
        assert!(serde_json::from_str::<Permission>("\"yolo\"").is_err(), "unknown modes are rejected at the boundary");
    }
}
