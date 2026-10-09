//! A real host and a checksum-verified Engram release, with isolated disposable data.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[allow(dead_code)]
mod common;

use base64::Engine as _;
use serde_json::{Value, json};
use std::{
    fmt::Write as _,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

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
        assert!(std::env::var_os("CODYNC_TEST_ENGRAM").is_some(), "set CODYNC_TEST_ENGRAM to the pinned binary");
        let home = std::env::temp_dir().join(format!("codync-memory-e2e-{}", uuid::Uuid::new_v4()));
        common::seed_memory_runtime(&home);
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let child = Command::new(env!("CARGO_BIN_EXE_codync-host"))
            .args(["serve", "--bind", "127.0.0.1", "--port", &port.to_string()])
            .env("CODYNC_HOME", &home)
            .env("CODYNC_CLOUD", "off")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let deadline = Instant::now() + Duration::from_secs(20);
        while reqwest::get(format!("{base}/health")).await.is_err() {
            assert!(Instant::now() < deadline, "host did not start");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let token = std::fs::read_to_string(home.join("token")).unwrap().trim().to_owned();
        Self { child, home, base, token }
    }

    async fn call(&self, method: &str, body: Value) -> Value {
        let response = reqwest::Client::new()
            .post(format!("{}/api/{method}", self.base))
            .bearer_auth(&self.token)
            .json(&body)
            .timeout(Duration::from_secs(120))
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body: Value = response.json().await.unwrap();
        assert!(status.is_success(), "{method}: {body}");
        body
    }

    async fn bot(&self, name: &str) -> String {
        self.call("createBot", json!({"name":name, "backend":"custom", "command":"unused", "connectors":[]}))
            .await["bot"]["id"].as_str().unwrap().to_owned()
    }
}

#[tokio::test]
#[ignore = "requires the pinned Engram release at CODYNC_TEST_ENGRAM"]
async fn migrate_manage_backup_and_forget_without_cross_bot_reads() {
    let host = Host::start().await;
    let a = host.bot("Memory A").await;
    let b = host.bot("Memory B").await;
    let legacy = host.home.join("bots").join(&a).join("memory");
    std::fs::create_dir_all(legacy.join("log")).unwrap();
    let mut facts = String::new();
    for i in 0..65 {
        writeln!(facts, "- (2026-01-01) 台灣繁體中文偏好 {i}").unwrap();
    }
    std::fs::write(legacy.join("profile.md"), &facts).unwrap();
    std::fs::write(legacy.join("log/2026-02.md"), "- (2026-02-02) 發布 Codync\n").unwrap();

    let first = host.call("memory", json!({"botId":a,"offset":0})).await;
    assert_eq!(first["total"], 66);
    assert_eq!(first["facts"].as_array().unwrap().len(), 50);
    assert_eq!(first["nextOffset"], 50);
    let legacy_client = host.call("memory", json!({"botId":a})).await;
    assert_eq!(legacy_client["facts"].as_array().unwrap().len(), 66, "older clients cannot paginate");
    let search = host.call("memory", json!({"botId":a, "query":"台灣繁體中文"})).await;
    assert_eq!(search["total"], 65, "UI search must not inherit the MCP top-20 cap");
    let short = host.call("memory", json!({"botId":a, "query":"台灣", "offset":50})).await;
    assert_eq!(short["facts"].as_array().unwrap().len(), 15);
    assert!(short["nextOffset"].is_null());
    assert_eq!(host.call("memory", json!({"botId":b, "query":"台灣"})).await["total"], 0);
    assert_eq!(std::fs::read_to_string(legacy.join("profile.md")).unwrap(), facts);
    let one = &first["facts"][0];
    assert_eq!(one["createdAt"], 1_767_225_600_000_i64);
    let id = one["id"].as_str().unwrap();
    host.call("saveMemory", json!({"botId":a,"id":id,"title":"Updated preference","content":"Prefers English now","scope":"personal","memoryType":"learning"})).await;
    host.call("pinMemory", json!({"botId":a,"id":id,"pinned":true})).await;
    let pinned = host.call("memory", json!({"botId":a,"filter":"pinned"})).await;
    assert_eq!(pinned["total"], 1);
    let detail = host.call("memoryDetail", json!({"botId":a,"id":id})).await;
    assert!(detail["history"]["result"].as_str().unwrap().contains("台灣繁體中文"));
    let exported = host.call("exportMemory", json!({"botId":a})).await;
    let backup: Value = serde_json::from_str(exported["json"].as_str().unwrap()).unwrap();
    assert_eq!(backup["observations"].as_array().unwrap().len(), 66);
    host.call("forgetMemory", json!({"botId":a,"id":id})).await;
    assert_eq!(host.call("memory", json!({"botId":a,"filter":"pinned"})).await["total"], 0);
    let native_session = format!("codync-{a}:codync-background");
    let prompt = host.call("memoryCall", json!({"botId":a,"name":"mem_save_prompt","arguments":{"session_id":native_session,"content":"UNIQUE_SECRET_CAPTURE"}})).await;
    assert_ne!(prompt["isError"], true);
    host.call("clearMemory", json!({"botId":a})).await;
    let cleared = host.call("exportMemory", json!({"botId":a})).await;
    let cleared: Value = serde_json::from_str(cleared["json"].as_str().unwrap()).unwrap();
    assert!(!cleared.to_string().contains("UNIQUE_SECRET_CAPTURE"));
    assert_eq!(cleared["observations"], json!([]));
    assert_eq!(host.call("memory", json!({"botId":a})).await["total"], 0);
    // Import into another bot's isolated store, then search the source content.
    host.call("importMemory", json!({"botId":b,"json":exported["json"]})).await;
    assert_eq!(host.call("memory", json!({"botId":b})).await["total"], 66);
    assert_eq!(
        host.call("memory", json!({"botId":a})).await["total"],
        0,
        "legacy files must not resurrect cleared facts"
    );
}

