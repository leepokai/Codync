//! Memory is Markdown files, in a folder per layer (the shared one, and each bot's own): `MEMORY.md`, one short
//! fact per line, which every session opens with in full, and notes on one topic each, read when it comes up.
//!
//! The file on disk is the live copy, read and edited by the bot with its own tools or the memory tools. The crate
//! keeps each file's versions (full content, author, cause), who wrote each fact of `MEMORY.md` ([`facts`]), decides
//! what a change may be ([`check_change`], [`admit`]), and renders the index an opening carries ([`index`]). The
//! caller lists, reads and writes the files.

mod facts;
mod index;

pub use facts::{Fact, admit, fact_id, facts, facts_of, set_origin, settle};
pub use index::{Index, index};

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

use crate::check::{self, Refusal};
use crate::store::{self, Result};
use crate::tokens;

/// The file every session opens with in full.
pub const MAIN: &str = "MEMORY.md";
/// The most one note may hold, in estimated tokens.
pub const FILE_CAP: usize = 2_000;
/// The most notes one layer may hold, its topic folders included.
pub const FOLDER_CAP: usize = 20;
/// The longest note name, its folders and `.md` included.
const NAME_MAX: usize = 128;
/// The most topic folders a note may sit in, one inside the other.
pub const DEPTH_MAX: usize = 3;

/// Who made a version, or wrote a fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Author {
    User,
    Bot,
    Dream,
    /// Changed outside Codync, or by no bot's tool call.
    Computer,
    /// Brought over from an earlier memory.
    Imported,
}

impl Author {
    /// The user wrote it: in Codync, or in a file on the computer. Dreaming never takes it out or rewrites it;
    /// tidying may move it word for word into a note.
    pub fn is_users(self) -> bool {
        matches!(self, Self::User | Self::Computer)
    }
}

/// Why a version was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Cause {
    Edit,
    Delete,
    /// An earlier version brought back.
    Restore,
    /// A change taken back.
    Undo,
    Dream,
}

/// A file as last recorded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct File {
    /// The bot whose folder holds it; `None` for the shared one.
    pub bot: Option<String>,
    pub name: String,
    /// SHA-256 of the content; empty once deleted.
    pub hash: String,
    pub size: u64,
    /// Its modification time on disk, Unix milliseconds.
    pub mtime: i64,
    pub description: Option<String>,
    pub tokens: usize,
    pub deleted: bool,
    /// The version holding its current content.
    pub version: i64,
    pub updated_at: i64,
    pub rev: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: i64,
    pub bot: Option<String>,
    pub name: String,
    /// `None`: the file was deleted.
    pub content: Option<String>,
    pub author: Author,
    pub author_bot: Option<String>,
    pub cause: Cause,
    pub at: i64,
    pub rev: i64,
}

/// A change to record as a new version.
#[derive(Clone, Copy, Debug)]
pub struct Change<'a> {
    pub bot: Option<&'a str>,
    pub name: &'a str,
    /// `None`: the file is gone.
    pub content: Option<&'a str>,
    pub author: Author,
    pub author_bot: Option<&'a str>,
    pub cause: Cause,
    /// The user's message the facts it adds to `MEMORY.md` were learned from.
    pub message: Option<&'a str>,
    /// The file on disk after the change: modification time (Unix ms) and size.
    pub mtime: i64,
    pub size: u64,
}

/// `MEMORY.md` at the top, or a note: a lowercase ASCII slug, in up to [`DEPTH_MAX`] topic folders named the same
/// way (`health/allergies.md`), since Unicode and case variants of a name would become duplicate notes.
pub fn valid_name(name: &str) -> bool {
    name == MAIN
        || name.len() <= NAME_MAX
            && name.strip_suffix(".md").is_some_and(|stem| match stem.rsplit_once('/') {
                Some((folder, leaf)) => valid_folder(folder) && slug(leaf),
                None => slug(stem),
            })
}

/// A topic folder a note may sit in (`health`, `health/food`): up to [`DEPTH_MAX`] slugs, one inside the other.
pub fn valid_folder(path: &str) -> bool {
    path.split('/').count() <= DEPTH_MAX && path.split('/').all(slug)
}

