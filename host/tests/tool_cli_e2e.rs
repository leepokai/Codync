//! Script RPC protocol checks against an isolated loopback host, without provider calls.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::{Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

const TOKEN: &str = "isolated-tool-cli-test-token";

struct Mock {
    home: PathBuf,
    port: String,
    requests: Arc<Mutex<Vec<Value>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Mock {
    fn drop(&mut self) {
        self.task.abort();
        std::fs::remove_dir_all(&self.home).unwrap();
    }
}

async fn call(
    State(requests): State<Arc<Mutex<Vec<Value>>>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    assert_eq!(headers["authorization"].to_str().unwrap(), format!("Bearer {TOKEN}"));
    requests.lock().unwrap().push(body.clone());
    if body["name"] == "partial_failure" {
        (StatusCode::BAD_GATEWAY, Json(json!({"error":"Partial work happened; receipt unavailable"})))
    } else {
        (StatusCode::OK, Json(body))
    }
}

impl Mock {
    async fn start() -> Self {
        let home = std::env::temp_dir().join(format!("codync-tool-cli-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("token"), TOKEN).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let app = Router::new().route("/api/chatCall", post(call)).with_state(requests.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port().to_string();
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self { home, port, requests, task }
    }

    async fn run(&self, args: &[&str], input: Option<&[u8]>) -> Output {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_codync-host"));
        command
            .args(["tool", "--bot", "rpc-bot", "--server", "chat", "--port", &self.port])
            .args(args)
            .env("CODYNC_HOME", &self.home)
            .env("CODYNC_CLOUD", "off")
            .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().unwrap();
        if let Some(input) = input {
            let mut stdin = child.stdin.take().unwrap();
            stdin.write_all(input).await.unwrap();
            drop(stdin);
        }
        tokio::time::timeout(Duration::from_secs(15), child.wait_with_output()).await.unwrap().unwrap()
    }
}

fn result(output: &Output, code: i32) -> Value {
    assert_eq!(output.status.code(), Some(code), "{}", String::from_utf8_lossy(&output.stderr));
    let text = std::str::from_utf8(&output.stdout).unwrap();
    assert_eq!(text.lines().count(), 1, "One JSON result per call");
    serde_json::from_str(text).unwrap()
}

#[tokio::test]
async fn hermes_rpc_scripts_use_file_stdin_and_json_arguments_with_one_result_per_call() {
    let host = Mock::start().await;
    let query = json!({"text":"Researché","nested":{"count":3}});
    let raw = query.to_string();
    let path = host.home.join("arguments.json");
    std::fs::write(&path, &raw).unwrap();
    for output in [
        host.run(&["echo", "--arguments", &raw], None).await,
        host.run(&["echo", "--file", path.to_str().unwrap()], None).await,
        host.run(&["echo", "--file", "-"], Some(raw.as_bytes())).await,
    ] {
        let result = result(&output, 0);
        assert_ne!(result["isError"], true);
        let echoed: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(echoed["botId"], "rpc-bot");
        assert_eq!(echoed["arguments"], query);
    }
    assert_eq!(host.requests.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn hermes_rpc_scripts_never_retry_partial_failure_and_reject_bad_inputs_before_dispatch() {
    let host = Mock::start().await;
    assert_eq!(result(&host.run(&["partial_failure"], None).await, 1)["isError"], true);
    assert_eq!(host.requests.lock().unwrap().len(), 1);
    let invalid = host.run(&["echo", "--arguments", "[]"], None).await;
    assert_eq!(invalid.status.code(), Some(1));
    assert_eq!(invalid.stdout, [] as [u8; 0]);
    let path = host.home.join("oversized.json");
    std::fs::write(&path, vec![b' '; 256 * 1024 + 1]).unwrap();
    assert_eq!(host.run(&["echo", "--file", path.to_str().unwrap()], None).await.status.code(), Some(1));
    assert_eq!(host.run(&["echo", "--file", "-", "--arguments", "{}"], None).await.status.code(), Some(2));
    assert_eq!(
        host.requests.lock().unwrap().len(),
        1,
        "Invalid input and partial failures must not cause extra requests"
    );
}
