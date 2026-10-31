//! Cursor's plan usage, using the app's `state.vscdb` or CLI's `auth.json` login.
//! Unified plans use the dashboard service's weekly allowance. The website's
//! usage summary reports only the legacy monthly pools. The token is never logged.

use base64::Engine as _;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::Window;

/// Whether the dashboard last named a unified plan, so a failed poll keeps its weekly reading.
static LAST_UNIFIED: AtomicBool = AtomicBool::new(false);

/// Which reading a poll reports.
#[derive(Debug, PartialEq)]
enum Route {
    /// The dashboard's weekly window, or `None` to keep the last one.
    Unified(Option<Vec<Window>>),
    /// The legacy plan's usage summary.
    Summary,
}

pub(super) async fn collect() -> Option<Vec<Window>> {
    let token = tokio::task::spawn_blocking(read_token).await.ok().flatten()?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()?;

    let status_request = client
        .post("https://api2.cursor.sh/aiserver.v1.DashboardService/GetSandUsageStatus")
        .bearer_auth(&token)
        .header("Connect-Protocol-Version", "1")
        .json(&serde_json::json!({}));
    let status = read_response(status_request).await;
    let route = choose(status.as_ref(), LAST_UNIFIED.load(Ordering::Relaxed));
    LAST_UNIFIED.store(matches!(route, Route::Unified(_)), Ordering::Relaxed);
    if let Route::Unified(windows) = route {
        return windows;
    }

    let summary_request = client
        .get("https://cursor.com/api/usage-summary")
        .header("Cookie", session_cookie(&token)?)
        .header("Accept", "application/json");
    let summary = read_response(summary_request).await?;
    Some(parse(&summary))
}

/// The dashboard service is internal: when it fails, a legacy plan still reads its summary,
/// while a unified plan keeps its last weekly reading (its summary pools read zero).
fn choose(status: Option<&Value>, last_unified: bool) -> Route {
    let unified = status.map_or(last_unified, |s| s["isUnified"].as_bool() == Some(true));
    if unified { Route::Unified(status.and_then(parse_unified)) } else { Route::Summary }
}

async fn read_response(request: reqwest::RequestBuilder) -> Option<Value> {
    let response = request.send().await.ok()?;
    if !response.status().is_success() {
        tracing::debug!(status = %response.status(), "cursor usage request was refused");
        return None;
    }
    response.json::<Value>().await.ok()
}

fn parse_unified(status: &Value) -> Option<Vec<Window>> {
    let percent = non_negative(status["usagePercent"].as_f64())?;
    let resets = status["nextResetTimestampUtc"].as_str().and_then(iso_ms);
    Some(vec![Window::new("weekly", "Weekly", percent, resets)])
}

fn cli_auth_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    return Some(dirs::home_dir()?.join(".cursor/auth.json"));
    #[cfg(windows)]
    return Some(dirs::config_dir()?.join("Cursor/auth.json"));
    #[cfg(not(any(target_os = "macos", windows)))]
    Some(dirs::config_dir()?.join("cursor/auth.json"))
}

fn read_token() -> Option<String> {
    let cli_path = cli_auth_path();
    let app_path = dirs::config_dir().map(|dir| dir.join("Cursor/User/globalStorage/state.vscdb"));
    read_saved_token(cli_path.as_deref(), app_path.as_deref())
}

fn read_saved_token(cli_path: Option<&Path>, app_path: Option<&Path>) -> Option<String> {
    // A stale CLI login must not hide an already-working desktop login.
    let usable = |token: &String| session_cookie(token).is_some();
    app_path.and_then(read_app_token).filter(usable).or_else(|| read_cli_token(cli_path?).filter(usable))
}

fn read_cli_token(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    let auth: Value = serde_json::from_str(&raw).ok()?;
    Some(token_text(auth["accessToken"].as_str()?))
}

fn read_app_token(path: &Path) -> Option<String> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
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

