//! Cursor's usage summary. Cursor has no usage command: the token is the one
//! the Cursor app saved in `state.vscdb`, and the summary is
//! `GET https://cursor.com/api/usage-summary`. The token is never logged.

use base64::Engine as _;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

use super::Window;

pub(super) async fn collect() -> Option<Vec<Window>> {
    let token = tokio::task::spawn_blocking(read_token).await.ok().flatten()?;
    let sub = jwt_sub(&token)?;
    let cookie = cookie(&sub, &token)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()?;
    let response = client
        .get("https://cursor.com/api/usage-summary")
        .header("Cookie", cookie)
        .header("Accept", "application/json")
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        tracing::debug!(status = %response.status(), "cursor usage summary was refused");
        return None;
    }
    let body: Value = response.json().await.ok()?;
    Some(parse(&body))
}

fn db_path() -> Option<PathBuf> {
    let path = dirs::config_dir()?.join("Cursor/User/globalStorage/state.vscdb");
    path.is_file().then_some(path)
}

fn read_token() -> Option<String> {
    let path = db_path()?;
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    conn.busy_timeout(Duration::from_millis(200)).ok()?;
    let raw =
        conn.query_row("SELECT value FROM ItemTable WHERE key = 'cursorAuth/accessToken'", [], stored_text).ok()?;
    let token = token_text(&raw);
    (!token.is_empty()).then_some(token)
}

/// `state.vscdb` stores the token as text or as a blob.
fn stored_text(row: &rusqlite::Row<'_>) -> rusqlite::Result<String> {
    let (rusqlite::types::ValueRef::Text(bytes) | rusqlite::types::ValueRef::Blob(bytes)) = row.get_ref(0)? else {
        return Ok(String::new());
    };
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

/// The column is sometimes the raw token and sometimes a JSON string.
fn token_text(raw: &str) -> String {
    let trimmed = raw.trim();
    serde_json::from_str::<String>(trimmed).unwrap_or_else(|_| trimmed.to_owned())
}

fn jwt_sub(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).or_else(|_| URL_SAFE.decode(payload)).ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    let sub = value["sub"].as_str().filter(|s| !s.is_empty())?;
    Some(sub.to_owned())
}

fn cookie(sub: &str, token: &str) -> Option<String> {
    if sub.contains([';', ' ', '\r', '\n']) || token.contains([';', ' ', '\r', '\n']) {
        return None;
    }
    Some(format!("WorkosCursorSessionToken={sub}::{token}"))
}

fn parse(body: &Value) -> Vec<Window> {
    let mut windows = Vec::new();
    let resets = body["billingCycleEnd"].as_str().and_then(iso_ms);
    let plan = &body["individualUsage"]["plan"];
    if body["isUnlimited"].as_bool() == Some(true) {
        windows.push(Window::note("unlimited", "Monthly", "Unlimited"));
    } else if plan["enabled"].as_bool().unwrap_or(true)
        && let Some(percent) = non_negative(plan["totalPercentUsed"].as_f64())
    {
        windows.push(Window::new("monthly", "Monthly", percent, resets));
    }
    if let Some(percent) = non_negative(plan["autoPercentUsed"].as_f64()) {
        windows.push(Window::new("auto", "Auto", percent, resets));
    }
    if let Some(percent) = non_negative(plan["apiPercentUsed"].as_f64()) {
        windows.push(Window::new("api", "API", percent, resets));
    }
    if let Some(window) = spend("on_demand", "On-Demand", &body["individualUsage"]["onDemand"], resets) {
        windows.push(window);
    }
    if body["limitType"].as_str() == Some("team")
        && let Some(window) = spend("team", "Team", &body["teamUsage"]["onDemand"], resets)
    {
        windows.push(window);
    }
    windows
}

fn spend(id: &str, label: &str, node: &Value, resets: Option<i64>) -> Option<Window> {
    if node["enabled"].as_bool() == Some(false) {
        return None;
    }
    let limit = node["limit"].as_f64().filter(|n| *n > 0.0)?;
    let used = node["used"].as_f64().unwrap_or(0.0);
    Some(Window::new(id, label, (used / limit * 100.0).clamp(0.0, 100.0), resets))
}

fn non_negative(value: Option<f64>) -> Option<f64> {
    value.filter(|n| n.is_finite() && *n >= 0.0).map(|n| n.clamp(0.0, 100.0))
}

fn iso_ms(text: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(text).ok().map(|t| t.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_summary_keeps_each_percent_the_api_reports() {
        let body = json!({
            "limitType": "team",
            "isUnlimited": false,
            "billingCycleEnd": "2026-04-01T00:00:00.000Z",
            "individualUsage": {
                "plan": {"enabled": true, "totalPercentUsed": 28.32, "autoPercentUsed": 23.05, "apiPercentUsed": 63.44},
                "onDemand": {"enabled": false, "used": 0, "limit": null}
            },
            "teamUsage": {"onDemand": {"enabled": true, "used": 0, "limit": 10000}}
        });
        let w = parse(&body);
        assert_eq!(w[0].label, "Monthly");
        assert!((w[0].percent - 28.32).abs() < 1e-9);
        assert!(w[0].resets_at.is_some());
        assert_eq!(w[1].label, "Auto");
        assert_eq!(w[2].label, "API");
        assert_eq!(w[3].label, "Team");
        assert!(w[3].percent.abs() < f64::EPSILON);
        assert_eq!(w.len(), 4, "a disabled on-demand row is absent");
    }

    #[test]
    fn an_unlimited_plan_is_a_note() {
        let w = parse(&json!({"isUnlimited": true, "individualUsage": {"plan": {"totalPercentUsed": 10}}}));
        assert_eq!(w[0].text.as_deref(), Some("Unlimited"));
        assert!(w.iter().all(|window| window.id != "monthly" || window.text.is_some()));
    }

    #[test]
    fn the_subject_comes_out_of_the_token_payload() {
        let payload = URL_SAFE_NO_PAD.encode(br#"{"sub":"user_123"}"#);
        let token = format!("eyJhbGciOiJub25lIn0.{payload}.sig");
        assert_eq!(jwt_sub(&token).as_deref(), Some("user_123"));
        assert_eq!(token_text("\"abc.def.ghi\""), "abc.def.ghi");
        assert!(cookie("user_123", &token).unwrap().starts_with("WorkosCursorSessionToken=user_123::"));
        assert!(cookie("a;b", "t").is_none());
    }

    #[test]
    fn a_database_value_is_the_saved_token() {
        let path = std::env::temp_dir().join(format!("codync-cursor-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let conn = Connection::open(&path).unwrap();
        conn.execute("CREATE TABLE ItemTable (key TEXT, value BLOB)", []).unwrap();
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES ('cursorAuth/accessToken', ?1)",
            [rusqlite::types::Value::Blob(br#""tok""#.to_vec())],
        )
        .unwrap();
        drop(conn);
        let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let raw = conn
            .query_row("SELECT value FROM ItemTable WHERE key = 'cursorAuth/accessToken'", [], stored_text)
            .unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(token_text(&raw), "tok");
    }
}