fn slug(part: &str) -> bool {
    !part.is_empty() && part.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The most a layer's `MEMORY.md` may hold (`bot`: `None` for the shared one), in estimated tokens.
pub fn cap(bot: Option<&str>) -> usize {
    if bot.is_some() { tokens::BOT_CAP } else { tokens::YOURS_CAP }
}

/// SHA-256 of a file's content, as hex.
pub fn hash(content: &str) -> String {
    Sha256::digest(content.as_bytes()).iter().fold(String::with_capacity(64), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

/// The `description:` line of the frontmatter, which says when to read the file.
pub fn description(content: &str) -> Option<String> {
    let rest = frontmatter(content)?;
    for line in rest.lines() {
        let line = line.trim();
        if line == "---" {
            return None;
        }
        if let Some(value) = line.strip_prefix("description:") {
            let value = value.trim().trim_matches(['"', '\'']).trim();
            return (!value.is_empty()).then(|| value.to_owned());
        }
    }
    None
}

/// What follows a frontmatter's opening `---`, when the content starts with one.
fn frontmatter(content: &str) -> Option<&str> {
    let rest = content.trim_start_matches('\u{feff}').strip_prefix("---")?;
    rest.strip_prefix("\r\n").or_else(|| rest.strip_prefix('\n'))
}

/// The checks a change a bot or dreaming makes to a file (`before`: `None` when there was none) passes, on what it
/// brings in: what memory may say, and the caps. Lines the file had aren't checked again, and a file over its cap
/// may still shrink, so a line the user wrote never blocks the bot's changes around it. `MEMORY.md` holds one short
/// fact per line within its layer's cap (it may be empty); a note needs the description its index line comes from.
/// In a turn the user started in the bot's own chat (`asked`), `MEMORY.md` may grow to twice its cap, so what the
/// user asks to remember is never turned away for space: dreaming then moves its detail out into notes.
pub fn check_change(
    bot: Option<&str>,
    name: &str,
    before: Option<&str>,
    after: &str,
    asked: bool,
) -> std::result::Result<(), Refusal> {
    let new = facts::gained(before, Some(after));
    for line in &new {
        check::content(line)?;
    }
    let size = tokens::estimate(after);
    let grows = size > before.map_or(0, tokens::estimate);
    if name == MAIN {
        let long = |line: &&str| tokens::estimate(store::unmarked(line)) > tokens::LINE_CAP;
        if new.iter().filter(|line| !line.starts_with('#')).any(long) {
            return Err(Refusal::TooLong);
        }
        // ponytail: twice the cap bounds the opening when dreaming falls behind; past it, the bot writes a note.
        if grows && size > cap(bot) * if asked { 2 } else { 1 } {
            return Err(Refusal::Full);
        }
        return Ok(());
    }
    if after.trim().is_empty() {
        return Err(Refusal::Empty);
    }
    if grows && size > FILE_CAP {
        return Err(Refusal::FileTooLong);
    }
    if description(after).is_none() {
        return Err(Refusal::NoDescription);
    }
    Ok(())
}

/// The file in a layer (`bot`: `None` for the shared one), deleted or not.
pub fn get(conn: &Connection, bot: Option<&str>, name: &str) -> Result<Option<File>> {
    Ok(conn.query_row(&format!("{SELECT} WHERE bot = ?1 AND name = ?2"), params![key(bot), name], file).optional()?)
}

/// Every file recorded in a layer, by name, deleted ones included.
pub fn layer(conn: &Connection, bot: Option<&str>) -> Result<Vec<File>> {
    let mut stmt = conn.prepare(&format!("{SELECT} WHERE bot = ?1 ORDER BY name"))?;
    Ok(stmt.query_map([key(bot)], file)?.collect::<rusqlite::Result<_>>()?)
}

/// Records a change as the file's newest version; for `MEMORY.md`, also who wrote the facts it adds.
pub fn record(conn: &Connection, change: &Change<'_>, now: i64, rev: i64) -> Result<Version> {
    if change.name == MAIN {
        facts::track(conn, change, now)?;
    }
    let bot = key(change.bot);
    conn.execute(
        "INSERT INTO memory_file_versions (bot, name, content, author, author_bot, cause, at, rev)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            bot,
            change.name,
            change.content,
            author_name(change.author),
            change.author_bot,
            cause_name(change.cause),
            now,
            rev
        ],
    )?;
    let version = conn.last_insert_rowid();
    let content = change.content.unwrap_or_default();
    let digest = change.content.map(hash).unwrap_or_default();
    conn.execute(
        "INSERT INTO memory_files (bot, name, hash, size, mtime, description, tokens, version, updated_at, rev)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT (bot, name) DO UPDATE SET hash = ?3, size = ?4, mtime = ?5, description = ?6, tokens = ?7,
             version = ?8, updated_at = ?9, rev = ?10",
        params![
            bot,
            change.name,
            digest,
            i64::try_from(change.size).unwrap_or(i64::MAX),
            change.mtime,
            description(content),
            i64::try_from(tokens::estimate(content)).unwrap_or(i64::MAX),
            version,
            now,
            rev
        ],
    )?;
    Ok(self::version(conn, version)?.expect("the version was just written"))
}

/// The file on disk has new times or size but the same content.
pub fn observed(conn: &Connection, bot: Option<&str>, name: &str, mtime: i64, size: u64) -> Result<()> {
    conn.execute(
        "UPDATE memory_files SET mtime = ?3, size = ?4 WHERE bot = ?1 AND name = ?2",
        params![key(bot), name, mtime, i64::try_from(size).unwrap_or(i64::MAX)],
    )?;
    Ok(())
}

/// A file's versions, newest first.
pub fn versions(conn: &Connection, bot: Option<&str>, name: &str, limit: usize) -> Result<Vec<Version>> {
    let mut stmt = conn.prepare(&format!("{VERSION} WHERE bot = ?1 AND name = ?2 ORDER BY id DESC LIMIT ?3"))?;
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    Ok(stmt.query_map(params![key(bot), name, limit], version_row)?.collect::<rusqlite::Result<_>>()?)
}

pub fn version(conn: &Connection, id: i64) -> Result<Option<Version>> {
    Ok(conn.query_row(&format!("{VERSION} WHERE id = ?1"), [id], version_row).optional()?)
}

/// The content the newest version of a live file holds.
pub fn current(conn: &Connection, bot: Option<&str>, name: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT v.content FROM memory_files f JOIN memory_file_versions v ON v.id = f.version
             WHERE f.bot = ?1 AND f.name = ?2",
            params![key(bot), name],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten())
}

/// Forgets a layer's files, their versions and who wrote its facts. Returns the names that were live, for the
/// caller to delete.
pub fn forget_layer(conn: &Connection, bot: Option<&str>) -> Result<Vec<String>> {
    let live = layer(conn, bot)?.into_iter().filter(|f| !f.deleted).map(|f| f.name).collect();
    let bot = key(bot);
    conn.execute(
        "DELETE FROM memory_dream_files WHERE version IN (SELECT id FROM memory_file_versions WHERE bot = ?1)",
        [&bot],
    )?;
    conn.execute("DELETE FROM memory_file_versions WHERE bot = ?1", [&bot])?;
    conn.execute("DELETE FROM memory_files WHERE bot = ?1", [&bot])?;
    conn.execute("DELETE FROM memory_facts WHERE bot = ?1", [&bot])?;
    Ok(live)
}

/// Drops what memory kept from before `before` (Unix ms): versions no file holds now, and the dream log. What a
/// bot forgot is gone from the history once its versions are this old.
pub fn prune(conn: &Connection, before: i64) -> Result<()> {
    let old = "SELECT id FROM memory_file_versions WHERE at < ?1 AND id NOT IN (SELECT version FROM memory_files)";
    conn.execute(&format!("DELETE FROM memory_dream_files WHERE version IN ({old})"), [before])?;
    conn.execute(&format!("DELETE FROM memory_file_versions WHERE id IN ({old})"), [before])?;
    for table in ["memory_dream_lines", "memory_dream_files"] {
        conn.execute(
            &format!("DELETE FROM {table} WHERE dream IN (SELECT id FROM memory_dreams WHERE at < ?1)"),
            [before],
        )?;
    }
    conn.execute("DELETE FROM memory_dreams WHERE at < ?1", [before])?;
    Ok(())
}

/// A line of a live file that contains a search.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub bot: Option<String>,
    pub name: String,
    /// 1-based.
    pub line: usize,
    pub text: String,
}

