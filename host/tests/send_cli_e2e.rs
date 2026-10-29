//! A real host and fake ACP agent exercised through the non-interactive CLI.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
#[path = "send_cli_e2e/support.rs"]
mod support;
#[path = "send_cli_e2e/transport.rs"]
mod transport;

use serde_json::json;
use std::time::{Duration, Instant};
use support::{Host, json_output};
use transport::{MockHost, Mode};

#[tokio::test]
async fn sends_plain_text_json_and_an_unwaited_acknowledgement() {
    let host = Host::start().await;
    let bot = host.bot("Worker").await;
    let plain = host.run(&["--bot", &bot, "--wait", "CLI_MODE=normal"]).await;
    assert_eq!(plain.status.code(), Some(0), "{}", String::from_utf8_lossy(&plain.stderr));
    assert_eq!(String::from_utf8(plain.stdout).unwrap(), "CLI reply\n");
    let output = host.run(&["--bot", &bot, "--wait", "--json", "CLI_MODE=normal"]).await;
    let result = json_output(&output, 0);
    assert_eq!(result["botId"], bot);
    assert_eq!(result["status"], "completed");
    assert_eq!(result["stopReason"], "end_turn");
    assert_eq!(result["reply"], "CLI reply");
    assert!(result["entrySeq"].as_i64().unwrap() > 0);
    assert!(result["turn"].as_i64().unwrap() > 0);
    assert_eq!(result["notices"].as_array().unwrap().as_slice(), [] as [serde_json::Value; 0]);
    assert!(result["elapsedSeconds"].as_f64().unwrap() >= 0.0);
    assert_eq!(result["waitedForApprovalSeconds"], 0.0);
    let ack = host.run(&["--bot", &bot, "CLI_MODE=normal"]).await;
    assert_eq!(ack.status.code(), Some(0));
    let ack = String::from_utf8(ack.stdout).unwrap();
    assert!(ack.starts_with("seq=") && ack.contains(" turn="));
}

#[tokio::test]
async fn returns_failure_for_limits_refusals_agent_errors_and_a_stopped_turn() {
    let host = Host::start().await;
    let bot = host.bot("Failures").await;
    for reason in ["max_turn_requests", "max_tokens", "refusal", "error"] {
        let message = format!("CLI_MODE={reason}");
        let result = json_output(&host.run(&["--bot", &bot, "--wait", "--json", &message]).await, 1);
        assert_eq!(result["status"], "failed");
        assert_eq!(result["stopReason"], reason);
        assert_ne!(result["notices"].as_array().unwrap().as_slice(), [] as [serde_json::Value; 0]);
    }
    let child = host.command(&["--bot", &bot, "--wait", "--json", "CLI_MODE=hang"]).spawn().unwrap();
    host.entry(&bot, |e| e["data"]["text"] == "Waiting indefinitely").await;
    host.call("stop", json!({"botId":bot})).await;
    let output = tokio::time::timeout(Duration::from_secs(30), child.wait_with_output()).await.unwrap().unwrap();
    let result = json_output(&output, 1);
    assert_eq!(result["status"], "failed");
    assert_eq!(result["stopReason"], "cancelled");
}

