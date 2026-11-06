//! OpenCode Go's plan, added up from this computer's opencode database.
//! The caps are the plan's ($12 / 5 h, $30 / week, $60 / month). This is an
//! estimate of what this machine spent, not the dashboard's own number.

use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};

use super::Window;

const CAP_5H: f64 = 12.0;
const CAP_WEEK: f64 = 30.0;
const CAP_MONTH: f64 = 60.0;

const QUERY_BOTH: &str = include_str!("opencode-both.sql");
const QUERY_SESSION: &str = include_str!("opencode-session.sql");
const QUERY_MESSAGE: &str = include_str!("opencode-message.sql");

pub(super) fn collect() -> Option<Vec<Window>> {
    from_db(&db_path()?)
}

fn db_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| dirs::home_dir().map(|home| home.join(".local/share")))?;
    let path = base.join("opencode/opencode.db");
    path.is_file().then_some(path)
}

fn from_db(path: &Path) -> Option<Vec<Window>> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    conn.busy_timeout(std::time::Duration::from_millis(200)).ok()?;
    let query = if has_table(&conn, "message") && has_table(&conn, "session_message") {
        QUERY_BOTH
    } else if has_table(&conn, "session_message") {
        QUERY_SESSION
    } else if has_table(&conn, "message") {
        QUERY_MESSAGE
    } else {
        return None;
    };
    let row = conn
        .query_row(query, [], |row| {
            Ok((
                integer(row, 0),
                number(row, 1),
                integer(row, 2),
                number(row, 3),
                integer(row, 4),
                number(row, 5),
                integer(row, 7),
                integer(row, 8),
            ))
        })
        .ok()?;
    let (now_ms, five_hour, oldest, weekly, week_end, monthly, month_end, go_messages) = row;
    if go_messages.unwrap_or(0) == 0 {
        return None;
    }
    let now_ms = now_ms?;
    let five_reset = oldest.unwrap_or(now_ms).saturating_add(18_000_000);
    Some(vec![
        Window::new("five_hour", "5-hour", share(five_hour?, CAP_5H), Some(five_reset)),
        Window::new("seven_day", "Weekly", share(weekly?, CAP_WEEK), week_end),
        Window::new("monthly", "Monthly", share(monthly?, CAP_MONTH), month_end),
    ])
}

fn share(spent: f64, cap: f64) -> f64 {
    if cap <= 0.0 { 0.0 } else { (spent / cap * 100.0).clamp(0.0, 100.0) }
}

fn has_table(conn: &Connection, name: &str) -> bool {
    conn.query_row("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1", [name], |_| Ok(())).is_ok()
}

fn number(row: &rusqlite::Row<'_>, index: usize) -> Option<f64> {
    match row.get_ref(index).ok()? {
        #[allow(clippy::cast_precision_loss)]
        rusqlite::types::ValueRef::Integer(n) => Some(n as f64),
        rusqlite::types::ValueRef::Real(n) => Some(n),
        rusqlite::types::ValueRef::Text(bytes) => std::str::from_utf8(bytes).ok()?.parse().ok(),
        _ => None,
    }
}

fn integer(row: &rusqlite::Row<'_>, index: usize) -> Option<i64> {
    match row.get_ref(index).ok()? {
        rusqlite::types::ValueRef::Integer(n) => Some(n),
        rusqlite::types::ValueRef::Real(n) => super::finite_i64(n),
        rusqlite::types::ValueRef::Null | rusqlite::types::ValueRef::Blob(_) => None,
        rusqlite::types::ValueRef::Text(bytes) => std::str::from_utf8(bytes).ok()?.parse().ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_go_message_fills_the_three_windows() {
        let path = std::env::temp_dir().join(format!("codync-opencode-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let conn = Connection::open(&path).unwrap();
        conn.execute("CREATE TABLE session_message (id TEXT, data TEXT, type TEXT, time_created INTEGER)", []).unwrap();
        let millis = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
        let now = i64::try_from(millis).unwrap();
        let data = format!(r#"{{"cost":6,"time":{{"created":{now}}},"model":{{"providerID":"opencode-go"}}}}"#);
        conn.execute(
            "INSERT INTO session_message (id, data, type, time_created) VALUES ('m', ?1, 'assistant', ?2)",
            rusqlite::params![data, now],
        )
        .unwrap();
        drop(conn);
        let windows = from_db(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!((windows[0].percent - 50.0).abs() < 0.01, "5-hour {}", windows[0].percent);
        assert!((windows[1].percent - 20.0).abs() < 0.01, "week {}", windows[1].percent);
        assert!((windows[2].percent - 10.0).abs() < 0.01, "month {}", windows[2].percent);
        assert!(windows[0].resets_at.is_some());
    }

    #[test]
    fn another_providers_messages_are_not_a_go_plan() {
        let path = std::env::temp_dir().join(format!("codync-opencode-other-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let conn = Connection::open(&path).unwrap();
        conn.execute("CREATE TABLE session_message (id TEXT, data TEXT, type TEXT, time_created INTEGER)", []).unwrap();
        let millis = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
        let now = i64::try_from(millis).unwrap();
        let data = format!(r#"{{"cost":6,"time":{{"created":{now}}},"model":{{"providerID":"anthropic"}}}}"#);
        conn.execute(
            "INSERT INTO session_message (id, data, type, time_created) VALUES ('m', ?1, 'assistant', ?2)",
            rusqlite::params![data, now],
        )
        .unwrap();
        drop(conn);
        let windows = from_db(&path);
        let _ = std::fs::remove_file(&path);
        assert!(windows.is_none());
    }
}
