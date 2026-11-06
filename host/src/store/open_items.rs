//! Persistence for tracked open items (`open_items`, Idea 8) and routine deduplication
//! (`reported_items`, Idea 9).

use super::visibility::PUBLIC;
use super::{Store, now_ms};
use crate::LockExt;
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpenItem {
    pub id: String,
    pub bot_id: String,
    pub title: String,
    #[serde(default)]
    pub note: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_after: Option<i64>,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MessageBookmark {
    pub id: String,
    pub bot_id: String,
    pub entry_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    pub author: String,
    pub preview: String,
    #[serde(default)]
    pub note: String,
    pub created_at: i64,
    pub updated_at: i64,
}

pub(super) fn open(c: &Connection) -> Result<()> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS open_items (
            id TEXT PRIMARY KEY,
            bot_id TEXT NOT NULL,
            title TEXT NOT NULL,
            note TEXT NOT NULL DEFAULT '',
            source_url TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            check_after INTEGER,
            status TEXT NOT NULL DEFAULT 'open'
        );
        CREATE INDEX IF NOT EXISTS idx_open_items_bot ON open_items(bot_id, status, updated_at DESC);
        CREATE TABLE IF NOT EXISTS reported_items (
            bot_id TEXT NOT NULL,
            key TEXT NOT NULL,
            note TEXT,
            at INTEGER NOT NULL,
            PRIMARY KEY (bot_id, key)
        );
        CREATE INDEX IF NOT EXISTS idx_reported_items_at ON reported_items(bot_id, at DESC);
        CREATE TABLE IF NOT EXISTS message_bookmarks (
            id TEXT PRIMARY KEY,
            bot_id TEXT NOT NULL,
            entry_id TEXT NOT NULL,
            thread_id TEXT,
            author TEXT NOT NULL DEFAULT '',
            preview TEXT NOT NULL DEFAULT '',
            note TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            UNIQUE (bot_id, entry_id)
        );
        CREATE INDEX IF NOT EXISTS idx_message_bookmarks_bot ON message_bookmarks(bot_id, updated_at DESC);",
    )?;
    Ok(())
}

fn row_item(r: &rusqlite::Row) -> rusqlite::Result<OpenItem> {
    Ok(OpenItem {
        id: r.get(0)?,
        bot_id: r.get(1)?,
        title: r.get(2)?,
        note: r.get(3)?,
        source_url: r.get(4)?,
        created_at: r.get(5)?,
        updated_at: r.get(6)?,
        check_after: r.get(7)?,
        status: r.get(8)?,
    })
}

fn hours_millis(hours: f64) -> Result<i64> {
    let duration = std::time::Duration::try_from_secs_f64(hours * 3600.0)
        .context("hours must be finite, nonnegative and within range")?;
    i64::try_from(duration.as_millis()).context("hours exceed the supported timestamp range")
}

impl Store {
    pub fn list_open_items(&self, bot_id: &str) -> Result<Vec<OpenItem>> {
        let c = self.db.locked();
        let mut st = c.prepare(
            "SELECT id, bot_id, title, note, source_url, created_at, updated_at, check_after, status
             FROM open_items WHERE bot_id = ?1 AND status = 'open'
             ORDER BY updated_at DESC LIMIT 50",
        )?;
        Ok(st.query_map([bot_id], row_item)?.collect::<rusqlite::Result<_>>()?)
    }

