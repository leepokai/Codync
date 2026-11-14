//! The log of dreaming's runs: the lines each added to and took out of `MEMORY.md`, and the notes it appended to.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::files::MAIN;
use crate::store;

/// One logged run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dream {
    pub id: i64,
    pub bot: String,
    /// Unix milliseconds.
    pub at: i64,
    /// Taken back by an app from before memory became files.
    pub undone: bool,
    pub added: Vec<Change>,
    /// Lines it took out.
    pub archived: Vec<Change>,
    /// The note versions it wrote.
    pub files: Vec<FileChange>,
}

/// A note a run appended to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FileChange {
    pub name: String,
    pub shared: bool,
    pub version: i64,
}

/// A line a run added or took out.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Change {
    pub id: i64,
    pub text: String,
}

/// Starts logging a run of `bot`'s dreaming.
pub fn begin(conn: &Connection, bot: &str, now: i64) -> store::Result<i64> {
    conn.execute("INSERT INTO memory_dreams (bot, at) VALUES (?1, ?2)", params![bot, now])?;
    Ok(conn.last_insert_rowid())
}

/// Logs a line the run added to a `MEMORY.md` (`added`) or took out of one.
pub fn record_line(conn: &Connection, dream: i64, added: bool, text: &str) -> store::Result<()> {
    let change = if added { "added" } else { "removed" };
    conn.execute(
        "INSERT INTO memory_dream_lines (dream, change, text) VALUES (?1, ?2, ?3)",
        params![dream, change, text],
    )?;
    Ok(())
}

/// Logs a file version the run wrote.
pub fn record_file(conn: &Connection, dream: i64, version: i64) -> store::Result<()> {
    conn.execute("INSERT INTO memory_dream_files (dream, version) VALUES (?1, ?2)", params![dream, version])?;
    Ok(())
}

pub fn get(conn: &Connection, id: i64) -> store::Result<Option<Dream>> {
    let Some((bot, at, undone)) = conn
        .query_row("SELECT bot, at, undone FROM memory_dreams WHERE id = ?1", [id], |row| {
            Ok((row.get::<_, String>(0)?, row.get(1)?, row.get(2)?))
        })
        .optional()?
    else {
        return Ok(None);
    };
    let mut dream = Dream { id, bot, at, undone, added: Vec::new(), archived: Vec::new(), files: Vec::new() };
    let mut stmt = conn.prepare("SELECT id, change, text FROM memory_dream_lines WHERE dream = ?1 ORDER BY id")?;
    for row in
        stmt.query_map([id], |row| Ok((row.get::<_, String>(1)?, Change { id: row.get(0)?, text: row.get(2)? })))?
    {
        let (kind, change) = row?;
        if kind == "added" { dream.added.push(change) } else { dream.archived.push(change) }
    }
    let mut stmt = conn.prepare(
        "SELECT v.name, v.bot = '', v.id FROM memory_dream_files d JOIN memory_file_versions v ON v.id = d.version
         WHERE d.dream = ?1 AND v.name != ?2 ORDER BY v.id",
    )?;
    for row in stmt.query_map(params![id, MAIN], |row| {
        Ok(FileChange { name: row.get(0)?, shared: row.get(1)?, version: row.get(2)? })
    })? {
        dream.files.push(row?);
    }
    Ok(Some(dream))
}

/// Drops a run that, once its files were written or failed, changed nothing.
pub fn discard_if_empty(conn: &Connection, dream: i64) -> store::Result<()> {
    let written: i64 = conn.query_row(
        "SELECT (SELECT count(*) FROM memory_dream_lines WHERE dream = ?1)
              + (SELECT count(*) FROM memory_dream_files WHERE dream = ?1)",
        [dream],
        |row| row.get(0),
    )?;
    if written == 0 {
        conn.execute("DELETE FROM memory_dreams WHERE id = ?1", [dream])?;
    }
    Ok(())
}

/// The bot's latest runs, newest first.
pub fn recent(conn: &Connection, bot: &str, limit: usize) -> store::Result<Vec<Dream>> {
    let mut stmt = conn.prepare("SELECT id FROM memory_dreams WHERE bot = ?1 ORDER BY at DESC, id DESC LIMIT ?2")?;
    let ids: Vec<i64> = stmt
        .query_map(params![bot, i64::try_from(limit).unwrap_or(i64::MAX)], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    ids.into_iter().filter_map(|id| get(conn, id).transpose()).collect()
}
