//! The `bots` table: roster rows, sessions, read marks and unread counts.

use super::model::{BotConfig, BotRow, EntryKind, LastMessage, ReadScope};
use super::{Store, logged, next_rev};
use crate::LockExt;
use anyhow::Result;
use rusqlite::{OptionalExtension, params};
use serde_json::Value;

/// Raw `bots` row before the config JSON is parsed.
type RawBot = (String, String, i64, bool, Option<String>, i64);

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

const BOT_COLS: &str = "id, config, rev, deleted, session_id, read_rev";

impl Store {
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
             ON CONFLICT(id) DO UPDATE SET rev = ?2, config = ?3 WHERE deleted = 0",
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
        c.execute("DELETE FROM kv WHERE k IN (SELECT 'read.' || id FROM entries WHERE bot_id = ?1)", [id])?;
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

    /// Main chat read: bumps the bot's `read_rev`. A thread never opened counts from `read_rev`
    /// (see `unread`), so it keeps the old one: reading the chat doesn't read its threads.
    pub fn mark_read(&self, id: &str) -> Result<i64> {
        let c = self.db.locked();
        let rev = next_rev(&c)?;
        c.execute(
            "INSERT OR IGNORE INTO kv(k, v)
             SELECT DISTINCT 'read.' || thread_id, (SELECT read_rev FROM bots WHERE id = ?1)
             FROM entries WHERE bot_id = ?1 AND thread_id IS NOT NULL",
            [id],
        )?;
        c.execute("UPDATE bots SET read_rev = ?2, rev = ?2 WHERE id = ?1", params![id, rev])?;
        Ok(rev)
    }

    /// One thread read (kv `read.<root>`); the bot's row changes too, since its badge does.
    pub fn mark_thread_read(&self, id: &str, root: &str) -> Result<i64> {
        let c = self.db.locked();
        let rev = next_rev(&c)?;
        c.execute("UPDATE bots SET rev = ?2 WHERE id = ?1", params![id, rev])?;
        c.execute(
            "INSERT INTO kv(k, v) VALUES(?1, ?2) ON CONFLICT(k) DO UPDATE SET v = ?2",
            params![format!("read.{root}"), rev.to_string()],
        )?;
        Ok(rev)
    }

    /// The main chat and every thread read.
    pub fn mark_all_read(&self, id: &str) -> Result<i64> {
        let rev = self.mark_read(id)?;
        let c = self.db.locked();
        c.execute(
            "UPDATE kv SET v = ?2 WHERE k IN (SELECT 'read.' || id FROM entries WHERE bot_id = ?1 AND thread_id IS NULL)",
            params![id, rev.to_string()],
        )?;
        Ok(rev)
    }

    /// Final agent messages + pending permission cards newer than what the user has read in
    /// `scope`. A thread counts from when it was last read (never read: the main chat's
    /// `read_rev`). A thread root or a reacted-to message doesn't count again when it
    /// changes: the user replied or reacted, so they have seen it.
    pub fn unread(&self, id: &str, read_rev: i64, scope: ReadScope) -> i64 {
        let mut args: Vec<&dyn rusqlite::ToSql> = vec![&id, &read_rev];
        let filter = match &scope {
            ReadScope::All => "",
            ReadScope::Chat => "AND thread_id IS NULL",
            ReadScope::Thread(root) => {
                args.push(root);
                "AND thread_id = ?3"
            }
        };
        let c = self.db.locked();
        let n = c.query_row(
            &format!(
                "SELECT COUNT(*) FROM entries WHERE bot_id = ?1 {filter} AND
                   ((kind = 'agent' AND json_extract(data, '$.final') = 1) OR
                    (kind = 'permission' AND json_extract(data, '$.status') = 'pending')) AND
                   json_extract(data, '$.reactions') IS NULL AND
                   CASE WHEN thread_id IS NULL THEN rev > ?2 AND json_extract(data, '$.thread') IS NULL
                        ELSE rev > COALESCE((SELECT CAST(v AS INTEGER) FROM kv WHERE k = 'read.' || thread_id), ?2) END"
            ),
            args.as_slice(),
            |r| r.get(0),
        );
        logged("unread", n).unwrap_or(0)
    }

    /// Roots whose summary shows unread replies.
    pub fn unread_roots(&self, id: &str) -> Vec<String> {
        let c = self.db.locked();
        let roots = c
            .prepare("SELECT id FROM entries WHERE bot_id = ?1 AND json_extract(data, '$.thread.unread') > 0")
            .and_then(|mut st| st.query_map([id], |r| r.get(0))?.collect());
        logged("unread roots", roots).unwrap_or_default()
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
                let mut text = data["text"].as_str().unwrap_or_default().to_owned();
                // A files-only message previews as its file names.
                if text.is_empty() {
                    let names: Vec<&str> = data["attachments"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|a| a["name"].as_str())
                        .collect();
                    text = names.join(", ");
                }
                let author =
                    (kind != EntryKind::User.as_str()).then(|| data["author"].as_str().unwrap_or(id).to_owned());
                Ok(LastMessage { text, at, author })
            },
        );
        logged("last message", row.optional()).flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::model::{BotKind, EntryKind, Lane, Permission};
    use crate::store::tests::temp_store;

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
            auto_name: false,
            created_at: 0,
        };
        s.save_bot(&cfg).unwrap();
        let u = s.insert_entry(&Lane::main("b1"), EntryKind::User, 1, &serde_json::json!({"text": "hi"})).unwrap();
        let a = s
            .insert_entry(&Lane::main("b1"), EntryKind::Agent, 1, &serde_json::json!({"text": "yo", "final": false}))
            .unwrap();
        assert!(a.rev > u.rev);
        assert_eq!(s.unread("b1", 0, ReadScope::All), 0, "narration is not unread");
        let a2 = s.update_entry(&a.id, &serde_json::json!({"text": "yo!", "final": true})).unwrap().unwrap();
        assert_eq!(s.unread("b1", 0, ReadScope::All), 1);
        assert_eq!(s.entries_since(a2.rev - 1, 500).unwrap().len(), 1);
        assert_eq!(s.entries_since(0, 1).unwrap().len(), 1, "fresh sync is capped per bot");
        assert_eq!(s.last_message("b1").unwrap().text, "yo!");
        let r = s.mark_read("b1").unwrap();
        assert_eq!(s.unread("b1", r, ReadScope::All), 0);
        assert_eq!(s.history("b1", a.seq, 10).unwrap().len(), 1);
        assert_eq!(s.bot("b1").unwrap().unwrap().config.permission, Permission::Ask);
        assert!(s.bot("missing").unwrap().is_none());

        // Reading the chat leaves a thread nobody opened unread.
        let reply = serde_json::json!({"text": "done", "final": true});
        s.insert_entry(&Lane::in_thread("b1", &u.id), EntryKind::Agent, 2, &reply).unwrap();
        let r = s.mark_read("b1").unwrap();
        assert_eq!(s.unread("b1", r, ReadScope::Thread(&u.id)), 1);
        assert_eq!(s.unread("b1", r, ReadScope::All), 1);

        // A deleted bot stays deleted, and its winding-down actor leaves no lines behind.
        s.delete_bot("b1").unwrap();
        assert!(s.insert_entry(&Lane::main("b1"), EntryKind::Notice, 3, &reply).is_err());
        s.save_bot(&cfg).unwrap();
        assert!(s.bot("b1").unwrap().unwrap().deleted);
    }
}
