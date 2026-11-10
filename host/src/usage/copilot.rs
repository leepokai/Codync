//! GitHub Copilot's plan: `account.getQuota` on the CLI's headless server, with no session and no
//! prompt. The server asks GitHub itself; signed out, it answers with an error and opens nothing.
//! Its protocol frames messages with `Content-Length` headers, unlike the agents' ACP.

use crate::LockExt;
use crate::hub::Hub;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::path::Path;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};

use super::reading::{self, UsageReadStage};
use super::{Window, iso_ms};

const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// ponytail: each start leaves an empty log file in `~/.copilot/logs`, even with `--log-level none`,
/// so this reads less often than the other agents. Back to every poll if the CLI stops doing that.
const READ_EVERY: Duration = Duration::from_mins(30);
/// A failure tries again sooner: a `copilot login` shouldn't wait out [`READ_EVERY`].
const RETRY_AFTER: Duration = Duration::from_mins(10);

static LAST_READ: Mutex<Option<LastRead>> = Mutex::new(None);

struct LastRead {
    at: Instant,
    /// `None`: it failed.
    windows: Option<Vec<Window>>,
}

impl LastRead {
    fn due(&self, now: Instant) -> bool {
        let wait = if self.windows.is_some() { READ_EVERY } else { RETRY_AFTER };
        now.duration_since(self.at) >= wait
    }

    /// What a read between server starts lands. The reading held stands, with its own time; one
    /// that was dropped (the agent turned off and on again) comes back from here.
    fn between_starts(&self, held: bool) -> Option<Vec<Window>> {
        if held { None } else { self.windows.clone() }
    }
}

pub(super) async fn collect(hub: &Hub) -> Option<Vec<Window>> {
    let now = Instant::now();
    if let Some(last) = LAST_READ.locked().as_ref().filter(|last| !last.due(now)) {
        return last.between_starts(hub.usage.locked().providers.iter().any(|provider| provider.id == "copilot"));
    }
    let bin = super::command::program("copilot")?;
    let windows = reading::guarded(hub, "copilot", read(&bin)).await;
    *LAST_READ.locked() = Some(LastRead { at: now, windows: windows.clone() });
    windows
}

async fn read(bin: &Path) -> Result<Vec<Window>, UsageReadStage> {
    let mut command = tokio::process::Command::new(bin);
    command
        .args(["--headless", "--no-auto-update", "--log-level", "none", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    if let Some(home) = dirs::home_dir() {
        command.current_dir(home);
    }
    let mut child = command.spawn().map_err(|_| UsageReadStage::Start)?;
    let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        return Err(UsageReadStage::Start);
    };
    let reply = tokio::time::timeout(READ_TIMEOUT, read_quota(stdin, stdout))
        .await
        .map_err(|_| UsageReadStage::Timeout)?
        .map_err(|error| {
            tracing::debug!(error = format!("{error:#}"), "couldn't read Copilot's quota");
            UsageReadStage::Prompt
        })?;
    parse(&reply, crate::store::now_ms()).ok_or(UsageReadStage::Parse)
}

async fn read_quota(mut stdin: ChildStdin, stdout: ChildStdout) -> Result<Value> {
    let mut reader = BufReader::new(stdout);
    write_frame(&mut stdin, 1, "connect").await?;
    write_frame(&mut stdin, 2, "account.getQuota").await?;
    loop {
        let message = read_frame(&mut reader).await?;
        if message["id"] != 2 {
            continue;
        }
        if !message["error"].is_null() {
            bail!("the server refused: {}", message["error"]["message"]);
        }
        return Ok(message["result"].clone());
    }
}

async fn write_frame(stdin: &mut (impl AsyncWrite + Unpin), id: i64, method: &str) -> Result<()> {
    let body = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": {}}).to_string();
    stdin.write_all(format!("Content-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await?;
    Ok(stdin.flush().await?)
}

async fn read_frame(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<Value> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            bail!("the server closed");
        }
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse::<usize>().ok();
        }
    }
    let mut body = vec![0; length.context("a message without a length")?];
    reader.read_exact(&mut body).await?;
    Ok(serde_json::from_slice(&body)?)
}

/// One bar per capped quota. A plan with none is a note, as Cursor's unlimited plan is.
/// `None`: not the reply the host knows, so the server's format changed.
fn parse(reply: &Value, now_ms: i64) -> Option<Vec<Window>> {
    let snapshots = reply["quotaSnapshots"].as_object()?;
    let mut windows: Vec<Window> = snapshots.iter().filter_map(|(id, snapshot)| capped(id, snapshot, now_ms)).collect();
    if windows.is_empty() && snapshots.values().any(|s| s["isUnlimitedEntitlement"] == true) {
        windows.push(Window::note("unlimited", "Plan", "Unlimited"));
    }
    Some(windows)
}

