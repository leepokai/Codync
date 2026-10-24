//! A real host on disposable data, and what the end-to-end tests ask of it.

// Each test binary compiles `common` and uses only part of this.
#![allow(dead_code)]

use serde_json::{Value, json};
use std::{
    path::PathBuf,
    process::Child,
    time::{Duration, Instant},
};

pub struct Host {
    child: Child,
    /// The host's `CODYNC_HOME`.
    pub home: PathBuf,
    base: String,
    token: String,
}

impl Drop for Host {
    fn drop(&mut self) {
        super::stop(&mut self.child);
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

impl Host {
    pub async fn start() -> Self {
        let home = std::env::temp_dir().join(format!("codync-memory-e2e-{}", uuid::Uuid::new_v4()));
        let (child, base) = super::serve(&home).await;
        let token = std::fs::read_to_string(home.join("token")).unwrap().trim().to_owned();
        Self { child, home, base, token }
    }

    /// Stops the host, lets `between` change its files, and starts it again on the same data.
    pub async fn restart(&mut self, between: impl FnOnce(&std::path::Path)) {
        super::stop(&mut self.child);
        between(&self.home);
        let (child, base) = super::serve(&self.home).await;
        self.child = child;
        self.base = base;
    }

    pub async fn post(&self, method: &str, body: &Value) -> (bool, Value) {
        let response = reqwest::Client::new()
            .post(format!("{}/api/{method}", self.base))
            .bearer_auth(&self.token)
            .json(body)
            .timeout(Duration::from_secs(120))
            .send()
            .await
            .unwrap();
        let ok = response.status().is_success();
        (ok, response.json().await.unwrap())
    }

    pub async fn call(&self, method: &str, body: Value) -> Value {
        let (ok, body) = self.post(method, &body).await;
        assert!(ok, "{method}: {body}");
        body
    }

    /// A call that must be refused: its error text.
    pub async fn refused(&self, method: &str, body: Value) -> String {
        let (ok, body) = self.post(method, &body).await;
        assert!(!ok, "{method} should have been refused: {body}");
        body["error"].as_str().unwrap_or_default().to_owned()
    }

    pub async fn bot(&self, name: &str, permission: &str) -> String {
        let bot =
            json!({"name": name, "backend": "custom", "command": "unused", "connectors": [], "permission": permission});
        self.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned()
    }

    /// The event stream (its catch-up, then live events), each event as it arrives.
    pub async fn events(&self) -> tokio::sync::mpsc::UnboundedReceiver<Value> {
        use futures::StreamExt as _;
        let response = reqwest::Client::new()
            .get(format!("{}/events?client=test", self.base))
            .bearer_auth(&self.token)
            .send()
            .await
            .unwrap();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            let (mut body, mut buf) = (response.bytes_stream(), String::new());
            while let Some(Ok(chunk)) = body.next().await {
                buf.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(i) = buf.find('\n') {
                    let line: String = buf.drain(..=i).collect();
                    if let Some(data) = line.trim_end().strip_prefix("data:")
                        && let Ok(event) = serde_json::from_str(data.trim_start())
                        && tx.send(event).is_err()
                    {
                        return;
                    }
                }
            }
        });
        rx
    }

    /// What a client holds after catching up (see `common::snapshot`).
    pub async fn snapshot(&self) -> Value {
        super::snapshot(&self.base, &self.token).await
    }

    pub async fn memory(&self, bot: &str) -> Value {
        self.call("memory", json!({"botId": bot})).await
    }

    pub async fn tool(&self, bot: &str, name: &str, arguments: Value) -> Value {
        self.call("memoryCall", json!({"botId": bot, "name": name, "arguments": arguments})).await
    }

    pub async fn tool_refused(&self, bot: &str, name: &str, arguments: Value) -> String {
        self.refused("memoryCall", json!({"botId": bot, "name": name, "arguments": arguments})).await
    }

    /// A memory layer's folder: a bot's own, or the shared one (`None`).
    pub fn memory_folder(&self, bot: Option<&str>) -> PathBuf {
        bot.map_or_else(|| self.home.join("shared-memory"), |bot| self.home.join("bots").join(bot).join("bot-memory"))
    }

    /// Writes a memory file the way an editor would, already settled, so the host's next scan takes it.
    pub fn write_memory(&self, bot: Option<&str>, name: &str, content: &str) {
        let dir = self.memory_folder(bot);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(std::time::SystemTime::now() - Duration::from_secs(5)).unwrap();
    }

    /// A bot running `tests/fixtures/<fixture>` with `args`.
    pub async fn fixture_bot(&self, name: &str, fixture: &str, args: &str) -> String {
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(fixture);
        let command = format!("{} {args}", super::python_agent(&script));
        let bot = json!({"name": name, "backend": "custom", "command": command, "connectors": [], "notify": false});
        self.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned()
    }

    /// Sends `text` and waits for the bot's reply.
    pub async fn reply(&self, bot: &str, text: &str) {
        let finals = |history: &Value| {
            history["entries"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["kind"] == "agent" && e["data"]["final"] == true)
                .count()
        };
        let before = finals(&self.call("history", json!({"botId": bot})).await);
        let nonce = uuid::Uuid::new_v4().to_string();
        self.call("send", json!({"botId": bot, "text": text, "clientNonce": nonce})).await;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let history = self.call("history", json!({"botId": bot})).await;
            if finals(&history) > before {
                return;
            }
            assert!(Instant::now() < deadline, "{bot} didn't answer {text:?}: {history}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// The JSON lines a fixture agent logged to `file` in a bot's workspace.
    pub fn log(&self, bot: &str, file: &str) -> Vec<Value> {
        let log = std::fs::read_to_string(self.home.join("bots").join(bot).join("workspace").join(file)).unwrap();
        log.lines().map(|l| serde_json::from_str::<Value>(l).unwrap()).collect()
    }

    /// The requests `instructions_agent.py` logged in a bot's workspace.
    pub fn requests(&self, bot: &str, method: &str) -> Vec<Value> {
        self.log(bot, "requests.jsonl").into_iter().filter(|r| r["method"] == method).collect()
    }

    /// The text of every prompt a bot's agent got, oldest first.
    pub fn prompts(&self, bot: &str) -> Vec<String> {
        self.requests(bot, "session/prompt")
            .iter()
            .map(|r| r["params"]["prompt"][0]["text"].as_str().unwrap().to_owned())
            .collect()
    }
}
