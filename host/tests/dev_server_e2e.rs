//! Dev servers through a real host: it runs them, so they outlive the agent; clients see their ports and can stop them;
//! what a bot leaves running through its shell only shows.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

#[path = "dev_server_e2e/cards.rs"]
mod cards;
#[path = "dev_server_e2e/detected.rs"]
mod detected;

use common::host::Host;
use serde_json::{Value, json};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// A free port below where the system hands out its own, the range a server someone opens sits in. Looked for from
/// somewhere random: tests run side by side and a port only stays free until its server takes it.
fn free_low_port() -> u16 {
    let from = 20_000 + u16::try_from(uuid::Uuid::new_v4().as_u128() % 10_000).unwrap();
    (from..32_000).find(|port| TcpListener::bind(("127.0.0.1", *port)).is_ok()).unwrap()
}

fn serves(port: u16) -> bool {
    TcpStream::connect(("127.0.0.1", port)).is_ok()
}

fn server_command(port: u16) -> String {
    common::port_server(port)
}

/// A program that keeps running and never listens.
fn sleeper(seconds: u32) -> String {
    format!("{} -c \"import time; time.sleep({seconds})\"", common::python())
}

fn kill(pid: &str) {
    let mut command = Command::new(if cfg!(windows) { "taskkill" } else { "kill" });
    if cfg!(windows) {
        command.args(["/F", "/PID", pid])
    } else {
        command.args(["-KILL", pid])
    };
    command.status().unwrap();
}

async fn start(host: &Host, bot: &str, command: &str, name: &str) -> Value {
    let arguments = json!({"command": command, "name": name});
    host.call("chatCall", json!({"botId": bot, "name": "start_dev_server", "arguments": arguments})).await
}

async fn servers(host: &Host, bot: &str) -> Vec<Value> {
    common::bot(&host.snapshot().await, bot)["devServers"].as_array().unwrap().clone()
}

async fn workspace(host: &Host, bot: &str) -> PathBuf {
    PathBuf::from(common::bot(&host.snapshot().await, bot)["cwd"].as_str().unwrap())
}

