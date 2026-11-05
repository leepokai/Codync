//! OpenCode Go's plan windows, from the console's own usage endpoint.
//!
//! OpenCode reports no quota over ACP. Its console answers
//! `GET https://opencode.ai/zen/go/v1/usage` with `{usage: {rolling, weekly,
//! monthly}}` for the credential the CLI is signed in with: the OpenCode 2
//! database's active credential (an OAuth session or a console key), else the
//! `opencode-go` key in `auth.json`, else `OPENCODE_API_KEY`. Keys are never
//! logged.

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::Window;

const USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";
const CONSOLE_URL: &str = "https://opencode.ai/console";

pub(super) async fn collect() -> Option<Vec<Window>> {
    let keys = tokio::task::spawn_blocking(credentials).await.ok()?;
    for key in keys {
        if let Some(windows) = fetch(&key).await {
            return Some(windows);
        }
    }
    None
}

/// The console key or OAuth token the CLI uses, in its own order.
fn credentials() -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    if let Some(key) =
        std::env::var("OPENCODE_API_KEY").ok().map(|key| key.trim().to_owned()).filter(|key| !key.is_empty())
    {
        keys.push(key);
    }
    for dir in data_dirs() {
        if let Some(key) = auth_file_key(&dir.join("auth.json")) {
            keys.push(key);
        }
        if let Some(key) = database_key(&dir) {
            keys.push(key);
        }
    }
    let mut seen = std::collections::HashSet::new();
    keys.retain(|key| seen.insert(key.clone()));
    keys
}

async fn fetch(key: &str) -> Option<Vec<Window>> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).build().ok()?;
    let response = client.get(USAGE_URL).bearer_auth(key).header("Accept", "application/json").send().await.ok()?;
    if matches!(response.status().as_u16(), 401 | 403) {
        return None;
    }
    if !response.status().is_success() {
        return None;
    }
    let body: Value = response.json().await.ok()?;
    let windows = parse_usage(&body);
    (!windows.is_empty()).then_some(windows)
}

/// The console's reading: one window per bucket, each with a percent and the
/// instant it resets.
pub fn parse_usage(body: &Value) -> Vec<Window> {
    [("rolling", "five_hour", "5-hour"), ("weekly", "seven_day", "Weekly"), ("monthly", "monthly", "Monthly")]
        .into_iter()
        .filter_map(|(bucket, id, label)| {
            let window = &body["usage"][bucket];
            let percent = window["percent"].as_f64().filter(|p| p.is_finite())?;
            let resets = window["resetsAt"].as_str().and_then(iso_ms);
            Some(Window::new(id, label, percent.clamp(0.0, 100.0), resets))
        })
        .collect()
}

fn iso_ms(text: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(text).ok().map(|t| t.timestamp_millis())
}

/// The CLI's data folders: XDG's (or `~/.local/share`), and Windows' `APPDATA`.
fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let base = match std::env::var_os("XDG_DATA_HOME").filter(|path| !path.as_os_str().is_empty()) {
        Some(raw) => Some(PathBuf::from(raw)),
        None => dirs::home_dir().map(|home| home.join(".local").join("share")),
    };
    if let Some(base) = base {
        dirs.push(base.join("opencode"));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("opencode"));
    }
    dirs.dedup();
    dirs
}

/// OpenCode 1.x keeps `opencode-go` in `auth.json`.
fn auth_file_key(path: &Path) -> Option<String> {
    let value: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let key = value["opencode-go"]["key"].as_str()?.trim();
    (!key.is_empty()).then(|| key.to_owned())
}

/// OpenCode 2 keeps the active console credential in its database: an OAuth
/// session (`access`) or a console key (`key`). Another server is not the
/// console this endpoint belongs to.
fn database_key(dir: &Path) -> Option<String> {
    let path = database_path(dir);
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    conn.busy_timeout(Duration::from_millis(200)).ok()?;
    let stored: String = conn
        .query_row(
            "SELECT value FROM credential
             WHERE integration_id = 'opencode' AND active IS NOT 0
             ORDER BY active DESC, time_created DESC, id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok()?;
    let value: Value = serde_json::from_str(&stored).ok()?;
    let server = value["metadata"]["server"].as_str();
    if server.is_some_and(|server| server.trim_end_matches('/') != CONSOLE_URL) {
        return None;
    }
    let key = value["key"].as_str().or_else(|| value["access"].as_str())?.trim();
    (!key.is_empty()).then(|| key.to_owned())
}

fn database_path(dir: &Path) -> PathBuf {
    match std::env::var_os("OPENCODE_DB").filter(|path| !path.as_os_str().is_empty()) {
        Some(raw) => {
            let path = PathBuf::from(raw);
            if path.is_absolute() { path } else { dir.join(path) }
        }
        None => dir.join("opencode.db"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_console_reading_is_three_windows() {
        let w = parse_usage(&json!({"usage": {
            "rolling": {"status": "ok", "percent": 2, "resetsAt": "2026-11-05T23:58:15.000Z"},
            "weekly": {"status": "ok", "percent": 1, "resetsAt": "2026-11-09T00:00:00.000Z"},
            "monthly": {"status": "ok", "percent": 0, "resetsAt": "2026-12-05T17:43:32.000Z"}
        }}));
        assert_eq!(w.len(), 3);
        assert_eq!((w[0].id.as_str(), w[0].percent), ("five_hour", 2.0));
        assert_eq!((w[1].id.as_str(), w[1].percent), ("seven_day", 1.0));
        assert_eq!((w[2].id.as_str(), w[2].percent), ("monthly", 0.0));
        assert!(w.iter().all(|window| window.resets_at.is_some()));
    }

    #[test]
    fn a_missing_or_malformed_window_is_silence_not_zero() {
        let w = parse_usage(&json!({"usage": {"rolling": {"percent": "lots"}, "weekly": {}}}));
        assert_eq!(w.len(), 0);
        assert_eq!(parse_usage(&json!({})).len(), 0);
    }

    #[test]
    fn an_active_database_credential_names_its_key_and_account() {
        let dir = std::env::temp_dir().join(format!("codync-opencode-cred-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("opencode.db");
        let _ = std::fs::remove_file(&path);
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "CREATE TABLE credential (id TEXT, integration_id TEXT, value TEXT, active INTEGER, time_created INTEGER)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO credential (id, integration_id, value, active, time_created) VALUES ('c', 'opencode', ?1, 1, 1)",
            [r#"{"type":"oauth","access":"tok","metadata":{"server":"https://opencode.ai/console","email":"me@example.com"}}"#],
        )
        .unwrap();
        drop(conn);
        assert_eq!(database_key(&dir).as_deref(), Some("tok"));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn another_server_and_an_inactive_credential_have_no_key() {
        let dir = std::env::temp_dir().join(format!("codync-opencode-other-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("opencode.db");
        let _ = std::fs::remove_file(&path);
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "CREATE TABLE credential (id TEXT, integration_id TEXT, value TEXT, active INTEGER, time_created INTEGER)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO credential (id, integration_id, value, active, time_created) VALUES ('c', 'opencode', ?1, 1, 1)",
            [r#"{"type":"key","key":"tok","metadata":{"server":"https://example.com"}}"#],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO credential (id, integration_id, value, active, time_created) VALUES ('d', 'opencode', ?1, 0, 2)",
            [r#"{"type":"key","key":"old"}"#],
        )
        .unwrap();
        drop(conn);
        assert!(database_key(&dir).is_none());
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }
}
