//! MCP layout changes and OpenCode per-turn `session/load` with `mcpServers` (leepokai/Codync#508).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::json;

fn agent_requests(host: &Host, bot: &str, fixture: &str, method: &str) -> Vec<serde_json::Value> {
    host.log(bot, &format!("{fixture}.jsonl")).into_iter().filter(|r| r["method"] == method).collect()
}

#[tokio::test]
async fn mcp_layout_change_reloads_servers_without_a_new_session() {
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

    assert_eq!(agent_requests(&host, &bot, "mcp", "session/new").len(), 1, "the chat keeps its session");
    let loads = agent_requests(&host, &bot, "mcp", "session/load");
    assert_eq!(loads.len(), 1, "the agent reloads with the new MCP servers");
    assert!(loads[0]["params"]["mcpServers"].as_array().is_some_and(|a| !a.is_empty()));
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

#[tokio::test]
async fn an_opencode_bot_whose_servers_dropped_starts_again_with_them() {
    if common::python_missing() {
        return;
    }
    let host = Host::start().await;
    let command = common::fixture_command("dropping_servers_agent.py", "");
    let bot = host
        .call(
            "createBot",
            json!({"name": "OC drops", "backend": "opencode", "command": command, "connectors": [], "notify": false}),
        )
        .await["bot"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    host.reply(&bot, "one").await;
    host.reply(&bot, "two").await;
    host.reply(&bot, "EVICT").await;
    host.reply(&bot, "three").await;

    let prompts = host.requests(&bot, "session/prompt");
    let loads = host.requests(&bot, "session/load");
    let last_prompt = prompts.last().unwrap();
    let last_load = loads.last().unwrap();
    assert_ne!(
        last_prompt["pid"], prompts[0]["pid"],
        "the agent that dropped its servers is started again, and the turn runs in the new process"
    );
    assert_eq!(last_load["pid"], last_prompt["pid"], "the session reopens in the new process");
    assert!(last_load["params"]["mcpServers"].as_array().is_some_and(|s| !s.is_empty()));
    let log = std::fs::read_to_string(host.home.join("host.log")).unwrap();
    assert!(log.contains("ServersLost"), "the restart is told apart from the other stops");
}
