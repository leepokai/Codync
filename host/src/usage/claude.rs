//! Claude plan windows: the CLI's `/usage` report, the status-line hook, and
//! the ACP adapter's rate-limit event.

use serde_json::Value;

use super::Window;

/// Flags for `claude -p /usage`. Stream JSON carries the structured
/// `usage_report`. The empty strict MCP config keeps the check from starting
/// every MCP server the user has set up.
const USAGE_ARGS: [&str; 9] = [
    "-p",
    "/usage",
    "--output-format",
    "stream-json",
    "--verbose",
    "--no-session-persistence",
    "--strict-mcp-config",
    "--mcp-config",
    r#"{"mcpServers":{}}"#,
];

/// `claude -p /usage`: Claude Code's own local usage report, no model call.
pub(super) async fn collect() -> Option<Vec<Window>> {
    let bin = super::command::program("claude")?;
    let out = tokio::time::timeout(
        std::time::Duration::from_mins(1),
        tokio::process::Command::new(bin)
            .args(USAGE_ARGS)
            .current_dir(dirs::home_dir()?)
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    let windows = parse_usage_stream(&String::from_utf8_lossy(&out.stdout));
    (!windows.is_empty()).then_some(windows)
}

/// The `usage_report` event's limits, exact instants included; a CLI without
/// one leaves its report text in the `result` event. The text's reset times
/// follow the CLI's date formatting, which differs between platforms.
fn parse_usage_stream(stdout: &str) -> Vec<Window> {
    let events: Vec<Value> = stdout.lines().filter_map(|line| serde_json::from_str(line).ok()).collect();
    if let Some(limits) = events.iter().find_map(|e| e["usage_report"]["rate_limits"]["limits"].as_array()) {
        return limits.iter().filter_map(report_window).collect();
    }
    events.iter().find_map(|e| e["result"].as_str()).map(parse_claude_usage).unwrap_or_default()
}

/// One of `usage_report.rate_limits.limits`:
/// `{"kind": "weekly_scoped", "percent": 0, "resets_at": "2026-10-31T03:59:59+00:00", "scope": {"model": {"display_name": "Fable"}}}`.
fn report_window(limit: &Value) -> Option<Window> {
    let (id, label) = match limit["kind"].as_str()? {
        "session" => ("five_hour".to_owned(), "5-hour".to_owned()),
        "weekly_all" => ("seven_day".to_owned(), "Weekly".to_owned()),
        "weekly_scoped" => {
            let model = limit["scope"]["model"]["display_name"].as_str()?;
            (format!("seven_day_{}", model.to_lowercase()), format!("Weekly · {model}"))
        }
        _ => return None,
    };
    let resets = limit["resets_at"].as_str().and_then(super::iso_ms);
    Some(Window::new(id, label, limit["percent"].as_f64()?, resets))
}

/// Lines like `Current week (all models): 53% used · resets Sep 26 at 12pm (Asia/Taipei)`.
pub fn parse_claude_usage(text: &str) -> Vec<Window> {
    text.lines()
        .filter_map(|line| {
            let (name, rest) = line.trim().split_once(": ")?;
            let percent: f64 = rest.split_once("% used")?.0.trim().parse().ok()?;
            let resets = rest.split_once("resets ").map(|(_, r)| r.trim().to_owned());
            let (id, label) = match name {
                "Current session" => ("five_hour".to_owned(), "5-hour".to_owned()),
                "Current week (all models)" => ("seven_day".to_owned(), "Weekly".to_owned()),
                n => {
                    let model = n.strip_prefix("Current week (")?.strip_suffix(')')?;
                    (format!("seven_day_{}", model.to_lowercase()), format!("Weekly · {model}"))
                }
            };
            Some(Window { resets_text: resets, ..Window::new(id, label, percent, None) })
        })
        .collect()
}

/// `POST /ingest/statusline` body: Claude Code's statusline JSON.
pub fn ingest_statusline(hub: &crate::hub::Hub, body: &Value) {
    let rl = &body["rate_limits"];
    let windows: Vec<Window> = [("five_hour", "5-hour"), ("seven_day", "Weekly")]
        .into_iter()
        .filter_map(|(key, label)| {
            let percent = rl[key]["used_percentage"].as_f64()?;
            Some(Window::new(key, label, percent, rl[key]["resets_at"].as_i64().map(|s| s.saturating_mul(1000))))
        })
        .collect();
    if !windows.is_empty() {
        super::merge(hub, "claude", "Claude", "statusline", windows);
    }
}

/// Claude ACP adapter: `usage_update._meta["_claude/rateLimit"]` (the SDK's `SDKRateLimitInfo`).
pub fn ingest_claude_rate_limit(hub: &crate::hub::Hub, info: &Value) {
    let windows = claude_windows(info);
    if !windows.is_empty() {
        super::merge(hub, "claude", "Claude", "agent", windows);
    }
}

/// The limit the event is about, and the others it lists in `unifiedWindows`: the only place a
/// limit that is merely `allowed` has its utilization. A `claude setup-token` login is read this
/// way, because its `/usage` reports no plan limits. Claude Code marks the field internal and the
/// SDK's types leave it out, so an event without it reads as before.
fn claude_windows(info: &Value) -> Vec<Window> {
    let listed = info["unifiedWindows"].as_object().into_iter().flatten();
    let mut windows: Vec<Window> = listed.filter_map(|(kind, limit)| unified_window(kind, limit)).collect();
    if let Some(own) = claude_window(info) {
        windows.retain(|w| w.id != own.id);
        windows.push(own);
    }
    windows
}

fn claude_window(info: &Value) -> Option<Window> {
    let (id, label) = claude_kind(info["rateLimitType"].as_str()?)?;
    // The SDK reports a 0–1 fraction (from the unified rate-limit headers). A
    // limit that refused a request comes without one: it is used up.
    let percent = match info["utilization"].as_f64() {
        Some(u) if u <= 1.0 => u * 100.0,
        Some(u) => u,
        None if info["status"] == "rejected" => 100.0,
        None => return None,
    };
    Some(Window::new(id, label, percent, reset_ms(&info["resetsAt"])))
}

/// One of `unifiedWindows`: `{"utilization": 0.26, "resetsAt": 1794470400, "status": "allowed"}`.
fn unified_window(kind: &str, limit: &Value) -> Option<Window> {
    let (id, label) = claude_kind(kind)?;
    // Always a fraction here, and above 1 once usage runs past the cap.
    let percent = (limit["utilization"].as_f64()? * 100.0).clamp(0.0, 100.0);
    Some(Window::new(id, label, percent, reset_ms(&limit["resetsAt"])))
}

/// A limit kind's window id and label, the ones the `/usage` report gives it, so both readings
/// land on one row. `seven_day_overage_included` is what Claude Code itself calls the "Fable
/// limit"; the event names no model, so the name is fixed here as it is there.
fn claude_kind(kind: &str) -> Option<(&'static str, &'static str)> {
    match kind {
        "five_hour" => Some(("five_hour", "5-hour")),
        "seven_day" => Some(("seven_day", "Weekly")),
        "seven_day_opus" => Some(("seven_day_opus", "Weekly · Opus")),
        "seven_day_sonnet" => Some(("seven_day_sonnet", "Weekly · Sonnet")),
        "seven_day_overage_included" => Some(("seven_day_fable", "Weekly · Fable")),
        _ => None,
    }
}

/// `resetsAt` as epoch milliseconds: it comes in seconds, but accept milliseconds too.
fn reset_ms(at: &Value) -> Option<i64> {
    at.as_i64().map(|t| if t > 100_000_000_000 { t } else { t.saturating_mul(1000) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn usage_check_starts_no_mcp_servers() {
        let strict = USAGE_ARGS.iter().position(|a| *a == "--strict-mcp-config");
        assert!(strict.is_some(), "--strict-mcp-config missing");
        let config = USAGE_ARGS.iter().position(|a| *a == "--mcp-config").expect("--mcp-config missing");
        let empty: Value = serde_json::from_str(USAGE_ARGS[config + 1]).unwrap();
        assert_eq!(empty, json!({ "mcpServers": {} }));
    }

    #[test]
    fn parses_claude_usage_report() {
        let w = parse_claude_usage(
            "You are currently using your subscription\n\nCurrent session: 11% used · resets Sep 25 at 5am (Asia/Taipei)\nCurrent week (all models): 53% used · resets Sep 26 at 12pm (Asia/Taipei)\nCurrent week (Fable): 83% used · resets Sep 26 at 12pm (Asia/Taipei)\n\nLast 24h · 794 requests",
        );
        assert_eq!(w.len(), 3);
        assert_eq!(w[0].id, "five_hour");
        assert!((w[0].percent - 11.0).abs() < f64::EPSILON);
        assert_eq!(w[0].resets_text.as_deref(), Some("Sep 25 at 5am (Asia/Taipei)"));
        assert_eq!(w[2].label, "Weekly · Fable");
    }

    #[test]
    fn usage_report_gives_exact_resets() {
        let stream = [
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"assistant","usage_report":{"rate_limits":{"limits":[{"kind":"session","percent":4,"resets_at":"2026-10-29T20:10:00.568718+00:00"},{"kind":"weekly_all","percent":97,"resets_at":"2026-10-31T04:00:00+00:00"},{"kind":"weekly_scoped","percent":0,"resets_at":"2026-10-31T03:59:59+00:00","scope":{"model":{"display_name":"Fable"}}},{"kind":"extra","percent":1}]}}}"#,
            r#"{"type":"result","result":"Current session: 50% used"}"#,
        ]
        .join("\n");
        let w = parse_usage_stream(&stream);
        assert_eq!(w.len(), 3);
        assert_eq!((w[0].id.as_str(), w[0].resets_at), ("five_hour", Some(1_793_304_600_568)));
        assert!((w[1].percent - 97.0).abs() < f64::EPSILON);
        assert_eq!((w[2].id.as_str(), w[2].label.as_str()), ("seven_day_fable", "Weekly · Fable"));
    }

    #[test]
    fn report_text_when_the_cli_has_no_usage_report() {
        let w =
            parse_usage_stream(r#"{"type":"result","result":"Current session: 11% used · resets 2pm (Asia/Taipei)"}"#);
        assert_eq!((w.len(), w[0].resets_text.as_deref()), (1, Some("2pm (Asia/Taipei)")));
    }

    #[test]
    fn claude_rate_limit_fraction_becomes_percent() {
        let w = claude_window(&json!({"rateLimitType": "five_hour", "utilization": 0.42, "resetsAt": 1_790_000_000}))
            .unwrap();
        assert!((w.percent - 42.0).abs() < 1e-9);
        assert_eq!(w.resets_at, Some(1_790_000_000_000));
        assert!(claude_window(&json!({"rateLimitType": "overage", "utilization": 0.1})).is_none());
    }

    #[test]
    fn a_refused_limit_is_used_up() {
        let w = claude_window(&json!({"status": "rejected", "rateLimitType": "five_hour", "resetsAt": 1_790_000_000}))
            .unwrap();
        assert!((w.percent - 100.0).abs() < f64::EPSILON);
        assert!(claude_window(&json!({"status": "allowed", "rateLimitType": "five_hour"})).is_none());
    }

    #[test]
    fn allowed_limits_are_read_from_unified_windows() {
        // Two events of a `claude setup-token` login, as Claude Code 2.1.321 sent them.
        let weekly = claude_windows(
            &serde_json::from_str(
                r#"{"status":"allowed","resetsAt":1794470400,"rateLimitType":"seven_day","overageStatus":"rejected","overageDisabledReason":"out_of_credits","isUsingOverage":false,"unifiedWindows":{"seven_day":{"utilization":0.26,"resetsAt":1794470400,"status":"allowed"}}}"#,
            )
            .unwrap(),
        );
        assert_eq!(weekly.len(), 1);
        assert_eq!((weekly[0].id.as_str(), weekly[0].label.as_str()), ("seven_day", "Weekly"));
        assert!((weekly[0].percent - 26.0).abs() < 1e-9);
        assert_eq!(weekly[0].resets_at, Some(1_794_470_400_000));

        let mut with_fable = claude_windows(
            &serde_json::from_str(
                r#"{"status":"allowed","resetsAt":1794470400,"rateLimitType":"seven_day","overageStatus":"rejected","overageDisabledReason":"out_of_credits","isUsingOverage":false,"unifiedWindows":{"seven_day":{"utilization":0.26,"resetsAt":1794470400,"status":"allowed"},"seven_day_overage_included":{"utilization":0.06,"resetsAt":1794470400,"status":"allowed"}}}"#,
            )
            .unwrap(),
        );
        with_fable.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(with_fable.len(), 2);
        assert_eq!(with_fable[0].id, "seven_day");
        assert!((with_fable[0].percent - 26.0).abs() < 1e-9);
        assert_eq!((with_fable[1].id.as_str(), with_fable[1].label.as_str()), ("seven_day_fable", "Weekly · Fable"));
        assert!((with_fable[1].percent - 6.0).abs() < 1e-9);
    }

    #[test]
    fn the_fable_limit_lands_on_the_usage_reports_row() {
        let report = report_window(&json!({
            "kind": "weekly_scoped",
            "percent": 6,
            "scope": {"model": {"display_name": "Fable"}},
        }))
        .unwrap();
        let hit = claude_window(&json!({"status": "rejected", "rateLimitType": "seven_day_overage_included"})).unwrap();
        assert_eq!((hit.id.as_str(), hit.label.as_str()), (report.id.as_str(), report.label.as_str()));
        assert!((hit.percent - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn unified_windows_add_to_the_limit_the_event_is_about() {
        let mut w = claude_windows(&json!({
            "status": "allowed_warning",
            "rateLimitType": "seven_day",
            "utilization": 0.91,
            "resetsAt": 1_794_470_400,
            "unifiedWindows": {
                "five_hour": {"utilization": 1.04, "resetsAt": 1_794_000_000_000_i64, "status": "rejected"},
                "seven_day": {"utilization": 0.9, "resetsAt": 1_794_470_400, "status": "allowed_warning"},
                "monthly": {"utilization": 0.5, "resetsAt": 1_794_470_400},
            },
        }));
        w.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(w.len(), 2, "a kind the host has no row for is left out");
        assert_eq!((w[0].id.as_str(), w[0].resets_at), ("five_hour", Some(1_794_000_000_000)));
        assert!((w[0].percent - 100.0).abs() < f64::EPSILON, "past the cap is used up, not 1.04%");
        assert_eq!(w[1].id, "seven_day");
        assert!((w[1].percent - 91.0).abs() < 1e-9, "the event's own reading wins");
    }

    #[test]
    fn an_event_without_unified_windows_reads_as_before() {
        let warning = json!({"status": "allowed_warning", "rateLimitType": "five_hour", "utilization": 0.8});
        assert_eq!(claude_windows(&warning), claude_window(&warning).into_iter().collect::<Vec<_>>());
        assert_eq!(claude_windows(&warning).len(), 1);
        assert_eq!(claude_windows(&json!({"status": "allowed", "rateLimitType": "five_hour"})), []);
        assert_eq!(claude_windows(&json!({"status": "allowed", "unifiedWindows": null})), []);
    }
}
