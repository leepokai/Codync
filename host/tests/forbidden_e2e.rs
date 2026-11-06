//! A real host and a scripted agent: a bot that approves everything still refuses the forbidden commands.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Command;

/// A bot on `command_agent.py` with the given approval setting.
async fn bot(host: &Host, permission: &str) -> String {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/command_agent.py");
    let spec = json!({"name": "Runner", "backend": "custom", "command": common::python_agent(&script),
        "connectors": [], "notify": false, "permission": permission});
    host.call("createBot", spec).await["bot"]["id"].as_str().unwrap().to_owned()
}

/// Asks the bot to run `command` and returns the agent's final text, which carries the outcome it got.
async fn outcome(host: &Host, bot: &str, command: &str) -> String {
    host.reply(bot, &format!("run:{command}")).await;
    let history = host.call("history", json!({"botId": bot})).await;
    let entries = history["entries"].as_array().unwrap();
    let last = entries.iter().rev().find(|e| e["kind"] == "agent" && e["data"]["final"] == true).expect("a reply");
    last["data"]["text"].as_str().unwrap().to_owned()
}

fn notices(history: &Value) -> Vec<String> {
    history["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "notice")
        .filter_map(|e| e["data"]["text"].as_str().map(str::to_owned))
        .collect()
}

#[tokio::test]
async fn a_bot_that_approves_everything_still_refuses_the_forbidden_commands() {
    if Command::new(common::python()).arg("--version").output().is_err() {
        eprintln!("skipping: Python not available");
        return;
    }
    let host = Host::start().await;
    let bot = bot(&host, "auto").await;

    let allowed = outcome(&host, &bot, "echo hello").await;
    assert!(allowed.contains("\"yes\""), "an ordinary command is approved: {allowed}");

    for command in ["rm -rf /", r"Remove-Item -Recurse -Force C:\", "echo ok && sudo reboot now"] {
        let refused = outcome(&host, &bot, command).await;
        assert!(refused.contains("\"no\""), "`{command}` should be refused: {refused}");
    }

    let history = host.call("history", json!({"botId": bot})).await;
    let said = notices(&history);
    assert!(said.iter().any(|n| n.contains("Blocked command") && n.contains("rm -rf /")), "{said:?}");
    assert!(said.iter().any(|n| n.contains("Remove-Item -r*")), "the notice names the pattern: {said:?}");
}
