//! Shared by the end-to-end tests.

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Use the checksum-verified release fixture so isolated hosts don't each download it.
pub fn seed_memory_runtime(home: &std::path::Path) {
    let Some(binary) = std::env::var_os("CODYNC_TEST_ENGRAM") else {
        return;
    };
    let dir = home.join("engram/3.2.1");
    std::fs::create_dir_all(&dir).expect("memory runtime folder");
    std::fs::copy(binary, dir.join(if cfg!(windows) { "engram.exe" } else { "engram" })).expect("copy Engram fixture");
    std::fs::write(dir.join(".installed"), "3.2.1").expect("mark fixture installed");
}

/// Runs `codync-host serve` for `home` until it answers: the process and its base URL. A port
/// picked here is only free for a moment; when something else takes it before the host binds, the
/// host exits and this starts it again on another one.
pub async fn serve(home: &std::path::Path) -> (Child, String) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let port = std::net::TcpListener::bind("127.0.0.1:0").expect("free port").local_addr().expect("port").port();
        let base = format!("http://127.0.0.1:{port}");
        let mut child = Command::new(env!("CARGO_BIN_EXE_codync-host"))
            .args(["serve", "--bind", "127.0.0.1", "--port", &port.to_string()])
            .env("CODYNC_HOME", home)
            .env("CODYNC_CLOUD", "off")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start the host");
        while child.try_wait().expect("host state").is_none() {
            if reqwest::get(format!("{base}/health")).await.is_ok() {
                return (child, base);
            }
            assert!(Instant::now() < deadline, "host didn't start");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(Instant::now() < deadline, "host didn't start");
    }
}

/// Stops a test host the way launchd does (SIGTERM), so it shuts down its own
/// children: a SIGKILL would orphan whatever it had running, such as a harness
/// sign-in check (`cursor-agent status` then spins forever). Kills it after 5 s.
pub fn stop(child: &mut Child) {
    let _ = Command::new("kill").arg(child.id().to_string()).status();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(Some(_)) = child.try_wait() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Python as the test machine names it.
pub fn python() -> &'static str {
    if cfg!(windows) { "python" } else { "python3" }
}

/// The agent command that runs a Python fixture (quoted for `sh -c` / `cmd /c`).
pub fn python_agent(script: &std::path::Path) -> String {
    let path = script.display().to_string();
    if cfg!(windows) {
        format!("{} -u \"{path}\"", python())
    } else {
        format!("{} -u '{}'", python(), path.replace('\'', "'\\''"))
    }
}

/// What a client holds after the event stream's catch-up: `{"rev", "bots": [...], "entries": [...]}`,
/// the newest row per id. Reads until the stream goes quiet.
pub async fn snapshot(base: &str, token: &str) -> serde_json::Value {
    use futures::StreamExt as _;
    let res = reqwest::Client::new()
        .get(format!("{base}/events?since=0&client=test"))
        .bearer_auth(token)
        .send()
        .await
        .expect("open the event stream");
    let mut body = res.bytes_stream();
    let mut buf = String::new();
    let (mut rev, mut bots, mut entries) = (serde_json::Value::Null, Vec::<serde_json::Value>::new(), Vec::new());
    while let Ok(Some(Ok(chunk))) = tokio::time::timeout(Duration::from_millis(300), body.next()).await {
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(i) = buf.find('\n') {
            let line: String = buf.drain(..=i).collect();
            let Some(event) = line
                .trim_end()
                .strip_prefix("data:")
                .and_then(|data| serde_json::from_str::<serde_json::Value>(data.trim_start()).ok())
            else {
                continue;
            };
            let (rows, row) = match event["type"].as_str() {
                Some("hello") => {
                    rev = event["rev"].clone();
                    continue;
                }
                Some("bot") => (&mut bots, event["bot"].clone()),
                Some("entry") => (&mut entries, event["entry"].clone()),
                _ => continue,
            };
            rows.retain(|r| r["id"] != row["id"]);
            rows.push(row);
        }
    }
    serde_json::json!({"rev": rev, "bots": bots, "entries": entries})
}
