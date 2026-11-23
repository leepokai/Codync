//! Claude's windows that arrive without a check: its status line and the ACP adapter's rate-limit event, each on
//! the login it came from. The `/usage` check itself is the agent crate's (`codync_agent::usage::claude`).

use codync_agent::usage::claude;
use serde_json::Value;

use super::{Reading, Window};
use crate::hub::Hub;

/// `POST /ingest/statusline` body: Claude Code's statusline JSON, from a session on `profile` (the default login
/// without one).
pub fn ingest_statusline(hub: &Hub, profile: Option<&str>, body: &Value) {
    let windows = claude::statusline(body);
    if !windows.is_empty() {
        land(hub, "statusline", profile, windows);
    }
}

/// Claude ACP adapter: `usage_update._meta["_claude/rateLimit"]` (the SDK's `SDKRateLimitInfo`).
pub fn ingest_claude_rate_limit(hub: &Hub, profile: Option<&str>, info: &Value) {
    let windows = claude::rate_limit(info);
    if !windows.is_empty() {
        land(hub, "agent", profile, windows);
    }
}

/// Some of the windows, from the status line or a rate-limit event, land on the login they came from.
fn land(hub: &Hub, source: &'static str, profile: Option<&str>, windows: Vec<Window>) {
    let Ok(reader) = super::services::reader("claude") else { return };
    hub.update_usage(|usage| usage.land(reader, source, profile, Reading::Some(windows)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unified_windows_land_on_the_login_the_event_came_from() {
        use crate::LockExt;

        let hub = super::super::services::tests::hub();
        let reader = super::super::services::reader("claude").expect("reader");
        hub.update_usage(|usage| usage.list_login_profiles(reader, None, vec![("work".into(), "Work".into())]));
        ingest_claude_rate_limit(
            &hub,
            Some("work"),
            &json!({"status": "allowed", "unifiedWindows": {"seven_day_overage_included": {"utilization": 0.06}}}),
        );
        let usage = hub.usage.locked();
        assert_eq!(usage.windows("claude", Some("work")).expect("work")[0].id, "seven_day_fable");
    }
}
