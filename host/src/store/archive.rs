//! Older threads (`chat::work::archive`): ended tasks out of their bots' boxes, and the thread bots no task in a box
//! names any more, parked out of `bots` so the lists of threads every pass reads stay short.

use super::{BotConfig, Store, next_rev};
use crate::LockExt;
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::BTreeSet;
use std::fmt;

/// The tables, made with the others (`Store::open`): nothing older changes, so the schema stays.
pub(super) const TABLES: &str = "CREATE TABLE IF NOT EXISTS thread_archive(
        bot TEXT NOT NULL, task TEXT NOT NULL, thread TEXT, ended_at INTEGER NOT NULL, data TEXT NOT NULL,
        PRIMARY KEY(bot, task));
     CREATE INDEX IF NOT EXISTS thread_archive_ended ON thread_archive(ended_at);
     CREATE INDEX IF NOT EXISTS thread_archive_thread ON thread_archive(thread);
     CREATE TABLE IF NOT EXISTS parked_threads(
        id TEXT PRIMARY KEY, bot TEXT NOT NULL, config TEXT NOT NULL, session_id TEXT,
        read_rev INTEGER NOT NULL DEFAULT 0);
     CREATE INDEX IF NOT EXISTS parked_threads_bot ON parked_threads(bot);";

/// One archived task: its bot and the task as JSON.
pub struct Archived {
    pub bot: String,
    pub data: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnparkError {
    Storage,
    Decode,
}

impl UnparkError {
    pub(crate) fn category(self) -> &'static str {
        match self {
            Self::Storage => "storage",
            Self::Decode => "decode",
        }
    }
}

impl fmt::Display for UnparkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "couldn't restore a parked thread ({})", self.category())
    }
}

impl std::error::Error for UnparkError {}

pub(crate) struct ExpiredArchive<T> {
    pub tasks: Vec<T>,
    pub threads: Vec<(String, Result<(), UnparkError>)>,
}

/// Leaves out the tasks whose thread is gone (a host from before the archive deleted it a week after it ended): they
/// can't be opened any more, only recalled.
const THREAD_KEPT: &str = "(thread IS NULL OR thread IN (SELECT id FROM parked_threads)
     OR thread IN (SELECT id FROM bots WHERE deleted = 0))";

fn archived(row: &rusqlite::Row<'_>) -> rusqlite::Result<Archived> {
    Ok(Archived { bot: row.get(0)?, data: row.get(1)? })
}

