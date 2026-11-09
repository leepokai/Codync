//! Antigravity's quota: the `/usage` report of its ACP server, which is the login its bots run on,
//! or the language server of a running Antigravity app. Codync sends no login to Google itself.

use crate::hub::Hub;
use serde_json::Value;
use std::time::Duration;

use super::Window;

const BODY: &str =
    r#"{"metadata":{"ideName":"antigravity","extensionName":"antigravity","ideVersion":"unknown","locale":"en"}}"#;

/// The running app answers without a download, until a bot has brought the ACP server here.
pub(super) async fn collect(hub: &Hub) -> Option<Vec<Window>> {
    let server_here = crate::service::data_dir().join("agents/antigravity-acp").is_dir();
    if !server_here && let Some(windows) = from_language_server().await {
        return Some(windows);
    }
    if signed_in_or_unknown()
        && let Some(windows) = super::acp_read::command_reading(hub, "antigravity", "usage", parse_report).await
    {
        return Some(windows);
    }
    if server_here { from_language_server().await } else { None }
}

/// Windows has no command `BROWSER` could name to keep the signed-out adapter from opening a
/// browser, so its reading needs a saved login (off macOS the adapter keeps it only in these files).
#[cfg(windows)]
fn signed_in_or_unknown() -> bool {
    let home = std::env::var_os("GEMINI_HOME")
        .filter(|home| !home.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".gemini")));
    home.is_some_and(|home| {
        ["acp_token.json", "acp_business_token.json"]
            .iter()
            .any(|file| home.join("antigravity-acp").join(file).is_file())
    })
}

#[cfg(not(windows))]
const fn signed_in_or_unknown() -> bool {
    true
}

/// The `### Quota Limits` part of the ACP server's `/usage` report. `None`: it has no numbers this
/// time, or its format changed. A plan with no limits at all is an empty reading.
fn parse_report(text: &str) -> Option<Vec<Window>> {
    let (_, limits) = text.split_once("### Quota Limits")?;
    let mut group = "";
    let mut windows = Vec::new();
    for line in limits.lines() {
        if let Some(name) = line.strip_prefix("- **").and_then(|rest| rest.strip_suffix("**")) {
            group = name;
        } else if let Some(window) = report_window(group, line) {
            windows.push(window);
        }
    }
    let none_to_report = ["Not applicable", "No quota limits"].iter().any(|text| limits.contains(text));
    (!windows.is_empty() || none_to_report).then_some(windows)
}

/// `  - **Weekly Limit Remaining:** 42% remaining (resets Nov 06, 2026, 05:21 UTC)`.
fn report_window(group: &str, line: &str) -> Option<Window> {
    let (bucket, reading) = line.trim_start().strip_prefix("- **")?.split_once(":** ")?;
    let (left, reset) = reading.split_once("% remaining")?;
    let used = (100.0 - left.trim().parse::<f64>().ok()?).clamp(0.0, 100.0);
    let (id, label) = report_names(group, bucket);
    Some(Window::new(id, label, used, report_reset(reset)))
}

/// A limit as both sources name it. Its row keeps the id and label named for the model whichever
/// source reads it.
struct Limit {
    /// The language server's `bucketId`.
    bucket: &'static str,
    /// Part of the ACP report's group name.
    group: &'static str,
    /// The ACP report's bucket, as [`report_names`] shortens it.
    span: &'static str,
    id: &'static str,
    label: &'static str,
}

const LIMITS: [Limit; 5] = [
    Limit { bucket: "gemini-5h", group: "Gemini Models", span: "5h", id: "five_hour", label: "Gemini 5h" },
    Limit { bucket: "gemini-weekly", group: "Gemini Models", span: "7d", id: "seven_day", label: "Gemini 7d" },
    Limit { bucket: "3p-5h", group: "Claude and GPT models", span: "5h", id: "claude_five_hour", label: "Claude 5h" },
    Limit {
        bucket: "3p-weekly",
        group: "Claude and GPT models",
        span: "7d",
        id: "claude_seven_day",
        label: "Claude 7d",
    },
    Limit { bucket: "gemini-argon-wtu-weekly", group: "Argon", span: "7d", id: "argon", label: "Argon Weekly" },
];

