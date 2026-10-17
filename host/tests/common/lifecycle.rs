//! Persistent ACP host fixture shared by lifecycle and setup regressions.

#![allow(dead_code)]

use crate::common;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

#[derive(Clone, Debug, Deserialize)]
struct Descendant {
    pid: u32,
    role: String,
    generation: String,
    allocation: String,
    session: Option<String>,
}

impl Descendant {
    fn same_allocation(&self, other: &Self) -> bool {
        self.role == other.role
            && self.generation == other.generation
            && self.allocation == other.allocation
            && self.session == other.session
    }
}

type Inventory = Arc<Mutex<BTreeMap<(u32, String), Descendant>>>;

struct Registration {
    live: Inventory,
    descendant: Descendant,
}

impl Drop for Registration {
    fn drop(&mut self) {
        let key = (self.descendant.pid, self.descendant.allocation.clone());
        let mut live = self.live.lock().unwrap();
        if live.get(&key).is_some_and(|current| current.same_allocation(&self.descendant)) {
            live.remove(&key);
        }
    }
}

fn peer_closed(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::NotConnected
            | std::io::ErrorKind::UnexpectedEof
    )
}

fn reserve_host_port() -> std::net::TcpListener {
    // Outside the default ephemeral ranges on Linux, macOS and Windows CI,
    // so a parallel witness listener or outgoing connection cannot claim it.
    // Custom OS ranges and other processes can still collide during handoff;
    // host startup failures remain fatal, without retrying the launched host.
    const FIRST: u16 = 10_240;
    const COUNT: u16 = 22_528;
    for _ in 0..32 {
        let offset = u16::try_from(uuid::Uuid::new_v4().as_u128() % u128::from(COUNT)).unwrap();
        let port = FIRST + offset;
        match std::net::TcpListener::bind(("127.0.0.1", port)) {
            Ok(reservation) => return reservation,
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {}
            Err(error) => panic!("couldn't reserve lifecycle fixture port {port}: {error}"),
        }
    }
    panic!("no available lifecycle fixture HTTP port after 32 candidates in 10240..32768");
}

pub struct Host {
    child: Child,
    pub home: PathBuf,
    base: String,
    token: String,
    witness_port: u16,
    live: Inventory,
    witness_error: Arc<Mutex<Option<String>>>,
    witness_task: tokio::task::JoinHandle<()>,
}

impl Drop for Host {
    fn drop(&mut self) {
        common::stop(&mut self.child);
        self.witness_task.abort();
        let _ = std::fs::remove_dir_all(&self.home);
        if !std::thread::panicking() {
            self.assert_witness();
        }
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
            .stderr(std::fs::File::create(self.home.join("host-startup.log")).unwrap())
            .spawn()
            .unwrap();
        self.wait_ready().await;
    }

