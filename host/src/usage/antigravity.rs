//! Antigravity's quota from the language server of the running app or `agy`.
//! The request stays on 127.0.0.1. When nothing is running, there is no window:
//! the cloud fallback would send the saved login to Google, and Codync doesn't.

use serde_json::Value;
use std::time::Duration;

use super::Window;

const BODY: &str =
    r#"{"metadata":{"ideName":"antigravity","extensionName":"antigravity","ideVersion":"unknown","locale":"en"}}"#;

pub(super) async fn collect() -> Option<Vec<Window>> {
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
    [
        ("gemini-5h", "five_hour", "5-hour"),
        ("gemini-weekly", "seven_day", "Weekly"),
        ("3p-5h", "claude_five_hour", "Claude 5h"),
        ("3p-weekly", "claude_seven_day", "Claude 7d"),
        ("gemini-argon-wtu-weekly", "argon", "Argon Weekly"),
    ]
    .into_iter()
    .filter_map(|(bucket, id, label)| {
        let item = by_id.get(bucket)?;
        let left = item["remainingFraction"].as_f64()?;
        Some(Window::new(id, label, ((1.0 - left) * 100.0).clamp(0.0, 100.0), epoch_ms(&item["resetTime"])))
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
            let id = label
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
                .collect::<String>();
            Some(Window::new(
                id,
                label,
                ((1.0 - left) * 100.0).clamp(0.0, 100.0),
                epoch_ms(&item["quotaInfo"]["resetTime"]),
            ))
        })
        .collect()
}

fn epoch_ms(v: &Value) -> Option<i64> {
    if let Some(text) = v.as_str() {
        return chrono::DateTime::parse_from_rfc3339(text).ok().map(|t| t.timestamp_millis());
    }
    let n = v.as_i64().or_else(|| v.as_f64().and_then(super::finite_i64))?;
    Some(if n > 100_000_000_000 { n } else { n.saturating_mul(1000) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_summary_is_the_share_used() {
        let body = json!({"response":{"groups":[{"buckets":[
            {"bucketId":"gemini-5h","remainingFraction":0.2,"resetTime":"2026-10-25T12:00:00Z"},
            {"bucketId":"3p-weekly","remainingFraction":0.5}
        ]}]}});
        let w = parse(&body);
        assert_eq!(w[0].label, "5-hour");
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
