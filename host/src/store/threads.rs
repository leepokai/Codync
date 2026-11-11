//! Threads: a side conversation opened from a chat, so a reply can dig into one message without filling the chat
//! (`OpenBot` Mobile's thread). Each thread keeps its own title, its branch origin, an archive flag and its own read
//! mark (`thread_reads`); its lines still land in the chat. Document, Slack and routine threads live here too, without
//! a row: any `thread_id` on an entry is a thread.

use super::visibility::PUBLIC;
use super::{Store, next_rev, now_ms};
use crate::LockExt;
use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::{Value, json};

/// Longest thread title a person can set.
pub const MAX_TITLE: usize = 120;

pub(super) fn open(c: &Connection) -> Result<()> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS threads (
            id TEXT PRIMARY KEY,
            bot_id TEXT NOT NULL,
            title TEXT NOT NULL DEFAULT '',
            from_entry TEXT,
            archived INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_threads_bot ON threads(bot_id, archived);
        CREATE INDEX IF NOT EXISTS entries_thread ON entries(thread_id, seq);",
    )?;
    Ok(())
}

/// What a root line's thread chip shows: how many replies it has, how many are unread, when the last one landed
/// and who wrote the recent ones. None while the thread has no reply yet. Takes the connection so a caller
/// holding the store lock (`decorate_thread`) doesn't lock the mutex again.
fn summary(c: &Connection, root: &str) -> Result<Option<Value>> {
    let replies = format!("thread_id = ?1 AND {PUBLIC} AND kind IN ('user', 'agent')");
    let count: i64 = c.query_row(&format!("SELECT COUNT(*) FROM entries WHERE {replies}"), [root], |r| r.get(0))?;
    if count == 0 {
        return Ok(None);
    }
    let last_at: i64 =
        c.query_row(&format!("SELECT COALESCE(MAX(updated_at), 0) FROM entries WHERE {replies}"), [root], |r| {
            r.get(0)
        })?;
    let mut st = c.prepare(&format!(
        "SELECT json_extract(data, '$.author') FROM entries WHERE thread_id = ?1 AND {PUBLIC} AND kind = 'agent'
         ORDER BY seq DESC LIMIT 3"
    ))?;
    let authors: Vec<String> =
        st.query_map([root], |r| r.get::<_, Option<String>>(0))?.filter_map(Result::ok).flatten().collect();
    Ok(Some(json!({
        "count": count,
        "unread": super::bots::thread_unread(c, root),
        "lastAt": last_at,
        "authors": authors,
    })))
}

/// A thread as a list shows it: never the lines themselves, only its last one.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadInfo {
    pub id: String,
    pub bot_id: String,
    pub title: String,
    /// The message the thread was branched from.
    pub from_entry: Option<String>,
    /// That message's text, for "Branched from …".
    pub branch_text: Option<String>,
    pub archived: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub unread: i64,
    pub last_text: Option<String>,
    pub last_author: Option<String>,
}