#[tokio::test]
async fn a_timeout_leaves_the_bot_working() {
    let host = Host::start().await;
    let bot = host.bot("Timeout").await;
    json_output(&host.run(&["--bot", &bot, "--wait", "--json", "CLI_MODE=normal"]).await, 0);
    let started = Instant::now();
    let output = host.run(&["--bot", &bot, "--wait", "--json", "--timeout", "2", "CLI_MODE=hang"]).await;
    let result = json_output(&output, 4);
    assert_eq!(result["status"], "timeout");
    assert!(started.elapsed() < Duration::from_secs(8));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("timed out after 2s (the bot is still working; nothing was stopped)")
    );
    let snapshot = common::snapshot(&host.base, &host.token).await;
    assert_eq!(snapshot["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot).unwrap()["status"], "working");
    host.call("stop", json!({"botId":bot})).await;
}

#[tokio::test]
async fn approval_wait_is_reported_and_excluded_from_the_timeout() {
    let host = Host::start().await;
    let bot = host.bot("Approval").await;
    json_output(&host.run(&["--bot", &bot, "--wait", "--json", "CLI_MODE=normal"]).await, 0);
    let child =
        host.command(&["--bot", &bot, "--wait", "--json", "--timeout", "2", "CLI_MODE=permission"]).spawn().unwrap();
    let card = host.entry(&bot, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    host.call("respondPermission", json!({"entryId":card["id"],"optionId":"allow"})).await;
    let output = tokio::time::timeout(Duration::from_secs(30), child.wait_with_output()).await.unwrap().unwrap();
    let result = json_output(&output, 0);
    assert_eq!(result["reply"], "Approved reply");
    assert!(result["waitedForApprovalSeconds"].as_f64().unwrap() >= 2.5);
    assert!(result["elapsedSeconds"].as_f64().unwrap() >= 3.0);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.matches("waiting for approval:").count(), 1);
    assert!(stderr.contains("Read fixture files"));
}

#[tokio::test]
async fn resolves_exact_names_and_rejects_missing_or_ambiguous_names() {
    let host = Host::start().await;
    let first = host.bot("Reviewer").await;
    let result = json_output(&host.run(&["--bot", "rEvIeWeR", "--wait", "--json", "CLI_MODE=normal"]).await, 0);
    assert_eq!(result["botId"], first);
    let missing = host.run(&["--bot", "missing-name", "test"]).await;
    assert_eq!(missing.status.code(), Some(2));
    assert_eq!(missing.stdout, [] as [u8; 0]);
    let second = host.bot("REVIEWER").await;
    let ambiguous = host.run(&["--bot", "reviewer", "test"]).await;
    assert_eq!(ambiguous.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&ambiguous.stderr);
    assert!(stderr.contains(&first) && stderr.contains(&second));
    json_output(&host.run(&["--bot", &first, "--wait", "--json", "CLI_MODE=normal"]).await, 0);
}

#[tokio::test]
async fn accepts_utf8_files_stdin_and_quote_replies_without_exposing_the_token() {
    let host = Host::start().await;
    let bot = host.bot("Input").await;
    let file = host.home.join("message.txt");
    std::fs::write(&file, "CLI_MODE=echo\nCLI_TEXT=Read-only UTF-8: 測試").unwrap();
    let output = host.run(&["--bot", &bot, "--wait", "--file", file.to_str().unwrap()]).await;
    assert_eq!(output.status.code(), Some(0), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "Read-only UTF-8: 測試\n");
    let output = host.stdin(&bot, "CLI_MODE=echo\nCLI_TEXT=stdin reply").await;
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "stdin reply\n");
    let quoted = host.call("send", json!({"botId":bot,"text":"CLI_MODE=normal"})).await["entry"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    host.entry(&bot, |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    let result =
        json_output(&host.run(&["--bot", &bot, "--wait", "--json", "--reply-to", &quoted, "CLI_MODE=normal"]).await, 0);
    assert_eq!(result["reply"], "CLI reply");
    let sent = host.entry(&bot, |e| e["seq"] == result["entrySeq"]).await;
    assert_eq!(sent["data"]["replyTo"]["id"], quoted.as_str());
    let message = format!("CLI_MODE=echo\nCLI_TEXT={}", host.token);
    let output = host.run(&["--bot", &bot, "--wait", "--json", &message]).await;
    let result = json_output(&output, 0);
    assert_eq!(result["reply"], "[redacted]");
    assert!(!String::from_utf8_lossy(&output.stderr).contains(&host.token));
    assert!(!String::from_utf8_lossy(&output.stdout).contains(&host.token));
}

#[tokio::test]
async fn queued_calls_wait_for_their_combined_turn_instead_of_an_older_reply() {
    let host = Host::start().await;
    let bot = host.bot("Queue").await;
    host.call("send", json!({"botId":bot,"text":"CLI_MODE=hold"})).await;
    host.entry(&bot, |e| e["data"]["text"] == "OLD is working").await;
    let first = host.command(&["--bot", &bot, "--wait", "--json", "CLI_MODE=normal"]).spawn().unwrap();
    let second = host.command(&["--bot", &bot, "--wait", "--json", "CLI_MODE=normal"]).spawn().unwrap();
    let (first, second) = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::join!(first.wait_with_output(), second.wait_with_output())
    })
    .await
    .unwrap();
    let first = json_output(&first.unwrap(), 0);
    let second = json_output(&second.unwrap(), 0);
    assert_eq!(first["reply"], "CLI reply");
    assert_eq!(second["reply"], "CLI reply");
    assert_eq!(first["turn"], second["turn"]);
    assert_ne!(first["entrySeq"], second["entrySeq"]);
}

