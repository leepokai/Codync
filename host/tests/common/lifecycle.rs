//! Persistent ACP host fixture shared by lifecycle and setup regressions.

#![allow(dead_code)]

use crate::common;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

pub struct Host {
    child: Child,
    pub home: PathBuf,
    base: String,
    token: String,
    witness_port: u16,
    live: Arc<AtomicUsize>,
    witness_task: tokio::task::JoinHandle<()>,
}

impl Drop for Host {
    fn drop(&mut self) {
        common::stop(&mut self.child);
        self.witness_task.abort();
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

impl Host {
    pub async fn restart(&mut self) {
        common::stop(&mut self.child);
        let port = self.base.rsplit(':').next().unwrap();
        self.child = Command::new(env!("CARGO_BIN_EXE_codync-host"))
            .args(["serve", "--bind", "127.0.0.1", "--port", port])
            .env("CODYNC_CLOUD", "off")
            .env("CODYNC_HOME", &self.home)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(20), async {
            while reqwest::get(format!("{}/health", self.base)).await.is_err() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
    }

    pub async fn start() -> Self {
        let home = std::env::temp_dir().join(format!("codync-lifecycle-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        common::seed_memory_runtime(&home);
        let witness = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let witness_port = witness.local_addr().unwrap().port();
        let live = Arc::new(AtomicUsize::new(0));
        let observed = live.clone();
        let witness_task = tokio::spawn(async move {
            while let Ok((stream, _)) = witness.accept().await {
                let observed = observed.clone();
                tokio::spawn(async move {
                    let mut reader = BufReader::new(stream);
                    let mut greeting = String::new();
                    if reader.read_line(&mut greeting).await.unwrap() == 0 {
                        return;
                    }
                    observed.fetch_add(1, Ordering::SeqCst);
                    let mut remaining = Vec::new();
                    let closed = reader.read_to_end(&mut remaining).await;
                    if let Err(error) = closed {
                        assert!(
                            matches!(
                                error.kind(),
                                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                            ),
                            "{error}"
                        );
                    }
                    observed.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let child = Command::new(env!("CARGO_BIN_EXE_codync-host"))
            .args(["serve", "--bind", "127.0.0.1", "--port", &port.to_string()])
            .env("CODYNC_CLOUD", "off")
            .env("CODYNC_HOME", &home)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut host = Self {
            child,
            home,
            base: format!("http://127.0.0.1:{port}"),
            token: String::new(),
            witness_port,
            live,
            witness_task,
        };
        tokio::time::timeout(Duration::from_secs(20), async {
            while reqwest::get(format!("{}/health", host.base)).await.is_err() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        std::fs::read_to_string(host.home.join("token")).unwrap().trim().clone_into(&mut host.token);
        host
    }

    pub async fn call(&self, method: &str, body: Value) -> Value {
        let res = reqwest::Client::new()
            .post(format!("{}/api/{method}", self.base))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = res.status();
        let value: Value = res.json().await.unwrap();
        assert!(status.is_success(), "{method}: {value}");
        value
    }

    pub async fn bot(&self, name: &str, mut config: Value) -> (String, PathBuf) {
        let cwd = self.home.join(name);
        std::fs::create_dir_all(&cwd).unwrap();
        config["witnessPort"] = json!(self.witness_port);
        std::fs::write(cwd.join("fixture.json"), config.to_string()).unwrap();
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lifecycle_agent.py");
        let response = self
            .call(
                "createBot",
                json!({"name":name,"backend":"custom","cwd":cwd,
            "command":common::python_agent(&script),"permission":"ask","notify":false}),
            )
            .await;
        (response["bot"]["id"].as_str().unwrap().to_owned(), cwd)
    }

    pub async fn send_entry(&self, bot: &str, text: &str, thread: Option<&str>) -> Value {
        let response = self
            .call(
                "send",
                json!({"botId":bot,"text":text,"threadId":thread,
            "clientNonce":uuid::Uuid::new_v4().to_string()}),
            )
            .await;
        response["entry"].clone()
    }

    pub async fn entries(&self, bot: &str, thread: Option<&str>) -> Vec<Value> {
        let response = if let Some(root) = thread {
            self.call("thread", json!({"botId":bot,"rootId":root})).await
        } else {
            self.call("history", json!({"botId":bot,"limit":500})).await
        };
        response["entries"].as_array().unwrap().clone()
    }

    pub async fn wait_entry(&self, bot: &str, thread: Option<&str>, pred: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let entries = self.entries(bot, thread).await;
            if let Some(entry) = entries.iter().rev().find(|e| pred(e)) {
                return entry.clone();
            }
            assert!(Instant::now() < deadline, "missing expected entry: {entries:?}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    pub async fn send(&self, bot: &str, text: &str, thread: Option<&str>) -> Value {
        let entry = self.send_entry(bot, text, thread).await;
        self.wait_entry(bot, thread, |e| {
            e["turn"] == entry["turn"] && e["kind"] == "agent" && e["data"]["final"] == true
        })
        .await
    }

    pub async fn wait_live(&self, expected: usize, budget: Duration) {
        // Starting multiple Python generations is slower on loaded Windows runners.
        let budget = if cfg!(windows) && budget <= Duration::from_secs(5) { budget * 3 } else { budget };
        let deadline = Instant::now() + budget;
        while self.live.load(Ordering::SeqCst) != expected {
            assert!(
                Instant::now() < deadline,
                "expected {expected} tool descendants, saw {}",
                self.live.load(Ordering::SeqCst)
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

pub fn sid(reply: &Value) -> &str {
    reply["data"]["text"].as_str().unwrap().split_whitespace().next().unwrap().strip_prefix("SID=").unwrap()
}

pub fn events(cwd: &Path) -> Vec<Value> {
    std::fs::read_to_string(cwd.join("events.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

pub fn assert_bounded(cwd: &Path) {
    let events = events(cwd);
    assert!(!events.iter().any(|e| e["event"] == "duplicate_allocation"), "{events:?}");
    assert!(events.iter().all(|e| e["live"].as_u64().unwrap() <= 2), "{events:?}");
}
