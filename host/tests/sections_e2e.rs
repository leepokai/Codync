//! Roster sections through a real host: `section` on bots and groups, and `renameSection`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};

/// Each bot's `section` as a client holds it after catching up (`null`: none).
async fn sections(host: &Host, ids: &[&str]) -> Vec<Value> {
    let snapshot = host.snapshot().await;
    let bots = snapshot["bots"].as_array().unwrap();
    ids.iter().map(|id| bots.iter().find(|b| b["id"] == *id).unwrap()["section"].clone()).collect()
}

#[tokio::test]
async fn bots_and_groups_take_a_trimmed_checked_section() {
    let host = Host::start().await;
    let (a, b) = (host.bot("A", "auto").await, host.bot("B", "auto").await);
    let group = host.call("createBot", json!({"name": "G", "kind": "group", "members": [a, b]})).await["bot"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let made = host
        .call(
            "createBot",
            json!({"name": "C", "backend": "custom", "command": "unused", "connectors": [], "section": " Side "}),
        )
        .await;
    assert_eq!(made["bot"]["section"], "Side", "createBot takes one too");

    let set = host.call("updateBot", json!({"id": a, "section": "  Work "})).await;
    assert_eq!(set["bot"]["section"], "Work", "trimmed");
    host.call("updateBot", json!({"id": a, "name": "A2"})).await;
    host.call("updateBot", json!({"id": group, "section": "Home"})).await;
    assert_eq!(
        sections(&host, &[a.as_str(), group.as_str()]).await,
        [json!("Work"), json!("Home")],
        "a patch without `section` leaves it"
    );
    let cleared = host.call("updateBot", json!({"id": a, "section": null})).await;
    assert!(cleared["bot"]["section"].is_null(), "null clears it");
    let blank = host.call("updateBot", json!({"id": group, "section": " \t "})).await;
    assert!(blank["bot"]["section"].is_null(), "nothing left after trimming: no section");

    // Characters count, not bytes: 40 two-byte ones fit.
    let longest = "İ".repeat(40);
    let fits = host.call("updateBot", json!({"id": b, "section": longest})).await;
    assert_eq!(fits["bot"]["section"], longest.as_str());
    let long = host.refused("updateBot", json!({"id": b, "section": "x".repeat(41)})).await;
    assert_eq!(long, "A section name can be at most 40 characters.");
    let control = host.refused("updateBot", json!({"id": group, "section": "Wo\u{7}rk"})).await;
    assert_eq!(control, "A section name can't contain control characters.", "groups are checked too");
}

#[tokio::test]
async fn rename_section_answers_like_reorder_bots() {
    let host = Host::start().await;
    let a = host.bot("A", "auto").await;
    host.call("updateBot", json!({"id": a, "section": "Work"})).await;
    assert_eq!(host.call("renameSection", json!({"from": "Work", "to": "Play"})).await, json!({}));
    assert_eq!(sections(&host, &[a.as_str()]).await, [json!("Play")]);
    let typo = host.refused("renameSection", json!({"from": "Play", "to": 5})).await;
    assert!(typo.starts_with("`to` must be a section name or null"), "{typo}");
    let missing = host.refused("renameSection", json!({"from": "Play"})).await;
    assert!(missing.starts_with("`to` must be a section name or null"), "{missing}");
    assert_eq!(sections(&host, &[a.as_str()]).await, [json!("Play")], "a missing `to` moves nothing");
    host.call("renameSection", json!({"from": "Play", "to": null})).await;
    assert_eq!(sections(&host, &[a.as_str()]).await, [Value::Null]);
}