fn report_names(group: &str, bucket: &str) -> (String, String) {
    let span = if bucket.contains("Five Hour") {
        "5h"
    } else if bucket.contains("Weekly") {
        "7d"
    } else {
        bucket.trim_end_matches(" Limit Remaining")
    };
    if let Some(limit) = LIMITS.iter().find(|limit| group.contains(limit.group) && span == limit.span) {
        return (limit.id.to_owned(), limit.label.to_owned());
    }
    // A group Google adds later still gets its row.
    let label = format!("{} {span}", group.trim_end_matches(" Models").trim_end_matches(" models"));
    (super::slug(&label), label)
}

fn report_reset(text: &str) -> Option<i64> {
    let at = text.trim().strip_prefix("(resets ")?.strip_suffix(')')?;
    let parsed = chrono::NaiveDateTime::parse_from_str(at, "%b %d, %Y, %H:%M UTC").ok()?;
    Some(parsed.and_utc().timestamp_millis())
}

async fn from_language_server() -> Option<Vec<Window>> {
    let target = tokio::task::spawn_blocking(find_server).await.ok().flatten()?;
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;
    for port in &target.https {
        if let Some(windows) = ask(&client, &format!("https://127.0.0.1:{port}"), &target.csrf).await {
            return Some(windows);
        }
    }
    if let Some(port) = target.http
        && let Some(windows) = ask(&client, &format!("http://127.0.0.1:{port}"), &target.csrf).await
    {
        return Some(windows);
    }
    tracing::debug!("antigravity language server had no quota");
    None
}

struct Server {
    csrf: String,
    https: Vec<u16>,
    http: Option<u16>,
}

async fn ask(client: &reqwest::Client, base: &str, csrf: &str) -> Option<Vec<Window>> {
    for method in ["RetrieveUserQuotaSummary", "GetUserStatus"] {
        let url = format!("{base}/exa.language_server_pb.LanguageServerService/{method}");
        let response = client
            .post(&url)
            .header("X-Codeium-Csrf-Token", csrf)
            .header("Connect-Protocol-Version", "1")
            .header("Content-Type", "application/json")
            .body(BODY)
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            continue;
        }
        let body: Value = response.json().await.ok()?;
        let windows = parse(&body);
        if !windows.is_empty() {
            return Some(windows);
        }
    }
    None
}

fn find_server() -> Option<Server> {
    let processes = processes();
    let (pid, command) = processes.into_iter().find(|(_, command)| is_antigravity(command))?;
    let csrf = flag(&command, "--csrf_token")?;
    let https = listen_ports(pid);
    let http = flag(&command, "--extension_server_port").and_then(|p| p.parse().ok());
    Some(Server { csrf, https, http })
}

fn is_antigravity(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    let named = lower.contains("language_server") || lower.contains("agy");
    let ours = lower.contains("antigravity") || lower.contains("--app_data_dir");
    named && ours && command.contains("--csrf_token")
}

fn flag(command: &str, name: &str) -> Option<String> {
    let mut parts = command.split_whitespace();
    while let Some(part) = parts.next() {
        if let Some(value) = part.strip_prefix(name).and_then(|rest| rest.strip_prefix('=')).filter(|s| !s.is_empty()) {
            return Some(value.to_owned());
        }
        if part == name {
            return parts.next().filter(|s| !s.is_empty()).map(str::to_owned);
        }
    }
    None
}

/// macOS `pgrep -a` includes ancestors and prints only pids. `-lf` prints the pid
/// and the full argument list. Linux `-a -f` is the command that does the same.
#[cfg(unix)]
fn processes() -> Vec<(u32, String)> {
    #[cfg(target_os = "macos")]
    let args: &[&str] = &["-lf", "language_server|agy"];
    #[cfg(not(target_os = "macos"))]
    let args: &[&str] = &["-a", "-f", "language_server|agy"];
    let Ok(out) = std::process::Command::new("pgrep").args(args).output() else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let (pid, command) = line.split_once(' ')?;
            Some((pid.parse().ok()?, command.to_owned()))
        })
        .collect()
}

