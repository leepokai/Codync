//! Shared by the end-to-end tests.

pub mod host;
pub mod steer;

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The seed of the cloud key a test host trusts for device certificates.
pub const TEST_CLOUD_SEED: [u8; 32] = [9; 32];

fn test_cloud_key() -> String {
    use base64::Engine as _;
    let key = ed25519_dalek::SigningKey::from_bytes(&TEST_CLOUD_SEED).verifying_key();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key.as_bytes())
}

/// The agent directory (`PI_CODING_AGENT_DIR`) oh-my-pi and pi get under a test host on `home`, so the
/// sessions its agent probes open land there, and go with `home`, not in the user's `~/.omp/agent`.
pub fn agent_dir(home: &Path) -> PathBuf {
    home.join("pi-agent")
}

/// `codync-host serve` on `home`, isolated from the computer: the cloud off, no agent CLI checks, its credential key
/// and oh-my-pi's sessions kept in `home`. It stops when its stdin closes: the pipe lives in the returned `Child` and
/// the test process, so a run that is killed takes its hosts with it.
pub fn host_command(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_codync-host"));
    command
        .arg("serve")
        .env("CODYNC_HOME", home)
        .env("CODYNC_CLOUD", "off")
        // Pairing checks the phone's account: tests sign certificates with `TEST_CLOUD_SEED`.
        .env("CODYNC_DEVICE_CERT_KEY", test_cloud_key())
        // A test host leaves the computer's agent CLIs and the user's logins alone.
        .env("CODYNC_AGENT_CHECKS", "off")
        // And the computer's credential store: its key goes with `home`, so no test can leave one behind.
        .env("CODYNC_VAULT", "folder")
        .env("PI_CODING_AGENT_DIR", agent_dir(home))
        .env("CODYNC_EXIT_WITH_STDIN", "1")
        .stdin(Stdio::piped());
    command
}

/// Runs `codync-host serve` for `home` until it answers: the process and its base URL. A port
/// picked here is only free for a moment; when something else takes it before the host binds, the
/// host exits and this starts it again on another one.
#[allow(dead_code, reason = "tests on `host::Host` start it through `serve_with`")]
pub async fn serve(home: &Path) -> (Child, String) {
    serve_with(home, &[]).await
}

/// [`serve`] with `env` on top of the test host's own.
pub async fn serve_with(home: &Path, env: &[(&str, &std::ffi::OsStr)]) -> (Child, String) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let _ = std::fs::create_dir_all(home);
    // The host logs to stdout (`tracing`) and panics to stderr: both go to one file.
    let log = home.join("host.log");
    let mut last = String::new();
    loop {
        let port = std::net::TcpListener::bind("127.0.0.1:0").expect("free port").local_addr().expect("port").port();
        let base = format!("http://127.0.0.1:{port}");
        let mut child = host_command(home)
            .args(["--bind", "127.0.0.1", "--port", &port.to_string()])
            .envs(env.iter().copied())
            .stdout(output(&log))
            .stderr(output(&log))
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

/// The end of `log` a test host appends to; nowhere when it can't be opened.
fn output(log: &Path) -> Stdio {
    std::fs::OpenOptions::new().create(true).append(true).open(log).map_or_else(|_| Stdio::null(), Stdio::from)
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
/// sign-in check (`cursor-agent status` then spins forever). Kills it after 20 s, this
/// test's own limit (launchd gives `HOST_EXIT`, 40 s): an agent's background program keeps the host
/// waiting its whole 10 s exit grace.
pub fn stop(child: &mut Child) {
    // Where no `kill` delivers it (Windows), waiting would only delay the kill.
    let signalled = Command::new("kill").arg(child.id().to_string()).status().is_ok_and(|s| s.success());
    let deadline = Instant::now() + if signalled { Duration::from_secs(20) } else { Duration::ZERO };
    while Instant::now() < deadline {
        if let Ok(Some(_)) = child.try_wait() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Kills process `pid` at once.
#[allow(dead_code, reason = "only tests that leave a process of their own use it")]
pub fn kill(pid: &str) {
    let mut command = Command::new(if cfg!(windows) { "taskkill" } else { "kill" });
    if cfg!(windows) {
        command.args(["/F", "/PID", pid])
    } else {
        command.args(["-KILL", pid])
    };
    command.status().unwrap();
}

/// Whether process `pid` still runs.
#[allow(dead_code, reason = "only tests that follow a process of their own use it")]
pub fn alive(pid: &str) -> bool {
    Command::new("kill").args(["-0", pid]).stderr(Stdio::null()).status().unwrap().success()
}

/// Waits until `done` holds, checking every 100 ms; fails naming `what` after `within`.
#[allow(dead_code, reason = "only some tests wait this way")]
pub async fn eventually(what: &str, within: Duration, mut done: impl AsyncFnMut() -> bool) {
    let deadline = Instant::now() + within;
    while !done().await {
        assert!(Instant::now() < deadline, "timed out waiting until {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Python as the test machine names it.
pub fn python() -> &'static str {
    if cfg!(windows) { "python" } else { "python3" }
}

/// Whether the test machine lacks Python, which a test of a Python fixture then skips.
#[allow(dead_code, reason = "only the tests of Python fixtures check")]
pub fn python_missing() -> bool {
    let missing = Command::new(python()).arg("--version").output().is_err();
    if missing {
        eprintln!("skipping: Python not available");
    }
    missing
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

/// The agent command that runs the Python fixture `fixture` (in `tests/fixtures`) with `args`.
pub fn fixture_command(fixture: &str, args: &str) -> String {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(fixture);
    format!("{} {args}", python_agent(&script))
}

/// The command of a program that listens on `port` of this computer until it's stopped.
#[allow(dead_code, reason = "only the tests of dev servers use it")]
pub fn port_server(port: u16) -> String {
    fixture_command("port_server.py", &port.to_string())
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

/// Bot `name`'s own folder in `home`'s store: where its agent runs, and a fixture agent logs.
#[allow(dead_code, reason = "not every test binary looks folders up")]
pub fn folder(home: &Path, name: &str) -> std::path::PathBuf {
    let db = rusqlite::Connection::open(home.join("codync.db")).expect("read the host's store");
    let sql =
        "SELECT json_extract(config, '$.cwd') FROM bots WHERE json_extract(config, '$.name') = ?1 AND deleted = 0";
    let cwd: String = db.query_row(sql, [name], |r| r.get(0)).expect("a bot of that name");
    cwd.into()
}

/// Bot `id` in a [`snapshot`] (which also holds the lead bot every host makes).
#[allow(dead_code, reason = "not every test binary looks bots up")]
pub fn bot<'a>(sync: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    sync["bots"].as_array().and_then(|bots| bots.iter().find(|b| b["id"] == id)).expect("the bot is in the snapshot")
}
