//! The five pause orderings on `feat/bot-pause`. Each test holds the actor at the gap the
//! ordering needs (`CODYNC_RACE_GATE_DIR`, inert when unset) and asserts the invariant.
//! On today's head the invariant is false, so the test fails.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{Duration, Instant};

struct Gates {
    dir: PathBuf,
}

impl Gates {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("codync-race-gates-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }

    async fn host(&self) -> Host {
        Host::start_with(&[("CODYNC_RACE_GATE_DIR", self.dir.as_os_str())]).await
    }

    fn arm(&self, name: &str) {
        std::fs::write(self.dir.join(format!("{name}.hold")), "").unwrap();
    }

    fn release(&self, name: &str) {
        let path = self.dir.join(format!("{name}.hold"));
        if path.exists() {
            std::fs::remove_file(path).unwrap();
        }
    }

    async fn entered(&self, name: &str) {
        let path = self.dir.join(format!("{name}.entered"));
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if path.exists() {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for the {name} gate");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

async fn wait_for(what: &str, mut check: impl AsyncFnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if check().await {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn prompt_blob(host: &Host) -> String {
    let Ok(bots) = std::fs::read_dir(host.home.join("bots")) else { return String::new() };
    let mut out = String::new();
    for bot in bots.flatten() {
        let path = bot.path().join("workspace").join("prompts.jsonl");
        if let Ok(text) = std::fs::read_to_string(path) {
            out.push_str(&text);
            out.push('\n');
        }
    }
    out
}

fn around(text: &str, needle: &str) -> String {
    let Some(at) = text.find(needle) else { return text.chars().take(240).collect() };
    let start = text.floor_char_boundary(at.saturating_sub(80));
    let end = text.ceil_char_boundary((at + needle.len() + 160).min(text.len()));
    text[start..end].to_owned()
}

/// A bot-request turn that carries `needle`. The asker's own `MESSAGE …|needle` prompt does not count:
/// that line never says "sent you this request".
async fn leaked(host: &Host, bot: &str, needle: &str) -> Option<String> {
    let Ok(bots) = std::fs::read_dir(host.home.join("bots")) else { return None };
    for entry in bots.flatten() {
        let path = entry.path().join("workspace").join("prompts.jsonl");
        let Ok(text) = std::fs::read_to_string(path) else { continue };
        if text.contains("sent you this request") && text.contains(needle) {
            return Some(around(&text, needle));
        }
    }
    let snapshot = host.snapshot().await;
    let row = snapshot["bots"].as_array()?.iter().find(|b| b["id"] == bot)?;
    let work = row["work"].as_array()?;
    work.iter().find(|task| task.to_string().contains(needle)).map(ToString::to_string)
}

/// The cut-off turn's continuation. The fixture answers by quoting the resume prompt, and the
/// prompt log keeps the same sentence. Neither is a bot-request turn.
async fn resumed_within(host: &Host, bot: &str, secs: u64) -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        let prompts = prompt_blob(host);
        if prompts.contains("Codync restarted") {
            return Some(around(&prompts, "Codync restarted"));
        }
        let history = host.call("history", json!({"botId": bot})).await;
        let text = history.to_string();
        if text.contains("Codync restarted") {
            return Some(around(&text, "Codync restarted"));
        }
        if Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn leaked_within(host: &Host, bot: &str, needle: &str, secs: u64) -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        if let Some(found) = leaked(host, bot, needle).await {
            return Some(found);
        }
        if Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn routine(host: &Host, bot: &str, name: &str, instruction: &str) -> String {
    let body = json!({"botId": bot, "name": name, "instruction": instruction, "triggers": [{"type": "webhook"}]});
    host.call("saveRoutine", body).await["routine"]["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn a_request_queued_before_a_pause_does_not_start_after_resume() {
    let gates = Gates::new();
    let host = gates.host().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;
    host.works_in_chat(&bot).await;
    let asker = host.fixture_bot("Ada", "work_agent.py", "").await;
    host.reply(&asker, "hello").await;

    host.call("send", json!({"botId": bot, "text": "HOLD:chat", "clientNonce": "hold-chat"})).await;
    wait_for("the held turn's prompt", async || prompt_blob(&host).contains("HOLD:chat")).await;

    gates.arm("unboxed_request_queued");
    host.call(
        "send",
        json!({"botId": asker, "text": format!("MESSAGE {bot}|PRE_PAUSE_REQUEST"), "clientNonce": "pre-pause"}),
    )
    .await;
    gates.entered("unboxed_request_queued").await;
    gates.release("unboxed_request_queued");

    gates.arm("before_next_in_queue");
    std::fs::write(host.workspace(&bot).join("release-chat"), "").unwrap();
    gates.entered("before_next_in_queue").await;
    host.call("updateBot", json!({"id": bot, "paused": true})).await;
    host.call("updateBot", json!({"id": bot, "paused": false})).await;
    gates.release("before_next_in_queue");

    let leaked = leaked_within(&host, &bot, "PRE_PAUSE_REQUEST", 15).await;
    assert!(leaked.is_none(), "a request queued before the pause started after resume: {leaked:?}");
}

#[tokio::test]
async fn a_request_that_missed_pause_cleanup_does_not_start_after_resume() {
    let gates = Gates::new();
    let host = gates.host().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;
    let asker = host.fixture_bot("Ada", "work_agent.py", "").await;
    host.reply(&asker, "hello").await;

    gates.arm("before_boxed_request_insert");
    host.call(
        "send",
        json!({"botId": asker, "text": format!("MESSAGE {bot}|MISSED_CLEANUP_REQUEST"), "clientNonce": "missed"}),
    )
    .await;
    gates.entered("before_boxed_request_insert").await;
    host.call("updateBot", json!({"id": bot, "paused": true})).await;
    host.call("updateBot", json!({"id": bot, "paused": false})).await;
    gates.release("before_boxed_request_insert");

    let leaked = leaked_within(&host, &bot, "MISSED_CLEANUP_REQUEST", 15).await;
    assert!(leaked.is_none(), "a request that missed pause cleanup started after resume: {leaked:?}");
}

#[tokio::test]
async fn a_routine_admitted_after_resume_survives_a_stale_pause() {
    let gates = Gates::new();
    let host = gates.host().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;
    host.works_in_chat(&bot).await;
    let old = routine(&host, &bot, "Old", "SHOULD_NOT_RUN").await;
    let fresh = routine(&host, &bot, "Fresh", "PASS").await;

    host.call("send", json!({"botId": bot, "text": "HOLD:chat", "clientNonce": "hold-chat"})).await;
    wait_for("the held turn's prompt", async || prompt_blob(&host).contains("HOLD:chat")).await;

    gates.arm("routine_queued");
    host.call("runRoutine", json!({"botId": bot, "id": old})).await;
    gates.entered("routine_queued").await;
    gates.release("routine_queued");

    gates.arm("before_refuse_paused");
    host.call("updateBot", json!({"id": bot, "paused": true})).await;
    gates.entered("before_refuse_paused").await;
    host.call("updateBot", json!({"id": bot, "paused": false})).await;
    let started = host.call("runRoutine", json!({"botId": bot, "id": fresh})).await;
    let run_id = started["run"]["id"].as_str().unwrap().to_owned();
    gates.release("before_refuse_paused");

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut run;
    loop {
        run = host.call("routines", json!({"botId": bot})).await["runs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == run_id)
            .cloned()
            .unwrap_or(Value::Null);
        if run["status"] != "pending" || Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_ne!(
        run["status"], "cancelled",
        "a routine admitted after resume was cancelled by a stale pause notification: {run}"
    );
}

#[tokio::test]
async fn a_failed_turn_from_before_a_pause_is_not_retried_after_resume() {
    let gates = Gates::new();
    let host = gates.host().await;
    let command = common::fixture_command("relay_agent.py", "primary");
    let bot = host
        .call(
            "createBot",
            json!({"name": "Relay", "backend": "custom", "command": command, "connectors": [], "notify": false}),
        )
        .await["bot"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let fallbacks = json!([{"backend": "codex", "command": common::fixture_command("relay_agent.py", "standin")}]);
    host.call("updateBot", json!({"id": bot, "fallbacks": fallbacks})).await;
    std::fs::write(host.workspace(&bot).join("primary.limit"), "").unwrap();

    gates.arm("before_fallback");
    host.call("send", json!({"botId": bot, "text": "Fix the login tests", "clientNonce": "limit"})).await;
    gates.entered("before_fallback").await;
    host.call("updateBot", json!({"id": bot, "paused": true})).await;
    host.call("updateBot", json!({"id": bot, "paused": false})).await;
    gates.release("before_fallback");

    let deadline = Instant::now() + Duration::from_secs(20);
    let mut carried;
    loop {
        let log = host.log(&bot, "standin.jsonl");
        carried = log.into_iter().find_map(|row| {
            let text = row["params"]["prompt"][0]["text"].as_str().unwrap_or("");
            (row["method"] == "session/prompt" && text.contains("Fix the login tests"))
                .then(|| around(text, "usage limit"))
        });
        if carried.is_some() || Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(carried.is_none(), "a turn that failed before the pause was retried after resume: {carried:?}");
}

#[tokio::test]
async fn a_turn_cut_off_while_paused_is_not_resumed_if_resume_wins_startup() {
    let gates = Gates::new();
    let mut host = gates.host().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;
    host.reply(&bot, "hello").await;

    gates.arm("after_inflight_marked");
    host.call("send", json!({"botId": bot, "text": "CUT_OFF_TURN", "clientNonce": "cut-off"})).await;
    gates.entered("after_inflight_marked").await;
    host.call("updateBot", json!({"id": bot, "paused": true})).await;

    gates.arm("before_startup_pause_read");
    host.crash();
    gates.release("after_inflight_marked");
    host.restart(|_| {}).await;
    gates.entered("before_startup_pause_read").await;
    host.call("updateBot", json!({"id": bot, "paused": false})).await;
    gates.release("before_startup_pause_read");

    let leaked = resumed_within(&host, &bot, 15).await;
    assert!(leaked.is_none(), "a turn cut off while paused was resumed because resume won startup: {leaked:?}");
}

/// A workflow step queued behind a running turn is stopped when the bot is paused. It is not
/// started after the resume, and it is not recorded as a failure.
#[tokio::test]
async fn a_workflow_step_queued_before_a_pause_ends_cancelled() {
    let gates = Gates::new();
    let host = gates.host().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;

    host.call("send", json!({"botId": bot, "text": "HOLD:chat", "clientNonce": "hold-chat"})).await;
    wait_for("the held turn's prompt", async || prompt_blob(&host).contains("HOLD:chat")).await;

    let created =
        host.call("createWorkflow", json!({"title": "Pause step", "graph": {"nodes": [], "edges": []}})).await;
    let workflow = created["workflow"]["id"].as_str().unwrap();
    let graph = json!({
        "nodes": [{
            "id": "a",
            "kind": "agent",
            "at": {"x": 80, "y": 80},
            "fields": {"botId": bot, "task": "WORKFLOW_SHOULD_NOT_RUN"}
        }],
        "edges": []
    });
    host.call(
        "saveWorkflowGraph",
        json!({"id": workflow, "baseRevision": created["workflow"]["revision"], "graph": graph}),
    )
    .await;

    gates.arm("workflow_step_queued");
    let run = host.call("startWorkflowRun", json!({"id": workflow, "requestId": "pause-step-1"})).await;
    let run_id = run["run"]["id"].as_str().unwrap().to_owned();
    gates.entered("workflow_step_queued").await;
    host.call("updateBot", json!({"id": bot, "paused": true})).await;
    host.call("updateBot", json!({"id": bot, "paused": false})).await;
    gates.release("workflow_step_queued");

    let deadline = Instant::now() + Duration::from_secs(20);
    let detail = loop {
        let detail = host.call("workflowRun", json!({"runId": run_id})).await;
        let status = detail["run"]["steps"]
            .as_array()
            .and_then(|steps| steps.iter().find(|step| step["kind"] == "agent"))
            .and_then(|step| step["status"].as_str())
            .unwrap_or("");
        if status != "running" || Instant::now() >= deadline {
            break detail;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let step = detail["run"]["steps"]
        .as_array()
        .and_then(|steps| steps.iter().find(|step| step["kind"] == "agent"))
        .cloned()
        .unwrap_or(Value::Null);
    assert_eq!(
        step["status"], "stopped",
        "a workflow step queued before the pause failed at admission instead of being cancelled: {detail}"
    );
    assert!(step["error"].is_null(), "a cancelled workflow step carries an error: {step}");
    assert_eq!(detail["run"]["status"], "stopped", "{detail}");
    assert!(!prompt_blob(&host).contains("WORKFLOW_SHOULD_NOT_RUN"), "a cancelled workflow step was sent to the agent");
}

/// The actor was spawned paused and has not reached its startup branch. A resume and a new routine
/// land in that gap. Startup must not cancel the new run, and it must not resume the cut-off turn.
#[tokio::test]
async fn a_routine_admitted_during_paused_startup_is_not_cancelled() {
    let gates = Gates::new();
    let mut host = gates.host().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;
    host.works_in_chat(&bot).await;
    host.reply(&bot, "hello").await;
    let fresh = routine(&host, &bot, "Fresh", "PASS").await;

    gates.arm("after_inflight_marked");
    host.call("send", json!({"botId": bot, "text": "CUT_OFF_TURN", "clientNonce": "cut-off"})).await;
    gates.entered("after_inflight_marked").await;
    host.call("updateBot", json!({"id": bot, "paused": true})).await;

    gates.arm("before_startup_pause_read");
    host.crash();
    gates.release("after_inflight_marked");
    host.restart(|_| {}).await;
    gates.entered("before_startup_pause_read").await;
    host.call("updateBot", json!({"id": bot, "paused": false})).await;
    let started = host.call("runRoutine", json!({"botId": bot, "id": fresh})).await;
    let run_id = started["run"]["id"].as_str().unwrap().to_owned();
    gates.release("before_startup_pause_read");

    let deadline = Instant::now() + Duration::from_secs(15);
    let run = loop {
        let run = host.call("routines", json!({"botId": bot})).await["runs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == run_id)
            .cloned()
            .unwrap_or(Value::Null);
        if run["status"] != "pending" || Instant::now() >= deadline {
            break run;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_ne!(run["status"], "cancelled", "a routine admitted after resume was cancelled by paused startup: {run}");
    let leaked = resumed_within(&host, &bot, 5).await;
    assert!(leaked.is_none(), "paused startup resumed the cut-off turn because resume won the branch: {leaked:?}");
}

fn user_status(history: &Value, text: &str) -> Option<String> {
    history["entries"].as_array()?.iter().find_map(|entry| {
        (entry["kind"] == "user" && entry["data"]["text"] == text)
            .then(|| entry["data"]["status"].as_str().unwrap_or("").to_owned())
    })
}

/// A user message queued behind a running turn is delivered after resume.
/// A message sent while the bot is paused stays cancelled.
#[tokio::test]
async fn a_user_message_queued_before_a_pause_is_delivered_on_resume() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;

    host.call("send", json!({"botId": bot, "text": "HOLD:chat", "clientNonce": "hold-chat"})).await;
    wait_for("the held turn's prompt", async || prompt_blob(&host).contains("HOLD:chat")).await;

    host.call("send", json!({"botId": bot, "text": "QUEUED_BEFORE_PAUSE", "clientNonce": "queued-before"})).await;
    wait_for("the second message to queue behind the turn", async || {
        let history = host.call("history", json!({"botId": bot})).await;
        user_status(&history, "QUEUED_BEFORE_PAUSE").as_deref() == Some("queued")
    })
    .await;

    host.call("updateBot", json!({"id": bot, "paused": true})).await;
    std::fs::write(host.workspace(&bot).join("release-chat"), "").unwrap();
    wait_for("the paused bot to be idle", async || common::bot(&host.snapshot().await, &bot)["status"] == "idle").await;

    host.call("send", json!({"botId": bot, "text": "WHILE_PAUSED", "clientNonce": "while-paused"})).await;
    wait_for("a message sent while paused to be cancelled", async || {
        let history = host.call("history", json!({"botId": bot})).await;
        user_status(&history, "WHILE_PAUSED").as_deref() == Some("cancelled")
    })
    .await;

    host.call("updateBot", json!({"id": bot, "paused": false})).await;
    wait_for("the message queued before the pause to reach the agent", async || {
        prompt_blob(&host).contains("QUEUED_BEFORE_PAUSE")
    })
    .await;

    let history = host.call("history", json!({"botId": bot})).await;
    assert_ne!(
        user_status(&history, "QUEUED_BEFORE_PAUSE").as_deref(),
        Some("cancelled"),
        "a message queued before the pause was cancelled instead of delivered: {history}"
    );
    let prompts = prompt_blob(&host);
    assert!(
        !prompts.contains("WHILE_PAUSED"),
        "a message sent while paused was delivered after resume: {}",
        around(&prompts, "WHILE_PAUSED")
    );
}