#[tokio::test]
async fn help_arguments_and_unreachable_hosts_use_the_documented_exit_codes() {
    let host = Host::start().await;
    let help = host.run(&["--help"]).await;
    assert_eq!(help.status.code(), Some(0));
    let help = String::from_utf8(help.stdout).unwrap();
    for flag in ["--bot", "--file", "--wait", "--timeout", "--json", "--reply-to", "--port", "Exit codes:"] {
        assert!(help.contains(flag), "help must describe {flag}");
    }
    for args in [
        vec!["--bot", "missing"],
        vec!["--bot", "missing", "--file", "-", "text"],
        vec!["--bot", "missing", "--timeout", "0", "text"],
    ] {
        assert_eq!(host.run(&args).await.status.code(), Some(2));
    }
    let invalid = host.home.join("invalid.txt");
    std::fs::write(&invalid, [0xff]).unwrap();
    assert_eq!(host.run(&["--bot", "missing", "--file", invalid.to_str().unwrap()]).await.status.code(), Some(2));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    drop(listener);
    let output = host.run(&["--port", &port, "--bot", "missing", "text"]).await;
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(output.stdout, [] as [u8; 0]);
    assert!(String::from_utf8_lossy(&output.stderr).contains("codync-host isn't running here. Start it with"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(&host.token));
}

#[tokio::test]
async fn a_final_reply_waits_for_the_working_turn_to_become_idle() {
    let host = Host::start().await;
    let mock = MockHost::start(Mode::EarlyFinal, &host.token).await;
    let started = Instant::now();
    let output = host.run(&["--port", &mock.port, "--bot", "Transport", "--wait", "--json", "test"]).await;
    let result = json_output(&output, 0);
    assert_eq!(result["reply"], "Early final");
    assert!(started.elapsed() >= Duration::from_millis(1100));
    assert!(result["elapsedSeconds"].as_f64().unwrap() >= 1.1);
}

#[tokio::test]
async fn an_ambiguous_send_response_is_retried_with_the_same_nonce() {
    let host = Host::start().await;
    let mock = MockHost::start(Mode::DroppedResponse, &host.token).await;
    json_output(&host.run(&["--port", &mock.port, "--bot", "Transport", "--json", "test"]).await, 0);
    let requests = mock.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["clientNonce"], requests[1]["clientNonce"]);
    assert!(uuid::Uuid::parse_str(requests[0]["clientNonce"].as_str().unwrap()).is_ok());
}

#[tokio::test]
async fn a_refused_request_does_not_retry_or_reveal_the_token() {
    let host = Host::start().await;
    let mock = MockHost::start(Mode::Rejected, &host.token).await;
    let output = host.run(&["--port", &mock.port, "--bot", "Transport", "--json", "test"]).await;
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(output.stdout, [] as [u8; 0]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("denied [redacted]"));
    assert!(!stderr.contains(&host.token));
    assert_eq!(mock.requests().len(), 1);
}

#[tokio::test]
async fn post_acceptance_api_errors_and_lost_events_fail_without_stopping_the_bot() {
    let host = Host::start().await;
    for mode in [Mode::HistoryRejected, Mode::Resync] {
        let mock = MockHost::start(mode, &host.token).await;
        let output = host.run(&["--port", &mock.port, "--bot", "Transport", "--wait", "--json", "test"]).await;
        let result = json_output(&output, 3);
        assert_eq!(result["status"], "failed");
        assert_eq!(result["stopReason"], "error");
        assert_ne!(result["notices"].as_array().unwrap().as_slice(), [] as [serde_json::Value; 0]);
        if mode == Mode::HistoryRejected {
            assert!(result["elapsedSeconds"].as_f64().unwrap() >= 1.1);
            assert!(String::from_utf8_lossy(&output.stderr).contains("denied [redacted]"));
        }
        assert!(!String::from_utf8_lossy(&output.stderr).contains(&host.token));
        assert!(!String::from_utf8_lossy(&output.stdout).contains(&host.token));
        assert_eq!(mock.requests().len(), 1);
    }
}

#[tokio::test]
async fn an_existing_approval_outside_history_is_reported_and_pauses_the_clock() {
    let host = Host::start().await;
    let mock = MockHost::start(Mode::PriorApproval, &host.token).await;
    let output =
        host.run(&["--port", &mock.port, "--bot", "Transport", "--wait", "--json", "--timeout", "2", "test"]).await;
    let result = json_output(&output, 0);
    assert_eq!(result["status"], "completed");
    assert!(result["waitedForApprovalSeconds"].as_f64().unwrap() >= 2.5);
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).matches("waiting for approval: Approve earlier work").count(),
        1
    );
}