    pub fn track_open_item(
        &self,
        bot_id: &str,
        title: &str,
        note: &str,
        source_url: Option<&str>,
        check_after_hours: Option<f64>,
    ) -> Result<OpenItem> {
        let title: String = title.trim().chars().take(160).collect();
        if title.is_empty() {
            bail!("title is required");
        }
        let note: String = note.trim().chars().take(500).collect();
        let source_url = source_url.map(str::trim).filter(|s| !s.is_empty());
        let now = now_ms();
        let check_after = check_after_hours
            .filter(|h| *h > 0.0 && h.is_finite())
            .map(|h| {
                hours_millis(h).and_then(|millis| now.checked_add(millis).context("check time exceeds timestamp range"))
            })
            .transpose()?;
        let c = self.db.locked();
        if let Some(mut existing) = c
            .query_row(
                "SELECT id, bot_id, title, note, source_url, created_at, updated_at, check_after, status
                 FROM open_items WHERE bot_id = ?1 AND status = 'open' AND lower(title) = lower(?2) LIMIT 1",
                params![bot_id, title],
                row_item,
            )
            .optional()?
        {
            if !note.is_empty() {
                existing.note = note;
            }
            if let Some(url) = source_url {
                existing.source_url = Some(url.to_owned());
            }
            if check_after.is_some() {
                existing.check_after = check_after;
            }
            existing.updated_at = now;
            c.execute(
                "UPDATE open_items SET note = ?2, source_url = ?3, check_after = ?4, updated_at = ?5 WHERE id = ?1",
                params![existing.id, existing.note, existing.source_url, existing.check_after, existing.updated_at],
            )?;
            return Ok(existing);
        }
        let id = format!("item_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
        c.execute(
            "INSERT INTO open_items(id, bot_id, title, note, source_url, created_at, updated_at, check_after, status)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, 'open')",
            params![id, bot_id, title, note, source_url, now, check_after],
        )?;
        Ok(OpenItem {
            id,
            bot_id: bot_id.to_owned(),
            title,
            note,
            source_url: source_url.map(str::to_owned),
            created_at: now,
            updated_at: now,
            check_after,
            status: "open".to_owned(),
        })
    }

    pub fn update_open_item(
        &self,
        bot_id: &str,
        id: &str,
        note: Option<&str>,
        check_after_hours: Option<f64>,
    ) -> Result<OpenItem> {
        let now = now_ms();
        let c = self.db.locked();
        let mut item = c
            .query_row(
                "SELECT id, bot_id, title, note, source_url, created_at, updated_at, check_after, status
                 FROM open_items WHERE bot_id = ?1 AND id = ?2",
                params![bot_id, id],
                row_item,
            )
            .optional()?
            .with_context(|| format!("unknown open item `{id}`"))?;
        if let Some(n) = note {
            item.note = n.trim().chars().take(500).collect();
        }
        if let Some(h) = check_after_hours.filter(|h| *h > 0.0 && h.is_finite()) {
            item.check_after = Some(now.checked_add(hours_millis(h)?).context("check time exceeds timestamp range")?);
        }
        item.updated_at = now;
        c.execute(
            "UPDATE open_items SET note = ?2, check_after = ?3, updated_at = ?4 WHERE bot_id = ?1 AND id = ?5",
            params![bot_id, item.note, item.check_after, item.updated_at, id],
        )?;
        Ok(item)
    }

    pub fn close_open_item(&self, bot_id: &str, id: &str, status: &str, note: Option<&str>) -> Result<bool> {
        let status = if status == "dismissed" { "dismissed" } else { "resolved" };
        let now = now_ms();
        let c = self.db.locked();
        let changed = match note {
            Some(n) => c.execute(
                "UPDATE open_items SET status = ?3, note = ?4, updated_at = ?5 WHERE bot_id = ?1 AND id = ?2",
                params![bot_id, id, status, n.trim(), now],
            )?,
            None => c.execute(
                "UPDATE open_items SET status = ?3, updated_at = ?4 WHERE bot_id = ?1 AND id = ?2",
                params![bot_id, id, status, now],
            )?,
        };
        Ok(changed > 0)
    }

    pub fn mark_reported(&self, bot_id: &str, key: &str, note: Option<&str>) -> Result<()> {
        let key: String = key.trim().chars().take(240).collect();
        if key.is_empty() {
            bail!("key is required");
        }
        let now = now_ms();
        let cutoff = now - 30 * 86_400_000;
        let c = self.db.locked();
        c.execute(
            "INSERT INTO reported_items(bot_id, key, note, at) VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(bot_id, key) DO UPDATE SET note = excluded.note, at = excluded.at",
            params![bot_id, key, note.map(|s| s.chars().take(240).collect::<String>()), now],
        )?;
        let _ = c.execute("DELETE FROM reported_items WHERE bot_id = ?1 AND at < ?2", params![bot_id, cutoff]);
        Ok(())
    }

    pub fn was_reported(&self, bot_id: &str, keys: &[String], within_hours: Option<f64>) -> Result<Value> {
        let now = now_ms();
        let since = now
            .checked_sub(hours_millis(within_hours.unwrap_or(24.0 * 14.0))?)
            .context("report window exceeds timestamp range")?;
        let c = self.db.locked();
        let mut reported = Vec::new();
        let mut fresh = Vec::new();
        for raw in keys.iter().take(50) {
            let k: String = raw.trim().chars().take(240).collect();
            let row = c
                .query_row(
                    "SELECT note, at FROM reported_items WHERE bot_id = ?1 AND key = ?2 AND at >= ?3",
                    params![bot_id, k, since],
                    |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?)),
                )
                .optional()?;
            if let Some((note, at)) = row {
                reported.push(json!({"key": raw, "at": at, "note": note}));
            } else {
                fresh.push(raw.clone());
            }
        }
        Ok(json!({"reported": reported, "fresh": fresh}))
    }

    pub fn list_bookmarks(&self, bot_id: &str) -> Result<Vec<MessageBookmark>> {
        let c = self.db.locked();
        let mut st = c.prepare(&format!(
            "SELECT id, bot_id, entry_id, thread_id, author, preview, note, created_at, updated_at
             FROM message_bookmarks WHERE bot_id = ?1
             AND NOT EXISTS(SELECT 1 FROM entries WHERE entries.id = message_bookmarks.entry_id
                            AND entries.bot_id = ?1 AND NOT ({PUBLIC}))
             ORDER BY updated_at DESC LIMIT 100"
        ))?;
        Ok(st
            .query_map([bot_id], |r| {
                Ok(MessageBookmark {
                    id: r.get(0)?,
                    bot_id: r.get(1)?,
                    entry_id: r.get(2)?,
                    thread_id: r.get(3)?,
                    author: r.get(4)?,
                    preview: r.get(5)?,
                    note: r.get(6)?,
                    created_at: r.get(7)?,
                    updated_at: r.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn save_bookmark(
        &self,
        bot_id: &str,
        entry_id: &str,
        thread_id: Option<&str>,
        author: &str,
        preview: &str,
        note: &str,
    ) -> Result<MessageBookmark> {
        let entry_id = entry_id.trim();
        if entry_id.is_empty() {
            bail!("entryId is required");
        }
        if self.entry(entry_id).is_some_and(|entry| entry.bot_id == bot_id && super::is_internal(&entry.data)) {
            bail!("internal messages cannot be bookmarked");
        }
        let preview: String = preview.trim().chars().take(320).collect();
        let note: String = note.trim().chars().take(600).collect();
        let author: String = author.trim().chars().take(80).collect();
        let thread_id = thread_id.map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
        let now = now_ms();
        let id = format!("bm_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
        let c = self.db.locked();
        c.execute(
            "INSERT INTO message_bookmarks(id, bot_id, entry_id, thread_id, author, preview, note, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
             ON CONFLICT(bot_id, entry_id) DO UPDATE SET
               note = excluded.note,
               preview = CASE WHEN excluded.preview != '' THEN excluded.preview ELSE message_bookmarks.preview END,
               updated_at = excluded.updated_at",
            params![id, bot_id, entry_id, thread_id, author, preview, note, now],
        )?;
        Ok(c.query_row(
            "SELECT id, bot_id, entry_id, thread_id, author, preview, note, created_at, updated_at
             FROM message_bookmarks WHERE bot_id = ?1 AND entry_id = ?2",
            params![bot_id, entry_id],
            |r| {
                Ok(MessageBookmark {
                    id: r.get(0)?,
                    bot_id: r.get(1)?,
                    entry_id: r.get(2)?,
                    thread_id: r.get(3)?,
                    author: r.get(4)?,
                    preview: r.get(5)?,
                    note: r.get(6)?,
                    created_at: r.get(7)?,
                    updated_at: r.get(8)?,
                })
            },
        )?)
    }

    pub fn delete_bookmark(&self, bot_id: &str, entry_id: &str) -> Result<bool> {
        let c = self.db.locked();
        let removed = c.execute(
            "DELETE FROM message_bookmarks WHERE bot_id = ?1 AND (entry_id = ?2 OR id = ?2)",
            params![bot_id, entry_id],
        )?;
        Ok(removed > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::temp_store;

    #[test]
    fn open_items_and_reported_dedup_round_trip() {
        let s = temp_store();
        let a = s.track_open_item("b1", "PR #42 waiting on CI", "Check GitHub Actions", None, Some(2.0)).unwrap();
        let b = s.track_open_item("b1", "pr #42 waiting on ci", "Updated note", None, None).unwrap();
        assert_eq!(a.id, b.id, "case-insensitive deduplication by title");
        assert_eq!(b.note, "Updated note");
        assert_eq!(s.list_open_items("b1").unwrap().len(), 1);
        assert!(s.close_open_item("b1", &a.id, "resolved", Some("CI green")).unwrap());
        assert_eq!(s.list_open_items("b1").unwrap().len(), 0);

        s.mark_reported("b1", "issue:101", Some("Sent summary")).unwrap();
        let check = s.was_reported("b1", &["issue:101".into(), "issue:102".into()], Some(24.0)).unwrap();
        assert_eq!(check["reported"].as_array().unwrap().len(), 1);
        assert_eq!(check["fresh"], json!(["issue:102"]));

        let bm = s
            .save_bookmark("b1", "entry-7", None, "b1", "Important architecture decision", "Review before release")
            .unwrap();
        assert_eq!(bm.entry_id, "entry-7");
        assert_eq!(bm.note, "Review before release");
        let updated_bm = s.save_bookmark("b1", "entry-7", None, "b1", "", "Updated note").unwrap();
        assert_eq!(updated_bm.id, bm.id);
        assert_eq!(updated_bm.preview, "Important architecture decision");
        assert_eq!(updated_bm.note, "Updated note");
        assert_eq!(s.list_bookmarks("b1").unwrap().len(), 1);
        assert!(s.delete_bookmark("b1", "entry-7").unwrap());
        assert_eq!(s.list_bookmarks("b1").unwrap().len(), 0);
    }
}
