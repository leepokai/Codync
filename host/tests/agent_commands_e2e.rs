//! The agent's own `/` commands, through a real host and a fixture agent that lists some: the apps get the list
//! with the bot, and a message that starts with one reaches the agent as typed, so the agent runs it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::json;

#[tokio::test]
async fn an_agents_commands_are_listed_and_a_message_with_one_goes_as_typed() {
    if common::python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Commands", "commands_agent.py", "").await;

    // The list comes with the session the first message opens, in time for that message.
    host.reply(&bot, "/review the last commit").await;
    assert_eq!(host.prompts(&bot), ["/review the last commit"], "nothing in front of the command");
    let sync = host.snapshot().await;
    assert_eq!(
        common::bot(&sync, &bot)["agentCommands"],
        json!([
            {"name": "review", "description": "Review the changes", "hint": "what to look at"},
            {"name": "compact", "description": "Summarize the conversation"},
        ])
    );

    // The profile the command went without comes with the next message.
    host.reply(&bot, "thanks").await;
    let prompts = host.prompts(&bot);
    assert!(prompts[1].starts_with("<bot-profile>") && prompts[1].ends_with("thanks"), "{prompts:?}");

    host.reply(&bot, "/compact").await;
    assert_eq!(host.prompts(&bot)[2], "/compact");
}