/// Lines of the live files a bot sees (its own, and the shared ones when it uses them) that contain `query`,
/// ignoring case.
pub fn search(conn: &Connection, bot: &str, shared: bool, query: &str, limit: usize) -> Result<Vec<Hit>> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT f.bot, f.name, v.content FROM memory_files f JOIN memory_file_versions v ON v.id = f.version
         WHERE f.hash != '' AND (f.bot = ?1 OR (?2 AND f.bot = '')) ORDER BY f.bot = '', f.name",
    )?;
    let rows = stmt.query_map(params![bot, shared], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?))
    })?;
    let mut hits = Vec::new();
    for row in rows {
        let (owner, name, content) = row?;
        for (i, line) in content.unwrap_or_default().lines().enumerate() {
            if line.to_lowercase().contains(&needle) {
                hits.push(Hit { bot: layer_of(owner.clone()), name: name.clone(), line: i + 1, text: line.to_owned() });
                if hits.len() == limit {
                    return Ok(hits);
                }
            }
        }
    }
    Ok(hits)
}

/// How a layer is stored: `''` is the shared one, since a primary key can't tell NULLs apart.
fn key(bot: Option<&str>) -> String {
    bot.unwrap_or_default().to_owned()
}

fn layer_of(key: String) -> Option<String> {
    (!key.is_empty()).then_some(key)
}

