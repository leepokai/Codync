//! Fast mode changes are not session allocations.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[allow(dead_code)]
mod common;
#[path = "common/lifecycle.rs"]
mod lifecycle;

use lifecycle::{Host, events, sid};
use serde_json::{Value, json};

fn assert_speed(reply: &Value, on: bool) {
    let expected = if on { " FAST=on" } else { " FAST=off" };
    assert!(reply["data"]["text"].as_str().unwrap().ends_with(expected), "{reply}");
}

#[tokio::test]
async fn fast_mode_changes_keep_nonresumable_main_and_thread_sessions_live() {
    let host = Host::start().await;
    let (bot, cwd) = host.bot("FastNoResume", json!({"fastMode":true,"load":false})).await;
    host.call("updateBot", json!({"id":bot,"fastMode":true})).await;
    let main = host.send(&bot, "main", None).await;
    assert_speed(&main, true);
    let root = main["id"].as_str().unwrap();
    let thread = host.send(&bot, "thread", Some(root)).await;
    assert_speed(&thread, true);
    host.call("updateBot", json!({"id":bot,"fastMode":false})).await;
    let main_off = host.send(&bot, "main off", None).await;
    let thread_off = host.send(&bot, "thread off", Some(root)).await;
    assert_speed(&main_off, false);
    assert_speed(&thread_off, false);
    assert_eq!(sid(&main_off), sid(&main));
    assert_eq!(sid(&thread_off), sid(&thread));
    assert_eq!(events(&cwd).iter().filter(|event| event["event"] == "started").count(), 1);
}
