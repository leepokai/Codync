//! Local routine lifecycle tests with real host processes and persistent fake ACP
//! sessions. No paid provider is involved.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[allow(dead_code)]
mod common;

#[path = "routines_e2e/timeout.rs"]
mod timeout;

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
        let home = std::env::temp_dir().join(format!("codync-routines-e2e-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let (child, base) = common::serve(&home).await;
        let mut host = Self { child, home, base, token: String::new() };
        std::fs::read_to_string(host.home.join("token")).unwrap().trim().clone_into(&mut host.token);
        host
    }

    async fn restart(&mut self) {
        common::stop(&mut self.child);
        (self.child, self.base) = common::serve(&self.home).await;
    }

    async fn call(&self, method: &str, body: Value) -> Value {
        let response = reqwest::Client::new()
            .post(format!("{}/api/{method}", self.base))
            .bearer_auth(&self.token)
            .json(&body)
            // As long as `common::host`'s: on a busy machine (parallel tests) `createBot` alone can take over 10 s.
            .timeout(Duration::from_secs(120))
            .send()
            .await
            .unwrap();
        let status = response.status();
        let value: Value = response.json().await.unwrap();
        assert!(status.is_success(), "{method}: {value}");
        value
    }

    async fn bot(&self, name: &str) -> String {
        self.bot_fixture(name, "team_agent.py").await
    }

    async fn stable_bot(&self, name: &str) -> String {
        self.bot_fixture(name, "routine_agent.py").await
    }

    async fn bot_fixture(&self, name: &str, fixture: &str) -> String {
        let cwd = self.home.join(name);
        std::fs::create_dir_all(&cwd).unwrap();
        let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(fixture);
        self.call(
            "createBot",
            json!({
                "name": name, "backend": "custom", "cwd": cwd, "permission": "ask", "notify": false,
                "command": common::python_agent(&agent),
            }),
        )
        .await["bot"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }
    async fn save(&self, bot: &str, name: &str, instruction: &str, triggers: Value, timeout: u64) -> String {
        self.call(
            "saveRoutine",
            json!({"botId":bot,"name":name,"instruction":instruction,"triggers":triggers,"timeoutSeconds":timeout}),
        )
        .await["routine"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    async fn wait_status(&self, bot: &str, routine: &str, status: &str) -> Value {
        tokio::time::timeout(Duration::from_secs(25), async {
            loop {
                let listing = self.call("routines", json!({"botId":bot})).await;
                if let Some(run) = listing["runs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|r| r["routineId"] == routine && r["status"] == status)
                {
                    return run.clone();
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("routine {routine} did not reach {status}"))
    }

    fn agent_events(&self, name: &str) -> Vec<Value> {
        std::fs::read_to_string(self.home.join(name).join("events.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    async fn wait_event(&self, name: &str, event: &str, contains: &str) -> Value {
        tokio::time::timeout(Duration::from_secs(25), async {
            loop {
                if let Some(e) = self
                    .agent_events(name)
                    .into_iter()
                    .find(|e| e["event"] == event && e["text"].as_str().unwrap().contains(contains))
                {
                    return e;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("missing agent event {event} containing {contains}"))
    }

    async fn send(&self, bot: &str, text: &str) {
        self.call("send", json!({"botId":bot,"text":text,"clientNonce":uuid::Uuid::new_v4().to_string()})).await;
    }
}

#[tokio::test]
async fn routines_execute_pause_deduplicate_and_reject_wrong_webhook_keys() {
    let mut host = Host::start().await;
    let bot = host.bot("routine-bot").await;
    let result = host.call("saveRoutine", json!({"botId":bot,"name":"Check status","instruction":"Report a short status","triggers":[{"type":"webhook"}]})).await;
    let id = result["routine"]["id"].as_str().unwrap();
    assert!(result["routine"].get("webhookKey").is_none());
    let credentials = host.call("routineWebhook", json!({"botId":bot,"id":id})).await;
    assert!(credentials["url"].is_null(), "no public URL while the cloud is off");
    let url = credentials["localUrl"].as_str().unwrap();
    let client = reqwest::Client::new();
    assert!(!client.post(url).bearer_auth("wrong").json(&json!({})).send().await.unwrap().status().is_success());
    let first: Value = client
        .post(url)
        .bearer_auth(credentials["key"].as_str().unwrap())
        .header("x-delivery-id", "delivery-1")
        .json(&json!({"text":"hello"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let second: Value = client
        .post(url)
        .bearer_auth(credentials["key"].as_str().unwrap())
        .header("x-delivery-id", "delivery-1")
        .json(&json!({"text":"hello"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(first["run"]["id"], second["run"]["id"]);
    let db = rusqlite::Connection::open(host.home.join("codync.db")).unwrap();
    let run_sessions = || -> i64 {
        db.query_row("SELECT COUNT(*) FROM kv WHERE k GLOB 'lane.session.*/routine-*'", [], |r| r.get(0)).unwrap()
    };
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let listing = host.call("routines", json!({"botId":bot})).await;
            // A finished run's session is forgotten.
            if listing["runs"][0]["status"] == "succeeded" && run_sessions() == 0 {
                break;
            }
            assert_ne!(listing["runs"][0]["status"], "failed", "{listing}");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    let servers: Value =
        serde_json::from_str(&std::fs::read_to_string(host.home.join("routine-bot/servers.json")).unwrap()).unwrap();
    assert!(servers.as_array().unwrap().iter().any(|s| s["name"] == "routines" || s["name"] == "codync-routines"));
    host.call("setRoutineEnabled", json!({"botId":bot,"id":id,"enabled":false})).await;
    assert!(
        !client
            .post(url)
            .bearer_auth(credentials["key"].as_str().unwrap())
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status()
            .is_success()
    );
    host.restart().await;
    let restored = host.call("routines", json!({"botId":bot})).await;
    assert_eq!(restored["routines"][0]["enabled"], false);
    assert_eq!(restored["runs"][0]["status"], "succeeded");
    host.call("deleteRoutine", json!({"botId":bot,"id":id})).await;
    assert_eq!(host.call("routines", json!({"botId":bot})).await["routines"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn interval_fires_without_a_client_and_invalid_schedules_fail() {
    let host = Host::start().await;
    let bot = host.bot("scheduled-bot").await;
    let response = reqwest::Client::new()
        .post(format!("{}/api/saveRoutine", host.base))
        .bearer_auth(&host.token)
        .json(&json!({"botId":bot,"name":"Bad","instruction":"test","triggers":[{"type":"interval","seconds":0}]}))
        .send()
        .await
        .unwrap();
    assert!(!response.status().is_success());
    let saved = host
        .call(
            "saveRoutine",
            json!({"botId":bot,"name":"Scheduled","instruction":"EMPTY","triggers":[{"type":"interval","seconds":1}]}),
        )
        .await;
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let listing = host.call("routines", json!({"botId":bot})).await;
            if listing["runs"].as_array().unwrap().iter().any(|r| r["status"] == "succeeded") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    host.call("setRoutineEnabled", json!({"botId":bot,"id":saved["routine"]["id"],"enabled":false})).await;
    let history = host.call("history", json!({"botId":bot})).await;
    assert!(!history["entries"].as_array().unwrap().iter().any(|e| e["kind"] == "agent" && e["data"]["final"] == true));
}

#[tokio::test]
async fn crash_resumes_the_same_session_and_publishes_one_result() {
    let mut host = Host::start().await;
    let bot = host.stable_bot("resume").await;
    let id = host.save(&bot, "resume-routine", "BLOCK[resume]", json!([{"type":"webhook"}]), 30).await;
    host.call("runRoutine", json!({"botId":bot,"id":id})).await;
    let started = host.wait_event("resume", "started", "BLOCK[resume]").await;
    host.wait_status(&bot, &id, "running").await;
    host.restart().await;
    let resumed = host.wait_event("resume", "resumed", "BLOCK[resume]").await;
    assert_eq!(started["session"], resumed["session"]);
    std::fs::write(host.home.join("resume/release-resume"), "").unwrap();
    host.wait_status(&bot, &id, "succeeded").await;
    // The run's reply is its result, in the chat once.
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let history = host.call("history", json!({"botId":bot})).await;
            let results: Vec<_> = history["entries"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["kind"] == "agent" && e["data"]["final"] == true)
                .collect();
            if !results.is_empty() {
                assert_eq!(results.len(), 1);
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    host.restart().await;
    host.send(&bot, "BARRIER").await;
    host.wait_event("resume", "finished", "BARRIER").await;
    assert_eq!(
        host.agent_events("resume")
            .iter()
            .filter(|e| e["event"] == "started" && e["text"].as_str().unwrap().contains("BLOCK[resume]"))
            .count(),
        1
    );
}

#[tokio::test]
async fn a_message_sent_during_a_run_gets_a_turn_of_its_own() {
    let host = Host::start().await;
    let bot = host.stable_bot("turns").await;
    let id = host.save(&bot, "slow", "BLOCK[run]", json!([{"type":"webhook"}]), 30).await;
    host.call("runRoutine", json!({"botId":bot,"id":id})).await;
    // Nothing of the run is in the chat yet but its notice.
    host.wait_event("turns", "started", "BLOCK[run]").await;
    host.send(&bot, "MEANWHILE").await;
    std::fs::write(host.home.join("turns/release-run"), "").unwrap();
    let entries = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let history = host.call("history", json!({"botId":bot})).await;
            let entries = history["entries"].as_array().unwrap().clone();
            if entries.iter().filter(|e| e["kind"] == "agent" && e["data"]["final"] == true).count() == 2 {
                return entries;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let turn =
        |f: &dyn Fn(&Value) -> bool| entries.iter().filter(|e| f(e)).map(|e| e["turn"].clone()).collect::<Vec<_>>();
    let notice = turn(&|e| e["kind"] == "notice" && e["data"]["runId"].is_string());
    let user = turn(&|e| e["kind"] == "user");
    assert_ne!(notice, user);
    let finals = turn(&|e| e["kind"] == "agent" && e["data"]["final"] == true);
    assert_eq!(finals, [notice[0].clone(), user[0].clone()], "each reply stays in its own turn");
}

#[tokio::test]
async fn an_agent_started_for_a_run_stops_when_the_run_ends() {
    let host = Host::start().await;
    let bot = host.stable_bot("only-runs").await;
    let id = host.save(&bot, "one-off", "ONE_OFF", json!([{"type":"webhook"}]), 30).await;
    host.call("runRoutine", json!({"botId":bot,"id":id})).await;
    // The run's outcome, not the agent's own "finished": a message sent before the host took the reply would wait in
    // the queue and keep the agent running.
    host.wait_status(&bot, &id, "succeeded").await;
    host.send(&bot, "LATER").await;
    host.wait_event("only-runs", "finished", "LATER").await;
    let events = host.agent_events("only-runs");
    let count = |event: &str| events.iter().filter(|e| e["event"] == event).count();
    // The message started the agent again: the run's agent stopped with the run. Windows never stops an agent (it
    // can't tell a dev server from an MCP server, see `lifecycle`), so there the run's session closes instead.
    assert_eq!((count("initialize"), count("closed")), if cfg!(windows) { (1, 1) } else { (2, 0) });
}

#[tokio::test]
async fn a_finished_run_closes_its_session_and_the_main_one_stays() {
    let host = Host::start().await;
    let bot = host.stable_bot("spent").await;
    host.send(&bot, "MAIN_CHAT").await;
    let main = host.wait_event("spent", "finished", "MAIN_CHAT").await;
    let id = host.save(&bot, "one-off", "ONE_OFF", json!([{"type":"webhook"}]), 30).await;
    host.call("runRoutine", json!({"botId":bot,"id":id})).await;
    let run = host.wait_event("spent", "finished", "ONE_OFF").await;
    let closed = host.wait_event("spent", "closed", "").await;
    assert_eq!(closed["session"], run["session"]);
    assert_ne!(run["session"], main["session"]);
    host.send(&bot, "MAIN_AGAIN").await;
    assert_eq!(host.wait_event("spent", "finished", "MAIN_AGAIN").await["session"], main["session"]);
    assert_eq!(host.agent_events("spent").iter().filter(|e| e["event"] == "closed").count(), 1);
}

#[tokio::test]
async fn busy_bot_keeps_work_pending_and_honors_pause_delete_and_edit_across_restart() {
    let mut host = Host::start().await;
    let bot = host.stable_bot("queue").await;
    host.send(&bot, "BLOCK[user]").await;
    host.wait_event("queue", "started", "BLOCK[user]").await;
    let paused = host.save(&bot, "paused", "SHOULD_NOT_RUN_PAUSED", json!([{"type":"webhook"}]), 30).await;
    let deleted = host.save(&bot, "deleted", "SHOULD_NOT_RUN_DELETED", json!([{"type":"webhook"}]), 30).await;
    let kept = host.save(&bot, "kept", "OLD_INSTRUCTION", json!([{"type":"webhook"}]), 30).await;
    for id in [&paused, &deleted, &kept] {
        host.call("runRoutine", json!({"botId":bot,"id":id})).await;
    }
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let listing = host.call("routines", json!({"botId":bot})).await;
    assert!(listing["runs"].as_array().unwrap().iter().all(|r| r["status"] == "pending"));
    host.call("setRoutineEnabled", json!({"botId":bot,"id":paused,"enabled":false})).await;
    host.call("deleteRoutine", json!({"botId":bot,"id":deleted})).await;
    host.call(
        "saveRoutine",
        json!({"botId":bot,"id":kept,"name":"kept","instruction":"NEW_INSTRUCTION","triggers":[{"type":"webhook"}]}),
    )
    .await;
    host.restart().await;
    host.wait_event("queue", "resumed", "BLOCK[user]").await;
    std::fs::write(host.home.join("queue/release-user"), "").unwrap();
    host.wait_status(&bot, &kept, "succeeded").await;
    host.wait_event("queue", "finished", "NEW_INSTRUCTION").await;
    assert!(!host.agent_events("queue").iter().any(|e| {
        let text = e["text"].as_str().unwrap();
        text.contains("SHOULD_NOT_RUN") || text.contains("OLD_INSTRUCTION")
    }));
}

#[tokio::test]
async fn unavailable_resume_never_replays_work_in_a_fresh_session() {
    let mut host = Host::start().await;
    let bot = host.stable_bot("no-resume").await;
    let id = host.save(&bot, "unloadable", "BLOCK[original]", json!([{"type":"webhook"}]), 30).await;
    host.call("runRoutine", json!({"botId":bot,"id":id})).await;
    host.wait_event("no-resume", "started", "BLOCK[original]").await;
    std::fs::write(host.home.join("no-resume/no-load"), "").unwrap();
    host.restart().await;
    host.wait_status(&bot, &id, "interrupted").await;
    let next = host.save(&bot, "next", "AFTER_INTERRUPTION", json!([{"type":"webhook"}]), 30).await;
    host.call("runRoutine", json!({"botId":bot,"id":next})).await;
    host.wait_status(&bot, &next, "succeeded").await;
    assert_eq!(
        host.agent_events("no-resume")
            .iter()
            .filter(|e| e["text"].as_str().unwrap().contains("BLOCK[original]"))
            .count(),
        1
    );
}

#[tokio::test]
async fn recurring_work_continues_after_a_failed_run() {
    let host = Host::start().await;
    let bot = host.stable_bot("failure").await;
    let id = host
        .save(&bot, "retry-next-deadline", "FAIL_ONCE[transient]", json!([{"type":"interval","seconds":2}]), 30)
        .await;
    host.wait_status(&bot, &id, "failed").await;
    host.wait_status(&bot, &id, "succeeded").await;
    host.call("setRoutineEnabled", json!({"botId":bot,"id":id,"enabled":false})).await;
}

#[tokio::test]
async fn only_one_host_can_own_a_data_directory_even_on_another_port() {
    let host = Host::start().await;
    let mut other = Command::new(env!("CARGO_BIN_EXE_codync-host"))
        .args(["serve", "--bind", "127.0.0.1", "--port", "0"])
        .env("CODYNC_CLOUD", "off")
        .env("CODYNC_HOME", &host.home)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let status = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(status) = other.try_wait().unwrap() {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    if status.is_err() {
        let _ = other.kill();
        let _ = other.wait();
    }
    assert!(!status.expect("second host did not exit").success());
}

#[tokio::test]
async fn agent_discovers_and_calls_routine_tools_over_real_mcp_stdio() {
    let host = Host::start().await;
    let bot = host.stable_bot("mcp-setup").await;
    host.send(&bot, "VERIFY_ROUTINE_TOOLS").await;
    host.wait_event("mcp-setup", "tools_verified", "Routine MCP lifecycle").await;
    let listing = host.call("routines", json!({"botId":bot})).await;
    let routine = &listing["routines"][0];
    assert_eq!(routine["name"], "Edited through MCP");
    assert_eq!(routine["enabled"], false);
    host.wait_status(&bot, routine["id"].as_str().unwrap(), "succeeded").await;
    host.wait_event("mcp-setup", "finished", "MCP_TASK_RESULT").await;
}

#[tokio::test]
async fn schedule_editor_preview_and_save_share_host_rules() {
    let host = Host::start().await;
    let bot = host.stable_bot("schedule-editor").await;
    let preview = host
        .call(
            "routineSchedule",
            json!({"draft": {
                "calendarStyle":"days", "hour":23, "minute":47,
                "selectedDays":[5,1,3], "zone":"Asia/Taipei"
            }}),
        )
        .await;
    assert_eq!(preview["triggers"][0]["expression"], "47 23 * * 1,3,5");
    let saved = host
        .call(
            "saveRoutine",
            json!({
                "botId":bot,"name":"Calendar editor","instruction":"Do not execute this paused test.",
                "enabled":false,"schedule":preview["draft"],"timeoutSeconds":"60"
            }),
        )
        .await;
    assert_eq!(saved["routine"]["triggers"], preview["triggers"]);
    assert_eq!(saved["routine"]["timeoutSeconds"], 60);
    let loaded = host.call("routineSchedule", json!({"triggers":saved["routine"]["triggers"]})).await;
    assert_eq!(loaded["draft"]["calendarStyle"], "days");
    assert_eq!(loaded["draft"]["zone"], "Asia/Taipei");
    // Saving directly must validate even when a caller bypasses the preview UI.
    for schedule in [
        json!({"calendarStyle":"days","selectedDays":[]}),
        json!({"calendarStyle":"custom","expression":"61 9 * * *"}),
        json!({"zone":"invalid/zone"}),
    ] {
        let response = reqwest::Client::new()
            .post(format!("{}/api/saveRoutine", host.base))
            .bearer_auth(&host.token)
            .json(&json!({"botId":bot,"name":"Invalid","instruction":"Never run", "schedule":schedule}))
            .send()
            .await
            .unwrap();
        assert!(!response.status().is_success());
    }
    assert_eq!(host.call("routines", json!({"botId":bot})).await["runs"].as_array().unwrap().len(), 0);
}
