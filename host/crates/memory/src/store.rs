//! What memory keeps beside its files: tombstones of what the user had the bot forget, marks dreaming leaves for
//! itself, and the crate's error type.

use rusqlite::{Connection, OptionalExtension, params};

use crate::check::Refusal;

#[derive(Debug)]
pub enum Error {
    Refused(Refusal),
    NotFound,
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(refusal) => refusal.fmt(f),
            Self::NotFound => f.write_str("No such memory."),
            Self::Sqlite(e) => write!(f, "memory store: {e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sqlite(e) => Some(e),
            // Display already says why: as a source too, the reason would print twice in a chain.
            Self::Refused(_) | Self::NotFound => None,
        }
    }
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

impl From<Refusal> for Error {
    fn from(refusal: Refusal) -> Self {
        Self::Refused(refusal)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Keeps lines the user had a bot forget out of a layer (`None`: the shared one), and the messages they were learned
/// from: no bot outside the user's own turn, and no dream, writes them again or draws memory from those messages.
pub fn bury<'a>(
    conn: &Connection,
    bot: Option<&str>,
    lines: impl IntoIterator<Item = &'a str>,
    messages: impl IntoIterator<Item = &'a str>,
) -> Result<()> {
    let marks = lines.into_iter().map(fingerprint).chain(messages.into_iter().map(cited));
    mark(conn, bot, marks)
}

/// The user asked for a forgotten line again, in their own turn: it may come back.
pub fn unbury(conn: &Connection, bot: Option<&str>, line: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM memory_tombstones WHERE scope = ?1 AND hash = ?2",
        params![scope(bot), fingerprint(line)],
    )?;
    Ok(())
}

/// Whether the user had a bot forget `line` in a layer.
pub fn forgotten(conn: &Connection, bot: Option<&str>, line: &str) -> Result<bool> {
    marked(conn, bot, &fingerprint(line))
}

/// Whether memory learned from `message` was forgotten in a layer `bot` uses: the message is spent for both, so a
/// fact forgotten in one layer doesn't come back in the other.
pub fn spent(conn: &Connection, bot: &str, message: &str) -> Result<bool> {
    Ok(marked(conn, None, &cited(message))? || marked(conn, Some(bot), &cited(message))?)
}

/// Leaves marks in a layer: tombstones, or notes dreaming keeps for itself.
pub(crate) fn mark(conn: &Connection, bot: Option<&str>, marks: impl Iterator<Item = String>) -> Result<()> {
    for mark in marks {
        conn.execute(
            "INSERT OR IGNORE INTO memory_tombstones (scope, hash) VALUES (?1, ?2)",
            params![scope(bot), mark],
        )?;
    }
    Ok(())
}

pub(crate) fn marked(conn: &Connection, bot: Option<&str>, mark: &str) -> Result<bool> {
    Ok(conn
        .query_row("SELECT 1 FROM memory_tombstones WHERE scope = ?1 AND hash = ?2", params![scope(bot), mark], |_| {
            Ok(())
        })
        .optional()?
        .is_some())
}

/// Removes what was kept for a deleted bot: its own files, its tombstones and its dream log. The shared memory
/// stays, since the user may still want it.
pub fn drop_bot(conn: &Connection, bot: &str) -> Result<()> {
    crate::files::forget_layer(conn, Some(bot))?;
    conn.execute("DELETE FROM memory_tombstones WHERE scope = ?1", [scope(Some(bot))])?;
    for table in ["memory_dream_lines", "memory_dream_files"] {
        conn.execute(
            &format!("DELETE FROM {table} WHERE dream IN (SELECT id FROM memory_dreams WHERE bot = ?1)"),
            [bot],
        )?;
    }
    conn.execute("DELETE FROM memory_dreams WHERE bot = ?1", [bot])?;
    Ok(())
}

/// A tombstone for a message rather than a line: no bot or dream may draw memory for the layer from it again.
fn cited(message: &str) -> String {
    format!("message:{message}")
}

fn scope(bot: Option<&str>) -> String {
    bot.map_or_else(|| "yours".to_owned(), |bot| format!("bot:{bot}"))
}

/// A line without its list marker (`- `, `* `, `+ `).
pub(crate) fn unmarked(line: &str) -> &str {
    let line = line.trim();
    ["- ", "* ", "+ "].iter().find_map(|marker| line.strip_prefix(marker)).map_or(line, str::trim_start)
}

/// Only a hash of a forgotten line is kept: its list marker, case, spacing and trailing punctuation don't make it new.
pub(crate) fn fingerprint(text: &str) -> String {
    let text = unmarked(text);
    let normal: String =
        text.split_whitespace().collect::<Vec<_>>().join(" ").trim_end_matches(['.', '。', '!', '！']).to_lowercase();
    crate::files::hash(&normal)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn a_forgotten_line_stays_forgotten_in_its_layer_until_asked_for_again() {
        let conn = db();
        bury(&conn, Some("a"), ["- Lives in Taipei"], ["m1"]).unwrap();
        assert!(forgotten(&conn, Some("a"), "lives in  taipei.").unwrap(), "marker, case and spacing don't matter");
        assert!(!forgotten(&conn, None, "Lives in Taipei").unwrap(), "per layer");
        assert!(spent(&conn, "a", "m1").unwrap());
        assert!(!spent(&conn, "b", "m1").unwrap());
        unbury(&conn, Some("a"), "Lives in Taipei").unwrap();
        assert!(!forgotten(&conn, Some("a"), "Lives in Taipei").unwrap());
    }
}
