//! Dev servers through a real host: it runs them, so they outlive the agent; clients see their ports and can stop them;
//! what a bot leaves running through its shell only shows.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// A free port below where the system hands out its own, the range a server someone opens sits in.
fn free_low_port() -> u16 {
    (20_000..32_000).find(|port| TcpListener::bind(("127.0.0.1", *port)).is_ok()).unwrap()
}

fn serves(port: u16) -> bool {
    TcpStream::connect(("127.0.0.1", port)).is_ok()
}

fn server_command(port: u16) -> String {
    format!("{} -m http.server {port} --bind 127.0.0.1", common::python())
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
async fn under_ask_a_start_needs_a_turn_to_ask_in() {
    let host = Host::start().await;
    let bot = host.bot("Builder", "ask").await;
    let arguments = json!({"command": sleeper(300)});
    let refused =
        host.refused("chatCall", json!({"botId": bot, "name": "start_dev_server", "arguments": arguments})).await;
    assert!(refused.contains("didn't allow"), "{refused}");
    assert_eq!(servers(&host, &bot).await, Vec::<Value>::new());
}

/// What makes the fixture agent call the tool through its real chat helper, in a turn.
fn start_through_agent(command: &str) -> String {
    format!("CHAT start_dev_server {}", json!({"command": command, "name": "web"}))
}

async fn entry(host: &Host, bot: &str, found: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(40);
    loop {
        let history = host.call("history", json!({"botId": bot})).await;
        if let Some(entry) = history["entries"].as_array().unwrap().iter().find(|e| found(e)) {
            return entry.clone();
        }
        assert!(Instant::now() < deadline, "{bot}: {history}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Has the agent start `command`, answers its card with `option`, and returns the agent's reply.
async fn through_card(host: &Host, bot: &str, command: &str, option: &str, check: impl Fn(&str)) -> String {
    host.call("send", json!({"botId": bot, "text": start_through_agent(command)})).await;
    let card = entry(host, bot, |e| {
        e["kind"] == "permission" && e["data"]["status"] == "pending" && e["data"]["title"] == "Start a dev server"
    })
    .await;
    check(card["data"]["detail"].as_str().unwrap());
    host.call("respondPermission", json!({"entryId": card["id"], "optionId": option})).await;
    let reply =
        entry(host, bot, |e| e["kind"] == "agent" && e["data"]["final"] == true && e["turn"] == card["turn"]).await;
    reply["data"]["text"].as_str().unwrap().to_owned()
}

async fn cards(host: &Host, bot: &str) -> usize {
    let history = host.call("history", json!({"botId": bot})).await;
    history["entries"].as_array().unwrap().iter().filter(|e| e["kind"] == "permission").count()
}

#[tokio::test]
async fn under_ask_a_start_waits_for_its_card() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Builder", "team_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "permission": "ask"})).await;

    let denied = free_port();
    let reply = through_card(&host, &bot, &server_command(denied), "deny", |_| {}).await;
    assert!(reply.starts_with("Refused: ") && reply.contains("didn't allow"), "{reply}");
    assert_eq!(servers(&host, &bot).await, Vec::<Value>::new());
    assert!(!serves(denied));

    let folder = workspace(&host, &bot).await;
    let port = free_port();
    let command = server_command(port);
    let reply = through_card(&host, &bot, &command, "allow", |detail| {
        assert!(detail.contains(&command) && detail.contains(folder.to_str().unwrap()), "{detail}");
    })
    .await;
    assert!(reply.starts_with("Done: "), "{reply}");
    assert!(serves(port));

    // Allowed once, so a stopped one comes back with no second card.
    let id = servers(&host, &bot).await[0]["id"].clone();
    stop(&host, &bot, &id, port).await;
    let before = cards(&host, &bot).await;
    host.reply(&bot, &start_through_agent(&command)).await;
    assert!(serves(port));
    assert_eq!(cards(&host, &bot).await, before);
    stop(&host, &bot, &id, port).await;
}

#[tokio::test]
async fn a_server_nobody_allowed_gets_its_card_once_the_bot_is_under_ask() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Builder", "team_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "permission": "auto"})).await;
    let port = free_port();
    let command = server_command(port);
    let started = start(&host, &bot, &command, "web").await;
    stop(&host, &bot, &started["id"], port).await;
    host.call("updateBot", json!({"id": bot, "permission": "ask"})).await;

    let reply = through_card(&host, &bot, &command, "deny", |_| {}).await;
    assert!(reply.starts_with("Refused: "), "{reply}");
    assert!(!serves(port));

    let reply = through_card(&host, &bot, &command, "allow", |_| {}).await;
    assert!(reply.starts_with("Done: "), "{reply}");
    assert!(serves(port));

    stop(&host, &bot, &started["id"], port).await;
    let before = cards(&host, &bot).await;
    host.reply(&bot, &start_through_agent(&command)).await;
    assert_eq!(cards(&host, &bot).await, before, "now someone has allowed it");
    stop(&host, &bot, &started["id"], port).await;
}

#[tokio::test]
async fn what_an_agent_leaves_running_shows_and_can_be_stopped() {
    let host = Host::start().await;
    let port = free_low_port();
    let bot = agent_bot(&host, port).await;
    host.reply(&bot, "run it").await;
    let list = wait_for(&host, &bot, "the agent's server shows", |list| {
        list.iter().any(|s| s["managed"] == false && s["ports"][0]["port"] == port)
    })
    .await;
    let found = list.iter().find(|s| s["managed"] == false).unwrap();
    assert_eq!(found["status"], "running");
    assert!(found["id"].as_str().unwrap().starts_with("detected:"));

    // It can't start again, so asking leaves it running.
    let refused = host.refused("devServerRestart", json!({"botId": bot, "id": found["id"]})).await;
    assert!(refused.contains("unknown dev server"), "{refused}");
    assert!(serves(port));

    stop(&host, &bot, &found["id"], port).await;
    wait_for(&host, &bot, "it no longer shows", <[Value]>::is_empty).await;
    // An id that was never found stops nothing.
    let error = host.refused("devServerStop", json!({"botId": bot, "id": "detected:1"})).await;
    assert!(error.contains("unknown dev server"), "{error}");
}
