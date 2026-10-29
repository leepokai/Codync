//! Shared by the end-to-end tests.

pub mod host;

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Runs `codync-host serve` for `home` until it answers: the process and its base URL. A port
/// picked here is only free for a moment; when something else takes it before the host binds, the
/// host exits and this starts it again on another one.
pub async fn serve(home: &Path) -> (Child, String) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let _ = std::fs::create_dir_all(home);
    let log = home.join("host-stderr.log");
    let mut last = String::new();
    loop {
        let port = std::net::TcpListener::bind("127.0.0.1:0").expect("free port").local_addr().expect("port").port();
        let base = format!("http://127.0.0.1:{port}");
        let mut child = Command::new(env!("CARGO_BIN_EXE_codync-host"))
            .args(["serve", "--bind", "127.0.0.1", "--port", &port.to_string()])
            .env("CODYNC_HOME", home)
            .env("CODYNC_CLOUD", "off")
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(&log).map_or_else(|_| Stdio::null(), Stdio::from))
            .spawn()
            .expect("start the host");
        while child.try_wait().expect("host state").is_none() {
            match ready(&base, home).await {
                Ok(()) => return (child, base),
                Err(e) => last = e,
            }
            assert!(Instant::now() < deadline, "{}", not_started(&log, &last));
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(Instant::now() < deadline, "{}", not_started(&log, &last));
    }
}

/// Why a test host didn't answer: the last readiness check's error and the end of the host's log.
fn not_started(log: &Path, last: &str) -> String {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    let tail = lines[lines.len().saturating_sub(30)..].join("\n");
    format!("host didn't start; last readiness check: {last}\n--- host log ---\n{tail}")
}

/// Whether `base` is this home's host: on a reused port another test's host answers `/health` too.
pub async fn ready(base: &str, home: &Path) -> Result<(), String> {
    let token = std::fs::read_to_string(home.join("token")).map_err(|e| format!("no token yet: {e}"))?;
    let res = reqwest::Client::new()
        .get(format!("{base}/events?since=0&client=test"))
        .bearer_auth(token.trim())
        .timeout(Duration::from_secs(1))
        .send()
        .await
        .map_err(|e| format!("{e:?}"))?;
    res.status().is_success().then_some(()).ok_or_else(|| format!("status {}", res.status()))
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

/// The agent command that runs a Python fixture (quoted for `sh -c` / `cmd /c`). UTF-8 mode makes it read the host's
/// UTF-8 the way real agents do: Windows Python reads pipes in the ANSI code page, turning a `”` into a lone surrogate.
pub fn python_agent(script: &Path) -> String {
    let path = script.display().to_string();
    if cfg!(windows) {
        format!("{} -X utf8 -u \"{path}\"", python())
    } else {
        format!("{} -X utf8 -u '{}'", python(), path.replace('\'', "'\\''"))
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

/// How many routine runs still hold a session in `home`'s store: the host keeps each as a
/// `lane.session.<bot>@<bot>/routine-<run>` row (`store::model::run_session_key`) and forgets
/// it when the run ends.
#[allow(dead_code, reason = "only the routine tests wait for runs")]
pub fn routine_sessions(home: &Path) -> i64 {
    rusqlite::Connection::open(home.join("codync.db"))
        .and_then(|db| {
            db.query_row("SELECT COUNT(*) FROM kv WHERE k GLOB 'lane.session.*/routine-*'", [], |r| r.get(0))
        })
        .expect("read the host's store")
}

/// Bot `id` in a [`snapshot`] (which also holds the lead bot every host makes).
#[allow(dead_code, reason = "not every test binary looks bots up")]
pub fn bot<'a>(sync: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    sync["bots"].as_array().and_then(|bots| bots.iter().find(|b| b["id"] == id)).expect("the bot is in the snapshot")
}
