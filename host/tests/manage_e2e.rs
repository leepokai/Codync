//! A bot that manages the team, through a real host and its `manage` MCP process: the approval card that shows a
//! change in full, the new bot's record of who made it, and a change the user denies.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

async fn wait(host: &Host, bot: &str, found: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let history = host.call("history", json!({"botId": bot})).await;
        if let Some(entry) = history["entries"].as_array().unwrap().iter().find(|e| found(e)) {
            return entry.clone();
        }
        assert!(Instant::now() < deadline, "{bot}: {history}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Has the fixture lead call `tool` with `args`, answers its approval card with `option`, and returns its reply.
async fn manage(host: &Host, lead: &str, tool: &str, args: &Value, option: &str, check: impl Fn(&str)) -> String {
    host.call("send", json!({"botId": lead, "text": format!("MANAGE {tool} {args}")})).await;
    let card = wait(host, lead, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await;
    check(card["data"]["detail"].as_str().unwrap());
    host.call("respondPermission", json!({"entryId": card["id"], "optionId": option})).await;
    let reply =
        wait(host, lead, |e| e["kind"] == "agent" && e["data"]["final"] == true && e["turn"] == card["turn"]).await;
    reply["data"]["text"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn a_lead_creates_a_bot_the_user_allowed_and_not_one_change_they_denied() {
    let host = Host::start().await;
    let lead = host.fixture_bot("Lead", "team_agent.py", "").await;
    host.call("updateBot", json!({"id": lead, "permission": "ask", "manageTeam": true})).await;

    let routine = json!({"name": "Scan", "instruction": "Scan for vulnerabilities", "triggers": [{"type": "interval", "seconds": 1800}]});
    let args = json!({"name": "Security Analyst", "instructions": "Reviews code for vulnerabilities.", "agent": "claude",
        "routines": [routine]});
    let reply = manage(&host, &lead, "create_bot", &args, "allow", |detail| {
        for line in [
            "Name: Security Analyst",
            "Agent: claude",
            "Approvals: ask",
            "Manage the team: true",
            "Instructions:\nReviews code for vulnerabilities.",
            "• Scan (Every 30 minutes): Scan for vulnerabilities",
        ] {
            assert!(detail.contains(line), "{line:?} not in {detail}");
        }
    })
    .await;
    let made: Value = serde_json::from_str(reply.strip_prefix("Done: ").unwrap_or_else(|| panic!("{reply}"))).unwrap();
    let id = made["bot"]["id"].as_str().unwrap();
    wait(&host, &lead, |e| e["data"]["text"] == "Created bot Security Analyst").await;
    let bot = |sync: &Value| sync["bots"].as_array().unwrap().iter().find(|b| b["id"] == id).cloned().unwrap();
    let analyst = bot(&host.snapshot().await);
    assert_eq!(
        (&analyst["createdBy"], &analyst["permission"], &analyst["manageTeam"]),
        (&json!(lead), &json!("ask"), &json!(true))
    );
    assert_eq!(host.call("routines", json!({"botId": id})).await["routines"][0]["name"], "Scan");

    let reply = manage(&host, &lead, "update_bot", &json!({"botId": id, "hidden": true}), "deny", |detail| {
        assert_eq!(detail, "Hidden: true");
    })
    .await;
    assert!(reply.starts_with("Refused: ") && reply.contains("didn't allow"), "{reply}");
    assert_eq!(bot(&host.snapshot().await)["hidden"], false);

    // The lead changes its own model: the turn that asked for it finishes, then the agent starts afresh.
    let reply = manage(&host, &lead, "update_bot", &json!({"botId": lead, "model": "fixture-2"}), "allow", |detail| {
        assert_eq!(detail, "Model: fixture-2");
    })
    .await;
    assert!(reply.starts_with("Done: "), "{reply}");
    // Its next message opens a new session, and the chat says nothing about it.
    let new_sessions = || host.log(&lead, "sessions.jsonl").iter().filter(|r| r["method"] == "session/new").count();
    let before = new_sessions();
    host.reply(&lead, "hi").await;
    assert_eq!(new_sessions(), before + 1);
    let sync = host.snapshot().await;
    let lead_bot = sync["bots"].as_array().unwrap().iter().find(|b| b["id"] == lead).unwrap();
    assert_eq!(lead_bot["model"], "fixture-2");

    // Only a bot's `create_bot` records a creator: a client's `createBot` can't claim one.
    let mine = json!({"name": "Mine", "backend": "custom", "command": "unused", "connectors": [], "createdBy": lead});
    let mine = host.call("createBot", mine).await["bot"]["id"].as_str().unwrap().to_owned();
    assert!(
        host.snapshot().await["bots"].as_array().unwrap().iter().any(|b| b["id"] == mine && b["createdBy"].is_null())
    );
}

/// A change whose wait ends without an answer ends its card too, and changes nothing.
#[tokio::test]
async fn a_card_ends_when_its_turn_stops_or_its_call_is_dropped() {
    let host = Host::start().await;
    let lead = host.fixture_bot("Lead", "team_agent.py", "").await;
    host.call("updateBot", json!({"id": lead, "permission": "ask", "manageTeam": true})).await;
    let card = |title: &'static str| move |e: &Value| e["kind"] == "permission" && e["data"]["title"] == title;
    let named = |sync: &Value, name: &str| sync["bots"].as_array().unwrap().iter().any(|b| b["name"] == name);

    // The user stops the turn: the card is cancelled and the tool refused.
    host.call("send", json!({"botId": lead, "text": "MANAGE create_bot {\"name\": \"Stopped\"}"})).await;
    let stopped = wait(&host, &lead, card("Create bot Stopped")).await;
    host.call("stop", json!({"botId": lead})).await;
    wait(&host, &lead, |e| e["id"] == stopped["id"] && e["data"]["status"] == "cancelled").await;
    wait(&host, &lead, |e| {
        e["kind"] == "agent" && e["data"]["text"].as_str().is_some_and(|t| t.starts_with("Refused: "))
    })
    .await;

    // The agent cancels the call (its tool timeout): the card expires and the bot works on.
    host.call("send", json!({"botId": lead, "text": "BLOCK"})).await;
    let workspace = host.home.join("bots").join(&lead).join("workspace");
    let servers: Value =
        serde_json::from_str(&std::fs::read_to_string(workspace.join("servers.json")).unwrap()).unwrap();
    let manage = servers.as_array().unwrap().iter().find(|s| s["name"] == "manage").unwrap();
    let args: Vec<&str> = manage["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
    // Started the way the agent would, with the server's own env: it carries the host's data directory.
    let env =
        manage["env"].as_array().unwrap().iter().map(|e| (e["name"].as_str().unwrap(), e["value"].as_str().unwrap()));
    let mut mcp = std::process::Command::new(manage["command"].as_str().unwrap())
        .args(args)
        .env_remove("CODYNC_HOME")
        .envs(env)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = mcp.stdin.take().unwrap();
    let mut send = |msg: Value| std::io::Write::write_all(&mut stdin, format!("{msg}\n").as_bytes()).unwrap();
    send(json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call",
        "params": {"name": "create_bot", "arguments": {"name": "Abandoned"}}}));
    let abandoned = wait(&host, &lead, card("Create bot Abandoned")).await;
    send(json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 7}}));
    wait(&host, &lead, |e| e["id"] == abandoned["id"] && e["data"]["status"] == "expired").await;
    let sync = host.snapshot().await;
    assert_eq!(sync["bots"].as_array().unwrap().iter().find(|b| b["id"] == lead).unwrap()["status"], "working");
    assert!(!named(&sync, "Stopped") && !named(&sync, "Abandoned"), "nothing was created");
    std::fs::write(workspace.join("release"), "").unwrap();
    drop(stdin);
    mcp.wait().unwrap();
}
