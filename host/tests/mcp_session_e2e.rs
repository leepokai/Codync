//! When Codync's MCP layout changes, the host opens a fresh session so bridges are sent again (leepokai/Codync#508).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::json;
use std::time::Duration;

fn agent_requests(host: &Host, bot: &str, fixture: &str, method: &str) -> Vec<serde_json::Value> {
    host.log(bot, &format!("{fixture}.jsonl"))
        .into_iter()
        .filter(|r| r["method"] == method)
        .collect()
}

#[tokio::test]
async fn mcp_layout_change_starts_a_fresh_session_instead_of_reusing_one() {
    if common::python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("MCP", "relay_agent.py", "mcp").await;

    host.reply(&bot, "one").await;
    assert_eq!(agent_requests(&host, &bot, "mcp", "session/new").len(), 1);

    host.reply(&bot, "two").await;
    assert_eq!(
        agent_requests(&host, &bot, "mcp", "session/load").len(),
        0,
        "the second turn reuses the session the agent already holds"
    );

    host.call("updateBot", json!({"id": bot, "manageTeam": true})).await;
    host.reply(&bot, "three").await;

    common::eventually("a fresh session is opened", Duration::from_secs(30), async || {
        agent_requests(&host, &bot, "mcp", "session/new").len() == 2
    })
    .await;
    let news = agent_requests(&host, &bot, "mcp", "session/new");
    let second_new = news.last().unwrap();
    assert!(
        second_new["params"]["mcpServers"].as_array().is_some_and(|a| !a.is_empty()),
        "a fresh session must carry Codync MCP servers"
    );
}

#[tokio::test]
async fn opencode_reopens_with_mcp_servers_every_turn() {
    if common::python_missing() {
        return;
    }
    let host = Host::start().await;
    let command = common::fixture_command("relay_agent.py", "ocode");
    let bot = host
        .call(
            "createBot",
            json!({"name": "OC", "backend": "opencode", "command": command, "connectors": [], "notify": false}),
        )
        .await["bot"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    host.reply(&bot, "one").await;
    assert_eq!(agent_requests(&host, &bot, "ocode", "session/new").len(), 1);

    host.reply(&bot, "two").await;
    let loads = agent_requests(&host, &bot, "ocode", "session/load");
    assert_eq!(loads.len(), 1, "OpenCode must reopen with mcpServers each turn");
    assert!(loads[0]["params"]["mcpServers"].as_array().is_some_and(|a| !a.is_empty()));
    assert_eq!(
        agent_requests(&host, &bot, "ocode", "session/new").len(),
        1,
        "the loaded session must stay the same conversation"
    );
}