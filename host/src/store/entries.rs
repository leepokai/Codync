//! The `entries` table: the transcript of every chat.

use super::model::{Entry, EntryKind, Lane};
use super::visibility::PUBLIC;
use super::{Store, logged, next_rev, now_ms};
use crate::LockExt;
use anyhow::{Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde_json::Value;
use std::collections::HashSet;

pub(super) fn row_entry(r: &rusqlite::Row) -> rusqlite::Result<Entry> {
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

pub(super) const ENTRY_COLS: &str = "seq, id, bot_id, rev, kind, turn, data, created_at, updated_at, thread_id";
/// The chat itself, without its side conversations (`Lane::thread`).
const MAIN: &str = "thread_id IS NULL";
/// A bot still winding down after its deletion can't leave lines behind (`?2` is the chat).
const LIVE_CHAT: &str = "WHERE NOT EXISTS (SELECT 1 FROM bots WHERE id = ?2 AND deleted = 1)";

impl Store {
    pub fn insert_entry(&self, lane: &Lane, kind: EntryKind, turn: i64, data: &Value) -> Result<Entry> {
        let c = self.db.locked();
        let rev = next_rev(&c)?;
        let now = now_ms();
        let id = uuid::Uuid::new_v4().to_string();
        let added = c.execute(
            &format!(
                "INSERT INTO entries(id, bot_id, rev, kind, turn, data, created_at, updated_at, thread_id)
                 SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8 {LIVE_CHAT}"
            ),
            params![id, lane.chat, rev, kind.as_str(), turn, data.to_string(), now, lane.thread],
        )?;
        ensure!(added == 1, "the chat was deleted");
        Ok(Entry {
            id,
            seq: c.last_insert_rowid(),
            bot_id: lane.chat.clone(),
            rev,
            kind: kind.as_str().to_owned(),
            turn,
            data: data.clone(),
            created_at: now,
            updated_at: now,
            thread_id: lane.thread.clone(),
        })
    }

    /// Deterministic IDs make routine transcript publication retryable after a crash.
    pub fn insert_entry_once(
        &self,
        id: &str,
        lane: &Lane,
        kind: EntryKind,
        turn: i64,
        data: &Value,
    ) -> Result<(Entry, bool)> {
        let mut c = self.db.locked();
        let tx = c.transaction()?;
        if let Some(entry) =
            tx.query_row(&format!("SELECT {ENTRY_COLS} FROM entries WHERE id = ?"), [id], row_entry).optional()?
        {
            return Ok((entry, false));
        }
        let rev = next_rev(&tx)?;
        let now = now_ms();
        let added = tx.execute(&format!("INSERT INTO entries(id, bot_id, rev, kind, turn, data, created_at, updated_at, thread_id) SELECT ?1, ?2, ?3, ?4, ?7, ?5, ?6, ?6, ?8 {LIVE_CHAT}"),
            params![id, lane.chat, rev, kind.as_str(), data.to_string(), now, turn, lane.thread])?;
        ensure!(added == 1, "the chat was deleted");
        let entry = tx.query_row(&format!("SELECT {ENTRY_COLS} FROM entries WHERE id = ?"), [id], row_entry)?;
        tx.commit()?;
        Ok((entry, true))
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

    /// Moves an entry back in time, for tests of what happens to old entries.
    #[cfg(test)]
    pub fn backdate_entry(&self, id: &str, created_at: i64) {
        let c = self.db.locked();
        c.execute("UPDATE entries SET created_at = ?2 WHERE id = ?1", params![id, created_at]).expect("backdate");
    }

    pub fn entry(&self, id: &str) -> Option<Entry> {
        logged("entry", self.entry_checked(id)).flatten()
    }

    /// Authorization and durable queues must distinguish missing rows from failed reads.
    pub fn entry_checked(&self, id: &str) -> rusqlite::Result<Option<Entry>> {
        let c = self.db.locked();
        let row = c.query_row(&format!("SELECT {ENTRY_COLS} FROM entries WHERE id = ?1"), [id], row_entry);
        row.optional()
    }

    pub fn find_by_nonce(&self, bot_id: &str, nonce: &str) -> Option<Entry> {
        logged("entry by nonce", self.find_by_nonce_checked(bot_id, nonce)).flatten()
    }

    pub fn find_by_nonce_checked(&self, bot_id: &str, nonce: &str) -> rusqlite::Result<Option<Entry>> {
        let c = self.db.locked();
        let row = c.query_row(
            &format!(
                "SELECT {ENTRY_COLS} FROM entries WHERE bot_id = ?1 AND kind = 'user' AND json_extract(data, '$.clientNonce') = ?2"
            ),
            params![bot_id, nonce],
            row_entry,
        );
        row.optional()
    }

    /// What a voice call carried in `bot_id`'s chat (`chat::calls`), oldest first: its messages and final replies.
    /// The newest `limit` of them.
    pub fn call_entries(&self, bot_id: &str, call: &str, limit: i64) -> Result<Vec<Entry>> {
        let c = self.db.locked();
        let mut st = c.prepare(&format!(
            "SELECT {ENTRY_COLS} FROM entries WHERE bot_id = ?1 AND {PUBLIC} AND json_extract(data, '$.call') = ?2 AND
               (kind = 'user' OR (kind = 'agent' AND json_extract(data, '$.final') = 1))
             ORDER BY seq DESC LIMIT ?3"
        ))?;
        let mut rows: Vec<Entry> =
            st.query_map(params![bot_id, call, limit], row_entry)?.collect::<rusqlite::Result<_>>()?;
        rows.reverse();
        Ok(rows)
    }

    /// The host-owned call receipt, which is outside the call's user/agent conversation.
    pub fn call_notice(&self, bot_id: &str, call: &str) -> Result<Option<Entry>> {
        let c = self.db.locked();
        Ok(c.query_row(
            &format!(
                "SELECT {ENTRY_COLS} FROM entries WHERE bot_id = ?1 AND kind = 'notice'
                AND json_extract(data, '$.call') = ?2 AND json_type(data, '$.callSeconds') = 'integer'
                ORDER BY seq DESC LIMIT 1"
            ),
            params![bot_id, call],
            row_entry,
        )
        .optional()?)
    }

    pub fn max_turn(&self, bot_id: &str) -> i64 {
        let c = self.db.locked();
        let n = c.query_row("SELECT COALESCE(MAX(turn), 0) FROM entries WHERE bot_id = ?1", [bot_id], |r| r.get(0));
        logged("max turn", n).unwrap_or(0)
    }

    /// The kind and title of every tool call of `turn` in `bot_id`'s conversations (any lane), in order.
    pub fn tools_in_turn(&self, bot_id: &str, turn: i64) -> Result<Vec<(String, String)>> {
        let c = self.db.locked();
        let mut st = c.prepare(
            "SELECT COALESCE(json_extract(data, '$.toolKind'), 'other'), COALESCE(json_extract(data, '$.title'), '')
             FROM entries WHERE bot_id = ?1 AND turn = ?2 AND kind = 'tool' ORDER BY seq",
        )?;
        Ok(st.query_map(params![bot_id, turn], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?)
    }

    /// Whether the agent used a tool in `turn` of `bot_id`'s chat that may have changed something:
    /// reading, searching, fetching and thinking don't count (ACP tool kinds).
    pub fn acted_in_turn(&self, bot_id: &str, turn: i64) -> Result<bool> {
        let c = self.db.locked();
        Ok(c.query_row(
            "SELECT EXISTS(SELECT 1 FROM entries WHERE bot_id = ?1 AND turn = ?2 AND kind = 'tool'
             AND COALESCE(json_extract(data, '$.toolKind'), 'other') NOT IN ('read', 'search', 'fetch', 'think'))",
            params![bot_id, turn],
            |r| r.get(0),
        )?)
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

    /// Older entries of a chat.
    pub fn history(&self, bot_id: &str, before_seq: i64, limit: i64) -> Result<Vec<Entry>> {
        let c = self.db.locked();
        let mut st = c.prepare(&format!(
            "SELECT {ENTRY_COLS} FROM entries WHERE bot_id = ?1 AND {PUBLIC} AND {MAIN} AND seq < ?2
             ORDER BY seq DESC LIMIT ?3"
        ))?;
        let mut rows: Vec<Entry> =
            st.query_map(params![bot_id, before_seq, limit], row_entry)?.collect::<rusqlite::Result<_>>()?;
        rows.reverse();
        Ok(rows)
    }

    /// The newest `limit` entries of side conversation `root` (a document's or a Slack thread's), oldest first.
    pub fn thread(&self, bot_id: &str, root: &str, limit: i64) -> Result<Vec<Entry>> {
        let c = self.db.locked();
        let mut st = c.prepare(&format!(
            "SELECT {ENTRY_COLS} FROM entries WHERE bot_id = ?1 AND {PUBLIC} AND thread_id = ?2 ORDER BY seq DESC LIMIT ?3"
        ))?;
        let mut rows: Vec<Entry> =
            st.query_map(params![bot_id, root, limit], row_entry)?.collect::<rusqlite::Result<_>>()?;
        rows.reverse();
        Ok(rows)
    }

    /// A chat's messages with files (shared by a bot, attached by the user), newest first: its Library.
    // ponytail: scans the chat's entries without an index; add one if a long chat's Library gets slow.
    pub fn file_messages(&self, bot_id: &str, limit: i64) -> Result<Vec<Entry>> {
        let c = self.db.locked();
        let mut st = c.prepare(&format!(
            "SELECT {ENTRY_COLS} FROM entries WHERE bot_id = ?1 AND {PUBLIC}
               AND (json_array_length(data, '$.files') > 0 OR json_array_length(data, '$.attachments') > 0)
             ORDER BY seq DESC LIMIT ?2"
        ))?;
        Ok(st.query_map(params![bot_id, limit], row_entry)?.collect::<rusqlite::Result<_>>()?)
    }

    /// Permission cards still waiting for this bot's user, newest first: its open approvals.
    pub fn pending_approvals(&self, bot_id: &str) -> Result<Vec<Entry>> {
        let c = self.db.locked();
        let mut st = c.prepare(&format!(
            "SELECT {ENTRY_COLS} FROM entries WHERE bot_id = ?1 AND kind = 'permission'
               AND json_extract(data, '$.status') = 'pending'
             ORDER BY rev DESC LIMIT 50"
        ))?;
        Ok(st.query_map([bot_id], row_entry)?.collect::<rusqlite::Result<_>>()?)
    }

    /// The side conversations this bot has written in, most recently used first: documents, routine threads
    /// and Slack threads, as their `thread_id`.
    pub fn thread_roots(&self, bot_id: &str) -> Result<Vec<String>> {
        let c = self.db.locked();
        let mut st = c.prepare(
            "SELECT thread_id FROM entries WHERE bot_id = ?1 AND thread_id IS NOT NULL AND thread_id <> ''
             GROUP BY thread_id ORDER BY MAX(rev) DESC LIMIT 50",
        )?;
        Ok(st.query_map([bot_id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
    }
    /// Cards still waiting for the user to sign in to agent `backend`, oldest first.
    // ponytail: scans notices without an index; sign-ins are rare, add one if it shows up in profiles.
    pub fn pending_sign_ins(&self, backend: &str) -> Result<Vec<Entry>> {
        let c = self.db.locked();
        let mut st = c.prepare(&format!(
            "SELECT {ENTRY_COLS} FROM entries WHERE kind = 'notice'
               AND json_extract(data, '$.connectionRequest.kind') = 'agent'
               AND json_extract(data, '$.connectionRequest.status') = 'pending'
               AND json_extract(data, '$.connectionRequest.backend') = ?1
             ORDER BY rev"
        ))?;
        Ok(st.query_map([backend], row_entry)?.collect::<rusqlite::Result<_>>()?)
    }

    /// The newest `limit` bot-to-bot notices in `bot_id`'s main chat that involve `peer_id`, oldest first.
    pub fn bot_conversation(&self, bot_id: &str, peer_id: &str, limit: i64) -> Result<Vec<Entry>> {
        let c = self.db.locked();
        let mut st = c.prepare(&format!(
            "SELECT {ENTRY_COLS} FROM entries WHERE bot_id = ?1 AND kind = 'notice'
               AND json_type(data, '$.botMessage') = 'object'
               AND ?2 IN (json_extract(data, '$.botMessage.sourceBotId'), json_extract(data, '$.botMessage.targetBotId'))
             ORDER BY seq DESC LIMIT ?3"
        ))?;
        let mut rows: Vec<Entry> =
            st.query_map(params![bot_id, peer_id, limit], row_entry)?.collect::<rusqlite::Result<_>>()?;
        rows.reverse();
        Ok(rows)
    }

    /// Messages in `bot_id`'s chat after `after_seq`, oldest first: the
    /// user's, final replies and voice calls' transcripts. Dreaming reads these.
    pub fn conversation_since(&self, bot_id: &str, after_seq: i64, limit: i64) -> Result<Vec<Entry>> {
        let c = self.db.locked();
        let mut st = c.prepare(&format!(
            "SELECT {ENTRY_COLS} FROM entries WHERE bot_id = ?1 AND {PUBLIC} AND seq > ?2 AND
               (kind = 'user' OR (kind = 'agent' AND json_extract(data, '$.final') = 1)
                OR (kind = 'notice' AND json_extract(data, '$.transcript') IS NOT NULL))
             ORDER BY seq LIMIT ?3"
        ))?;
        Ok(st.query_map(params![bot_id, after_seq, limit], row_entry)?.collect::<rusqlite::Result<_>>()?)
    }

    /// The newest entry of a bot's chat.
    pub fn last_seq(&self, bot_id: &str) -> i64 {
        let c = self.db.locked();
        logged(
            "last seq",
            c.query_row("SELECT COALESCE(MAX(seq), 0) FROM entries WHERE bot_id = ?1", [bot_id], |r| r.get(0)),
        )
        .unwrap_or(0)
    }

    /// Chat-visible messages (the user's, final replies) in `lane` after `after_seq`, oldest first.
    pub fn messages_after(&self, lane: &Lane, after_seq: i64, limit: i64) -> Result<Vec<Entry>> {
        let c = self.db.locked();
        let mut st = c.prepare(&format!(
            "SELECT {ENTRY_COLS} FROM entries WHERE bot_id = ?1 AND {PUBLIC} AND thread_id IS ?4 AND seq > ?2 AND
               (kind = 'user' OR (kind = 'agent' AND json_extract(data, '$.final') = 1))
             ORDER BY seq DESC LIMIT ?3"
        ))?;
        let mut rows: Vec<Entry> = st
            .query_map(params![lane.chat, after_seq, limit, lane.thread], row_entry)?
            .collect::<rusqlite::Result<_>>()?;
        rows.reverse();
        Ok(rows)
    }

    /// The newest `kind` entry in `lane` after `after_seq`.
    pub fn last_of_kind(&self, lane: &Lane, kind: EntryKind, after_seq: i64) -> Option<Entry> {
        let c = self.db.locked();
        let row = c.query_row(
            &format!(
                "SELECT {ENTRY_COLS} FROM entries WHERE bot_id = ?1 AND thread_id IS ?4 AND kind = ?2 AND seq > ?3
                 ORDER BY seq DESC LIMIT 1"
            ),
            params![lane.chat, kind.as_str(), after_seq, lane.thread],
            row_entry,
        );
        logged("last entry of kind", row.optional()).flatten()
    }

    /// The newest health notice in `lane` (the computer's memory banner), if any.
    pub fn last_health_notice(&self, lane: &Lane) -> Option<Entry> {
        let c = self.db.locked();
        let row = c.query_row(
            &format!(
                "SELECT {ENTRY_COLS} FROM entries WHERE bot_id = ?1 AND thread_id IS ?3 AND kind = 'notice'
                   AND json_extract(data, '$.health') = 1
                 ORDER BY seq DESC LIMIT 1"
            ),
            params![lane.chat, lane.thread],
            row_entry,
        );
        logged("last health notice", row.optional()).flatten()
    }

    /// The `seq` of `bot`'s last reply in `lane` (0 if it hasn't spoken there).
    pub fn last_spoke(&self, lane: &Lane, bot: &str) -> i64 {
        let c = self.db.locked();
        let n = c.query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM entries WHERE bot_id = ?1 AND thread_id IS ?3 AND kind = 'agent'
               AND json_extract(data, '$.final') = 1 AND COALESCE(json_extract(data, '$.internal'), 0) != 1 AND json_extract(data, '$.author') = ?2",
            params![lane.chat, bot, lane.thread],
            |r| r.get(0),
        );
        logged("last spoke", n).unwrap_or(0)
    }

    /// Permission cards left `pending` (and sends left `queued`) by a crash or restart
    /// can never be honored; returns how many were closed out.
    pub fn expire_pending(&self) -> Result<usize> {
        let ids: Vec<String> = {
            let c = self.db.locked();
            let mut st = c.prepare(
                "SELECT id FROM entries WHERE (kind = 'permission' AND json_extract(data, '$.status') = 'pending')
                  OR (kind = 'user' AND json_extract(data, '$.status') = 'queued'
                      AND NOT (COALESCE(json_extract(data,'$.source'),'')='slack'
                          AND EXISTS(SELECT 1 FROM slack_inbox s WHERE s.entry_id=entries.id AND s.state='queued')))
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
                    if let Some(message) = e.data.get_mut("botMessage").and_then(Value::as_object_mut) {
                        message.insert(
                            "detail".into(),
                            "Interrupted by host restart. Partial work may have happened; check before retrying."
                                .into(),
                        );
                    }
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

    /// Older memory banners are noise: a reading that hovered at a threshold
    /// used to write one every sample. Keep only the newest per chat, so the
    /// thread reads as one line (the newest is updated in place from now on).
    pub fn coalesce_health_notices(&self) -> Result<usize> {
        let rows: Vec<(String, String, Option<String>)> = {
            let c = self.db.locked();
            let mut st = c.prepare(
                "SELECT id, bot_id, thread_id FROM entries WHERE kind = 'notice'
                   AND json_extract(data, '$.text') LIKE 'This computer''s memory is %'
                 ORDER BY seq DESC",
            )?;
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?
        };
        let mut seen = HashSet::new();
        let mut deleted = 0;
        let c = self.db.locked();
        for (id, bot, thread) in rows {
            if seen.insert((bot, thread)) {
                continue;
            }
            deleted += c.execute("DELETE FROM entries WHERE id = ?1", [&id])?;
        }
        Ok(deleted)
    }

    /// Removes one message; its rev, or None when it was already gone.
    pub fn delete_entry(&self, id: &str) -> Result<Option<i64>> {
        let mut c = self.db.locked();
        let tx = c.transaction()?;
        if tx.execute("DELETE FROM entries WHERE id = ?1", [id])? == 0 {
            return Ok(None);
        }
        let rev = next_rev(&tx)?;
        tx.commit()?;
        Ok(Some(rev))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::temp_store;
    use serde_json::json;

    #[test]
    fn a_deleted_message_leaves_history_and_asks_for_a_rev_once() {
        let s = temp_store();
        let lane = Lane::main("b");
        let e = s.insert_entry(&lane, EntryKind::User, 1, &json!({"text":"bye"})).unwrap();
        let rev = s.delete_entry(&e.id).unwrap().expect("deleted");
        assert!(rev > e.rev);
        assert!(s.history("b", i64::MAX, 10).unwrap().is_empty());
        assert_eq!(s.delete_entry(&e.id).unwrap(), None, "already gone");
    }
    #[test]
    fn history_search_matches_every_term_in_chat_messages() {
        let s = temp_store();
        let main = Lane::main("b1");
        s.insert_entry(&main, EntryKind::User, 1, &json!({"text": "我們用 Swift 6 寫 Codync"})).unwrap();
        s.insert_entry(&main, EntryKind::Agent, 1, &json!({"text": "swift narration", "final": false})).unwrap();
        s.insert_entry(&main, EntryKind::Agent, 1, &json!({"text": "Swift 6 strict mode is on", "final": true}))
            .unwrap();
        s.insert_entry(&Lane::main("b2"), EntryKind::User, 1, &json!({"text": "Swift 6 elsewhere"})).unwrap();
        let texts = |terms: &[&str]| -> Vec<String> {
            let terms: Vec<String> = terms.iter().map(|t| (*t).to_owned()).collect();
            s.search_messages("b1", &terms, None, 10)
                .unwrap()
                .into_iter()
                .map(|e| e.data["text"].as_str().unwrap().to_owned())
                .collect()
        };
        assert_eq!(texts(&["SWIFT", "6"]), ["Swift 6 strict mode is on", "我們用 Swift 6 寫 Codync"]);
        assert_eq!(texts(&["codync", "寫"]), ["我們用 Swift 6 寫 Codync"]);
        assert_eq!(texts(&["narration"]).len(), 0);
    }

    #[test]
    fn history_search_reads_back_from_a_message() {
        let s = temp_store();
        let main = Lane::main("b1");
        let seqs: Vec<i64> = (1..=4)
            .map(|i| s.insert_entry(&main, EntryKind::User, 1, &json!({"text": format!("note {i}")})).unwrap().seq)
            .collect();
        let texts = |terms: &[String], from| -> Vec<String> {
            let rows = s.search_messages("b1", terms, from, 2).unwrap();
            rows.into_iter().map(|e| e.data["text"].as_str().unwrap().to_owned()).collect()
        };
        assert_eq!(texts(&[], Some(seqs[2])), ["note 3", "note 2"], "starts at the message itself");
        assert_eq!(texts(&["1".to_owned()], Some(seqs[2])), ["note 1"]);
        assert_eq!(texts(&[], None), ["note 4", "note 3"]);
    }

    #[test]
    fn old_memory_banners_keep_only_the_newest_per_chat() {
        let s = temp_store();
        let main = Lane::main("b1");
        for used in [82, 86, 84] {
            s.insert_entry(
                &main,
                EntryKind::Notice,
                1,
                &json!({"text": format!("This computer's memory is {used} % full")}),
            )
            .unwrap();
        }
        s.insert_entry(&main, EntryKind::Notice, 1, &json!({"text": "Something else"})).unwrap();
        s.insert_entry(
            &Lane::main("b2"),
            EntryKind::Notice,
            1,
            &json!({"text": "This computer's memory is 90 % full"}),
        )
        .unwrap();
        assert_eq!(s.coalesce_health_notices().unwrap(), 2, "two older banners in b1");
        let entries = s.entries_since(0, 100).unwrap();
        let banners = |bot: &str| {
            entries
                .iter()
                .filter(|e| {
                    e.bot_id == bot
                        && e.data["text"].as_str().is_some_and(|t| t.starts_with("This computer's memory is"))
                })
                .count()
        };
        assert_eq!(banners("b1"), 1);
        assert_eq!(banners("b2"), 1);
        assert!(entries.iter().any(|e| e.data["text"] == "Something else"), "other notices stay");
        assert_eq!(s.coalesce_health_notices().unwrap(), 0, "idempotent");
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
    fn bot_conversation_lists_one_peers_notices_oldest_first() {
        let s = temp_store();
        let notice = |bot: &str, source: &str, target: &str, text: &str| {
            let data = json!({"text": text, "sourceBotId": source, "targetBotId": target,
                "botMessage": {"sourceBotId": source, "targetBotId": target, "text": text}});
            s.insert_entry(&Lane::main(bot), EntryKind::Notice, 1, &data).unwrap();
        };
        notice("a", "a", "b", "first");
        notice("a", "a", "c", "other peer");
        notice("a", "b", "a", "second");
        notice("b", "a", "b", "other chat");
        s.insert_entry(&Lane::main("a"), EntryKind::Notice, 1, &json!({"text": "plain"})).unwrap();
        let old = s
            .insert_entry(
                &Lane::main("a"),
                EntryKind::Notice,
                1,
                &json!({"text": "Messaged b: old", "heading": "Messaged b: old", "delegationId": "old",
                "sourceBotId": "a", "targetBotId": "b", "status": "completed"}),
            )
            .unwrap();
        let old_data = old.data.clone();
        let texts: Vec<_> = s
            .bot_conversation("a", "b", 10)
            .unwrap()
            .into_iter()
            .map(|e| e.data["text"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(texts, ["first", "second"]);
        assert_eq!(s.entry(&old.id).unwrap().data, old_data, "existing notice is untouched");
        assert_eq!(s.bot_conversation("a", "b", 1).unwrap().len(), 1, "limit keeps the newest");
    }
}