async fn wait_for<F: Fn(&[Value]) -> bool>(host: &Host, bot: &str, what: &str, done: F) -> Vec<Value> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let list = servers(host, bot).await;
        if done(&list) {
            return list;
        }
        assert!(Instant::now() < deadline, "{what}: {list:?}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Waits until the first server runs on `port`.
async fn wait_running(host: &Host, bot: &str, port: u16) {
    wait_for(host, bot, "running with its port", |list| {
        list.first().is_some_and(|s| s["status"] == "running" && s["ports"][0]["port"] == port)
    })
    .await;
}

async fn wait_closed(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while serves(port) {
        assert!(Instant::now() < deadline, "port {port} stayed open");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn stop(host: &Host, bot: &str, id: &Value, port: u16) {
    host.call("devServerStop", json!({"botId": bot, "id": id})).await;
    wait_closed(port).await;
}

async fn agent_bot(host: &Host, port: u16) -> String {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/shell_server_agent.py");
    let command = format!("{} {port}", common::python_agent(&script));
    let bot = json!({"name": "Builder", "backend": "custom", "command": command, "connectors": [], "permission": "auto", "notify": false});
    host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn a_server_outlives_its_agent_and_shows_its_port() {
    let host = Host::start().await;
    let bot = agent_bot(&host, 0).await;
    host.reply(&bot, "hello").await;
    let port = free_port();

    let started = start(&host, &bot, &server_command(port), "web").await;
    assert_eq!(started["status"], "running", "{started}");
    assert_eq!(started["ports"][0]["port"], port);
    assert_eq!(started["ports"][0]["local"], format!("http://localhost:{port}"));
    assert_eq!(started["ports"][0]["remote"], json!([]), "it listens on loopback only");
    assert!(serves(port));

    let listed = servers(&host, &bot).await;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["name"], "web");
    assert_eq!(listed[0]["managed"], true);

    let agent = std::fs::read_to_string(workspace(&host, &bot).await.join("agent.pid")).unwrap();
    kill(agent.trim());
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(serves(port), "the server died with the agent");
    assert_eq!(servers(&host, &bot).await[0]["status"], "running");

    stop(&host, &bot, &started["id"], port).await;
}

/// lifecycle.rs stops an agent right after a routine's run with no other session open. Windows can't tell this fixture
/// from one with a dev server of its own (`background_work`), so its agent stays there.
#[cfg(unix)]
#[tokio::test]
async fn a_server_outlives_the_agent_of_a_routine_run() {
    let host = Host::start().await;
    let bot = agent_bot(&host, 0).await;
    let port = free_port();
    let started = start(&host, &bot, &server_command(port), "web").await;

    let routine = json!({"botId": bot, "name": "once", "instruction": "go", "triggers": [{"type": "webhook"}], "timeoutSeconds": 30});
    let routine = host.call("saveRoutine", routine).await["routine"]["id"].clone();
    host.call("runRoutine", json!({"botId": bot, "id": routine})).await;
    common::eventually("the run succeeded", Duration::from_secs(25), async || {
        host.call("routines", json!({"botId": bot})).await["runs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|run| run["routineId"] == routine && run["status"] == "succeeded")
    })
    .await;
    let agent = std::fs::read_to_string(workspace(&host, &bot).await.join("agent.pid")).unwrap();
    common::eventually("the run's agent stopped", Duration::from_secs(20), async || {
        !Command::new("kill").args(["-0", agent.trim()]).status().unwrap().success()
    })
    .await;

    assert!(serves(port), "the server died with the agent");
    assert_eq!(servers(&host, &bot).await[0]["status"], "running");
    stop(&host, &bot, &started["id"], port).await;
}

#[tokio::test]
async fn a_client_stops_restarts_reads_and_removes_a_server() {
    let host = Host::start().await;
    let bot = host.bot("Builder", "auto").await;
    let port = free_port();
    let started = start(&host, &bot, &server_command(port), "web").await;
    let id = started["id"].clone();

    stop(&host, &bot, &id, port).await;
    let list = wait_for(&host, &bot, "stopped", |list| list[0]["status"] == "stopped").await;
    assert_eq!(list[0]["ports"], json!([]));

    host.call("devServerRestart", json!({"botId": bot, "id": id})).await;
    wait_running(&host, &bot, port).await;
    assert!(serves(port));

    let logs = host.call("devServerLogs", json!({"botId": bot, "id": id})).await;
    assert!(logs["text"].is_string(), "{logs}");

    host.call("devServerRemove", json!({"botId": bot, "id": id})).await;
    wait_closed(port).await;
    assert_eq!(servers(&host, &bot).await, Vec::<Value>::new());
    let error = host.refused("devServerStop", json!({"botId": bot, "id": id})).await;
    assert!(error.contains("unknown dev server"), "{error}");
}

#[tokio::test]
async fn a_bot_only_reaches_its_own_servers() {
    let host = Host::start().await;
    let (owner, other) = (host.bot("Owner", "auto").await, host.bot("Other", "auto").await);
    let port = free_port();
    let started = start(&host, &owner, &server_command(port), "web").await;
    let id = started["id"].clone();
    let stop_other = json!({"botId": other, "name": "stop_dev_server", "arguments": {"id": id}});
    assert!(host.refused("chatCall", stop_other).await.contains("unknown dev server"));
    let logs = json!({"botId": other, "name": "dev_server_logs", "arguments": {"id": id}});
    assert!(host.refused("chatCall", logs).await.contains("unknown dev server"));
    assert!(serves(port));
    stop(&host, &owner, &id, port).await;
}

#[tokio::test]
async fn the_same_command_is_one_server_and_a_bot_runs_at_most_five() {
    let host = Host::start().await;
    let bot = host.bot("Builder", "auto").await;
    let port = free_port();
    let first = start(&host, &bot, &server_command(port), "web").await;
    let again = start(&host, &bot, &server_command(port), "web").await;
    assert_eq!(first["id"], again["id"]);
    assert_eq!(again["reused"], true);
    // The same folder written another way is still the same server.
    let arguments = json!({"command": server_command(port), "folder": "."});
    let dotted = host.call("chatCall", json!({"botId": bot, "name": "start_dev_server", "arguments": arguments})).await;
    assert_eq!(first["id"], dotted["id"], "{dotted}");
    assert_eq!(servers(&host, &bot).await.len(), 1);

    // Without a port a start waits its whole time, so one of these.
    start(&host, &bot, &sleeper(300), "watch").await;
    for n in 0..3 {
        start(&host, &bot, &server_command(free_port()), &format!("s{n}")).await;
    }
    let arguments = json!({"command": sleeper(400)});
    let refused =
        host.refused("chatCall", json!({"botId": bot, "name": "start_dev_server", "arguments": arguments})).await;
    assert!(refused.contains("at most 5"), "{refused}");

    let list = wait_for(&host, &bot, "a server without a port settles", |list| {
        list.iter().filter(|s| s["status"] == "running").count() == 5
    })
    .await;
    assert_eq!(list.iter().filter(|s| s["ports"] == json!([])).count(), 1);

    // A person starting a stopped one is held to the limit too.
    stop(&host, &bot, &first["id"], port).await;
    start(&host, &bot, &server_command(free_port()), "s3").await;
    let refused = host.refused("devServerRestart", json!({"botId": bot, "id": first["id"]})).await;
    assert!(refused.contains("at most 5"), "{refused}");
    assert!(!serves(port));
}

#[tokio::test]
async fn a_failed_start_says_why_and_a_bot_can_read_it() {
    let host = Host::start().await;
    let bot = host.bot("Builder", "auto").await;
    let failing = if cfg!(windows) { "echo no such script & exit 3" } else { "echo no such script; exit 3" };
    let answer = start(&host, &bot, failing, "docs").await;
    assert_eq!(answer["status"], "failed", "{answer}");
    assert_eq!(answer["exitCode"], 3);
    assert!(answer["log"].as_str().unwrap().contains("no such script"), "{answer}");

    let arguments = json!({"id": answer["id"]});
    let logs = host.call("chatCall", json!({"botId": bot, "name": "dev_server_logs", "arguments": arguments})).await;
    assert!(logs["log"].as_str().unwrap().contains("no such script"), "{logs}");
    // A program that ends by itself isn't brought back.
    assert_eq!(servers(&host, &bot).await[0]["status"], "failed");
}

#[tokio::test]
async fn a_host_restart_brings_back_what_was_running_only() {
    let mut host = Host::start().await;
    let bot = host.bot("Builder", "auto").await;
    let (kept, stopped) = (free_port(), free_port());
    let first = start(&host, &bot, &server_command(kept), "kept").await;
    let second = start(&host, &bot, &server_command(stopped), "stopped").await;
    stop(&host, &bot, &second["id"], stopped).await;

    host.restart(|_| {}).await;

    let list = wait_for(&host, &bot, "the running one is back", |list| {
        list.iter().any(|s| s["id"] == first["id"] && s["status"] == "running" && s["ports"][0]["port"] == kept)
    })
    .await;
    assert!(serves(kept));
    assert!(!serves(stopped));
    assert_eq!(list.iter().find(|s| s["id"] == second["id"]).unwrap()["status"], "stopped");
    stop(&host, &bot, &first["id"], kept).await;
}

#[tokio::test]
async fn a_crashed_host_doesnt_leave_its_server_holding_the_port() {
    let mut host = Host::start().await;
    let bot = host.bot("Builder", "auto").await;
    let port = free_port();
    // A line for each run, to tell the next one from what the crash left. On Windows a job ends with its host.
    let command =
        if cfg!(windows) { server_command(port) } else { format!("echo $$ >> runs; exec {}", server_command(port)) };
    let started = start(&host, &bot, &command, "web").await;
    assert_eq!(started["status"], "running", "{started}");

    host.crash();
    host.restart(|_| {}).await;

    wait_running(&host, &bot, port).await;
    assert!(serves(port));
    if cfg!(unix) {
        let runs = std::fs::read_to_string(workspace(&host, &bot).await.join("runs")).unwrap();
        let runs: Vec<&str> = runs.lines().collect();
        assert_eq!(runs.len(), 2, "{runs:?}");
        assert_ne!(runs[0], runs[1]);
        assert!(!Command::new("kill").args(["-0", runs[0]]).status().unwrap().success(), "the old one still runs");
    }
    stop(&host, &bot, &started["id"], port).await;
}

/// What a computer restart looks like to the host: these servers last ran on an earlier boot.
fn from_an_earlier_boot(home: &Path) {
    let db = rusqlite::Connection::open(home.join("codync.db")).unwrap();
    db.execute("UPDATE kv SET v = json_set(v, '$.bootMs', 1000) WHERE k LIKE 'dev-server:%'", []).unwrap();
}

#[tokio::test]
async fn after_a_computer_restart_a_server_stays_stopped_until_someone_starts_it() {
    let mut host = Host::start().await;
    let bot = host.bot("Builder", "auto").await;
    let port = free_port();
    let started = start(&host, &bot, &server_command(port), "web").await;

    host.restart(from_an_earlier_boot).await;
    let stopped = |list: &[Value]| list.first().is_some_and(|s| s["status"] == "stopped");
    wait_for(&host, &bot, "listed as stopped", stopped).await;
    // Long enough for a start it shouldn't have made.
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(stopped(&servers(&host, &bot).await));
    assert!(!serves(port));

    // Nor at the next host start.
    host.restart(|_| {}).await;
    wait_for(&host, &bot, "still stopped", stopped).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(stopped(&servers(&host, &bot).await));
    assert!(!serves(port));

    host.call("devServerRestart", json!({"botId": bot, "id": started["id"]})).await;
    wait_running(&host, &bot, port).await;
    stop(&host, &bot, &started["id"], port).await;
}

/// Sends a start and hangs up a moment later, as a cancelled tool call or a dying helper does.
async fn start_and_hang_up(host: &Host, body: &Value) {
    use tokio::io::AsyncWriteExt as _;
    let address = host.base.trim_start_matches("http://");
    let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
    let body = body.to_string();
    let request = format!(
        "POST /api/chatCall HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        host.token,
        body.len(),
    );
    socket.write_all(request.as_bytes()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(3)).await;
}

#[tokio::test]
async fn a_start_whose_request_goes_away_still_settles() {
    let host = Host::start().await;
    let bot = host.bot("Builder", "auto").await;
    let port = free_port();
    let arguments = json!({"command": server_command(port), "name": "web"});
    let body = json!({"botId": bot, "name": "start_dev_server", "arguments": arguments});
    // A hang-up before the host read the request leaves no row, so try again: that start is what's under test.
    let deadline = Instant::now() + Duration::from_secs(20);
    while servers(&host, &bot).await.is_empty() {
        assert!(Instant::now() < deadline, "the host never read the request");
        start_and_hang_up(&host, &body).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    wait_running(&host, &bot, port).await;
    let id = servers(&host, &bot).await[0]["id"].clone();
    stop(&host, &bot, &id, port).await;
    wait_for(&host, &bot, "stopped", |list| list[0]["status"] == "stopped").await;
}

#[tokio::test]
async fn deleting_a_bot_ends_its_servers() {
    let host = Host::start().await;
    let bot = host.bot("Builder", "auto").await;
    let port = free_port();
    start(&host, &bot, &server_command(port), "web").await;
    assert!(serves(port));
    host.call("deleteBot", json!({"botId": bot})).await;
    wait_closed(port).await;
}

#[tokio::test]
async fn a_log_that_cant_be_read_fails_reading_it_not_a_start() {
    let host = Host::start().await;
    let bot = host.bot("Builder", "auto").await;
    let port = free_port();
    let command = server_command(port);
    let started = start(&host, &bot, &command, "web").await;
    let id = started["id"].as_str().unwrap();
    // A folder where its log should be. The running server keeps writing to the file, moved aside.
    let log = host.home.join("dev-servers").join(format!("{id}.log"));
    std::fs::rename(&log, log.with_extension("moved")).unwrap();
    std::fs::create_dir(&log).unwrap();

    let again = start(&host, &bot, &command, "web").await;
    assert_eq!(again["id"], started["id"], "{again}");
    assert_eq!(again["status"], "running", "{again}");
    assert!(again["log"].as_str().unwrap().contains("couldn't read its log"), "{again}");

    let refused = host.refused("devServerLogs", json!({"botId": bot, "id": id})).await;
    assert!(refused.contains("reading"), "{refused}");
    let arguments = json!({"id": id});
    let refused =
        host.refused("chatCall", json!({"botId": bot, "name": "dev_server_logs", "arguments": arguments})).await;
    assert!(refused.contains("reading"), "{refused}");

    std::fs::remove_dir(&log).unwrap();
    let logs = host.call("devServerLogs", json!({"botId": bot, "id": id})).await;
    assert_eq!(logs["text"], "", "no file is only an empty log");
    stop(&host, &bot, &started["id"], port).await;
}

/// One call for each of `bodies`, all at once.
async fn at_once(host: &Host, method: &str, bodies: &[Value]) -> Vec<(bool, Value)> {
    futures::future::join_all(bodies.iter().map(|body| host.post(method, body))).await
}

fn refused(answers: &[(bool, Value)]) -> Vec<&Value> {
    answers.iter().filter(|(ok, _)| !ok).map(|(_, answer)| answer).collect()
}

fn active(list: &[Value]) -> usize {
    list.iter().filter(|s| s["status"] == "running" || s["status"] == "starting").count()
}

#[tokio::test]
async fn parallel_starts_make_one_server_of_one_command_and_keep_the_limit() {
    let host = Host::start().await;
    let bot = host.bot("Builder", "auto").await;
    let body = |command: String| json!({"botId": bot, "name": "start_dev_server", "arguments": {"command": command}});

    let same = server_command(free_port());
    let bodies: Vec<Value> = (0..4).map(|_| body(same.clone())).collect();
    let answers = at_once(&host, "chatCall", &bodies).await;
    assert!(answers.iter().all(|(ok, answer)| *ok && answer["id"] == answers[0].1["id"]), "{answers:?}");
    assert_eq!(answers.iter().filter(|(_, answer)| answer["reused"] == false).count(), 1, "{answers:?}");
    assert_eq!(servers(&host, &bot).await.len(), 1);

    // Four more fit.
    let bodies: Vec<Value> = (0..7).map(|_| body(server_command(free_port()))).collect();
    let answers = at_once(&host, "chatCall", &bodies).await;
    let refusals = refused(&answers);
    assert_eq!(refusals.len(), 3, "{answers:?}");
    assert!(refusals.iter().all(|answer| answer["error"].as_str().unwrap().contains("at most 5")), "{refusals:?}");
    assert_eq!(active(&servers(&host, &bot).await), 5);
}

#[tokio::test]
async fn parallel_restarts_keep_the_limit() {
    let host = Host::start().await;
    let bot = host.bot("Builder", "auto").await;
    let mut stopped = Vec::new();
    for name in ["a", "b"] {
        let port = free_port();
        let started = start(&host, &bot, &server_command(port), name).await;
        stop(&host, &bot, &started["id"], port).await;
        stopped.push(json!({"botId": bot, "id": started["id"]}));
    }
    for n in 0..4 {
        start(&host, &bot, &server_command(free_port()), &format!("s{n}")).await;
    }

    let answers = at_once(&host, "devServerRestart", &stopped).await;
    let refusals = refused(&answers);
    assert_eq!(refusals.len(), 1, "{answers:?}");
    assert!(refusals[0]["error"].as_str().unwrap().contains("at most 5"), "{refusals:?}");
    assert_eq!(active(&servers(&host, &bot).await), 5);
}
