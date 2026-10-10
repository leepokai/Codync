//! Generated file publication through real ACP/MCP and authenticated HTTP.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

struct Host {
    child: Child,
    home: PathBuf,
    base: String,
    token: String,
}

impl Drop for Host {
    fn drop(&mut self) {
        common::stop(&mut self.child);
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

impl Host {
    async fn start() -> Self {
        let home = std::env::temp_dir().join(format!("codync-files-e2e-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        common::seed_memory_runtime(&home);
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let child = Command::new(env!("CARGO_BIN_EXE_codync-host"))
            .args(["serve", "--bind", "127.0.0.1", "--port", &port.to_string()])
            .env("CODYNC_CLOUD", "off")
            .env("CODYNC_HOME", &home)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut host = Self { child, home, base: format!("http://127.0.0.1:{port}"), token: String::new() };
        let waited = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let response = reqwest::get(format!("{}/health", host.base)).await;
                if response.is_ok() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await;
        waited.unwrap();
        std::fs::read_to_string(host.home.join("token")).unwrap().trim().clone_into(&mut host.token);
        host
    }

    async fn restart(&mut self) {
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
        let waited = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let response = reqwest::get(format!("{}/health", self.base)).await;
                if response.is_ok() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await;
        waited.unwrap();
    }

    async fn call(&self, method: &str, body: Value) -> Value {
        let response_result = reqwest::Client::new()
            .post(format!("{}/api/{method}", self.base))
            .bearer_auth(&self.token)
            .json(&body)
            .timeout(Duration::from_secs(10))
            .send()
            .await;
        let response = response_result.unwrap();
        let status = response.status();
        let decoded = response.json().await;
        let value: Value = decoded.unwrap();
        assert!(status.is_success(), "{method}: {value}");
        value
    }
}

async fn ready(host: &Host, bot: &str, expected_files: usize) -> Vec<Value> {
    let waited = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let history = host.call("history", json!({"botId": bot})).await;
            let entries = history["entries"].as_array().unwrap();
            let files = entries.iter().filter(|e| e["data"]["files"].is_array()).count();
            if files == expected_files
                && entries.iter().any(|e| e["data"]["text"] == "The files are ready." && e["data"]["final"] == true)
            {
                return entries.clone();
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    waited.unwrap()
}

#[tokio::test]
async fn publishes_all_types_with_final_text_and_survives_restart_workspace_changes_and_threads() {
    use base64::Engine as _;
    let mut host = Host::start().await;
    let cwd = host.home.join("workspace");
    std::fs::create_dir_all(&cwd).unwrap();
    let data = [0, 255, 128, 17].repeat(150_000);
    for (name, bytes) in [
        ("binary.dat", data.clone()),
        (".hidden", vec![13]),
        ("empty", vec![]),
        ("report.pdf", b"%PDF-example".to_vec()),
    ] {
        std::fs::write(cwd.join(name), bytes).unwrap();
    }
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/file_agent.py");
    let created = host.call("createBot", json!({"name":"Files", "backend":"custom", "cwd":cwd, "command":common::python_agent(&agent), "connectors":[], "notify":false})).await;
    let bot = created["bot"]["id"].as_str().unwrap();
    let root = host.call("send", json!({"botId":bot,"text":"share-files"})).await;
    let entries = ready(&host, bot, 4).await;
    let file_entry = entries.iter().find(|e| e["data"]["files"][0]["name"] == "binary.dat").unwrap();
    let entry_id = file_entry["id"].as_str().unwrap();
    let file_id = file_entry["data"]["files"][0]["id"].as_str().unwrap();
    std::fs::remove_file(cwd.join("binary.dat")).unwrap();
    host.restart().await;
    let mut received = Vec::new();
    while received.len() < data.len() {
        let response =
            host.call("readFile", json!({"entryId":entry_id,"fileId":file_id,"offset":received.len()})).await;
        assert_eq!(response["size"], data.len());
        let bytes = base64::engine::general_purpose::STANDARD.decode(response["data"].as_str().unwrap()).unwrap();
        received.extend(bytes);
    }
    assert_eq!(received, data);
    let response_result = reqwest::Client::new()
        .post(format!("{}/api/readFile", host.base))
        .json(&json!({"entryId":entry_id,"fileId":file_id,"offset":0}))
        .send()
        .await;
    let response = response_result.unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    for body in [
        json!({"entryId":root["entry"]["id"],"fileId":file_id,"offset":0}),
        json!({"entryId":entry_id,"fileId":"../escape","offset":0}),
        json!({"entryId":entry_id,"fileId":file_id,"offset":-1}),
        json!({"entryId":entry_id,"fileId":file_id,"offset":data.len()+1}),
    ] {
        let response_result = reqwest::Client::new()
            .post(format!("{}/api/readFile", host.base))
            .bearer_auth(&host.token)
            .json(&body)
            .send()
            .await;
        let response = response_result.unwrap();
        assert!(!response.status().is_success());
    }
    std::fs::write(cwd.join("binary.dat"), &data).unwrap();
    host.call("send", json!({"botId":bot,"text":"share-files resumed"})).await;
    ready(&host, bot, 8).await;
    assert!(cwd.join("resumed").exists());
    host.call("send", json!({"botId":bot,"text":"share-files in a thread", "threadId":root["entry"]["id"]})).await;
    let waited = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let response = host.call("thread", json!({"botId":bot,"rootId":root["entry"]["id"]})).await;
            let files: Vec<_> = response["entries"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["data"]["files"].is_array())
                .cloned()
                .collect();
            if files.len() == 4 {
                let file = &files[0];
                host.call("readFile", json!({"entryId":file["id"],"fileId":file["data"]["files"][0]["id"],"offset":0}))
                    .await;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    waited.unwrap();
    let mut updated = created["bot"].clone();
    updated["cwd"] = host.home.to_string_lossy().into();
    host.call("updateBot", updated).await;
    host.call("readFile", json!({"entryId":entry_id,"fileId":file_id,"offset":0})).await;
    host.call("deleteBot", json!({"botId":bot})).await;
    let response_result = reqwest::Client::new()
        .post(format!("{}/api/readFile", host.base))
        .bearer_auth(&host.token)
        .json(&json!({"entryId":entry_id,"fileId":file_id,"offset":0}))
        .send()
        .await;
    let response = response_result.unwrap();
    assert!(!response.status().is_success());
}

#[tokio::test]
async fn rejects_invalid_sources_without_a_file_entry() {
    let host = Host::start().await;
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/file_agent.py");
    let large = std::fs::File::create(host.home.join("oversized")).unwrap();
    large.set_len(100 * 1024 * 1024 + 1).unwrap();
    let created = host.call("createBot", json!({"name":"Errors", "backend":"custom", "cwd":host.home, "command":common::python_agent(&agent), "connectors":[], "notify":false})).await;
    let bot = created["bot"]["id"].as_str().unwrap();
    host.call("send", json!({"botId":bot,"text":"invalid-files"})).await;
    ready(&host, bot, 0).await;
    let results: Value = serde_json::from_slice(&std::fs::read(host.home.join("file-results.json")).unwrap()).unwrap();
    assert!(results.as_array().unwrap().iter().all(|r| r["isError"] == true));
}

async fn rejected(path: &std::path::Path) {
    let waited = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Ok(data) = std::fs::read(path) {
                let results: Value = serde_json::from_slice(&data).unwrap();
                assert!(
                    results.as_array().unwrap().iter().all(|result| result["isError"] == true
                        && result["content"][0]["text"].as_str().unwrap().contains("own chat and threads")),
                    "{results}"
                );
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    waited.unwrap();
}

#[tokio::test]
async fn groups_routines_and_delegated_turns_cannot_publish_files() {
    let host = Host::start().await;
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/file_agent.py");
    let created = host.call("createBot", json!({"name":"Recipient", "backend":"custom", "cwd":host.home, "command":common::python_agent(&agent), "connectors":[], "notify":false})).await;
    let bot = created["bot"]["id"].as_str().unwrap();
    let results_path = host.home.join("file-results.json");
    let group = host.call("createBot", json!({"name":"Group", "kind":"group", "members":[bot]})).await;
    host.call("send", json!({"botId":group["bot"]["id"],"text":"share-files"})).await;
    rejected(&results_path).await;
    host.call("stop", json!({"botId":group["bot"]["id"]})).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let _ = std::fs::remove_file(&results_path);
    let routine = host
        .call(
            "saveRoutine",
            json!({"botId":bot,"name":"Files","instruction":"share-files","triggers":[{"type":"webhook"}]}),
        )
        .await;
    host.call("runRoutine", json!({"botId":bot,"id":routine["routine"]["id"]})).await;
    rejected(&results_path).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    std::fs::remove_file(&results_path).unwrap();
    let caller_dir = host.home.join("caller");
    std::fs::create_dir_all(&caller_dir).unwrap();
    let caller_agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
    let caller = host.call("createBot", json!({"name":"Caller", "backend":"custom", "cwd":caller_dir, "command":common::python_agent(&caller_agent), "connectors":[], "notify":false})).await;
    let caller = caller["bot"]["id"].as_str().unwrap();
    host.call("send", json!({"botId":caller,"text":"warm up"})).await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    host.call("send", json!({"botId":caller,"text":format!("DELEGATE {bot}")})).await;
    rejected(&results_path).await;
    let history = host.call("history", json!({"botId":bot})).await;
    assert!(history["entries"].as_array().unwrap().iter().all(|entry| entry["data"]["files"].is_null()));
}

#[tokio::test]
async fn files_only_turns_finish_without_duplicate_text_and_input_uploads_still_work() {
    use base64::Engine as _;
    let host = Host::start().await;
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/file_agent.py");
    for name in ["binary.dat", ".hidden", "empty", "report.pdf"] {
        std::fs::write(host.home.join(name), []).unwrap();
    }
    let created = host.call("createBot", json!({"name":"Files only", "backend":"custom", "cwd":host.home, "command":common::python_agent(&agent), "connectors":[], "notify":false})).await;
    let bot = created["bot"]["id"].as_str().unwrap();
    let upload_id = uuid::Uuid::new_v4().to_string();
    host.call(
        "upload",
        json!({"botId":bot,"uploadId":upload_id,"name":"input.txt","offset":0,"data":"aGVsbG8=","done":true}),
    )
    .await;
    let sent = host.call("send", json!({"botId":bot,"text":"files-only","attachments":[upload_id]})).await;
    assert_eq!(sent["entry"]["data"]["attachments"][0]["id"], upload_id);
    let response = host.call("readUpload", json!({"botId":bot,"uploadId":upload_id,"offset":0})).await;
    assert_eq!(base64::engine::general_purpose::STANDARD.decode(response["data"].as_str().unwrap()).unwrap(), b"hello");
    let waited = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let history = host.call("history", json!({"botId":bot})).await;
            let final_entries: Vec<_> = history["entries"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["kind"] == "agent" && e["data"]["final"] == true)
                .collect();
            let sync = host.call("sync", json!({"since":0})).await;
            if final_entries.len() == 4
                && sync["bots"].as_array().unwrap().iter().any(|b| b["id"] == bot && b["status"] == "idle")
            {
                assert!(final_entries.iter().all(|e| e["data"]["files"].is_array()));
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    waited.unwrap();
}
