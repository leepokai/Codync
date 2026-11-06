//! Read plan quotas only from adapters that handle `/usage` without a model turn.

use super::{Provider, Window};
use crate::LockExt;
use crate::agent::acp::{Acp, Incoming};
use crate::agent::{account, auth, backends};
use crate::hub::Hub;
use anyhow::{Result, bail};
use chrono::NaiveDateTime;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

const MAX_TEXT: usize = 64 * 1024;

pub(super) async fn refresh(hub: &Hub) {
    if backends::which("agy").is_none() {
        return;
    }
    let profiles = match account::list(Some("antigravity")) {
        Ok(profiles) => profiles,
        Err(error) => {
            tracing::warn!(%error, "could not list Antigravity quota profiles");
            return;
        }
    };
    let mut targets = vec![(None, "Antigravity".to_owned())];
    targets.extend(profiles.into_iter().map(|p| (Some(p.id), format!("Antigravity · {}", p.name))));
    let ids: Vec<_> = targets.iter().map(|(account, _)| report_id(account.as_deref())).collect();
    let mut usage = hub.usage.locked().clone();
    usage.providers.retain(|p| !p.id.starts_with("antigravity:") || ids.contains(&p.id));
    hub.set_usage(usage);
    for (account, name) in targets {
        let (windows, status) = match probe(&hub.store, account.as_deref()).await {
            Ok(Some(windows)) if !windows.is_empty() => (windows, None),
            Ok(None) => (vec![], Some("unsupported")),
            Ok(Some(_)) => (vec![], Some("unavailable")),
            // No provider output or credentials enter logs. Missing quota is not zero.
            Err(_) => {
                tracing::debug!("Antigravity quota is unavailable");
                (vec![], Some("unavailable"))
            }
        };
        let id = report_id(account.as_deref());
        let mut usage = hub.usage.locked().clone();
        usage.providers.retain(|p| p.id != id);
        usage.providers.push(Provider {
            id,
            name,
            windows,
            source: "antigravity",
            updated_at: crate::store::now_ms(),
            status,
        });
        usage.providers.sort_by(|a, b| a.id.cmp(&b.id));
        hub.set_usage(usage);
    }
}

fn report_id(account: Option<&str>) -> String {
    account.map_or_else(|| "antigravity".into(), |id| format!("antigravity:{id}"))
}

async fn probe(store: &crate::store::Store, account: Option<&str>) -> Result<Option<Vec<Window>>> {
    let env = account.map(|id| account::env(id, "antigravity")).transpose()?.unwrap_or_default();
    let (acp, mut inbox, init, _) = auth::start_with_env(store, "antigravity", &env).await?;
    let result = async {
        if !supports_local_usage(&init) {
            return Ok(None);
        }
        let session =
            request(&acp, &mut inbox, "session/new", json!({"cwd": auth::probe_dir(), "mcpServers": []}), None).await?;
        let session_id = session["sessionId"].as_str().ok_or_else(|| anyhow::anyhow!("missing session ID"))?;
        let mut text = String::new();
        request(
            &acp,
            &mut inbox,
            "session/prompt",
            json!({"sessionId": session_id, "prompt": [{"type": "text", "text": "/usage"}]}),
            Some((session_id, &mut text)),
        )
        .await?;
        Ok(Some(parse(&text)))
    }
    .await;
    acp.kill().await;
    result
}

fn supports_local_usage(init: &Value) -> bool {
    let Some(version) = init["agentInfo"]["version"].as_str() else { return false };
    let Ok(parts) = version.trim_start_matches('v').split('.').map(str::parse::<u32>).collect::<Result<Vec<_>, _>>()
    else {
        return false;
    };
    parts.len() == 3 && parts.as_slice() >= [1, 4, 1].as_slice()
}

async fn request(
    acp: &Arc<Acp>,
    inbox: &mut mpsc::UnboundedReceiver<Incoming>,
    method: &str,
    params: Value,
    mut capture: Option<(&str, &mut String)>,
) -> Result<Value> {
    let pending = acp.request(method, params);
    tokio::pin!(pending);
    let result = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            tokio::select! {
                result = &mut pending => break result,
                incoming = inbox.recv() => consume(acp, incoming, &mut capture).await?,
            }
        }
    })
    .await??;
    while let Ok(incoming) = inbox.try_recv() {
        consume(acp, Some(incoming), &mut capture).await?;
    }
    Ok(result)
}

