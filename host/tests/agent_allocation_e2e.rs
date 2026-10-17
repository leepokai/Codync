//! Interrupted allocations are recycled before another session is allocated.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[allow(dead_code)]
mod common;
#[path = "common/lifecycle.rs"]
mod lifecycle;

use lifecycle::{Host, events, sid};
use serde_json::json;
use std::time::Duration;

#[tokio::test]
async fn routine_deadline_during_allocation_releases_uncertain_tools_before_retry() {
    let host = Host::start().await;
    let (bot, cwd) = host.bot("AllocationDeadline", json!({})).await;
    // Keep the default no-close capability. A separate main lane pays startup
    // before the routine's deadline, while its thread still allocates afresh.
    let main = host.send(&bot, "prewarm the independent main lane", None).await;
    let generation = events(&cwd)[0]["generation"].clone();
    std::fs::write(cwd.join("hang-new"), "").unwrap();
    let saved = host
        .call(
            "saveRoutine",
            json!({"botId":bot,"name":"Setup timeout",
        "instruction":"ok","triggers":[{"type":"webhook"}],"timeoutSeconds":2}),
        )
        .await;
    let routine = saved["routine"]["id"].as_str().unwrap();
    host.call("runRoutine", json!({"botId":bot,"id":routine})).await;
    let allocation =
        host.wait_event(&cwd, |event| event["event"] == "allocated" && event["session"] != sid(&main)).await;
    assert_eq!(allocation["generation"], generation);
    let completing = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let listing = host.call("routines", json!({"botId":bot})).await;
            if let Some(run) = listing["runs"]
                .as_array()
                .unwrap()
                .iter()
                .find(|run| run["routineId"] == routine && run["status"] == "failed")
            {
                break run.clone();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    let failed = completing.unwrap();
    // The main's pair and the unconfirmed routine allocation's pair remain
    // until retrying the fresh thread forces safe process recycling.
    host.wait_live(4, Duration::from_secs(15)).await;
    std::fs::remove_file(cwd.join("hang-new")).unwrap();
    let root = failed["rootId"].as_str().unwrap();
    let replacement = host.send(&bot, "ok", Some(root)).await;
    assert_ne!(sid(&replacement), allocation["session"].as_str().unwrap());
    let observed = events(&cwd);
    let prompted = observed.iter().rev().find(|event| event["event"] == "prompted").unwrap();
    assert_ne!(prompted["generation"], generation);
    assert_eq!(prompted["session"], sid(&replacement));
    host.wait_live_session(2, prompted["generation"].as_str().unwrap(), sid(&replacement), Duration::from_secs(15))
        .await;
}

#[tokio::test]
async fn interrupted_nonresumable_allocation_keeps_confirmed_context_until_explicit_reset() {
    let host = Host::start().await;
    let (bot, cwd) = host.bot("NoResumeDeadline", json!({"load":false})).await;
    let main = host.send(&bot, "main", None).await;
    std::fs::write(cwd.join("hang-new"), "").unwrap();
    let saved = host
        .call(
            "saveRoutine",
            json!({"botId":bot,"name":"Cancelled allocation",
        "instruction":"ok","triggers":[{"type":"webhook"}],"timeoutSeconds":2}),
        )
        .await;
    let routine = saved["routine"]["id"].as_str().unwrap();
    host.call("runRoutine", json!({"botId":bot,"id":routine})).await;
    host.wait_event(&cwd, |event| event["event"] == "allocated" && event["session"] != sid(&main)).await;
    let completion = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let listing = host.call("routines", json!({"botId":bot})).await;
            if listing["runs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|run| run["routineId"] == routine && run["status"] == "failed")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    completion.unwrap();
    let continued = host.send(&bot, "main continues", None).await;
    assert_eq!(sid(&continued), sid(&main));
    std::fs::remove_file(cwd.join("hang-new")).unwrap();
    let failed = host.send_entry(&bot, "new thread", continued["id"].as_str()).await;
    host.wait_entry(&bot, continued["id"].as_str(), |entry| {
        entry["turn"] == failed["turn"] && entry["kind"] == "notice" && entry["data"]["style"] == "error"
    })
    .await;
    assert_eq!(events(&cwd).iter().filter(|event| event["event"] == "allocated").count(), 2);
    host.call("newSession", json!({"botId":bot})).await;
    let replacement = host.send(&bot, "new main", None).await;
    assert_ne!(sid(&replacement), sid(&main));
    host.wait_live(2, Duration::from_secs(15)).await;
}