#[cfg(windows)]
fn processes() -> Vec<(u32, String)> {
    let Ok(out) = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -match 'antigravity' } | Select-Object ProcessId, CommandLine | ConvertTo-Json -Compress",
        ])
        .output()
    else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_slice::<Value>(&out.stdout) else { return Vec::new() };
    let rows = value.as_array().cloned().unwrap_or_else(|| vec![value]);
    rows.into_iter()
        .filter_map(|row| {
            let pid = u32::try_from(row["ProcessId"].as_i64()?).ok()?;
            Some((pid, row["CommandLine"].as_str()?.to_owned()))
        })
        .collect()
}

#[cfg(unix)]
fn listen_ports(pid: u32) -> Vec<u16> {
    let out = std::process::Command::new("lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-a", "-p", &pid.to_string()])
        .output()
        .ok();
    let text = out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    ports_in(&text)
}

#[cfg(windows)]
fn listen_ports(pid: u32) -> Vec<u16> {
    let out = std::process::Command::new("netstat").args(["-ano", "-p", "tcp"]).output().ok();
    let text = out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let pid_text = pid.to_string();
    text.lines()
        .filter(|line| line.contains("LISTENING") && line.split_whitespace().next_back() == Some(pid_text.as_str()))
        .filter_map(|line| line.split(':').nth(1)?.split_whitespace().next()?.parse().ok())
        .collect()
}

#[cfg(unix)]
fn ports_in(text: &str) -> Vec<u16> {
    let mut ports = Vec::new();
    // Keep the IPv6 brackets, so `[::1]:port` stays one address instead of a stray port 1.
    for token in text.split(|c: char| !c.is_ascii_digit() && c != ':' && c != '[' && c != ']') {
        let Some(port) = token.rsplit(':').next().filter(|s| !s.is_empty()) else { continue };
        if token.contains(':')
            && let Ok(port) = port.parse()
            && !ports.contains(&port)
        {
            ports.push(port);
        }
    }
    ports
}

pub(super) fn parse(body: &Value) -> Vec<Window> {
    let groups = body["response"]["groups"].as_array().or_else(|| body["groups"].as_array());
    if let Some(groups) = groups {
        let windows = summary(groups);
        if !windows.is_empty() {
            return windows;
        }
    }
    models(body)
}

fn summary(groups: &[Value]) -> Vec<Window> {
    let mut by_id: std::collections::HashMap<&str, &Value> = std::collections::HashMap::new();
    for group in groups {
        if let Some(buckets) = group["buckets"].as_array() {
            for bucket in buckets {
                if let Some(id) = bucket["bucketId"].as_str() {
                    by_id.entry(id).or_insert(bucket);
                }
            }
        }
    }
    LIMITS
        .iter()
        .filter_map(|limit| {
            let item = by_id.get(limit.bucket)?;
            let left = item["remainingFraction"].as_f64()?;
            let used = ((1.0 - left) * 100.0).clamp(0.0, 100.0);
            Some(Window::new(limit.id, limit.label, used, epoch_ms(&item["resetTime"])))
        })
        .collect()
}

fn models(body: &Value) -> Vec<Window> {
    let configs = body["userStatus"]["cascadeModelConfigData"]["clientModelConfigs"]
        .as_array()
        .or_else(|| body["clientModelConfigs"].as_array());
    let Some(configs) = configs else { return Vec::new() };
    configs
        .iter()
        .filter_map(|item| {
            let label = item["label"].as_str().filter(|s| !s.is_empty())?;
            let left = item["quotaInfo"]["remainingFraction"].as_f64()?;
            Some(Window::new(
                super::slug(label),
                label,
                ((1.0 - left) * 100.0).clamp(0.0, 100.0),
                epoch_ms(&item["quotaInfo"]["resetTime"]),
            ))
        })
        .collect()
}

