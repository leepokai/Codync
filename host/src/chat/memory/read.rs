//! Bounded read-only queries over Engram's own indexes. Never creates or changes schema.

use super::facts::{Fact, Kind};
use anyhow::{Result, bail};
use rusqlite::{Connection, Row, types::Value};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

const COLUMNS: &str = "id, content, created_at, title, type, scope, project, topic_key,
    session_id, pinned, review_after, revision_count, tool_name";
const ORDER: &str = "pinned DESC, (scope = 'personal') DESC, created_at DESC, id DESC";

pub struct Page {
    pub facts: Vec<Fact>,
    pub total: usize,
}

pub fn browse(db: &mut Connection, query: &str, filter: &str, limit: usize, offset: usize) -> Result<Page> {
    let (condition, mut args) = conditions(query, filter)?;
    let tx = db.transaction()?;
    let total: i64 = tx.query_row(
        &format!("SELECT COUNT(*) FROM observations WHERE {condition}"),
        rusqlite::params_from_iter(&args),
        |r| r.get(0),
    )?;
    args.push(Value::Integer(i64::try_from(limit).unwrap_or(i64::MAX)));
    args.push(Value::Integer(i64::try_from(offset)?));
    let facts = select(&tx, &format!("{condition} ORDER BY {ORDER} LIMIT ? OFFSET ?"), args)?;
    tx.commit()?;
    Ok(Page { facts, total: usize::try_from(total)? })
}

pub fn list(db: &Connection, filter: &str, limit: usize) -> Result<Vec<Fact>> {
    let (condition, mut args) = conditions("", filter)?;
    args.push(Value::Integer(i64::try_from(limit).unwrap_or(i64::MAX)));
    select(db, &format!("{condition} ORDER BY {ORDER} LIMIT ?"), args)
}

pub fn find(db: &Connection, id: i64) -> Result<Option<Fact>> {
    Ok(select(db, "deleted_at IS NULL AND id = ?", vec![Value::Integer(id)])?.pop())
}

/// Recall older relevant facts using Engram's FTS index instead of scanning a recent window.
pub fn relevant(db: &Connection, text: &str, limit: usize) -> Result<Vec<Fact>> {
    let mut terms = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for word in text.split(|c: char| !c.is_alphanumeric()) {
        let chars: Vec<char> = word.to_lowercase().chars().collect();
        let candidates = if chars.iter().any(|c| ('\u{3400}'..='\u{9fff}').contains(c)) {
            chars.windows(3).map(|w| w.iter().collect::<String>()).collect::<Vec<_>>()
        } else if chars.len() >= 3 {
            vec![chars.iter().collect()]
        } else {
            Vec::new()
        };
        for term in candidates {
            if seen.insert(term.clone()) {
                terms.push(format!("\"{}\"", term.replace('"', "\"\"")));
            }
            if terms.len() == 64 {
                break;
            }
        }
        if terms.len() == 64 {
            break;
        }
    }
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    select(
        db,
        "deleted_at IS NULL AND id IN (SELECT rowid FROM observations_fts WHERE observations_fts MATCH ? ORDER BY rank LIMIT ?)",
        vec![Value::Text(terms.join(" OR ")), Value::Integer(i64::try_from(limit)?)],
    )
}

pub fn same_content(db: &Connection, content: &str) -> Result<Option<Fact>> {
    Ok(select(db, "deleted_at IS NULL AND normalized_hash = ? LIMIT 1", vec![Value::Text(content_hash(content))])?
        .pop())
}

pub fn content_hash(content: &str) -> String {
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    let mut hash = String::with_capacity(64);
    for b in Sha256::digest(normalized.as_bytes()) {
        let _ = write!(hash, "{b:02x}");
    }
    hash
}

fn select(db: &Connection, condition: &str, args: Vec<Value>) -> Result<Vec<Fact>> {
    let mut query = db.prepare(&format!("SELECT {COLUMNS} FROM observations WHERE {condition}"))?;
    Ok(query.query_map(rusqlite::params_from_iter(args), fact)?.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn conditions(query: &str, filter: &str) -> Result<(String, Vec<Value>)> {
    let mut clauses = vec!["deleted_at IS NULL".to_owned()];
    match filter {
        "profile" => clauses.push("scope = 'personal'".into()),
        "log" => clauses.push("scope != 'personal'".into()),
        "pinned" => clauses.push("pinned = 1".into()),
        "review" => clauses.push("datetime(review_after) <= datetime('now')".into()),
        "all" => {}
        _ => bail!("unknown memory filter"),
    }
    let mut args = Vec::new();
    let mut indexed = Vec::new();
    // Engram uses a trigram FTS5 index. One/two-character terms need literal LIKE;
    // escaping wildcards keeps punctuation and short CJK searches predictable.
    for term in query.split_whitespace().take(16) {
        if term.chars().count() >= 3 {
            indexed.push(format!("\"{}\"", term.replace('"', "\"\"")));
        } else {
            let escaped = term.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
            clauses.push("(title || ' ' || content || ' ' || COALESCE(topic_key, '')) LIKE ? ESCAPE '\\'".into());
            args.push(Value::Text(format!("%{escaped}%")));
        }
    }
    if !indexed.is_empty() {
        clauses.push("id IN (SELECT rowid FROM observations_fts WHERE observations_fts MATCH ?)".into());
        args.push(Value::Text(indexed.join(" AND ")));
    }
    Ok((clauses.join(" AND "), args))
}

fn fact(row: &Row<'_>) -> rusqlite::Result<Fact> {
    let scope: String = row.get(5)?;
    let date: String = row.get(2)?;
    let created_at = super::facts::parse_timestamp(&date)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, e.into()))?;
    Ok(Fact {
        id: row.get::<_, i64>(0)?.to_string(),
        content: row.get(1)?,
        created_at,
        kind: if scope == "personal" { Kind::Profile } else { Kind::Log },
        title: row.get(3)?,
        memory_type: row.get(4)?,
        scope,
        project: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
        topic_key: row.get(7)?,
        session_id: row.get(8)?,
        pinned: row.get(9)?,
        review_after: row.get(10)?,
        revision_count: row.get(11)?,
        source: row.get(12)?,
    })
}
