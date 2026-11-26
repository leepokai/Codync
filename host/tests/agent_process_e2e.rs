//! A real host and an agent that starts a background program: it must not outlive the agent's crash
//! or the host's shutdown.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn pid(folder: &Path, file: &str) -> String {
    std::fs::read_to_string(folder.join(file)).unwrap().trim().to_owned()
}

fn alive(pid: &str) -> bool {
    Command::new("kill").args(["-0", pid]).stderr(Stdio::null()).status().unwrap().success()
}

async fn gone_within(pid: &str, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while alive(pid) {
        if Instant::now() > deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    true
}

#[tokio::test]
async fn a_crashed_agent_takes_its_background_program_with_it() {
    if common::python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Builder", "background_agent.py", "").await;
    host.reply(&bot, "go").await;
    let folder = common::folder(&host.home, "Builder");
    let (agent, background) = (pid(&folder, "agent.pid"), pid(&folder, "background.pid"));
    assert!(alive(&background), "the background program should be running");
    common::kill(&agent);
    assert!(gone_within(&background, Duration::from_secs(10)).await, "the background program outlived its agent");
}

#[tokio::test]
async fn stopping_the_host_takes_the_background_program_with_it() {
    if common::python_missing() {
        return;
    }
    let mut host = Host::start().await;
    let bot = host.fixture_bot("Builder", "background_agent.py", "").await;
    host.reply(&bot, "go").await;
    let background = pid(&common::folder(&host.home, "Builder"), "background.pid");
    assert!(alive(&background), "the background program should be running");
    host.restart(|_| {}).await;
    assert!(gone_within(&background, Duration::from_secs(15)).await, "the background program outlived the host");
}