fn capped(id: &str, snapshot: &Value, now_ms: i64) -> Option<Window> {
    if snapshot["isUnlimitedEntitlement"] == true {
        return None;
    }
    let left = snapshot["remainingPercentage"].as_f64().filter(|left| left.is_finite())?;
    let label = match id {
        "premium_interactions" => "Premium requests",
        "chat" => "Chat",
        "completions" => "Completions",
        other => other,
    };
    let resets = snapshot["resetDate"]
        .as_str()
        .and_then(iso_ms)
        // Some plans report the moment of the request here.
        .filter(|at| *at > now_ms + 3_600_000);
    Some(Window::new(id, label, (100.0 - left).clamp(0.0, 100.0), resets))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_793_900_000_000;

    /// As the 1.0.105 server answered a real login.
    fn reply() -> Value {
        json!({"quotaSnapshots": {
            "chat": {"isUnlimitedEntitlement": true, "entitlementRequests": 0, "remainingPercentage": 100,
                "resetDate": "2026-12-01T00:00:00.000Z"},
            "completions": {"isUnlimitedEntitlement": true, "remainingPercentage": 100},
            "premium_interactions": {"isUnlimitedEntitlement": false, "entitlementRequests": 200, "usedRequests": 50,
                "remainingPercentage": 75, "resetDate": "2026-12-01T00:00:00.000Z"}
        }})
    }

    #[test]
    fn only_capped_quotas_are_bars() {
        let w = parse(&reply(), NOW).unwrap();
        assert_eq!(w.len(), 1);
        assert_eq!((w[0].id.as_str(), w[0].label.as_str()), ("premium_interactions", "Premium requests"));
        assert!((w[0].percent - 25.0).abs() < 1e-9);
        assert!(w[0].resets_at.is_some());
    }

    #[test]
    fn a_reset_already_past_is_not_shown() {
        let long_after_it = 2_000_000_000_000;
        let w = parse(&reply(), long_after_it).unwrap();
        assert_eq!(w[0].resets_at, None);
    }

    #[test]
    fn a_plan_with_no_caps_is_a_note() {
        let w = parse(&json!({"quotaSnapshots": {"chat": {"isUnlimitedEntitlement": true}}}), NOW).unwrap();
        assert_eq!(w[0].text.as_deref(), Some("Unlimited"));
    }

    #[test]
    fn a_reply_without_quotas_is_a_changed_format() {
        assert_eq!(parse(&json!({}), NOW), None);
        assert_eq!(parse(&json!({"quotaSnapshots": {"chat": {"remainingPercentage": "lots"}}}), NOW), Some(vec![]));
    }

    #[test]
    fn the_server_starts_at_most_once_per_interval_and_sooner_after_a_failure() {
        let at = Instant::now();
        let read = LastRead { at, windows: Some(Vec::new()) };
        assert!(!read.due(at + RETRY_AFTER));
        assert!(read.due(at + READ_EVERY));
        let failed = LastRead { at, windows: None };
        assert!(!failed.due(at + Duration::from_mins(5)));
        assert!(failed.due(at + RETRY_AFTER));
    }

    #[test]
    fn a_reading_turned_off_and_on_again_comes_back_without_a_new_start() {
        let windows = parse(&reply(), NOW);
        let last = LastRead { at: Instant::now(), windows: windows.clone() };
        assert_eq!(last.between_starts(false), windows, "the Usage screen has its bars again");
        assert_eq!(last.between_starts(true), None, "a reading held keeps the time it was made");
        assert_eq!(LastRead { at: Instant::now(), windows: None }.between_starts(false), None);
    }

    #[tokio::test]
    async fn messages_are_framed_with_their_length() {
        let (mut ours, theirs) = tokio::io::duplex(1024);
        let (their_read, mut their_write) = tokio::io::split(theirs);
        write_frame(&mut ours, 7, "account.getQuota").await.unwrap();
        let sent = read_frame(&mut BufReader::new(their_read)).await.unwrap();
        assert_eq!((sent["id"].as_i64(), sent["method"].as_str()), (Some(7), Some("account.getQuota")));

        let body = r#"{"id":2,"result":{}}"#;
        let frame = format!("content-length: {}\r\nContent-Type: application/json\r\n\r\n{body}", body.len());
        their_write.write_all(frame.as_bytes()).await.unwrap();
        let answer = read_frame(&mut BufReader::new(ours)).await.unwrap();
        assert_eq!(answer["id"], 2, "the header's name goes by any case, among others");
    }
}