impl Store {
    /// A chat's threads, the most recently written first. `archived` picks the drawer: archived ones or the live ones.
    pub fn threads(&self, bot_id: &str, archived: bool) -> Result<Vec<ThreadInfo>> {
        let c = self.db.locked();
        let mut st = c.prepare(
            "SELECT t.id FROM threads t WHERE t.bot_id = ?1 AND t.archived = ?2
             UNION
             SELECT e.thread_id FROM entries e WHERE e.bot_id = ?1 AND e.thread_id IS NOT NULL AND ?2 = 0
               AND NOT EXISTS (SELECT 1 FROM threads x WHERE x.id = e.thread_id)
             GROUP BY e.thread_id",
        )?;
        let ids: Vec<String> =
            st.query_map(params![bot_id, i64::from(archived)], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        drop(st);
        let mut threads: Vec<ThreadInfo> = ids.iter().map(|id| Self::thread_info(&c, id)).collect::<Result<_>>()?;
        threads.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| b.id.cmp(&a.id)));
        Ok(threads)
    }

    fn thread_info(c: &Connection, id: &str) -> Result<ThreadInfo> {
        let row: Option<(String, Option<String>, bool, i64, String)> = c
            .query_row("SELECT title, from_entry, archived, created_at, bot_id FROM threads WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .optional()?;
        let (title, from_entry, archived, created_at, bot_id) = row.unwrap_or_else(|| {
            let bot = c
                .query_row("SELECT bot_id FROM entries WHERE thread_id = ?1 ORDER BY seq LIMIT 1", [id], |r| {
                    r.get::<_, String>(0)
                })
                .ok()
                .unwrap_or_default();
            (String::new(), None, false, 0, bot)
        });
        let last: Option<(Option<String>, String, i64)> = c
            .query_row(
                &format!(
                    "SELECT json_extract(data, '$.author'), COALESCE(json_extract(data, '$.text'), ''), updated_at
                     FROM entries WHERE thread_id = ?1 AND {PUBLIC} AND kind IN ('user', 'agent')
                     ORDER BY seq DESC LIMIT 1"
                ),
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let created = c
            .query_row("SELECT created_at FROM entries WHERE thread_id = ?1 ORDER BY seq LIMIT 1", [id], |r| r.get(0))
            .optional()?;
        let branch_text = match &from_entry {
            Some(entry) => c
                .query_row("SELECT json_extract(data, '$.text') FROM entries WHERE id = ?1", [entry], |r| r.get(0))
                .optional()?,
            None => None,
        };
        Ok(ThreadInfo {
            id: id.to_owned(),
            bot_id,
            title,
            from_entry: from_entry.clone(),
            branch_text,
            archived,
            created_at: created_at.max(created.unwrap_or(0)),
            updated_at: last.as_ref().map_or_else(|| created_at.max(created.unwrap_or(0)), |l| l.2),
            unread: super::bots::thread_unread(c, id),
            last_text: last.as_ref().map(|l| l.1.clone()).filter(|t| !t.is_empty()),
            last_author: last.and_then(|l| l.0),
        })
    }

    /// Adds to a root entry's JSON what its thread chip reads: how many of its replies are unread, and the
    /// thread's summary (see [`summary`]). A line that is itself in a thread gets neither.
    pub fn decorate_thread(&self, entry: &mut Value) -> Result<()> {
        if entry["threadId"].is_string() {
            return Ok(());
        }
        let Some(id) = entry["id"].as_str().map(str::to_owned) else {
            return Ok(());
        };
        let c = self.db.locked();
        let unread = super::bots::thread_unread(&c, &id);
        if unread > 0 {
            entry["threadUnread"] = unread.into();
        }
        if let Some(summary) = summary(&c, &id)? {
            entry["data"]["thread"] = summary;
        }
        Ok(())
    }

    /// The thread summary of one root line, for a caller that wants it alone.
    pub fn thread_summary(&self, root: &str) -> Result<Option<Value>> {
        summary(&self.db.locked(), root)
    }

    /// One thread by id, or None when no line and no row live in it.
    pub fn thread_by_id(&self, id: &str) -> Result<Option<ThreadInfo>> {
        let c = self.db.locked();
        let known = c
            .query_row(
                "SELECT 1 FROM threads WHERE id = ?1 UNION ALL SELECT 1 FROM entries WHERE thread_id = ?1 LIMIT 1",
                [id],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .is_some();
        if !known {
            return Ok(None);
        }
        Self::thread_info(&c, id).map(Some)
    }

    /// Opens a thread on a chat, optionally branched from a message, and answers its root. A thread branched
    /// from a message is *that message's* thread (its replies hang off the message, so the chat counts them under
    /// it, as Slack does); opening it again answers the one it already has. A thread opened from the drawer has a
    /// root of its own.
    pub fn create_thread(&self, bot_id: &str, title: &str, from_entry: Option<&str>) -> Result<String> {
        let title = title.trim();
        if title.chars().count() > MAX_TITLE {
            bail!("A thread title is at most {MAX_TITLE} characters");
        }
        let id = from_entry.map_or_else(|| format!("thread:{}", uuid::Uuid::new_v4()), str::to_owned);
        let c = self.db.locked();
        c.execute(
            "INSERT OR IGNORE INTO threads(id, bot_id, title, from_entry, archived, created_at)
             VALUES(?1, ?2, ?3, ?4, 0, ?5)",
            params![id, bot_id, title, from_entry, now_ms()],
        )?;
        Ok(id)
    }

    pub fn rename_thread(&self, id: &str, title: &str) -> Result<()> {
        let title = title.trim();
        if title.chars().count() > MAX_TITLE {
            bail!("A thread title is at most {MAX_TITLE} characters");
        }
        let c = self.db.locked();
        if c.execute("UPDATE threads SET title = ?2 WHERE id = ?1", params![id, title])? == 0 {
            ensure_row(&c, id)?;
            c.execute("UPDATE threads SET title = ?2 WHERE id = ?1", params![id, title])?;
        }
        Ok(())
    }

    pub fn archive_thread(&self, id: &str, archived: bool) -> Result<()> {
        let c = self.db.locked();
        let archived = i64::from(archived);
        if c.execute("UPDATE threads SET archived = ?2 WHERE id = ?1", params![id, archived])? == 0 {
            ensure_row(&c, id)?;
            c.execute("UPDATE threads SET archived = ?2 WHERE id = ?1", params![id, archived])?;
        }
        Ok(())
    }

    /// Removes a thread with every line in it; its rev, or None when it was already gone.
    pub fn delete_thread(&self, id: &str) -> Result<Option<i64>> {
        let mut c = self.db.locked();
        let tx = c.transaction()?;
        let lines = tx.execute("DELETE FROM entries WHERE thread_id = ?1", [id])?;
        let row = tx.execute("DELETE FROM threads WHERE id = ?1", [id])?;
        tx.execute("DELETE FROM thread_reads WHERE thread_id = ?1", [id])?;
        tx.execute("DELETE FROM kv WHERE k LIKE ?1", params![format!("%/{id}")])?;
        if lines == 0 && row == 0 {
            return Ok(None);
        }
        let rev = next_rev(&tx)?;
        tx.commit()?;
        Ok(Some(rev))
    }
}

/// A thread no message has reached yet (an empty one just opened) still needs its row before a rename.
fn ensure_row(c: &Connection, id: &str) -> Result<()> {
    let bot: Option<String> =
        c.query_row("SELECT bot_id FROM entries WHERE thread_id = ?1 LIMIT 1", [id], |r| r.get(0)).optional()?;
    let Some(bot) = bot else { bail!("unknown thread") };
    c.execute("INSERT OR IGNORE INTO threads(id, bot_id, created_at) VALUES(?1, ?2, ?3)", params![id, bot, now_ms()])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::model::{EntryKind, Lane};
    use crate::store::tests::temp_store;
    use serde_json::json;

    #[test]
    fn a_branched_thread_lists_what_it_came_from() {
        let s = temp_store();
        let root = s.insert_entry(&Lane::main("b"), EntryKind::Agent, 1, &json!({"text": "Plan ready"})).unwrap();
        let thread = s.create_thread("b", "", Some(&root.id)).unwrap();
        assert_eq!(thread, root.id, "a message's thread is the message itself");
        assert_eq!(s.threads("b", false).unwrap().len(), 1);
        s.insert_entry(&Lane::in_thread("b", &thread), EntryKind::User, 2, &json!({"text": "why?"})).unwrap();
        let listed = s.threads("b", false).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, thread);
        assert_eq!(listed[0].branch_text.as_deref(), Some("Plan ready"));
        assert_eq!(listed[0].last_text.as_deref(), Some("why?"));
        assert_eq!(listed[0].unread, 0, "the user's own line is not unread");
        // The chat's root line carries the chip's summary, under the message it came from.
        let mut chat_line = serde_json::to_value(s.entry(&root.id).unwrap()).unwrap();
        s.decorate_thread(&mut chat_line).unwrap();
        assert_eq!(chat_line["data"]["thread"]["count"], 1);
        assert!(chat_line["data"]["thread"]["lastAt"].as_i64().unwrap() > 0);
        s.archive_thread(&thread, true).unwrap();
        assert_eq!(s.threads("b", false).unwrap().len(), 0);
        assert_eq!(s.threads("b", true).unwrap().len(), 1);
        assert!(s.delete_thread(&thread).unwrap().is_some());
        assert_eq!(s.threads("b", true).unwrap().len(), 0);
        assert!(s.delete_thread(&thread).unwrap().is_none());
        assert!(s.entry(&root.id).is_some(), "deleting a thread keeps the message it branched from");
    }

    #[test]
    fn opening_a_messages_thread_again_answers_the_same_one() {
        let s = temp_store();
        let root = s.insert_entry(&Lane::main("b"), EntryKind::User, 1, &json!({"text": "hello"})).unwrap();
        let first = s.create_thread("b", "", Some(&root.id)).unwrap();
        let again = s.create_thread("b", "", Some(&root.id)).unwrap();
        assert_eq!(first, again);
        let _ = s.insert_entry(&Lane::in_thread("b", &first), EntryKind::User, 2, &json!({"text": "more"})).unwrap();
        assert_eq!(s.threads("b", false).unwrap()[0].last_text.as_deref(), Some("more"));
        let own = s.create_thread("b", "", None).unwrap();
        assert_ne!(own, root.id);
        assert!(own.starts_with("thread:"));
    }

    #[test]
    fn a_document_thread_can_be_renamed_and_counts_its_unread_replies() {
        let s = temp_store();
        s.insert_entry(&Lane::main("b"), EntryKind::User, 1, &json!({"text": "look"})).unwrap();
        s.insert_entry(&Lane::in_thread("b", "doc:d1"), EntryKind::User, 2, &json!({"text": "about this"})).unwrap();
        s.insert_entry(&Lane::in_thread("b", "doc:d1"), EntryKind::Agent, 2, &json!({"text": "sure", "final": true}))
            .unwrap();
        let listed = s.threads("b", false).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "doc:d1");
        assert_eq!(listed[0].unread, 1, "only the agent's final reply counts");
        s.rename_thread("doc:d1", "Sidebar").unwrap();
        assert_eq!(s.threads("b", false).unwrap()[0].title, "Sidebar");
        s.mark_thread_read("doc:d1").unwrap();
        assert_eq!(s.threads("b", false).unwrap()[0].unread, 0);
    }

    #[test]
    fn a_title_longer_than_the_cap_is_refused() {
        let s = temp_store();
        let long = "x".repeat(MAX_TITLE + 1);
        assert!(s.create_thread("b", &long, None).is_err());
        let id = s.create_thread("b", "ok", None).unwrap();
        assert!(s.rename_thread(&id, &long).is_err());
    }
}
