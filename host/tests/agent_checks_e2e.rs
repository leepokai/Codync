//! The computer's agent CLIs and the host: a test host never starts them, and a host that stops ends the checks it was
//! running.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Fake `claude` and `cursor-agent` that log their process id and arguments, then never finish, not even when their stdin
/// closes. The PATH with them and no other agent CLI, and their log.
fn slow_agents(home: &Path) -> (OsString, PathBuf) {
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = home.join("agents.log");
    let script = format!("#!/bin/sh\necho \"$$ $*\" >> '{}'\nexec sleep 300\n", log.display());
    for agent in ["claude", "cursor-agent"] {
        let file = bin.join(agent);
        std::fs::write(&file, &script).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = std::env::join_paths([bin.as_path(), Path::new("/usr/bin"), Path::new("/bin")]).unwrap();
    (path, log)
}

fn call(base: &str, home: &Path, method: &str) -> reqwest::RequestBuilder {
    let token = std::fs::read_to_string(home.join("token")).unwrap();
    reqwest::Client::new()
        .post(format!("{base}/api/{method}"))
        .bearer_auth(token.trim())
        .json(&serde_json::json!({"refresh": true, "backend": "cursor"}))
}

fn alive(pid: &str) -> bool {
    std::process::Command::new("kill").args(["-0", pid]).stderr(std::process::Stdio::null()).status().unwrap().success()
}

#[tokio::test]
async fn a_stopped_host_ends_the_checks_it_was_running() {
    let home = std::env::temp_dir().join(format!("codync-agent-checks-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&home).unwrap();
    let (path, log) = slow_agents(&home);
    // The checks run for real here, so they get a home of their own: the computer's other agent CLIs and logins stay out.
    let env = [
        ("PATH", path.as_os_str()),
        ("HOME", home.as_os_str()),
        ("ZDOTDIR", home.as_os_str()),
        ("CODYNC_AGENT_CHECKS", "on".as_ref()),
    ];
    let (mut host, base) = common::serve_with(&home, &env).await;
    // A check that starts the agent, which never answers: the host is still waiting when it stops.
    tokio::spawn(call(&base, &home, "agentAuth").send());
    let read = || std::fs::read_to_string(&log).unwrap_or_default();
    common::eventually("the host ran its usage, sign-in and agent checks", Duration::from_secs(30), async || {
        let lines = read();
        lines.contains("/usage") && lines.contains("auth status") && lines.contains(" acp")
    })
    .await;
    // Not `common::stop`, which gives up sooner than an agent gets to exit by itself.
    std::process::Command::new("kill").arg(host.id().to_string()).status().unwrap();
    common::eventually("the host stopped", Duration::from_secs(20), async || host.try_wait().unwrap().is_some()).await;
    let lines = read();
    let pids: Vec<&str> = lines.lines().filter_map(|line| line.split_whitespace().next()).collect();
    common::eventually("the checks ended with the host", Duration::from_secs(5), async || {
        !pids.iter().any(|pid| alive(pid))
    })
    .await;
    let _ = std::fs::remove_dir_all(&home);
}

#[tokio::test]
async fn a_test_host_never_runs_the_agent_clis() {
    let home = std::env::temp_dir().join(format!("codync-agent-checks-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&home).unwrap();
    let (path, log) = slow_agents(&home);
    let (mut host, base) = common::serve_with(&home, &[("PATH", path.as_os_str())]).await;
    for method in ["refreshBackends", "usage", "agentAuth"] {
        let response = call(&base, &home, method).send().await.unwrap();
        let status = response.status();
        let body = response.text().await.unwrap();
        // `agentAuth` opens the vault first: a computer with no keyring, like Linux CI, refuses it there, before any agent.
        let no_keyring = method == "agentAuth" && body.contains("Secure credential storage");
        assert!(status.is_success() || no_keyring, "{method}: {status} {body}");
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(std::fs::read_to_string(&log).unwrap_or_default(), "");
    common::stop(&mut host);
    let _ = std::fs::remove_dir_all(&home);
}
