#![allow(dead_code)]
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