fn epoch_ms(v: &Value) -> Option<i64> {
    if let Some(text) = v.as_str() {
        return super::iso_ms(text);
    }
    let n = v.as_i64().or_else(|| v.as_f64().and_then(super::finite_i64))?;
    Some(if n > 100_000_000_000 { n } else { n.saturating_mul(1000) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const REPORT: &str = "### Session Usage\n- **Session total:** data not available yet\n\n### Quota Limits\n\
        - **Gemini Models**\n  *(Models within this group: Gemini Flash, Gemini Pro)*\n  \
        - **Weekly Limit Remaining:** 91% remaining (resets Nov 07, 2026, 05:50 UTC)\n  \
        - **Five Hour Limit Remaining:** 100% remaining (resets Nov 05, 2026, 21:08 UTC)\n\
        - **Claude and GPT models**\n  *(Models within this group: Claude Opus, Claude Sonnet, GPT-OSS)*\n  \
        - **Weekly Limit Remaining:** 42% remaining (resets Nov 06, 2026, 05:21 UTC)\n\
        - **Gemini 4 Argon**\n  *(For paid Pro plans, Argon quota is separate.)*\n  \
        - **Gemini 4 Argon Weekly Limit:** 100% remaining\n\
        - **Flash Lite Models**\n  - **Weekly Limit Remaining:** 25% remaining\n  \
        - **Daily Limit Remaining:** Disabled: not on this plan\n";

    #[test]
    fn the_acp_report_is_the_share_used_named_like_the_language_servers_reading() {
        let w = parse_report(REPORT).unwrap();
        let names: Vec<_> = w.iter().map(|w| (w.id.as_str(), w.label.as_str())).collect();
        assert_eq!(
            names,
            [
                ("seven_day", "Gemini 7d"),
                ("five_hour", "Gemini 5h"),
                ("claude_seven_day", "Claude 7d"),
                ("argon", "Argon Weekly"),
                ("flash_lite_7d", "Flash Lite 7d"),
            ]
        );
        assert!((w[0].percent - 9.0).abs() < 1e-9);
        assert!((w[2].percent - 58.0).abs() < 1e-9);
        assert_eq!(w[0].resets_at, super::super::iso_ms("2026-11-07T05:50:00Z"));
        assert_eq!(w[3].resets_at, None, "no reset was reported");
    }

    #[test]
    fn a_report_without_numbers_keeps_the_last_reading() {
        assert_eq!(parse_report("### Quota Limits\n- Unable to retrieve quota status (offline)"), None);
        assert_eq!(parse_report("### Quota Limits\n- data not available yet"), None);
        assert_eq!(parse_report("something else entirely"), None, "the format changed");
    }

    #[test]
    fn a_login_with_no_limits_is_an_empty_reading() {
        assert_eq!(parse_report("### Quota Limits\n- Not applicable for current authentication method."), Some(vec![]));
        assert_eq!(parse_report("### Quota Limits\n- No quota limits reported."), Some(vec![]));
    }

    #[test]
    fn a_summary_is_the_share_used() {
        let body = json!({"response":{"groups":[{"buckets":[
            {"bucketId":"gemini-5h","remainingFraction":0.2,"resetTime":"2026-10-25T12:00:00Z"},
            {"bucketId":"3p-weekly","remainingFraction":0.5}
        ]}]}});
        let w = parse(&body);
        // Named for its model, like the Claude rows: clients show a bare "5-hour" as Claude's "Session".
        assert_eq!(w[0].label, "Gemini 5h");
        assert!((w[0].percent - 80.0).abs() < 1e-9);
        assert!(w[0].resets_at.is_some());
        assert_eq!(w[1].label, "Claude 7d");
        assert!((w[1].percent - 50.0).abs() < 1e-9);
    }

    #[test]
    fn the_csrf_flag_and_an_unrelated_process_are_told_apart() {
        let line =
            "/Applications/Antigravity.app/language_server_macos --csrf_token secret --extension_server_port 1234";
        assert!(is_antigravity(line));
        assert_eq!(flag(line, "--csrf_token").as_deref(), Some("secret"));
        assert_eq!(flag(line, "--extension_server_port").as_deref(), Some("1234"));
        assert!(!is_antigravity("agy --help"));
    }

    #[cfg(unix)]
    #[test]
    fn a_listen_line_keeps_the_port_once() {
        let text = "codync-host 123 user 10u IPv4 TCP 127.0.0.1:19223 (LISTEN)\n\
                    codync-host 123 user 11u IPv6 TCP [::1]:19223 (LISTEN)\n\
                    codync-host 123 user 12u IPv4 TCP *:43122 (LISTEN)\n";
        assert_eq!(ports_in(text), vec![19223, 43122]);
    }
}
