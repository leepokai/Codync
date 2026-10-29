//! SQLite persistence. Every mutation stamps a global, monotonically increasing
//! `rev`; clients sync with `since: rev` and never need a separate event log.
//!
//! Calls are synchronous. They're single-row or indexed queries measured in
//! microseconds, so async callers use them directly; the one bulk read (a fresh
//! client's catch-up) is capped per bot, and the history search runs through `spawn_blocking`.

mod bots;
mod devices;
mod entries;
mod memory;
mod model;
mod schema;

pub use devices::{Device, DeviceSource, Scope};
pub use model::{
    AVATAR_COLORS, AVATAR_SHAPES, BotConfig, BotKind, BotRow, Entry, EntryKind, Fallback, Lane, Permission,
    message_text, run_session_key,
};

use crate::LockExt;
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};
use std::sync::Mutex;

pub struct Store {
    db: Mutex<Connection>,
    pub(crate) connector_lock: Mutex<()>,
    pub(crate) secret_key: Mutex<Option<zeroize::Zeroizing<Vec<u8>>>>,
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

impl Store {
    /// Durable control state must distinguish a missing key from a failed read.
    pub fn kv_read(&self, k: &str) -> Result<Option<String>> {
        Ok(self.db.locked().query_row("SELECT v FROM kv WHERE k = ?", [k], |r| r.get(0)).optional()?)
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

    pub fn kv_delete(&self, k: &str) -> Result<()> {
        self.db.locked().execute("DELETE FROM kv WHERE k = ?1", [k])?;
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

    /// Saves a row stamped with the next rev, in one transaction: `value` gets the rev and returns what to save.
    pub fn kv_set_stamped(&self, k: &str, value: impl FnOnce(i64) -> Result<String>) -> Result<()> {
        let mut c = self.db.locked();
        let tx = c.transaction()?;
        let rev = next_rev(&tx)?;
        tx.execute("INSERT INTO kv(k, v) VALUES(?1, ?2) ON CONFLICT(k) DO UPDATE SET v = ?2", params![k, value(rev)?])?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn temp_store() -> Store {
        let dir = std::env::temp_dir().join(format!("codync-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Store::open(&dir.join("t.db")).unwrap()
    }
}
