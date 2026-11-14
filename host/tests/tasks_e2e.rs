//! Tasks against a real host and its `chat` MCP server, with a scripted ACP agent: a bot hands work to a task,
//! which starts as a fork of the chat's session (or from a brief), raises its approval card in the chat, and
//! reports back as a turn there.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

async fn wait<T>(what: &str, mut check: impl AsyncFnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(found) = check().await {
            return found;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn history(host: &Host, bot: &str) -> Vec<Value> {
    host.call("history", json!({"botId": bot})).await["entries"].as_array().unwrap().clone()
}

/// The bot's chat-visible reply that starts with `prefix`, once it is there.
async fn reply(host: &Host, bot: &str, prefix: &str) -> String {
    wait(prefix, async || {
        let entries = history(host, bot).await;
        let found = entries.iter().find(|e| {
            e["kind"] == "agent"
                && e["data"]["final"] == true
                && e["data"]["text"].as_str().is_some_and(|t| t.starts_with(prefix))
        });
        found.map(|e| e["data"]["text"].as_str().unwrap().to_owned())
    })
    .await
}

async fn tasks(host: &Host, bot: &str) -> Vec<Value> {
    let snapshot = host.snapshot().await;
    let row = snapshot["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot).unwrap();
    row["tasks"].as_array().unwrap().clone()
}

#[tokio::test]
async fn a_task_forks_the_chat_and_its_report_comes_back_as_a_turn() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "task_agent.py", "").await;
    host.reply(&bot, "hello").await;
    let mut events = host.events().await;

    host.reply(&bot, "START Fix the login|Find why the redirect drops the path").await;
    let relayed = reply(&host, &bot, "Relayed:").await;
    assert_eq!(relayed, "Relayed: Report: forked; no chat", "the task's last message reached the chat's own turn");

    // The fork was taken from the chat's session, by the task's own agent process.
    let forks = host.log(&bot, "forks.jsonl");
    assert_eq!(forks.len(), 1);
    let prompts = host.log(&bot, "prompts.jsonl");
    let chat_session = prompts[0]["session"].as_str().unwrap();
    assert_eq!(forks[0]["from"], chat_session);
    let task_prompt = prompts.iter().find(|p| p["text"].as_str().unwrap().contains("[Task]")).unwrap();
    assert_eq!(task_prompt["session"], forks[0]["to"]);
    let text = task_prompt["text"].as_str().unwrap();
    assert!(text.contains("on: \"Fix the login\"") && text.contains("Find why the redirect drops the path"));
    assert!(text.contains("[The user's own words: «START Fix the login|Find why the redirect drops the path»]"));
    assert!(!text.contains("<bot-profile>"), "a fork already holds the instructions");

    // No client lists the task as a bot or mixes its transcript into a chat.
    let snapshot = host.snapshot().await;
    assert!(snapshot["bots"].as_array().unwrap().iter().all(|b| b["taskOf"].is_null()), "{}", snapshot["bots"]);
    assert!(tasks(&host, &bot).await.is_empty(), "a finished task leaves the capsule");
    let chat = history(&host, &bot).await;
    assert!(chat.iter().all(|e| !e["data"]["text"].as_str().unwrap_or_default().contains("[Task]")));

    // Its transcript went out as `taskEntry` events and is fetched by id.
    let mut task = None;
    while let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(300), events.recv()).await {
        let listed = |id: &Value| !id.as_str().unwrap_or_default().starts_with("task-");
        assert!(event["type"] != "entry" || listed(&event["entry"]["botId"]), "{event}");
        assert!(event["type"] != "bot" || listed(&event["bot"]["id"]), "{event}");
        if event["type"] == "taskEntry" {
            task = event["task"].as_str().map(str::to_owned);
        }
    }
    let task = task.expect("the task's entries went out as taskEntry events");
    let details = host.call("taskEntries", json!({"botId": bot, "taskId": task})).await;
    assert_eq!(details["task"]["title"], "Fix the login");
    assert_eq!(details["task"]["status"], "done");
    assert_eq!(details["task"]["start"], "fork");
    let finals: Vec<&str> = details["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "agent" && e["data"]["final"] == true)
        .filter_map(|e| e["data"]["text"].as_str())
        .collect();
    assert_eq!(finals, ["Report: forked; no chat"]);
}

#[tokio::test]
async fn an_agent_that_cannot_fork_starts_the_task_from_the_chat_so_far() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "task_agent.py", "no-fork").await;
    host.reply(&bot, "hello").await;
    host.reply(&bot, "START Update the docs|Rewrite the install steps").await;
    assert_eq!(reply(&host, &bot, "Relayed:").await, "Relayed: Report: briefed; saw the chat");
    assert_eq!(host.log(&bot, "forks.jsonl"), [] as [Value; 0]);
    let prompts = host.log(&bot, "prompts.jsonl");
    let text = prompts.iter().filter_map(|p| p["text"].as_str()).find(|t| t.contains("[Task]")).unwrap().to_owned();
    assert!(text.contains("<bot-profile>"), "a new session gets the bot's instructions");
    assert!(text.contains("hello"), "and what the chat said");
}