const SELECT: &str =
    "SELECT bot, name, hash, size, mtime, description, tokens, version, updated_at, rev FROM memory_files";

fn file(row: &Row<'_>) -> rusqlite::Result<File> {
    let hash: String = row.get(2)?;
    Ok(File {
        bot: layer_of(row.get(0)?),
        name: row.get(1)?,
        deleted: hash.is_empty(),
        hash,
        size: u64::try_from(row.get::<_, i64>(3)?).unwrap_or_default(),
        mtime: row.get(4)?,
        description: row.get(5)?,
        tokens: usize::try_from(row.get::<_, i64>(6)?).unwrap_or_default(),
        version: row.get(7)?,
        updated_at: row.get(8)?,
        rev: row.get(9)?,
    })
}

const VERSION: &str = "SELECT id, bot, name, content, author, author_bot, cause, at, rev FROM memory_file_versions";

fn version_row(row: &Row<'_>) -> rusqlite::Result<Version> {
    Ok(Version {
        id: row.get(0)?,
        bot: layer_of(row.get(1)?),
        name: row.get(2)?,
        content: row.get(3)?,
        author: parse_author(&row.get::<_, String>(4)?),
        author_bot: row.get(5)?,
        cause: parse_cause(&row.get::<_, String>(6)?),
        at: row.get(7)?,
        rev: row.get(8)?,
    })
}

fn author_name(author: Author) -> &'static str {
    match author {
        Author::User => "user",
        Author::Bot => "bot",
        Author::Dream => "dream",
        Author::Computer => "computer",
        Author::Imported => "imported",
    }
}

pub(crate) fn parse_author(name: &str) -> Author {
    match name {
        "bot" => Author::Bot,
        "dream" => Author::Dream,
        "computer" => Author::Computer,
        "imported" => Author::Imported,
        _ => Author::User,
    }
}

fn cause_name(cause: Cause) -> &'static str {
    match cause {
        Cause::Edit => "edit",
        Cause::Delete => "delete",
        Cause::Restore => "restore",
        Cause::Undo => "undo",
        Cause::Dream => "dream",
    }
}

fn parse_cause(name: &str) -> Cause {
    match name {
        "delete" => Cause::Delete,
        "restore" => Cause::Restore,
        "undo" => Cause::Undo,
        "dream" => Cause::Dream,
        _ => Cause::Edit,
    }
}

#[cfg(test)]
mod tests;