fn unpark(tx: &Connection, id: &str) -> Result<Option<BotConfig>, UnparkError> {
    let parked: Option<(String, Option<String>, i64)> = tx
        .query_row("DELETE FROM parked_threads WHERE id = ?1 RETURNING config, session_id, read_rev", [id], |r| {
            #[cfg(test)]
            if let Some(detail) = unpark_failure(id, UnparkError::Storage) {
                return Err(rusqlite::Error::InvalidParameterName(detail.to_owned()));
            }
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .optional()
        .map_err(|_| UnparkError::Storage)?;
    let Some((config, session, read_rev)) = parked else { return Ok(None) };
    let decoded = serde_json::from_str(&config);
    #[cfg(test)]
    let decoded =
        unpark_failure(id, UnparkError::Decode).map_or(decoded, |detail| Err(serde::de::Error::custom(detail)));
    let cfg = decoded.map_err(|_| UnparkError::Decode)?;
    let rev = next_rev(tx).map_err(|_| UnparkError::Storage)?;
    tx.execute(
        "INSERT OR REPLACE INTO bots(id, rev, config, session_id, read_rev) VALUES(?1, ?2, ?3, ?4, ?5)",
        params![id, rev, config, session, read_rev],
    )
    .map_err(|_| UnparkError::Storage)?;
    Ok(Some(cfg))
}

impl Store {
    /// Keeps `rows` (`(task, thread, ended_at, data)`) of `bot`'s in the archive, in one transaction.
    pub fn archive_tasks(&self, bot: &str, rows: &[(String, Option<String>, i64, String)]) -> Result<()> {
        let mut c = self.db.locked();
        let tx = c.transaction()?;
        for (task, thread, ended_at, data) in rows {
            tx.execute(
                "INSERT OR REPLACE INTO thread_archive(bot, task, thread, ended_at, data) VALUES(?1, ?2, ?3, ?4, ?5)",
                params![bot, task, thread, ended_at, data],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Archived tasks whose thread still opens that ended before `before`, of `bot` or of every bot, newest first,
    /// `limit` at most.
    pub fn archived_before(&self, bot: Option<&str>, before: i64, limit: i64) -> Result<Vec<Archived>> {
        let c = self.db.locked();
        let mut st = c.prepare(&format!(
            "SELECT bot, data FROM thread_archive WHERE ended_at < ?1 AND (?2 IS NULL OR bot = ?2) AND {THREAD_KEPT}
             ORDER BY ended_at DESC LIMIT ?3"
        ))?;
        Ok(st.query_map(params![before, bot, limit], archived)?.collect::<rusqlite::Result<_>>()?)
    }

    /// Archived tasks whose thread still opens that ended at `at`, of `bot` or of every bot: a page ends after them all.
    pub fn archived_at(&self, bot: Option<&str>, at: i64) -> Result<Vec<Archived>> {
        let c = self.db.locked();
        let mut st = c.prepare(&format!(
            "SELECT bot, data FROM thread_archive WHERE ended_at = ?1 AND (?2 IS NULL OR bot = ?2) AND {THREAD_KEPT}"
        ))?;
        Ok(st.query_map(params![at, bot], archived)?.collect::<rusqlite::Result<_>>()?)
    }

    /// `bot`'s archived tasks that ended at `since` or later, newest first, `limit` at most.
    pub fn archived_of(&self, bot: &str, since: i64, limit: i64) -> Result<Vec<String>> {
        let c = self.db.locked();
        let mut st = c.prepare(
            "SELECT data FROM thread_archive WHERE bot = ?1 AND ended_at >= ?2 ORDER BY ended_at DESC LIMIT ?3",
        )?;
        Ok(st.query_map(params![bot, since, limit], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
    }

    /// `bot`'s archived task `task`.
    pub fn archived_task(&self, bot: &str, task: &str) -> Result<Option<String>> {
        let c = self.db.locked();
        Ok(c.query_row("SELECT data FROM thread_archive WHERE bot = ?1 AND task = ?2", [bot, task], |r| r.get(0))
            .optional()?)
    }

    /// Takes `bot`'s task `task` out of the archive.
    pub fn unarchive_task(&self, bot: &str, task: &str) -> Result<()> {
        self.db.locked().execute("DELETE FROM thread_archive WHERE bot = ?1 AND task = ?2", [bot, task])?;
        Ok(())
    }

    /// Takes expired tasks out of the archive after decoding them, keeping threads with unreadable tasks or configs.
    pub(crate) fn drop_archived_before<T, E>(
        &self,
        before: i64,
        decode: impl Fn(&str) -> Result<T, E>,
    ) -> Result<ExpiredArchive<T>> {
        let mut c = self.db.locked();
        let mut tx = c.transaction()?;
        let mut decoded = Vec::new();
        let mut kept_threads = BTreeSet::new();
        {
            let mut st = tx.prepare("SELECT rowid, thread, data FROM thread_archive WHERE ended_at < ?1")?;
            let mut rows = st.query([before])?;
            while let Some(row) = rows.next()? {
                let rowid: i64 = row.get(0)?;
                let thread: Option<String> = row.get(1)?;
                let task =
                    row.get_ref(2)?.as_str().map_err(|_| "storage").and_then(|data| decode(data).map_err(|_| "decode"));
                match task {
                    Ok(task) => decoded.push((rowid, thread, task)),
                    Err(category) => {
                        tracing::warn!(category, "couldn't read an expired archived task");
                        kept_threads.extend(thread);
                    }
                }
            }
        }
        let candidates = {
            let mut st = tx.prepare(
                "SELECT DISTINCT old.thread FROM thread_archive old
                 WHERE old.ended_at < ?1 AND old.thread IS NOT NULL
                   AND NOT EXISTS (SELECT 1 FROM thread_archive newer WHERE newer.thread = old.thread AND newer.ended_at >= ?1)
                   AND NOT EXISTS (SELECT 1 FROM bots WHERE id = old.thread)
                 ORDER BY old.thread",
            )?;
            st.query_map([before], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut threads = Vec::new();
        for id in candidates {
            if kept_threads.contains(&id) {
                continue;
            }
            let thread = tx.savepoint()?;
            let result = unpark(&thread, &id).map(|_| ());
            if result.is_ok() {
                thread.commit()?;
            } else {
                thread.finish()?;
                kept_threads.insert(id.clone());
            }
            threads.push((id, result));
        }
        let tasks = {
            let mut st = tx.prepare("DELETE FROM thread_archive WHERE rowid = ?1")?;
            let mut removed = Vec::new();
            for (rowid, thread, task) in decoded {
                if !thread.is_some_and(|id| kept_threads.contains(&id)) {
                    st.execute([rowid])?;
                    removed.push(task);
                }
            }
            removed
        };
        tx.commit()?;
        Ok(ExpiredArchive { tasks, threads })
    }

    /// Takes all of `bot`'s tasks out of the archive.
    pub fn drop_archived_of(&self, bot: &str) -> Result<Vec<String>> {
        let c = self.db.locked();
        let mut st = c.prepare("DELETE FROM thread_archive WHERE bot = ?1 RETURNING data")?;
        Ok(st.query_map([bot], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
    }

    /// `bot`'s archived tasks `thread` worked, newest first.
    pub fn archived_by_thread(&self, bot: &str, thread: &str) -> Result<Vec<String>> {
        let c = self.db.locked();
        let mut st =
            c.prepare("SELECT data FROM thread_archive WHERE thread = ?1 AND bot = ?2 ORDER BY ended_at DESC")?;
        Ok(st.query_map([thread, bot], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
    }

    /// Moves thread bot `cfg` out of `bots` with its session, its transcript and everything else of it left as it
    /// is. No client ever listed a thread bot, so none hears it go.
    pub fn park_thread(&self, cfg: &BotConfig) -> Result<()> {
        let mut c = self.db.locked();
        let tx = c.transaction()?;
        let bot = cfg.thread_of.as_deref().unwrap_or_default();
        tx.execute(
            "INSERT OR REPLACE INTO parked_threads(id, bot, config, session_id, read_rev)
             SELECT id, ?2, config, session_id, read_rev FROM bots WHERE id = ?1",
            params![cfg.id, bot],
        )?;
        tx.execute("DELETE FROM bots WHERE id = ?1", [&cfg.id])?;
        tx.commit()?;
        Ok(())
    }

    /// Parked thread bot `id`.
    pub fn parked_thread(&self, id: &str) -> Result<Option<BotConfig>> {
        let c = self.db.locked();
        let config: Option<String> =
            c.query_row("SELECT config FROM parked_threads WHERE id = ?1", [id], |r| r.get(0)).optional()?;
        Ok(config.map(|config| serde_json::from_str(&config)).transpose()?)
    }

    /// Puts parked thread bot `id` back in `bots`, as it was: its agent goes on in the same session.
    pub(crate) fn unpark_thread(&self, id: &str) -> Result<Option<BotConfig>, UnparkError> {
        let mut c = self.db.locked();
        let tx = c.transaction().map_err(|_| UnparkError::Storage)?;
        let cfg = unpark(&tx, id)?;
        tx.commit().map_err(|_| UnparkError::Storage)?;
        Ok(cfg)
    }

    /// The ids of `bot`'s parked thread bots.
    pub fn parked_of(&self, bot: &str) -> Result<Vec<String>> {
        let c = self.db.locked();
        let mut st = c.prepare("SELECT id FROM parked_threads WHERE bot = ?1")?;
        Ok(st.query_map([bot], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
tokio::task_local! {
    static UNPARK_FAILURES: [(&'static str, UnparkError, &'static str); 2];
}

#[cfg(test)]
fn unpark_failure(id: &str, error: UnparkError) -> Option<&'static str> {
    UNPARK_FAILURES
        .try_with(|failures| {
            failures.iter().find_map(|&(thread, kind, detail)| (thread == id && kind == error).then_some(detail))
        })
        .ok()
        .flatten()
}

#[cfg(test)]
impl Store {
    pub(crate) fn with_unpark_failures<T>(
        read: &'static str,
        decode: &'static str,
        detail: &'static str,
        run: impl FnOnce() -> T,
    ) -> T {
        UNPARK_FAILURES.sync_scope([(read, UnparkError::Storage, detail), (decode, UnparkError::Decode, detail)], run)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread(id: &str) -> BotConfig {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": "T", "backend": "claude", "cwd": "/tmp", "threadOf": "b1",
        }))
        .expect("a thread")
    }

    #[test]
    fn an_archived_task_pages_by_when_it_ended_and_comes_back_once() {
        let s = Store::open(std::path::Path::new(":memory:")).expect("store");
        let rows = |ids: &[(&str, i64)]| {
            ids.iter().map(|(id, at)| ((*id).to_owned(), None, *at, format!("\"{id}\""))).collect::<Vec<_>>()
        };
        s.archive_tasks("b1", &rows(&[("t1", 10), ("t2", 20), ("t3", 30)])).expect("kept");
        let page: Vec<String> =
            s.archived_before(Some("b1"), 30, 1).expect("page").into_iter().map(|a| a.data).collect();
        assert_eq!(page, ["\"t2\""], "newest before the cursor first");
        assert_eq!(s.archived_of("b1", 20, 10).expect("recent"), ["\"t3\"", "\"t2\""]);
        assert_eq!(s.archived_task("b1", "t2").expect("read").as_deref(), Some("\"t2\""));
        s.unarchive_task("b1", "t2").expect("taken");
        assert_eq!(s.archived_task("b1", "t2").expect("gone"), None);
        let dropped = s.drop_archived_before(20, |data| serde_json::from_str::<String>(data)).expect("dropped").tasks;
        assert_eq!(dropped, ["t1"]);
        assert_eq!(s.drop_archived_of("b1").expect("all"), ["\"t3\""]);
    }

    #[test]
    fn a_parked_thread_leaves_the_bots_and_comes_back_as_it_was() {
        let s = Store::open(std::path::Path::new(":memory:")).expect("store");
        let cfg = thread(&format!("{}x", crate::store::THREAD_PREFIX));
        s.save_bot(&cfg).expect("saved");
        s.set_session(&cfg.id, Some("s1")).expect("a session");
        s.park_thread(&cfg).expect("parked");
        assert!(s.bot(&cfg.id).expect("read").is_none());
        assert_eq!(s.thread_bots().expect("threads").len(), 0);
        assert_eq!(s.parked_of("b1").expect("parked"), std::slice::from_ref(&cfg.id));
        assert_eq!(s.parked_thread(&cfg.id).expect("read").map(|c| c.id), Some(cfg.id.clone()));
        assert_eq!(s.unpark_thread(&cfg.id).expect("back").map(|c| c.cwd), Some("/tmp".to_owned()));
        let row = s.bot(&cfg.id).expect("read").expect("back");
        assert_eq!((row.deleted, row.session_id.as_deref()), (false, Some("s1")), "it goes on in its session");
        assert_eq!(s.parked_of("b1").expect("none"), Vec::<String>::new());
    }

    #[test]
    fn an_unpark_read_or_decode_failure_rolls_back_and_the_session_can_be_restored() {
        let s = Store::open(std::path::Path::new(":memory:")).expect("store");
        let cases = [("thread-read", UnparkError::Storage), ("thread-decode", UnparkError::Decode)];
        let parked = |id| {
            s.db.locked()
                .query_row("SELECT bot, config, session_id, read_rev FROM parked_threads WHERE id = ?1", [id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?))
                })
                .expect("the original parked row")
        };
        for (id, _) in cases {
            let cfg = thread(id);
            s.save_bot(&cfg).expect("saved");
            s.set_session(id, Some("sample-session")).expect("a session");
            s.mark_read(id).expect("read revision");
            s.park_thread(&cfg).expect("parked");
        }
        let originals = cases.map(|(id, _)| parked(id));
        let rev = s.current_rev();
        Store::with_unpark_failures("thread-read", "thread-decode", "synthetic archive diagnostic", || {
            for ((id, category), original) in cases.iter().zip(&originals) {
                let error = s.unpark_thread(id).expect_err("injected failure");
                assert_eq!(error, *category);
                assert_eq!(error.to_string(), format!("couldn't restore a parked thread ({})", category.category()));
                assert_eq!(&parked(id), original);
                assert!(s.bot(id).expect("read").is_none());
            }
            assert_eq!(s.current_rev(), rev);
        });
        for ((id, _), (_, config, session, read_rev)) in cases.into_iter().zip(originals) {
            let restored = s.unpark_thread(id).expect("retry").expect("the parked thread");
            assert_eq!(serde_json::to_string(&restored).expect("config"), config);
            let row = s.bot(id).expect("read").expect("restored thread bot");
            assert_eq!((row.session_id.as_deref(), row.read_rev), (Some(session.as_str()), read_rev));
        }
    }
}
