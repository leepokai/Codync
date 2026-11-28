//! The computer's agent CLIs and the host: a test host never starts them, and a host that stops ends the checks it was
//! running.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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
        .json(&serde_json::json!({"refresh": true, "backend": "cursor", "id": "claude", "shown": true}))
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
        ("CODYNC_AGENT_CHECKS", "claude,cursor".as_ref()),
    ];
    let (mut host, base) = common::serve_with(&home, &env).await;
    // No bot runs Claude here, so its usage is only read once it's asked for.
    assert!(call(&base, &home, "setUsageShown").send().await.unwrap().status().is_success());
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
    // Agents that never exit hold the host's stop for its exit grace (10 s) and unwind (15 s), with room on a busy machine.
    common::eventually("the host stopped", Duration::from_secs(40), async || host.try_wait().unwrap().is_some()).await;
    let lines = read();
    let pids: Vec<&str> = lines.lines().filter_map(|line| line.split_whitespace().next()).collect();
    common::eventually("the checks ended with the host", Duration::from_secs(5), async || {
        !pids.iter().any(|pid| common::alive(pid))
    })
    .await;
    let _ = std::fs::remove_dir_all(&home);
}

/// The background reading starts the agent so that it can't open a browser, and asks it without a session.
#[tokio::test]
async fn a_usage_reading_starts_the_agent_with_no_browser_to_open() {
    if common::python_missing() {
        return;
    }
    let home = std::env::temp_dir().join(format!("codync-agent-checks-{}", uuid::Uuid::new_v4()));
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/agent/tests/fixtures/usage_command_agent.py");
    // Each start logs to a file named for the `BROWSER` it got: the host's sign-in checks start the agent too.
    let script = format!(
        "#!/bin/sh\nUSAGE_AGENT_LOG=\"{home}/calls-${{BROWSER:-none}}.log\" USAGE_AGENT_COMMAND=usage PATH='{path}' exec {python} '{fixture}'\n",
        home = home.display(),
        path = std::env::var("PATH").unwrap(),
        python = common::python(),
        fixture = fixture.display(),
    );
    let file = bin.join("grok");
    std::fs::write(&file, script).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths([bin.as_path(), Path::new("/usr/bin"), Path::new("/bin")]).unwrap();
    let env = [
        ("PATH", path.as_os_str()),
        ("HOME", home.as_os_str()),
        ("ZDOTDIR", home.as_os_str()),
        ("CODYNC_AGENT_CHECKS", "grok".as_ref()),
    ];
    let (mut host, base) = common::serve_with(&home, &env).await;
    let token = std::fs::read_to_string(home.join("token")).unwrap();
    let client = reqwest::Client::new();
    let post = |method: &str, body: serde_json::Value| {
        client.post(format!("{base}/api/{method}")).bearer_auth(token.trim()).json(&body).send()
    };
    let turned_on = post("setUsageShown", serde_json::json!({"id": "grok", "shown": true})).await.unwrap();
    assert!(turned_on.status().is_success());
    let read = || std::fs::read_to_string(home.join("calls-true.log")).unwrap_or_default();
    common::eventually("the host read Grok's plan", Duration::from_secs(30), async || read().contains("x.ai/billing"))
        .await;
    let calls = read();
    assert!(!calls.contains("session/"), "no session and no prompt: {calls}");
    common::eventually("Grok's reading landed", Duration::from_secs(10), async || {
        let usage: serde_json::Value = post("usage", serde_json::json!({})).await.unwrap().json().await.unwrap();
        usage["providers"][0]["id"] == "grok" && usage["providers"][0]["windows"][0]["label"] == "Weekly"
    })
    .await;
    common::stop(&mut host);
    let _ = std::fs::remove_dir_all(&home);
}

#[tokio::test]
async fn a_test_host_never_runs_the_agent_clis() {
    let home = std::env::temp_dir().join(format!("codync-agent-checks-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&home).unwrap();
    let (path, log) = slow_agents(&home);
    let (mut host, base) = common::serve_with(&home, &[("PATH", path.as_os_str())]).await;
    for method in ["refreshBackends", "setUsageShown", "usage", "agentAuth"] {
        let response = call(&base, &home, method).send().await.unwrap();
        let status = response.status();
        let body = response.text().await.unwrap();
        assert!(status.is_success(), "{method}: {status} {body}");
    }
    // A model list starts the agent too, so a test host answers that it can't.
    let models = call(&base, &home, "agentModels").send().await.unwrap();
    assert!(!models.status().is_success(), "agentModels ran on a test host");
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(std::fs::read_to_string(&log).unwrap_or_default(), "");
    common::stop(&mut host);
    let _ = std::fs::remove_dir_all(&home);
}

/// The session folders oh-my-pi keeps in `agent_dir` whose names hold `home`'s own folder name: none while
/// it has no `sessions` folder yet. Any other read error fails the test, so an unreadable folder never passes.
fn omp_sessions(agent_dir: &Path, home: &Path) -> Vec<String> {
    let name = home.file_name().unwrap().to_string_lossy().into_owned();
    let dir = agent_dir.join("sessions");
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => panic!("reading {}: {e}", dir.display()),
    };
    entries
        .map(|entry| entry.unwrap_or_else(|e| panic!("reading {}: {e}", dir.display())).file_name())
        .map(|folder| folder.to_string_lossy().into_owned())
        .filter(|folder| folder.contains(&name))
        .collect()
}

/// oh-my-pi opens a session for the host's model check: it lands in the test home's agent directory, not in the
/// `~/.omp/agent` of the home it runs in (a throwaway one here, so a failure never touches the user's).
#[tokio::test]
async fn agent_probes_leave_no_session_in_the_users_omp() {
    let path = std::env::var_os("PATH").unwrap();
    let Some(omp) = std::env::split_paths(&path).map(|dir| dir.join("omp")).find(|bin| bin.is_file()) else {
        eprintln!("skipping: oh-my-pi not installed");
        return;
    };
    let home = std::env::temp_dir().join(format!("codync-agent-checks-{}", uuid::Uuid::new_v4()));
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink(&omp, bin.join("omp")).unwrap();
    let path = std::env::join_paths([bin.as_path(), Path::new("/usr/bin"), Path::new("/bin")]).unwrap();
    let env = [
        ("PATH", path.as_os_str()),
        ("HOME", home.as_os_str()),
        ("ZDOTDIR", home.as_os_str()),
        ("CODYNC_AGENT_CHECKS", "omp".as_ref()),
    ];
    let (mut host, base) = common::serve_with(&home, &env).await;
    let token = std::fs::read_to_string(home.join("token")).unwrap();
    // The lead bot's own sign-in check starts oh-my-pi as the host comes up: two of its processes on one agent
    // directory can lose one, so a probe that failed that way is tried again once the check is done.
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let models = reqwest::Client::new()
            .post(format!("{base}/api/agentModels"))
            .bearer_auth(token.trim())
            .json(&serde_json::json!({"backend": "omp"}))
            .send()
            .await
            .unwrap();
        let status = models.status();
        let body = models.text().await.unwrap();
        if status.is_success() {
            break;
        }
        assert!(Instant::now() < deadline, "agentModels: {status} {body}");
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert_eq!(omp_sessions(&home.join(".omp/agent"), &home), Vec::<String>::new(), "a probe session in ~/.omp");
    assert_ne!(omp_sessions(&common::agent_dir(&home), &home), Vec::<String>::new(), "no probe session");
    common::stop(&mut host);
    let _ = std::fs::remove_dir_all(&home);
}
