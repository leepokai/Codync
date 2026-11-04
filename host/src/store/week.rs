//! A look back at a stretch of days (the weekly summary): what the person and their bots did, counted from the
//! transcripts on this computer. Nothing here leaves the host and no message text is read.
//!
//! Idea from Coucou's Monday recap; no code copied.

use super::Store;
use anyhow::Result;
use rusqlite::params;
use serde::Serialize;

/// Bots named in the summary's ranking.
const TOP: i64 = 3;

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Week {
    /// Messages the person sent.
    pub messages: i64,
    /// Final replies the bots gave.
    pub replies: i64,
    /// Tool calls the bots made.
    pub tools: i64,
    /// Tool calls that edited files.
    pub file_edits: i64,
    /// Approval cards shown, answered and still waiting.
    pub approvals_asked: i64,
    pub approvals_answered: i64,
    pub approvals_waiting: i64,
    /// Bots with any activity.
    pub active_bots: i64,
    /// The busiest bots, most active first.
    pub top_bots: Vec<TopBot>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopBot {
    pub id: String,
    pub name: String,
    pub entries: i64,
}

impl Store {
    /// The activity of the chats since `since_ms` (Unix milliseconds), for bots that still exist.
    pub fn week(&self, since_ms: i64) -> Result<Week> {
        self.with_connection(|c| {
            let live = "FROM entries e JOIN bots b ON b.id = e.bot_id WHERE b.deleted = 0 AND e.created_at >= ?1";
            let count = |extra: &str| -> Result<i64> {
                Ok(c.query_row(&format!("SELECT COUNT(*) {live} {extra}"), [since_ms], |r| r.get(0))?)
            };
            let top_bots = c
                .prepare(&format!(
                    "SELECT e.bot_id, COALESCE(json_extract(b.config, '$.name'), ''), COUNT(*) AS n {live} \
                     GROUP BY e.bot_id ORDER BY n DESC LIMIT ?2"
                ))?
                .query_map(params![since_ms, TOP], |r| {
                    Ok(TopBot { id: r.get(0)?, name: r.get(1)?, entries: r.get(2)? })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(Week {
                messages: count("AND e.kind = 'user'")?,
                replies: count("AND e.kind = 'agent' AND json_extract(e.data, '$.final') = 1")?,
                tools: count("AND e.kind = 'tool'")?,
                file_edits: count("AND e.kind = 'tool' AND json_extract(e.data, '$.toolKind') = 'edit'")?,
                approvals_asked: count("AND e.kind = 'permission'")?,
                approvals_answered: count(
                    "AND e.kind = 'permission' AND json_extract(e.data, '$.status') = 'answered'",
                )?,
                approvals_waiting: count("AND e.kind = 'permission' AND json_extract(e.data, '$.status') = 'pending'")?,
                active_bots: c
                    .query_row(&format!("SELECT COUNT(DISTINCT e.bot_id) {live}"), [since_ms], |r| r.get(0))?,
                top_bots,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::{EntryKind, Lane};
    use super::super::tests::temp_store;
    use serde_json::json;

    #[test]
    fn it_counts_what_happened_since_a_time_and_reads_no_text() {
        let store = temp_store();
        let config = json!({"id": "b1", "name": "Reviewer", "backend": "claude"});
        store.save_bot(&serde_json::from_value(config).expect("bot")).expect("saved");
        let lane = Lane::main("b1");
        let add = |kind, data| store.insert_entry(&lane, kind, 1, &data).expect("entry");
        add(EntryKind::User, json!({"text": "secret"}));
        add(EntryKind::Agent, json!({"text": "done", "final": true}));
        add(EntryKind::Agent, json!({"text": "thinking out loud"}));
        add(EntryKind::Tool, json!({"toolKind": "edit"}));
        add(EntryKind::Tool, json!({"toolKind": "read"}));
        add(EntryKind::Permission, json!({"status": "answered"}));
        add(EntryKind::Permission, json!({"status": "pending"}));
        let week = store.week(0).expect("week");
        assert_eq!((week.messages, week.replies, week.tools, week.file_edits), (1, 1, 2, 1));
        assert_eq!((week.approvals_asked, week.approvals_answered, week.approvals_waiting), (2, 1, 1));
        assert_eq!(week.active_bots, 1);
        assert_eq!(week.top_bots.first().map(|b| b.name.as_str()), Some("Reviewer"));
        assert_eq!(store.week(i64::MAX).expect("later").messages, 0, "nothing after the end of time");
    }
}
