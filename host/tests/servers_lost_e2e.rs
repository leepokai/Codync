//! An agent that drops Codync's MCP servers from a live session, as OpenCode 2.x does once it has been idle a while
//! (#508), through a real host and a fixture agent: the next turn starts it again and reopens the session with them,
//! but not while that would end something it runs detached.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::Value;

/// The agent process (pid) behind each of `requests`.
fn pids(requests: &[Value]) -> Vec<u64> {
    requests.iter().map(|r| r["pid"].as_u64().unwrap()).collect()
}

#[tokio::test]
async fn an_agent_that_dropped_codyncs_servers_starts_again_with_them() {
    if common::python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Drops", "dropping_servers_agent.py", "").await;

    host.reply(&bot, "one").await;
    host.reply(&bot, "two").await;
    assert!(host.requests(&bot, "session/load").is_empty(), "its servers still run: the session stays as it is");

    if cfg!(unix) {
        // Something detached under it would end with it, so it keeps the session without them meanwhile.
        host.reply(&bot, "EVICT DETACH").await;
        host.reply(&bot, "three").await;
        assert!(host.requests(&bot, "session/load").is_empty(), "stopping it would end its detached work");
        host.reply(&bot, "ATTACH").await;
    } else {
        host.reply(&bot, "EVICT").await;
    }

    host.reply(&bot, "four").await;
    let opened = pids(&host.requests(&bot, "session/new"));
    let loaded = host.requests(&bot, "session/load");
    assert_eq!(loaded.len(), 1, "the session reopens in a new process");
    assert!(loaded[0]["params"]["mcpServers"].as_array().is_some_and(|s| !s.is_empty()), "{loaded:?}");
    assert_ne!(pids(&loaded), opened, "loading it in the same process wouldn't bring the servers back");
    let prompts = host.requests(&bot, "session/prompt");
    assert_eq!(prompts.last().unwrap()["pid"], loaded[0]["pid"], "the turn runs in the new process");
    let log = std::fs::read_to_string(host.home.join("host.log")).unwrap();
    assert!(log.contains("ServersLost"), "the restart is told apart from the other stops");

    // Its servers run again, so the next turn doesn't start it once more.
    host.reply(&bot, "five").await;
    assert_eq!(host.requests(&bot, "session/load").len(), 1);
}