fn session_cookie(token: &str) -> Option<String> {
    cookie(&jwt_sub(token)?, token)
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
    fn a_unified_super_plan_reports_its_weekly_usage_and_actual_reset() {
        let status = json!({
            "isUnified": true,
            "cursorPlanName": "Super",
            "usagePercent": 46.030_076_118_571_69,
            "nextResetTimestampUtc": "2026-10-27T23:30:44.854Z",
            "billingCycleEnd": "2026-11-01T00:00:00.000Z",
            "individualUsage": {"plan": {"totalPercentUsed": 0, "autoPercentUsed": 0, "apiPercentUsed": 0}}
        });
        let windows = parse_unified(&status).unwrap();
        assert_eq!(windows.len(), 1, "a unified allowance is not three legacy pools");
        assert_eq!((windows[0].id.as_str(), windows[0].label.as_str()), ("weekly", "Weekly"));
        assert!((windows[0].percent - 46.030_076_118_571_69).abs() < 1e-9);
        assert_eq!(windows[0].resets_at, Some(1_793_143_844_854));
    }

    #[test]
    fn missing_unified_usage_is_not_reported_as_zero() {
        for status in [json!({"isUnified": true}), json!({"usagePercent": -1}), json!({"usagePercent": "46%"})] {
            assert_eq!(parse_unified(&status), None);
        }
        let windows = parse_unified(&json!({"usagePercent": 0})).unwrap();
        assert!(windows[0].percent.abs() < f64::EPSILON, "an explicit zero is a valid reading");
    }

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
        let path = std::env::temp_dir().join(format!("codync-cursor-{}.db", uuid::Uuid::new_v4()));
        let conn = Connection::open(&path).unwrap();
        conn.execute("CREATE TABLE ItemTable (key TEXT, value BLOB)", []).unwrap();
        let app_token = test_token("user_app");
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES ('cursorAuth/accessToken', ?1)",
            [rusqlite::types::Value::Blob(format!("\"{app_token}\"").into_bytes())],
        )
        .unwrap();
        let cli_path = path.with_extension("json");
        for raw in ["{", "{}", r#"{"accessToken":""}"#, r#"{"accessToken":"not-a-jwt"}"#] {
            std::fs::write(&cli_path, raw).unwrap();
            assert_eq!(read_saved_token(Some(&cli_path), Some(&path)), Some(app_token.clone()));
            assert_eq!(read_saved_token(Some(&cli_path), None), None);
        }
        let token = test_token("user_cli");
        std::fs::write(&cli_path, json!({"accessToken": token}).to_string()).unwrap();
        assert_eq!(read_saved_token(Some(&cli_path), Some(&path)), Some(app_token.clone()));
        assert_eq!(read_saved_token(Some(&cli_path), None), Some(token.clone()));
        conn.execute("UPDATE ItemTable SET value = 'tok'", []).unwrap();
        assert_eq!(read_saved_token(Some(&cli_path), Some(&path)), Some(token), "a broken app value is not a login");
        drop(conn);
        std::fs::remove_file(&cli_path).unwrap();
        assert_eq!(read_saved_token(Some(&cli_path), Some(&path)), None);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(read_saved_token(Some(&cli_path), Some(&path)), None);
        assert!(!path.exists(), "reading an absent desktop database never creates one");
    }

    #[test]
    fn a_cli_only_login_supplies_usage_credentials_without_changing_them() {
        let path = std::env::temp_dir().join(format!("codync-cursor-{}.json", uuid::Uuid::new_v4()));
        let token = test_token("user_cli");
        let raw = json!({"accessToken": token, "refreshToken": "leave-this-alone"}).to_string();
        std::fs::write(&path, &raw).unwrap();
        let saved = read_saved_token(Some(&path), None);
        let after = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(saved, Some(token));
        assert_eq!(after, raw);
    }

    fn test_token(sub: &str) -> String {
        let payload = URL_SAFE_NO_PAD.encode(json!({ "sub": sub }).to_string());
        format!("eyJhbGciOiJub25lIn0.{payload}.sig")
    }

    #[test]
    fn a_failed_dashboard_keeps_a_unified_plan_off_the_legacy_summary() {
        let unified = json!({"isUnified": true, "usagePercent": 46});
        let Route::Unified(Some(windows)) = choose(Some(&unified), false) else { panic!("a unified reading") };
        assert_eq!(windows[0].id, "weekly");
        assert_eq!(choose(Some(&json!({"isUnified": true})), false), Route::Unified(None));
        assert_eq!(choose(None, true), Route::Unified(None), "a failed poll keeps the last weekly window");
        assert_eq!(choose(None, false), Route::Summary, "a legacy or unknown plan still reads its summary");
        assert_eq!(choose(Some(&json!({"isUnified": false})), true), Route::Summary);
    }
}
