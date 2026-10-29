use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::{Child, Output, Stdio};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

static HOSTS: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(1)));

pub struct Host {
    child: Child,
    pub base: String,
    pub home: PathBuf,
    pub token: String,
    port: u16,
    _serial: OwnedSemaphorePermit,
}

impl Drop for Host {
    fn drop(&mut self) {
        super::common::stop(&mut self.child);
        assert!(self.home.starts_with(std::env::temp_dir()));
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

impl Host {
    pub async fn start() -> Self {
        // Isolate process-heavy cases from each other on small Windows runners.
        let serial = HOSTS.clone().acquire_owned().await.unwrap();
        let home = std::env::temp_dir().join(format!("codync-send-cli-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let (child, base) = super::common::serve(&home).await;
        let token = std::fs::read_to_string(home.join("token")).unwrap().trim().to_owned();
        let port = base.rsplit(':').next().unwrap().parse().unwrap();
        let host = Self { child, base, home, token, port, _serial: serial };
        host.call("setAnalytics", json!({"enabled":false})).await;
        host
    }

    pub async fn call(&self, method: &str, body: Value) -> Value {
        let response = reqwest::Client::new()
            .post(format!("{}/api/{method}", self.base))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = response.status();
        let value: Value = response.json().await.unwrap();
        assert!(status.is_success(), "{method} failed: {value}");
        value
    }

    pub async fn bot(&self, name: &str) -> String {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/send_cli_agent.py");
        self.call(
            "createBot",
            json!({
                "name":name,"backend":"custom","command":super::common::python_agent(&fixture),
                "cwd":self.home,"permission":"ask","connectors":[],"skills":[],"notify":false,
            }),
        )
        .await["bot"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    pub fn command(&self, args: &[&str]) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_codync-host"));
        command.arg("send");
        if !args.contains(&"--port") {
            command.args(["--port", &self.port.to_string()]);
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