#[tokio::test]
async fn a_tasks_approval_card_is_answered_in_the_chat_and_it_cannot_ask_or_nest() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "task_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "permission": "ask"})).await;
    host.reply(&bot, "START Run the suite|PERMISSION then ASK and NEST").await;

    let card = wait("the task's card in the chat", async || {
        history(&host, &bot).await.into_iter().find(|e| e["kind"] == "permission" && e["data"]["status"] == "pending")
    })
    .await;
    assert_eq!(card["data"]["task"]["title"], "Run the suite");
    assert_eq!(card["data"]["title"], "Run the tests");
    let live = tasks(&host, &bot).await;
    assert_eq!(live.len(), 1);
    assert_eq!(live[0]["status"], "needsInput");
    assert_eq!(live[0]["title"], "Run the suite");
    // The chat itself stays free while the task waits.
    host.reply(&bot, "are you there?").await;

    host.call("respondPermission", json!({"entryId": card["id"], "optionId": "allow"})).await;
    let relayed = reply(&host, &bot, "Relayed:").await;
    assert!(relayed.contains("Refused: request_decision isn't available in a task"), "{relayed}");
    assert!(relayed.contains("Refused: start_task isn't available in a task"), "{relayed}");
    assert!(relayed.ends_with("approved"), "{relayed}");
}

#[tokio::test]
async fn one_task_at_a_time_takes_more_work_and_stops_without_a_report() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "task_agent.py", "").await;
    host.reply(&bot, "START First|HOLD until told").await;
    wait("the task at work", async || tasks(&host, &bot).await.first().filter(|t| t["status"] == "working").cloned())
        .await;

    host.reply(&bot, "START Second|more").await;
    let refused = reply(&host, &bot, "Refused: a task is already running (\"First\")").await;
    assert!(refused.contains("tell_task"), "{refused}");

    host.reply(&bot, "STATUS").await;
    let status = reply(&host, &bot, "Status:").await;
    assert!(status.contains("\"title\":\"First\"") && status.contains("\"status\":\"working\""), "{status}");

    host.reply(&bot, "TELL also check the logout").await;
    reply(&host, &bot, "Passed on:").await;

    let task = tasks(&host, &bot).await[0]["id"].as_str().unwrap().to_owned();
    host.call("stopTask", json!({"botId": bot, "taskId": task})).await;
    wait("the capsule to go", async || tasks(&host, &bot).await.is_empty().then_some(())).await;
    let details = host.call("taskEntries", json!({"botId": bot, "taskId": task})).await;
    assert_eq!(details["task"]["status"], "stopped");
    tokio::time::sleep(Duration::from_millis(400)).await;
    let chat = history(&host, &bot).await;
    assert!(chat.iter().all(|e| !e["data"]["text"].as_str().unwrap_or_default().starts_with("Relayed:")));
}

#[tokio::test]
async fn a_task_cut_off_by_a_host_restart_picks_up_and_still_reports() {
    let mut host = Host::start().await;
    let bot = host.fixture_bot("Mika", "task_agent.py", "").await;
    host.reply(&bot, "START Long one|HOLD until told").await;
    wait("the task at work", async || tasks(&host, &bot).await.first().filter(|t| t["status"] == "working").cloned())
        .await;

    host.restart(|_| {}).await;
    // Its actor resumes the turn that was cut off; what that turn says is the report.
    let relayed = reply(&host, &bot, "Relayed:").await;
    assert!(relayed.contains("Codync restarted"), "{relayed}");
    assert_eq!(tasks(&host, &bot).await, [] as [Value; 0]);
}
