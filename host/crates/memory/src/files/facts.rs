//! The facts of a file: its lines, who wrote each fact of `MEMORY.md` and what it was learned from, and what a bot
//! may change.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

use super::{Author, Change, MAIN, author_name, current, key, parse_author};
use crate::check::Refusal;
use crate::store::{self, Result, fingerprint};

/// One fact of a layer's `MEMORY.md` as it is now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fact {
    pub text: String,
    pub author: Author,
    pub author_bot: Option<String>,
    /// The user's message it was learned from.
    pub message: Option<String>,
    /// When it was written, Unix milliseconds.
    pub at: i64,
}

/// The facts a file holds: its non-empty lines past any frontmatter, without headings or list markers.
pub fn facts(content: &str) -> Vec<&str> {
    lines(content).filter(|line| !line.starts_with('#')).map(store::unmarked).filter(|fact| !fact.is_empty()).collect()
}

/// The non-empty lines past any frontmatter, trimmed.
fn lines(content: &str) -> impl Iterator<Item = &str> {
    let body = match super::frontmatter(content) {
        Some(rest) => {
            let mut lines = rest.split_inclusive('\n');
            let mut skipped = 0;
            for line in lines.by_ref() {
                skipped += line.len();
                if line.trim() == "---" {
                    break;
                }
            }
            &rest[skipped.min(rest.len())..]
        }
        None => content,
    };
    body.lines().map(str::trim).filter(|line| !line.is_empty())
}

/// A stable id for a fact, for the apps to key it by.
pub fn fact_id(text: &str) -> i64 {
    // 52 bits, so it stays exact as a JavaScript number.
    i64::from_str_radix(&fingerprint(text)[..13], 16).unwrap_or_default()
}

