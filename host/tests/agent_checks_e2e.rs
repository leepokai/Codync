//! The computer's agent CLIs and the host: a test host never starts them, and a host that stops ends the checks it was
//! running.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A `claude` first on the PATH that logs its process id and arguments, then never finishes. The PATH it's on, and its log.
fn slow_claude(home: &Path) -> (OsString, PathBuf) {
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = home.join("claude.log");
    let script = format!("#!/bin/sh\necho \"$$ $*\" >> '{}'\nexec sleep 300\n", log.display());
    let claude = bin.join("claude");
    std::fs::write(&claude, script).unwrap();
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(
        std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())),
    )
    .unwrap();
    (path, log)
}

fn alive(pid: &str) -> bool {
    std::process::Command::new("kill").args(["-0", pid]).stderr(std::process::Stdio::null()).status().unwrap().success()
}

#[tokio::test]
async fn a_stopped_host_ends_the_checks_it_was_running() {
    let home = std::env::temp_dir().join(format!("codync-agent-checks-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&home).unwrap();
    let (path, log) = slow_claude(&home);
    let env = [("PATH", path.as_os_str()), ("CODYNC_AGENT_CHECKS", "on".as_ref())];
    let (mut host, _) = common::serve_with(&home, &env).await;
    let read = || std::fs::read_to_string(&log).unwrap_or_default();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !read().contains("/usage") {
        assert!(Instant::now() < deadline, "the host never ran its usage check: {}", read());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    common::stop(&mut host);
    let lines = read();
    let pids: Vec<&str> = lines.lines().filter_map(|line| line.split_whitespace().next()).collect();
    let deadline = Instant::now() + Duration::from_secs(5);
    while pids.iter().any(|pid| alive(pid)) {
        assert!(Instant::now() < deadline, "still running after the host stopped: {lines}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let _ = std::fs::remove_dir_all(&home);
}

#[tokio::test]
async fn a_test_host_never_runs_the_agent_clis() {
    let home = std::env::temp_dir().join(format!("codync-agent-checks-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&home).unwrap();
    let (path, log) = slow_claude(&home);
    let (mut host, base) = common::serve_with(&home, &[("PATH", path.as_os_str())]).await;
    let token = std::fs::read_to_string(home.join("token")).unwrap();
    for method in ["refreshBackends", "usage"] {
        let response = reqwest::Client::new()
            .post(format!("{base}/api/{method}"))
            .bearer_auth(token.trim())
            .json(&serde_json::json!({"refresh": true}))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "{method}: {}", response.status());
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(std::fs::read_to_string(&log).unwrap_or_default(), "");
    common::stop(&mut host);
    let _ = std::fs::remove_dir_all(&home);
}
