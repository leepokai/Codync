//! Failed routine runs, through a real host and a fixture agent: one that failed before changing
//! anything is tried again on its own; one that acted waits for the user's retry, with its event.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A webhook routine of a new fixture bot (started with `args`) doing `instruction` within `timeout` seconds: the
/// bot and the routine.
async fn routine(host: &Host, name: &str, args: &str, instruction: &str, timeout: u64) -> (String, String) {
    let bot = host.fixture_bot(name, "routine_agent.py", args).await;
    let body = json!({"botId": bot, "name": name, "instruction": instruction, "triggers": [{"type": "webhook"}],
        "timeoutSeconds": timeout});
    let id = host.call("saveRoutine", body).await["routine"]["id"].as_str().unwrap().to_owned();
    (bot, id)
}

async fn deliver(host: &Host, bot: &str, routine: &str, text: &str) {
    let hook = host.call("routineWebhook", json!({"botId": bot, "id": routine})).await;
    let response = reqwest::Client::new()
        .post(hook["localUrl"].as_str().unwrap())
        .bearer_auth(hook["key"].as_str().unwrap())
        .json(&json!({"text": text}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
}

/// Waits for a run of `routine` in `status`.
async fn run(host: &Host, bot: &str, routine: &str, status: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let listing = host.call("routines", json!({"botId": bot})).await;
        let runs = listing["runs"].as_array().unwrap();
        if let Some(run) = runs.iter().find(|r| r["routineId"] == routine && r["status"] == status) {
            return run.clone();
        }
        assert!(Instant::now() < deadline, "no {status} run: {listing}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The data of the notice that reports run `id`'s outcome.
async fn outcome(host: &Host, bot: &str, id: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let history = host.call("history", json!({"botId": bot})).await;
        let notice = format!("routine-result-{id}");
        if let Some(e) = history["entries"].as_array().unwrap().iter().find(|e| e["id"] == notice.as_str()) {
            return e["data"].clone();
        }
        assert!(Instant::now() < deadline, "no outcome notice for {id}: {history}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn now_ms() -> i64 {
    i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()).unwrap()
}

#[tokio::test]
async fn a_run_that_failed_before_acting_is_tried_again_on_its_own() {
    let host = Host::start().await;
    let (bot, id) = routine(&host, "quiet", "", "FAIL_ONCE[quiet]", 30).await;
    let started = now_ms();
    deliver(&host, &bot, &id, "payload").await;
    let failed = run(&host, &bot, &id, "failed").await;
    let waiting = run(&host, &bot, &id, "pending").await;
    assert_eq!(waiting["attempt"], 1);
    assert!(waiting["notBefore"].as_i64().unwrap() >= started + 60_000, "{waiting}");
    assert_eq!(waiting["event"]["text"], "payload", "the retry gets the same delivery");
    let notice = outcome(&host, &bot, failed["id"].as_str().unwrap()).await;
    assert!(notice["text"].as_str().unwrap().ends_with("Trying again in 1 minute."), "{notice}");
    assert_eq!((&notice["style"], &notice["retry"]), (&json!("info"), &json!(false)));
    // Running the routine now doesn't add a run: the waiting retry goes now.
    let now = host.call("runRoutine", json!({"botId": bot, "id": id})).await;
    assert_eq!(now["run"]["id"], waiting["id"]);
    assert_eq!(run(&host, &bot, &id, "succeeded").await["id"], waiting["id"]);
}

#[tokio::test]
async fn a_run_that_acted_waits_for_the_user_to_retry_it() {
    let host = Host::start().await;
    let (bot, id) = routine(&host, "acted", "", "ACT_FIRST FAIL_ONCE[acted]", 30).await;
    deliver(&host, &bot, &id, "payload").await;
    let failed = run(&host, &bot, &id, "failed").await;
    let failed_id = failed["id"].as_str().unwrap();
    assert!(failed["retryable"] == true && failed["retryAt"].is_null(), "{failed}");
    let notice = outcome(&host, &bot, failed_id).await;
    assert_eq!((&notice["style"], &notice["retry"]), (&json!("error"), &json!(true)));
    let listing = host.call("routines", json!({"botId": bot})).await;
    assert!(!listing["runs"].as_array().unwrap().iter().any(|r| r["status"] == "pending"), "no automatic retry");

    let retried = host.call("retryRoutineRun", json!({"botId": bot, "runId": failed_id})).await;
    assert_eq!(retried["run"]["event"]["text"], "payload", "the retry gets the same delivery");
    assert_eq!(run(&host, &bot, &id, "succeeded").await["id"], retried["run"]["id"]);
    assert_eq!(outcome(&host, &bot, failed_id).await["retry"], false, "the notice no longer offers a retry");
    let again = host.refused("retryRoutineRun", json!({"botId": bot, "runId": failed_id})).await;
    assert!(again.contains("can't be retried"), "{again}");
}

#[tokio::test]
async fn a_run_that_hit_its_time_limit_waits_for_the_user() {
    let host = Host::start().await;
    let (bot, id) = routine(&host, "hung", "", "BLOCK[forever] IGNORE_CANCEL", 2).await;
    deliver(&host, &bot, &id, "payload").await;
    let failed = run(&host, &bot, &id, "failed").await;
    assert!(failed["detail"].as_str().unwrap().contains("time limit"), "{failed}");
    assert!(failed["retryable"] == true && failed["retryAt"].is_null(), "it could hold the bot that long again");
    let listing = host.call("routines", json!({"botId": bot})).await;
    assert!(!listing["runs"].as_array().unwrap().iter().any(|r| r["status"] == "pending"), "no automatic retry");
}

#[tokio::test]
async fn a_run_whose_agent_did_not_start_within_its_time_limit_waits_for_the_user() {
    let host = Host::start().await;
    let (bot, id) = routine(&host, "stuck", "--hang-new", "never starts", 1).await;
    deliver(&host, &bot, &id, "payload").await;
    let failed = run(&host, &bot, &id, "failed").await;
    assert!(failed["detail"].as_str().unwrap().contains("setup timed out"), "{failed}");
    assert!(failed["retryable"] == true && failed["retryAt"].is_null(), "it could hold the bot that long again");
    let listing = host.call("routines", json!({"botId": bot})).await;
    assert!(!listing["runs"].as_array().unwrap().iter().any(|r| r["status"] == "pending"), "no automatic retry");
}

/// How many events of `kind` `routine_agent.py` logged.
fn agent_events(host: &Host, bot: &str, kind: &str) -> usize {
    host.log(bot, "events.jsonl").iter().filter(|e| e["event"] == kind).count()
}

/// A time limit that cuts `session/new` short must not leave the agent to open that session (tools and all) under
/// an id the host never learns: an agent that can resume stops, and the next run starts it again.
#[tokio::test]
async fn a_session_cut_short_by_the_time_limit_is_not_left_open() {
    let host = Host::start().await;
    let (bot, id) = routine(&host, "slow", "--slow-new", "slow start", 2).await;
    deliver(&host, &bot, &id, "payload").await;
    let failed = run(&host, &bot, &id, "failed").await;
    assert!(failed["detail"].as_str().unwrap().contains("setup timed out"), "{failed}");
    // Past the moment the agent would have opened it.
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert_eq!(agent_events(&host, &bot, "late"), 0, "the agent opened a session the host never learned of");
    let retried = host.call("retryRoutineRun", json!({"botId": bot, "runId": failed["id"]})).await;
    assert_eq!(run(&host, &bot, &id, "succeeded").await["id"], retried["run"]["id"]);
}

/// An agent that can't resume keeps running, and keeps the sessions it has, rather than stop over a session cut
/// short; the next run still works.
#[tokio::test]
async fn an_agent_that_cannot_resume_keeps_running_after_a_session_cut_short() {
    let host = Host::start().await;
    let (bot, id) = routine(&host, "slow-kept", "--slow-new --no-load", "slow start", 2).await;
    deliver(&host, &bot, &id, "payload").await;
    let failed = run(&host, &bot, &id, "failed").await;
    assert!(failed["detail"].as_str().unwrap().contains("setup timed out"), "{failed}");
    let retried = host.call("retryRoutineRun", json!({"botId": bot, "runId": failed["id"]})).await;
    assert_eq!(run(&host, &bot, &id, "succeeded").await["id"], retried["run"]["id"]);
    assert_eq!(agent_events(&host, &bot, "initialize"), 1, "the agent was restarted");
}