    pub async fn start() -> Self {
        let home = std::env::temp_dir().join(format!("codync-lifecycle-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        common::seed_memory_runtime(&home);
        let witness = tokio::net::TcpListener::bind("127.0.0.1:0").await;
        let witness = witness.unwrap();
        let witness_port = witness.local_addr().unwrap().port();
        let live = Arc::new(Mutex::new(BTreeMap::<(u32, String), Descendant>::new()));
        let witness_error = Arc::new(Mutex::new(None));
        let reported_error = witness_error.clone();
        let observed = live.clone();
        let witness_task = tokio::spawn(async move {
            while let Ok((stream, _)) = witness.accept().await {
                let observed = observed.clone();
                let reported_error = reported_error.clone();
                tokio::spawn(async move {
                    let mut reader = BufReader::new(stream);
                    let mut greeting = String::new();
                    let greeting_result = reader.read_line(&mut greeting).await;
                    match greeting_result {
                        Ok(0) => return,
                        Ok(_) => {}
                        Err(error) => {
                            if !peer_closed(&error) {
                                reported_error
                                    .lock()
                                    .unwrap()
                                    .get_or_insert_with(|| format!("witness greeting failed: {:?}", error.kind()));
                            }
                            return;
                        }
                    }
                    let decoded = serde_json::from_str::<Descendant>(&greeting);
                    let descendant = match decoded {
                        Ok(descendant) => descendant,
                        Err(error) => {
                            reported_error
                                .lock()
                                .unwrap()
                                .get_or_insert_with(|| format!("invalid witness greeting: {:?}", error.classify()));
                            return;
                        }
                    };
                    let key = (descendant.pid, descendant.allocation.clone());
                    observed.lock().unwrap().insert(key.clone(), descendant.clone());
                    let _registration = Registration { live: observed, descendant: descendant.clone() };
                    let acknowledged = reader.get_mut().write_all(b"ready\n").await;
                    if let Err(error) = acknowledged {
                        if !peer_closed(&error) {
                            reported_error.lock().unwrap().get_or_insert_with(|| {
                                format!("witness acknowledgement failed: {:?}; {descendant:?}", error.kind())
                            });
                        }
                        return;
                    }
                    let mut remaining = Vec::new();
                    let closed = reader.read_to_end(&mut remaining).await;
                    if let Err(error) = closed
                        && !peer_closed(&error)
                    {
                        reported_error
                            .lock()
                            .unwrap()
                            .get_or_insert_with(|| format!("witness read failed: {:?}; {descendant:?}", error.kind()));
                    }
                });
            }
        });
        let reservation = reserve_host_port();
        let port = reservation.local_addr().unwrap().port();
        let port_arg = port.to_string();
        let mut command = Command::new(env!("CARGO_BIN_EXE_codync-host"));
        command
            .args(["serve", "--bind", "127.0.0.1", "--port", &port_arg])
            .env("CODYNC_CLOUD", "off")
            .env("CODYNC_HOME", &home)
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(home.join("host-startup.log")).unwrap());
        // Keep the availability probe until the configured child can be spawned.
        drop(reservation);
        let child = command.spawn().unwrap();
        let mut host = Self {
            child,
            home,
            base: format!("http://127.0.0.1:{port}"),
            token: String::new(),
            witness_port,
            live,
            witness_error,
            witness_task,
        };
        host.wait_ready().await;
        std::fs::read_to_string(host.home.join("token")).unwrap().trim().clone_into(&mut host.token);
        host
    }

    async fn wait_ready(&mut self) {
        // CI starts several hosts together; binary hashing and login-shell PATH
        // discovery compete for the runner before the HTTP server can respond.
        let deadline = Instant::now() + Duration::from_secs(60);
        let client = reqwest::Client::builder().timeout(Duration::from_secs(2)).build().unwrap();
        loop {
            let response = client.get(format!("{}/health", self.base)).send().await;
            if response.is_ok_and(|response| response.status().is_success()) {
                return;
            }
            let exited = self.child.try_wait().unwrap();
            if exited.is_some() || Instant::now() >= deadline {
                let log = std::fs::read_to_string(self.home.join("host-startup.log")).unwrap_or_default();
                panic!("host failed to become ready (exit={exited:?}): {log}");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    pub async fn call(&self, method: &str, body: Value) -> Value {
        let res = reqwest::Client::new()
            .post(format!("{}/api/{method}", self.base))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await;
        let res = res.unwrap();
        let status = res.status();
        let decoded = res.json().await;
        let value: Value = decoded.unwrap();
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

    pub async fn wait_event(&self, cwd: &Path, pred: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            self.assert_witness();
            let observed = events(cwd);
            if let Some(event) = observed.iter().rev().find(|event| pred(event)) {
                return event.clone();
            }
            assert!(Instant::now() < deadline, "missing expected fixture phase: {observed:?}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    pub async fn wait_live(&self, expected: usize, budget: Duration) {
        self.wait_inventory(expected, None, budget).await;
    }

    pub async fn wait_live_session(&self, expected: usize, generation: &str, session: &str, budget: Duration) {
        self.wait_inventory(expected, Some((generation, session)), budget).await;
    }

    async fn wait_inventory(&self, expected: usize, identity: Option<(&str, &str)>, budget: Duration) {
        // Starting multiple Python generations is slower on loaded Windows runners.
        let budget = if cfg!(windows) && budget <= Duration::from_secs(5) { budget * 3 } else { budget };
        let deadline = Instant::now() + budget;
        loop {
            self.assert_witness();
            let inventory = self.live.lock().unwrap().clone();
            let matches = identity.is_none_or(|(generation, session)| {
                inventory.values().all(|descendant| {
                    descendant.generation == generation && descendant.session.as_deref() == Some(session)
                })
            });
            if inventory.len() == expected && matches {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "expected {expected} tool descendants for {identity:?}, saw {}: {inventory:?}",
                inventory.len()
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn assert_witness(&self) {
        let error = self.witness_error.lock().unwrap();
        assert!(error.is_none(), "unexpected witness error: {error:?}");
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