/// A layer's `MEMORY.md` as facts, each with who wrote it. A fact written outside Codync counts as the computer's.
pub fn facts_of(conn: &Connection, bot: Option<&str>) -> Result<Vec<Fact>> {
    let content = current(conn, bot, MAIN)?.unwrap_or_default();
    let updated = super::get(conn, bot, MAIN)?.map_or(0, |f| f.updated_at);
    let mut stmt =
        conn.prepare("SELECT fingerprint, author, author_bot, message, at FROM memory_facts WHERE bot = ?1")?;
    let origins: HashMap<String, (Author, Option<String>, Option<String>, i64)> = stmt
        .query_map([key(bot)], |row| {
            Ok((
                row.get::<_, String>(0)?,
                (parse_author(&row.get::<_, String>(1)?), row.get(2)?, row.get(3)?, row.get(4)?),
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(facts(&content)
        .into_iter()
        .map(|text| {
            let (author, author_bot, message, at) =
                origins.get(&fingerprint(text)).cloned().unwrap_or((Author::Computer, None, None, updated));
            Fact { text: text.to_owned(), author, author_bot, message, at }
        })
        .collect())
}

/// Says who wrote a fact of a layer's `MEMORY.md`, and when, over what the change that brought it in recorded.
pub fn set_origin(conn: &Connection, bot: Option<&str>, fact: &Fact) -> Result<()> {
    conn.execute(
        "INSERT INTO memory_facts (bot, fingerprint, author, author_bot, message, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (bot, fingerprint) DO UPDATE SET author = ?3, author_bot = ?4, message = ?5, at = ?6",
        params![key(bot), fingerprint(&fact.text), author_name(fact.author), fact.author_bot, fact.message, fact.at],
    )?;
    Ok(())
}

/// Before a change to `MEMORY.md` is recorded: the facts it adds are the change's writer's, and the ones it removes
/// are no one's.
pub(super) fn track(conn: &Connection, change: &Change<'_>, now: i64) -> Result<()> {
    let before = current(conn, change.bot, MAIN)?.unwrap_or_default();
    let before: HashSet<String> = facts(&before).into_iter().map(fingerprint).collect();
    let after: HashSet<String> = change.content.map(facts).unwrap_or_default().into_iter().map(fingerprint).collect();
    for gone in before.difference(&after) {
        conn.execute("DELETE FROM memory_facts WHERE bot = ?1 AND fingerprint = ?2", params![key(change.bot), gone])?;
    }
    for new in after.difference(&before) {
        conn.execute(
            "INSERT OR IGNORE INTO memory_facts (bot, fingerprint, author, author_bot, message, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![key(change.bot), new, author_name(change.author), change.author_bot, change.message, now],
        )?;
    }
    Ok(())
}

/// Whether a bot may make a change to a file of a layer (`before` and `after`: `None` when there is no file). In a
/// turn the user started in its own chat (`asked`) it may change anything. Otherwise it may only add lines, and
/// none the user had a bot forget.
pub fn admit(
    conn: &Connection,
    bot: Option<&str>,
    before: Option<&str>,
    after: Option<&str>,
    asked: bool,
) -> Result<()> {
    if asked {
        return Ok(());
    }
    if !gained(after, before).is_empty() {
        return Err(Refusal::NotAsked.into());
    }
    // A forgotten fact can't come back as a heading or inside frontmatter either.
    let bare = |line: &str| fingerprint(line.trim_start_matches('#'));
    let had: HashSet<String> = before.into_iter().flat_map(every_line).map(bare).collect();
    for line in after.into_iter().flat_map(every_line).map(|line| line.trim_start_matches('#')) {
        if !had.contains(&fingerprint(line)) && store::forgotten(conn, bot, line)? {
            return Err(Refusal::Forgotten.into());
        }
    }
    Ok(())
}

/// The lines `to` has that `from` doesn't, counting repeats (`None`: no file).
pub(super) fn gained<'a>(from: Option<&str>, to: Option<&'a str>) -> Vec<&'a str> {
    let mut had: HashMap<&str, usize> = HashMap::new();
    for line in from.into_iter().flat_map(every_line) {
        *had.entry(line).or_default() += 1;
    }
    to.into_iter()
        .flat_map(every_line)
        .filter(|line| match had.get_mut(line) {
            Some(n) if *n > 0 => {
                *n -= 1;
                false
            }
            _ => true,
        })
        .collect()
}

/// Every non-empty line, frontmatter and headings included, trimmed.
fn every_line(content: &str) -> impl Iterator<Item = &str> {
    content.lines().map(str::trim).filter(|line| !line.is_empty())
}

/// Before an `asked` change made in a layer is recorded: what it took out of the layer's files the user had the bot
/// forget, with the messages those facts were learned from, and what it put back may stay.
pub fn settle(
    conn: &Connection,
    bot: Option<&str>,
    name: &str,
    before: Option<&str>,
    after: Option<&str>,
    asked: bool,
) -> Result<()> {
    if !asked {
        return Ok(());
    }
    for fact in added(before, after) {
        store::unbury(conn, bot, fact)?;
    }
    let kept: HashSet<String> = after.map(facts).unwrap_or_default().into_iter().map(fingerprint).collect();
    let elsewhere = elsewhere(conn, bot, name)?;
    let gone: Vec<&str> = before
        .map(facts)
        .unwrap_or_default()
        .into_iter()
        .filter(|fact| !kept.contains(&fingerprint(fact)) && !elsewhere.contains(&fingerprint(fact)))
        .collect();
    if gone.is_empty() {
        return Ok(());
    }
    let mut messages = Vec::new();
    if name == MAIN {
        let mut stmt = conn.prepare("SELECT message FROM memory_facts WHERE bot = ?1 AND fingerprint = ?2")?;
        for fact in &gone {
            let message = stmt.query_row(params![key(bot), fingerprint(fact)], |row| row.get::<_, Option<String>>(0));
            messages.extend(message.optional()?.flatten());
        }
    }
    store::bury(conn, bot, gone, messages.iter().map(String::as_str))
}

/// The facts `after` has that `before` didn't.
fn added<'a>(before: Option<&str>, after: Option<&'a str>) -> Vec<&'a str> {
    let had: HashSet<String> = before.map(facts).unwrap_or_default().into_iter().map(fingerprint).collect();
    after.map(facts).unwrap_or_default().into_iter().filter(|fact| !had.contains(&fingerprint(fact))).collect()
}

/// The facts the layer's other live files hold, so a fact moved between them isn't taken as forgotten.
fn elsewhere(conn: &Connection, bot: Option<&str>, name: &str) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare(
        "SELECT v.content FROM memory_files f JOIN memory_file_versions v ON v.id = f.version
         WHERE f.bot = ?1 AND f.name != ?2 AND f.hash != ''",
    )?;
    let contents = stmt.query_map(params![key(bot), name], |row| row.get::<_, Option<String>>(0))?;
    let mut found = HashSet::new();
    for content in contents {
        found.extend(facts(&content?.unwrap_or_default()).into_iter().map(fingerprint));
    }
    Ok(found)
}
