//! Failed session preparation must retain context and first-prompt instructions.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
#[path = "common/lifecycle.rs"]
mod lifecycle;

use lifecycle::{Host, assert_bounded, events, sid};
use serde_json::json;

fn saved_session(host: &Host, bot: &str, root: Option<&str>) -> String {
    let store =
        rusqlite::Connection::open_with_flags(host.home.join("codync.db"), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    if let Some(root) = root {
        let key = format!("lane.session.{bot}@{bot}/{root}");
        store.query_row("SELECT v FROM kv WHERE k=?1", [&key], |row| row.get(0)).unwrap()
    } else {
        store.query_row("SELECT session_id FROM bots WHERE id=?1", [bot], |row| row.get(0)).unwrap()
    }
}

#[tokio::test]
async fn retry_after_model_failure_keeps_the_new_session_and_delivers_its_profile_once() {
    let host = Host::start().await;
    for resumable in [true, false] {
        let (bot, cwd) = host.bot(&format!("ModelRetry{resumable}"), json!({"load":resumable})).await;
        host.call("updateBot", json!({"id":bot,"model":"fixture-model"})).await;
        std::fs::write(cwd.join("fail-model"), "").unwrap();
        let failed = host.send_entry(&bot, "ok", None).await;
        host.wait_entry(&bot, None, |entry| {
            entry["turn"] == failed["turn"] && entry["kind"] == "notice" && entry["data"]["style"] == "error"
        })
        .await;
        let saved = saved_session(&host, &bot, None);
        assert_ne!(saved, "");
        std::fs::remove_file(cwd.join("fail-model")).unwrap();
        let retried = host.send(&bot, "ok", None).await;
        assert_eq!(sid(&retried), saved);
        host.send(&bot, "ok again", None).await;
        let prompts: Vec<_> = events(&cwd).into_iter().filter(|event| event["event"] == "prompted").collect();
        assert_eq!(prompts.len(), 2);
        assert_eq!(prompts[0]["profile"], true, "initial instructions must survive setup failure");
        assert_eq!(prompts[1]["profile"], false, "completed instructions must not replay");
        assert_bounded(&cwd);
    }
}

#[tokio::test]
async fn retry_after_fork_model_failure_keeps_the_thread_and_delivers_its_intro_once() {
    let host = Host::start().await;
    let (bot, cwd) = host.bot("ForkModelRetry", json!({"fork":true,"close":true})).await;
    host.call("updateBot", json!({"id":bot,"model":"fixture-model"})).await;
    let main = host.send(&bot, "ok", None).await;
    let root = main["id"].as_str().unwrap();
    std::fs::write(cwd.join("fail-model"), "").unwrap();
    let failed = host.send_entry(&bot, "ok", Some(root)).await;
    host.wait_entry(&bot, Some(root), |entry| {
        entry["turn"] == failed["turn"] && entry["kind"] == "notice" && entry["data"]["style"] == "error"
    })
    .await;
    let saved = saved_session(&host, &bot, Some(root));
    assert_ne!(saved, sid(&main));
    std::fs::remove_file(cwd.join("fail-model")).unwrap();
    let retried = host.send(&bot, "ok", Some(root)).await;
    assert_eq!(sid(&retried), saved);
    host.send(&bot, "ok again", Some(root)).await;
    let prompts: Vec<_> =
        events(&cwd).into_iter().filter(|event| event["event"] == "prompted" && event["session"] == saved).collect();
    assert_eq!(prompts.len(), 2);
    assert_eq!(prompts[0]["forkIntro"], true, "thread instructions must survive setup failure");
    assert_eq!(prompts[1]["threadIntro"], false);
    assert_bounded(&cwd);
}

#[tokio::test]
async fn pending_thread_instructions_survive_other_lanes_and_host_restart() {
    let mut host = Host::start().await;
    let (bot, cwd) = host.bot("ThreadRestart", json!({"close":true})).await;
    host.call("updateBot", json!({"id":bot,"model":"fixture-model"})).await;
    let main = host.send(&bot, "ok", None).await;
    let root = main["id"].as_str().unwrap();
    std::fs::write(cwd.join("fail-model"), "").unwrap();
    let failed = host.send_entry(&bot, "ok", Some(root)).await;
    host.wait_entry(&bot, Some(root), |entry| {
        entry["turn"] == failed["turn"] && entry["kind"] == "notice" && entry["data"]["style"] == "error"
    })
    .await;
    let saved = saved_session(&host, &bot, Some(root));
    std::fs::remove_file(cwd.join("fail-model")).unwrap();
    host.send(&bot, "main continues", None).await;
    let prior = events(&cwd);
    let main_prompt = prior.iter().rev().find(|event| event["event"] == "prompted").unwrap();
    assert_eq!(main_prompt["profile"], false);
    assert_eq!(main_prompt["threadIntro"], false);
    host.restart().await;
    let resumed = host.send(&bot, "thread continues", Some(root)).await;
    assert_eq!(sid(&resumed), saved);
    let after = events(&cwd);
    let thread_prompt = after.iter().rev().find(|event| event["event"] == "prompted").unwrap();
    assert_eq!(thread_prompt["profile"], true);
    assert_eq!(thread_prompt["threadIntro"], true);
    assert_bounded(&cwd);
}

#[tokio::test]
async fn routine_model_deadline_preserves_the_allocated_session_for_manual_retry() {
    let host = Host::start().await;
    let (bot, cwd) = host.bot("ModelDeadline", json!({})).await;
    host.call("updateBot", json!({"id":bot,"model":"fixture-model"})).await;
    std::fs::write(cwd.join("hang-model"), "").unwrap();
    let saved = host
        .call(
            "saveRoutine",
            json!({"botId":bot,"name":"Model setup deadline","instruction":"ok",
            "triggers":[{"type":"webhook"}],"timeoutSeconds":2}),
        )
        .await;
    let routine = saved["routine"]["id"].as_str().unwrap();
    host.call("runRoutine", json!({"botId":bot,"id":routine})).await;
    let failed = tokio::time::timeout(std::time::Duration::from_secs(15), async {
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
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let root = failed["rootId"].as_str().unwrap();
    let identity = saved_session(&host, &bot, Some(root));
    std::fs::remove_file(cwd.join("hang-model")).unwrap();
    let retried = host.send(&bot, "manual retry", Some(root)).await;
    assert_eq!(sid(&retried), identity);
    let observed = events(&cwd);
    assert_eq!(observed.iter().filter(|event| event["event"] == "started").count(), 1);
    assert_eq!(observed.iter().filter(|event| event["event"] == "allocated").count(), 1);
    let prompt = observed.iter().find(|event| event["event"] == "prompted").unwrap();
    assert_eq!(prompt["profile"], true);
    assert_eq!(prompt["threadIntro"], true);
    assert_bounded(&cwd);
}

#[tokio::test]
async fn malformed_fork_identity_releases_the_unknown_allocation_before_fallback() {
    let host = Host::start().await;
    for returned in ["empty", "parent"] {
        let (bot, cwd) =
            host.bot(&format!("MalformedFork{returned}"), json!({"fork":true,"forkResponse":returned})).await;
        let main = host.send(&bot, "ok", None).await;
        let thread = host.send(&bot, "ok", main["id"].as_str()).await;
        assert_ne!(sid(&thread), "");
        assert_ne!(sid(&thread), sid(&main));
        let continued = host.send(&bot, "main continues", None).await;
        assert_eq!(sid(&continued), sid(&main));
        assert_bounded(&cwd);
    }
}

#[tokio::test]
async fn forking_before_the_parent_first_prompt_keeps_each_sessions_initial_profile() {
    let host = Host::start().await;
    let (bot, cwd) = host.bot("UnpromptedParent", json!({"fork":true,"close":true})).await;
    host.call("updateBot", json!({"id":bot,"model":"fixture-model"})).await;
    std::fs::write(cwd.join("fail-model"), "").unwrap();
    let failed = host.send_entry(&bot, "ok", None).await;
    host.wait_entry(&bot, None, |entry| {
        entry["turn"] == failed["turn"] && entry["kind"] == "notice" && entry["data"]["style"] == "error"
    })
    .await;
    let main = saved_session(&host, &bot, None);
    std::fs::remove_file(cwd.join("fail-model")).unwrap();
    let root = failed["id"].as_str().unwrap();
    host.send(&bot, "thread first", Some(root)).await;
    let parent = host.send(&bot, "parent first", None).await;
    assert_eq!(sid(&parent), main);
    let observed = events(&cwd);
    let prompts: Vec<_> = observed.iter().filter(|event| event["event"] == "prompted").collect();
    assert_eq!(prompts.len(), 2);
    assert_eq!(prompts[0]["profile"], true);
    assert_eq!(prompts[0]["forkIntro"], true);
    assert_eq!(prompts[1]["profile"], true);
    assert_eq!(prompts[1]["threadIntro"], false);
    assert_bounded(&cwd);
}
