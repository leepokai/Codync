//! Claude plan windows: the CLI's `/usage` report, the status-line hook, and
//! the ACP adapter's rate-limit event.

use serde_json::Value;

use super::Window;

/// Flags for `claude -p /usage`. The empty strict MCP config keeps the check
/// from starting every MCP server the user has set up.
const USAGE_ARGS: [&str; 8] = [
    "-p",
    "/usage",
    "--output-format",
    "json",
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
    let v: Value = serde_json::from_slice(&out.stdout).ok()?;
    let windows = parse_claude_usage(v["result"].as_str()?);
    (!windows.is_empty()).then_some(windows)
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
    if let Some(w) = claude_window(info) {
        super::merge(hub, "claude", "Claude", "agent", vec![w]);
    }
}

fn claude_window(info: &Value) -> Option<Window> {
    let kind = info["rateLimitType"].as_str()?;
    let label = match kind {
        "five_hour" => "5-hour",
        "seven_day" => "Weekly",
        "seven_day_opus" => "Weekly · Opus",
        "seven_day_sonnet" => "Weekly · Sonnet",
        _ => return None,
    };
    let u = info["utilization"].as_f64()?;
    // The SDK reports a 0–1 fraction (from the unified rate-limit headers).
    let percent = if u <= 1.0 { u * 100.0 } else { u };
    // Seconds, but accept milliseconds too.
    let resets = info["resetsAt"].as_i64().map(|t| if t > 100_000_000_000 { t } else { t.saturating_mul(1000) });
    Some(Window::new(kind, label, percent, resets))
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
    fn claude_rate_limit_fraction_becomes_percent() {
        let w = claude_window(&json!({"rateLimitType": "five_hour", "utilization": 0.42, "resetsAt": 1_790_000_000}))
            .unwrap();
        assert!((w.percent - 42.0).abs() < 1e-9);
        assert_eq!(w.resets_at, Some(1_790_000_000_000));
        assert!(claude_window(&json!({"rateLimitType": "overage", "utilization": 0.1})).is_none());
    }
}
