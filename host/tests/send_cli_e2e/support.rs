//! A real host (`common::host::Host`) and `codync-host send` run against it.

use super::common::host::Host as RealHost;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::{Output, Stdio};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

static HOSTS: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(1)));

pub struct Host {
    host: RealHost,
    // Dropped after the host has stopped.
    _serial: OwnedSemaphorePermit,
}

impl std::ops::Deref for Host {
    type Target = RealHost;

    fn deref(&self) -> &RealHost {
        &self.host
    }
}

impl Host {
    pub async fn start() -> Self {
        // Isolate process-heavy cases from each other on small Windows runners.
        let serial = HOSTS.clone().acquire_owned().await.unwrap();
        let host = RealHost::start().await;
        host.call("setAnalytics", json!({"enabled":false})).await;
        Self { host, _serial: serial }
    }

    /// A bot running `send_cli_agent.py` that asks before using tools.
    pub async fn bot(&self, name: &str) -> String {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/send_cli_agent.py");
        let bot = json!({"name":name,"backend":"custom","command":super::common::python_agent(&fixture),
            "permission":"ask","connectors":[],"notify":false});
        self.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned()
    }

    pub fn command(&self, args: &[&str]) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_codync-host"));
        command.arg("send");
        if !args.contains(&"--port") {
            command.args(["--port", self.base.rsplit(':').next().unwrap()]);
        }
        command
            .env("CODYNC_HOME", &self.home)
            .env("CODYNC_CLOUD", "off")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        command
    }

    pub async fn run(&self, args: &[&str]) -> Output {
        tokio::time::timeout(Duration::from_secs(45), self.command(args).output()).await.unwrap().unwrap()
    }

    pub async fn stdin(&self, bot: &str, message: &str) -> Output {
        let mut command = self.command(&["--bot", bot, "--file", "-", "--wait"]);
        command.stdin(Stdio::piped());
        let mut child = command.spawn().unwrap();
        child.stdin.take().unwrap().write_all(message.as_bytes()).await.unwrap();
        tokio::time::timeout(Duration::from_secs(45), child.wait_with_output()).await.unwrap().unwrap()
    }

    /// The newest entry of `bot`'s chat that matches, once there is one.
    pub async fn entry(&self, bot: &str, predicate: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let value = self.call("history", json!({"botId":bot})).await;
            if let Some(entry) = value["entries"].as_array().unwrap().iter().rev().find(|entry| predicate(entry)) {
                return entry.clone();
            }
            assert!(Instant::now() < deadline, "missing expected fixture entry");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

pub fn json_output(output: &Output, code: i32) -> Value {
    assert_eq!(output.status.code(), Some(code), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout.clone()).unwrap();
    assert_eq!(text.lines().count(), 1, "JSON output must occupy one line");
    serde_json::from_str(&text).unwrap()
}
