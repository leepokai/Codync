//! A routine running after another, through a real host and fixture agents: the finished run's outcome
//! queues the routine after it, on another bot, with its result as trigger data; saving refuses a loop.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

async fn save(host: &Host, bot: &str, name: &str, instruction: &str, trigger: Value) -> Value {
    let body = json!({"botId": bot, "name": name, "instruction": instruction, "triggers": [trigger]});
    host.call("saveRoutine", body).await
}

fn after(upstream: &str, filters: Value) -> Value {
    let mut filters = filters;
    filters["routineId"] = json!(upstream);
    json!({"type": "event", "source": "routine", "event": "succeeded", "filters": filters})
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

#[tokio::test]
async fn a_finished_run_starts_the_routine_after_it_on_another_bot() {
    let host = Host::start().await;
    let checker = host.fixture_bot("checker", "routine_agent.py", "").await;
    let fixer = host.fixture_bot("fixer", "routine_agent.py", "").await;
    let check = save(&host, &checker, "Server check", "Check the server", json!({"type": "webhook"})).await;
    let check = check["routine"]["id"].as_str().unwrap().to_owned();
    let repair =
        save(&host, &fixer, "Repair", "Repair what the check found", after(&check, json!({"passed": "false"}))).await;
    assert_eq!(repair["routine"]["triggerDescriptions"][0], "After \"Server check\" succeeds with something to report");
    let repair = repair["routine"]["id"].as_str().unwrap().to_owned();
    let hooks = host.call("routineWebhook", json!({"botId": fixer, "id": repair})).await;
    assert!(hooks["key"].is_string(), "the local endpoint stays, though the cloud registers no hook for it");
    let forged = json!({"source": "routine", "event": "succeeded", "routineId": check, "passed": "false"});
    let refused = reqwest::Client::new()
        .post(hooks["localUrl"].as_str().unwrap())
        .bearer_auth(hooks["key"].as_str().unwrap())
        .json(&forged)
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), reqwest::StatusCode::BAD_REQUEST, "only the host reports a run finishing");

    host.call("runRoutine", json!({"botId": checker, "id": check})).await;
    let first = run(&host, &checker, &check, "succeeded").await;
    run(&host, &fixer, &repair, "succeeded").await;
    // A finished run drops its event, so read the trigger data the repair run was given.
    let started = host.log(&fixer, "events.jsonl");
    let prompt = started
        .iter()
        .filter_map(|e| e["text"].as_str())
        .find(|text| text.contains("Scheduled routine: Repair"))
        .expect("the repair run's prompt");
    for field in [
        json!({"source": "routine"}),
        json!({"event": "succeeded"}),
        json!({"routineId": check}),
        json!({"runId": first["id"]}),
        json!({"passed": "false"}),
        json!({"depth": 1}),
        json!({"text": "Routine completed successfully."}),
    ] {
        let pair = field.to_string();
        assert!(prompt.contains(&pair[1..pair.len() - 1]), "{pair} missing from {prompt}");
    }
}

#[tokio::test]
async fn saving_refuses_an_unknown_upstream_and_a_loop() {
    let host = Host::start().await;
    let bot = host.fixture_bot("chain", "routine_agent.py", "").await;
    let missing = host
        .refused(
            "saveRoutine",
            json!({"botId": bot, "name": "Orphan", "instruction": "x", "triggers": [after("nope", json!({}))]}),
        )
        .await;
    assert!(missing.contains("not found"), "{missing}");
    let a = save(&host, &bot, "A", "a", json!({"type": "webhook"})).await["routine"]["id"].as_str().unwrap().to_owned();
    let b = save(&host, &bot, "B", "b", after(&a, json!({}))).await["routine"]["id"].as_str().unwrap().to_owned();
    let looped = host
        .refused(
            "saveRoutine",
            json!({"botId": bot, "id": a, "name": "A", "instruction": "a", "triggers": [after(&b, json!({}))]}),
        )
        .await;
    assert!(looped.contains("loop"), "{looped}");
}
