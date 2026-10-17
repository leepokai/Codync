//! Process ownership preserves the upstream allocation and recovery policy.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[allow(dead_code)]
mod common;
#[path = "common/lifecycle.rs"]
mod lifecycle;

use lifecycle::{Host, events, sid};
use serde_json::json;

#[tokio::test]
async fn nonresumable_agents_keep_multiple_threads_and_accept_new_main_sessions() {
    let host = Host::start().await;
    let (bot, cwd) = host.bot("NoSessionCap", json!({"load":false,"fork":true})).await;
    let main = host.send(&bot, "main", None).await;
    let first = host.send(&bot, "first thread", main["id"].as_str()).await;
    let next = host.send(&bot, "main continues", None).await;
    let second = host.send(&bot, "second thread", next["id"].as_str()).await;
    assert_ne!(sid(&first), sid(&second));
    assert_ne!(sid(&first), sid(&main));
    let fork_prompts = events(&cwd);
    assert!(fork_prompts.iter().any(|event| event["event"] == "prompted" && event["forkIntro"] == true));
    host.call("newSession", json!({"botId":bot})).await;
    let replacement = host.send(&bot, "fresh main", None).await;
    assert_ne!(sid(&replacement), sid(&main));
    let continued = host.send(&bot, "first thread continues", main["id"].as_str()).await;
    assert_eq!(sid(&continued), sid(&first));
    assert_eq!(events(&cwd).iter().filter(|event| event["event"] == "started").count(), 1);
}

#[tokio::test]
async fn claude_style_missing_saved_sessions_recover_main_and_threads_with_a_notice() {
    let mut host = Host::start().await;
    let (bot, cwd) = host.bot("MissingTranscript", json!({})).await;
    let main = host.send(&bot, "main", None).await;
    let root = main["id"].as_str().unwrap();
    let thread = host.send(&bot, "thread", Some(root)).await;
    host.restart().await;
    std::fs::write(cwd.join("missing-load"), "").unwrap();
    let recovered_main = host.send(&bot, "recover main", None).await;
    let recovered_thread = host.send(&bot, "recover thread", Some(root)).await;
    assert_ne!(sid(&recovered_main), sid(&main));
    assert_ne!(sid(&recovered_thread), sid(&thread));
    for lane in [None, Some(root)] {
        let entries = host.entries(&bot, lane).await;
        assert!(entries.iter().any(|entry| entry["kind"] == "notice"
            && entry["data"]["style"] == "divider"
            && entry["data"]["text"].as_str().is_some_and(|text| text.contains("starting a fresh one"))));
    }
}