#[tokio::test]
#[ignore = "requires the pinned Engram release at CODYNC_TEST_ENGRAM"]
async fn large_backups_round_trip_through_chunked_transport() {
    let host = Host::start().await;
    let bot = host.bot("Large backup").await;
    let session = "external-session";
    let observations: Vec<Value> = (0..80)
        .map(|i| {
            json!({
                "id":i+1, "sync_id":format!("large-{i}"), "session_id":session,
                "project":"external-project", "title":format!("Durable fact {i}"), "type":"learning", "scope":"project",
                "content":format!("Record {i}: {}", "long-lived knowledge ".repeat(500)),
                "created_at":"2026-01-01T00:00:00Z", "updated_at":"2026-01-01T00:00:00Z",
            })
        })
        .collect();
    let backup = serde_json::to_vec(&json!({"version":"0.2.0", "sessions":[{
        "id":session,"project":"external-project","directory":"", "started_at":"2026-01-01T00:00:00Z"
    }],"observations":observations,"prompts":[]}))
    .unwrap();
    assert!(backup.len() > 384 * 1024);
    let upload = uuid::Uuid::new_v4().to_string();
    for (index, chunk) in backup.chunks(384 * 1024).enumerate() {
        let offset = index * 384 * 1024;
        host.call(
            "upload",
            json!({"botId":bot,"uploadId":upload,"name":"backup.json","offset":offset,
            "data":base64::engine::general_purpose::STANDARD.encode(chunk),"done":offset+chunk.len()==backup.len()}),
        )
        .await;
    }
    host.call("importMemory", json!({"botId":bot,"uploadId":upload})).await;
    assert_eq!(host.call("memory", json!({"botId":bot})).await["total"], 80);
    let result = host.call("exportMemory", json!({"botId":bot})).await;
    let upload = result["uploadId"].as_str().expect("large export must use chunks");
    let mut bytes = Vec::new();
    loop {
        let part = host.call("readUpload", json!({"botId":bot,"uploadId":upload,"offset":bytes.len()})).await;
        bytes.extend(base64::engine::general_purpose::STANDARD.decode(part["data"].as_str().unwrap()).unwrap());
        if bytes.len() as u64 == part["size"].as_u64().unwrap() {
            break;
        }
    }
    let exported: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(exported["observations"].as_array().unwrap().len(), 80);
    for fact in exported["observations"].as_array().unwrap() {
        assert_eq!(fact["project"], format!("codync-{bot}"));
        assert!(fact["content"].as_str().unwrap().len() > 9_000);
    }
}

#[tokio::test]
#[ignore = "requires the pinned Engram release at CODYNC_TEST_ENGRAM"]
async fn new_session_flushes_automatic_facts_and_native_summary() {
    let host = Host::start().await;
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/memory_agent.py");
    let command = common::python_agent(&agent);
    let created = host
        .call(
            "createBot",
            json!({"name":"Keeper test","backend":"custom","command":command,"connectors":[],"notify":false}),
        )
        .await;
    let bot = created["bot"]["id"].as_str().unwrap();
    host.call(
        "send",
        json!({"botId":bot,"text":"Please always explain things to me concisely in Traditional Chinese."}),
    )
    .await;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let history = host.call("history", json!({"botId":bot})).await;
        if history["entries"].as_array().unwrap().iter().any(|e| e["kind"] == "agent" && e["data"]["final"] == true) {
            break;
        }
        assert!(Instant::now() < deadline, "agent did not finish: {history}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    host.call("newSession", json!({"botId":bot})).await;
    loop {
        let memory = host.call("memory", json!({"botId":bot})).await;
        let facts = memory["facts"].as_array().unwrap();
        if facts.iter().any(|f| f["memoryType"] == "session_summary") {
            assert!(
                facts
                    .iter()
                    .any(|f| f["kind"] == "profile" && f["content"].as_str().unwrap().contains("Traditional Chinese"))
            );
            break;
        }
        assert!(Instant::now() < deadline, "keeper did not save summary: {memory}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