async fn consume(acp: &Acp, incoming: Option<Incoming>, capture: &mut Option<(&str, &mut String)>) -> Result<()> {
    match incoming {
        Some(Incoming::Request { id, .. }) => {
            acp.respond_error(id, -32601, "tools are unavailable during quota inspection").await?;
        }
        Some(Incoming::Notification { method, params }) => {
            if let Some((session, text)) = capture.as_mut()
                && method == "session/update"
                && params["sessionId"].as_str() == Some(*session)
                && params["update"]["sessionUpdate"] == "agent_message_chunk"
                && let Some(chunk) = params["update"]["content"]["text"].as_str()
            {
                if text.len().saturating_add(chunk.len()) > MAX_TEXT {
                    bail!("quota response too large")
                }
                text.push_str(chunk);
            }
        }
        Some(Incoming::Closed { .. }) | None => bail!("quota inspection ended"),
    }
    Ok(())
}

fn parse(text: &str) -> Vec<Window> {
    let mut in_limits = false;
    let mut group = String::new();
    let mut windows = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('#') {
            in_limits = line.trim_start_matches('#').trim() == "Quota Limits";
            continue;
        }
        if !in_limits {
            continue;
        }
        let Some(item) = line.strip_prefix("- **") else { continue };
        if let Some(label) = item.strip_suffix("**") {
            label.clone_into(&mut group);
            continue;
        }
        let Some((label, value)) = item.split_once(":** ") else { continue };
        let Some((remaining, reset)) = value.split_once("% remaining") else { continue };
        let Ok(remaining) = remaining.parse::<f64>() else { continue };
        if !remaining.is_finite() || !(0.0..=100.0).contains(&remaining) || group.is_empty() {
            continue;
        }
        let bucket = label.trim_end_matches(" Remaining");
        let resets = reset.trim().strip_prefix("(resets ").and_then(|s| s.strip_suffix(')'));
        let at = resets
            .and_then(|s| NaiveDateTime::parse_from_str(s, "%b %d, %Y, %H:%M UTC").ok())
            .map(|t| t.and_utc().timestamp_millis());
        windows.push(Window {
            id: format!("{group}:{bucket}"),
            label: format!("{group} · {bucket}"),
            percent: 100.0 - remaining,
            resets_at: at,
            resets_text: resets.filter(|_| at.is_none()).map(str::to_owned),
            text: None,
        });
    }
    windows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_sends_usage_to_old_or_unknown_adapters() {
        for version in ["1.3.0", "1.4.0", "1.4.1-beta", "unknown", "1.4"] {
            assert!(!supports_local_usage(&json!({"agentInfo": {"version": version}})));
        }
        assert!(!supports_local_usage(&json!({})));
        assert!(supports_local_usage(&json!({"agentInfo": {"version": "1.4.1"}})));
        assert!(supports_local_usage(&json!({"agentInfo": {"version": "1.5.0"}})));
    }

    #[test]
    fn keeps_model_groups_separate_and_converts_remaining_to_used() {
        let windows = parse(
            "### Quota Limits\n- **Gemini Models**\n  - **Weekly Limit Remaining:** 96% remaining (resets Oct 29, 2026, 10:12 UTC)\n- **Claude and GPT models**\n  - **Weekly Limit Remaining:** 100% remaining\n### Other\n- **Weekly Limit Remaining:** 50% remaining",
        );
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].percent, 4.0);
        assert_eq!(windows[1].percent, 0.0);
        assert_ne!(windows[0].id, windows[1].id);
        assert!(windows[0].resets_at.is_some());
        assert!(windows[1].resets_at.is_none());
    }

    #[test]
    fn unavailable_or_invalid_numbers_are_not_zero_usage() {
        assert_eq!(parse("Not applicable for current authentication method").len(), 0);
        assert_eq!(parse("### Quota Limits\n- **Gemini Models**\n- **Weekly Limit Remaining:** NaN% remaining\n- **Weekly Limit Remaining:** 101% remaining").len(), 0);
    }

    #[test]
    fn separate_accounts_have_separate_report_identities() {
        assert_ne!(report_id(None), report_id(Some("acc_12345678")));
        assert_ne!(report_id(Some("acc_12345678")), report_id(Some("acc_87654321")));
    }
}

// The language server of the running app (or `agy`) also reports quotas, staying on 127.0.0.1. When nothing is
// running there is no window: the cloud fallback would send the saved login to Google.

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
        let windows = parse_quota(&body);
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

pub(super) fn parse_quota(body: &Value) -> Vec<Window> {
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
mod language_server_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_summary_is_the_share_used() {
        let body = json!({"response":{"groups":[{"buckets":[
            {"bucketId":"gemini-5h","remainingFraction":0.2,"resetTime":"2026-10-25T12:00:00Z"},
            {"bucketId":"3p-weekly","remainingFraction":0.5}
        ]}]}});
        let w = parse_quota(&body);
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
