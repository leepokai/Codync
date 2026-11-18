//! A bot's box against a real host and its `chat` MCP server, with a scripted ACP agent: the bot adds items, tasks
//! take them up to `maxAgents` (a fork of the chat's session, or from a brief), raise their approval cards in the
//! chat and report back as turns there; the user and the bot move, cut in, replace, stop and retry items.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
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

/// The bot's `work` on the wire.
async fn work(host: &Host, bot: &str) -> Vec<Value> {
    let snapshot = host.snapshot().await;
    let row = snapshot["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot).unwrap();
    row["work"].as_array().unwrap().clone()
}

/// The bot's item called `title`, once it reads `status`.
async fn item(host: &Host, bot: &str, title: &str, status: &str) -> Value {
    wait(&format!("{title} to be {status}"), async || {
        work(host, bot).await.into_iter().find(|i| i["title"] == title && i["status"] == status)
    })
    .await
}

fn id(item: &Value) -> &str {
    item["id"].as_str().unwrap()
}

/// Lets the task holding on `HOLD:<name>` finish.
fn release(host: &Host, bot: &str, name: &str) {
    std::fs::write(host.workspace(bot).join(format!("release-{name}")), "").unwrap();
}

/// A bot on `task_agent.py` that runs at most `agents` tasks at once.
async fn bot_with(host: &Host, agents: u8) -> String {
    let bot = host.fixture_bot("Mika", "task_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "maxAgents": agents})).await;
    bot
}

/// The chat-visible `Relayed:` replies, oldest first.
async fn relayed(host: &Host, bot: &str) -> Vec<String> {
    let texts = history(host, bot).await.into_iter().filter_map(|e| e["data"]["text"].as_str().map(str::to_owned));
    texts.filter(|t| t.starts_with("Relayed:")).collect()
}

/// The prompts the bot's tasks got for `title`, oldest first.
fn task_prompts(host: &Host, bot: &str, title: &str) -> Vec<Value> {
    task_prompts_in(&host.workspace(bot), title)
}

/// The prompts the tasks working in `dir` got for `title`, oldest first.
fn task_prompts_in(dir: &Path, title: &str) -> Vec<Value> {
    let named = format!(": \"{title}\".");
    let of_title = |p: &Value| p["text"].as_str().is_some_and(|t| t.contains("[Task]") && t.contains(&named));
    logged(dir, "prompts.jsonl").into_iter().filter(of_title).collect()
}

/// The JSON lines a fixture agent working in `dir` logged to `file` there.
fn logged(dir: &Path, file: &str) -> Vec<Value> {
    let log = std::fs::read_to_string(dir.join(file)).unwrap_or_default();
    log.lines().map(|l| serde_json::from_str::<Value>(l).unwrap()).collect()
}

/// No client ever hears of a task as a bot, or of its transcript as a chat's.
async fn no_task_bot_reached(events: &mut tokio::sync::mpsc::UnboundedReceiver<Value>) -> Vec<String> {
    let mut tasks = Vec::new();
    while let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(300), events.recv()).await {
        let listed = |id: &Value| !id.as_str().unwrap_or_default().starts_with("task-");
        assert!(event["type"] != "entry" || listed(&event["entry"]["botId"]), "{event}");
        assert!(event["type"] != "bot" || listed(&event["bot"]["id"]), "{event}");
        if event["type"] == "taskEntry" {
            tasks.extend(event["task"].as_str().map(str::to_owned));
        }
    }
    tasks
}

#[tokio::test]
async fn an_item_forks_the_chat_and_its_report_comes_back_as_a_turn() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "task_agent.py", "").await;
    host.reply(&bot, "hello").await;
    let mut events = host.events().await;

    host.reply(&bot, "ADD Fix the login|Find why the redirect drops the path").await;
    let relayed = reply(&host, &bot, "Relayed:").await;
    assert_eq!(relayed, "Relayed: Report Fix the login: forked; no chat", "its last message reached the chat's turn");

    // The fork was taken from the chat's session, by the task's own agent process.
    let forks = host.log(&bot, "forks.jsonl");
    assert_eq!(forks.len(), 1);
    let prompts = host.log(&bot, "prompts.jsonl");
    let chat_session = prompts[0]["session"].as_str().unwrap();
    assert_eq!(forks[0]["from"], chat_session);
    let task_prompt = &task_prompts(&host, &bot, "Fix the login")[0];
    assert_eq!(task_prompt["session"], forks[0]["to"]);
    let text = task_prompt["text"].as_str().unwrap();
    assert!(text.contains("Find why the redirect drops the path"));
    assert!(text.contains("[The user's own words: «ADD Fix the login|Find why the redirect drops the path»]"));
    assert!(!text.contains("<bot-profile>"), "a fork already holds the instructions");

    // No client lists the task as a bot or mixes its transcript into a chat.
    let snapshot = host.snapshot().await;
    assert!(snapshot["bots"].as_array().unwrap().iter().all(|b| b["taskOf"].is_null()), "{}", snapshot["bots"]);
    let chat = history(&host, &bot).await;
    assert!(chat.iter().all(|e| !e["data"]["text"].as_str().unwrap_or_default().contains("[Task]")));

    // The item stays in the box as done, with what its task reported and the task that had it.
    let done = item(&host, &bot, "Fix the login", "done").await;
    assert_eq!(done["result"], "Report Fix the login: forked; no chat");
    assert!(done["wait"].is_null() && done["cutIn"] == false && done["endedAt"].is_i64(), "{done}");
    let tasks = no_task_bot_reached(&mut events).await;
    assert_eq!(tasks.first().map(String::as_str), done["task"].as_str(), "its entries went out as taskEntry events");

    // Its transcript is fetched by the item's task.
    let details = host.call("taskEntries", json!({"botId": bot, "taskId": done["task"]})).await;
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
    assert_eq!(finals, ["Report Fix the login: forked; no chat"]);

    host.reply(&bot, "STATUS").await;
    let status = reply(&host, &bot, "Status:").await;
    assert!(status.contains("\"title\":\"Fix the login\"") && status.contains("\"status\":\"done\""), "{status}");
    host.call("dropWork", json!({"botId": bot, "itemId": id(&done)})).await;
    assert_eq!(work(&host, &bot).await, [] as [Value; 0]);
    let gone = host.refused("taskEntries", json!({"botId": bot, "taskId": done["task"]})).await;
    assert_eq!(gone, "unknown task", "a task no item names goes with its transcript");
}

#[tokio::test]
async fn an_agent_that_cannot_fork_starts_the_item_from_the_chat_so_far() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "task_agent.py", "no-fork").await;
    host.reply(&bot, "hello").await;
    host.reply(&bot, "ADD Update the docs|Rewrite the install steps").await;
    assert_eq!(reply(&host, &bot, "Relayed:").await, "Relayed: Report Update the docs: briefed; saw the chat");
    assert_eq!(host.log(&bot, "forks.jsonl"), [] as [Value; 0]);
    let text = task_prompts(&host, &bot, "Update the docs")[0]["text"].as_str().unwrap().to_owned();
    assert!(text.contains("<bot-profile>"), "a new session gets the bot's instructions");
    assert!(text.contains("hello"), "and what the chat said");
}

#[tokio::test]
async fn a_tasks_approval_card_is_answered_in_the_chat_and_it_cannot_add_work() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "task_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "permission": "ask"})).await;
    host.reply(&bot, "ADD Run the suite|PERMISSION then NEST").await;

    let card = wait("the task's card in the chat", async || {
        history(&host, &bot).await.into_iter().find(|e| e["kind"] == "permission" && e["data"]["status"] == "pending")
    })
    .await;
    let waits = item(&host, &bot, "Run the suite", "needsYou").await;
    assert_eq!(card["data"]["item"], waits["id"], "the card names the item");
    assert_eq!(card["data"]["task"], json!({"id": waits["task"], "title": "Run the suite"}));
    assert_eq!(card["data"]["title"], "Run the tests");
    // The chat itself stays free while the task waits.
    host.reply(&bot, "are you there?").await;

    // What a task may not do from where it is, whichever server it asks.
    for (method, tool) in [
        ("chatCall", "set_name"),
        ("chatCall", "steer_work"),
        ("chatCall", "work_status"),
        ("teamCall", "message_bot"),
        ("routineCall", "follow_up"),
        ("manageCall", "create_bot"),
    ] {
        let refused = host.refused(method, json!({"botId": waits["task"], "name": tool, "arguments": {}})).await;
        assert!(refused.contains(&format!("{tool} isn't available in a task")), "{method} {tool}: {refused}");
    }

    host.call("respondPermission", json!({"entryId": card["id"], "optionId": "allow"})).await;
    let relayed = reply(&host, &bot, "Relayed:").await;
    assert!(relayed.contains("Refused: add_work isn't available in a task"), "{relayed}");
    assert!(relayed.ends_with("approved"), "{relayed}");
}

#[tokio::test]
async fn items_wait_for_a_place_in_order_and_one_moved_first_starts_first() {
    let host = Host::start().await;
    for agents in [0, 7] {
        let bot = host.fixture_bot("Odd", "task_agent.py", "").await;
        let refused = host.refused("updateBot", json!({"id": bot, "maxAgents": agents})).await;
        assert_eq!(refused, "maxAgents is 1 to 6");
    }
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD One|HOLD:one").await;
    item(&host, &bot, "One", "working").await;
    host.reply(&bot, "ADD Two|HOLD:two").await;
    host.reply(&bot, "ADD Three|HOLD:three").await;
    host.reply(&bot, "ADD Four|quick").await;
    host.reply(&bot, "ADD Five|quick").await;

    let titles = |work: &[Value]| work.iter().map(|i| i["title"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
    let listed = work(&host, &bot).await;
    assert_eq!(titles(&listed), ["One", "Two", "Three", "Four", "Five"], "the working one, then the waiting in order");
    assert!(listed[1..].iter().all(|i| i["status"] == "waiting" && i["task"].is_null()), "{listed:?}");
    assert_eq!(listed[1]["wait"], json!({"kind": "next"}));
    assert_eq!(listed[2]["wait"], json!({"kind": "behind", "ahead": 1}));
    assert_eq!(listed[4]["wait"], json!({"kind": "behind", "ahead": 3}));

    // The user moves one first; the bot another, with its tool; a waiting one is dropped.
    host.call("workFirst", json!({"botId": bot, "itemId": id(&listed[2])})).await;
    host.reply(&bot, "STEER first|Four").await;
    host.call("dropWork", json!({"botId": bot, "itemId": id(&listed[4])})).await;
    let refused = host.refused("dropWork", json!({"botId": bot, "itemId": id(&listed[0])})).await;
    assert_eq!(refused, "\"One\" is being worked on: stop it first");
    let refused = host.refused("workFirst", json!({"botId": bot, "itemId": id(&listed[0])})).await;
    assert_eq!(refused, "\"One\" isn't waiting");
    assert_eq!(titles(&work(&host, &bot).await), ["One", "Four", "Three", "Two"]);

    // A place comes free: the first in order takes it, and only that one.
    release(&host, &bot, "one");
    item(&host, &bot, "One", "done").await;
    item(&host, &bot, "Four", "done").await;
    item(&host, &bot, "Three", "working").await;
    assert_eq!(item(&host, &bot, "Two", "waiting").await["wait"], json!({"kind": "next"}));

    // A higher ceiling starts what waits, at once.
    host.call("updateBot", json!({"id": bot, "maxAgents": 2})).await;
    item(&host, &bot, "Two", "working").await;
    release(&host, &bot, "two");
    release(&host, &bot, "three");
    wait("every report", async || (relayed(&host, &bot).await.join(" ").matches("Report ").count() == 4).then_some(()))
        .await;
}

#[tokio::test]
async fn an_item_that_could_not_take_the_place_cuts_nothing_out() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD Slow|HOLD:slow").await;
    let slow = item(&host, &bot, "Slow", "working").await;
    host.reply(&bot, "ADD More|HOLD:more|after=Slow").await;
    let more = item(&host, &bot, "More", "waiting").await;
    assert_eq!(more["wait"], json!({"kind": "after"}));

    // It follows what works: closing that session would only make both start over.
    host.call("workCutIn", json!({"botId": bot, "itemId": id(&more), "insteadOf": id(&slow)})).await;
    host.call("workCutIn", json!({"botId": bot, "itemId": id(&more)})).await;
    let listed = work(&host, &bot).await;
    let still = listed.iter().find(|i| i["title"] == "Slow").unwrap();
    assert!(still["status"] == "working" && still["task"] == slow["task"] && still["cutIn"] == false, "{still}");
    assert_eq!(item(&host, &bot, "More", "waiting").await["wait"], json!({"kind": "after"}));

    release(&host, &bot, "slow");
    let joined = item(&host, &bot, "More", "working").await;
    assert_eq!(joined["task"], slow["task"], "in the same session, as it asked");
    release(&host, &bot, "more");
    wait("both reports", async || (relayed(&host, &bot).await.len() == 2).then_some(())).await;
}

#[tokio::test]
async fn an_item_cuts_in_and_the_one_that_gave_way_starts_again_in_a_new_task() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD Slow|HOLD:slow").await;
    let slow = item(&host, &bot, "Slow", "working").await;
    host.reply(&bot, "ADD Urgent|HOLD:urgent").await;
    let urgent = item(&host, &bot, "Urgent", "waiting").await;
    let mut events = host.events().await;

    let refused = host.refused("workCutIn", json!({"botId": bot, "itemId": id(&urgent), "insteadOf": "nope"})).await;
    assert_eq!(refused, "no such work");
    host.call("workCutIn", json!({"botId": bot, "itemId": id(&urgent), "insteadOf": id(&slow)})).await;
    item(&host, &bot, "Urgent", "working").await;
    let aside = item(&host, &bot, "Slow", "waiting").await;
    assert!(aside["cutIn"] == true && aside["task"].is_null() && aside["startedAt"].is_null(), "{aside}");
    assert_eq!(aside["wait"], json!({"kind": "next"}));
    wait("the task that gave way to go", async || {
        let gone = host.post("taskEntries", &json!({"botId": bot, "taskId": slow["task"]})).await;
        (!gone.0).then_some(())
    })
    .await;

    // The place comes free again: it starts over in a new task and session, told it was begun before.
    release(&host, &bot, "urgent");
    let again = item(&host, &bot, "Slow", "working").await;
    assert_ne!(again["task"], slow["task"]);
    release(&host, &bot, "slow");
    wait("both reports", async || (relayed(&host, &bot).await.len() == 2).then_some(())).await;
    assert_eq!(
        relayed(&host, &bot).await,
        ["Relayed: Report Urgent: forked; no chat", "Relayed: Report Slow: forked; no chat; again"]
    );
    let prompts = task_prompts(&host, &bot, "Slow");
    assert_eq!(prompts.len(), 2);
    assert_ne!(prompts[0]["session"], prompts[1]["session"]);
    assert_eq!(item(&host, &bot, "Slow", "done").await["cutIn"], true);
    no_task_bot_reached(&mut events).await;

    // With a place free, cutting in is moving first; the bot's tool picks who gives way when none is named.
    host.call("updateBot", json!({"id": bot, "maxAgents": 2})).await;
    host.reply(&bot, "ADD A|HOLD:a").await;
    host.reply(&bot, "ADD B|HOLD:b").await;
    item(&host, &bot, "B", "working").await;
    host.reply(&bot, "ADD C|HOLD:c").await;
    host.reply(&bot, "STEER now|C").await;
    item(&host, &bot, "C", "working").await;
    assert!(item(&host, &bot, "B", "waiting").await["cutIn"] == true, "the one that started last gave way");
    item(&host, &bot, "A", "working").await;
}

#[tokio::test]
async fn a_replaced_brief_is_what_the_item_starts_from_waiting_or_working() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD Working|HOLD:never").await;
    let working = item(&host, &bot, "Working", "working").await;
    host.reply(&bot, "ADD Waiting|HOLD:never").await;
    item(&host, &bot, "Waiting", "waiting").await;

    // A waiting item: its brief changes and nothing else.
    host.reply(&bot, "STEER replace|Waiting|a quick look").await;
    reply(&host, &bot, "Steered:").await;
    assert_eq!(item(&host, &bot, "Working", "working").await["task"], working["task"]);

    // A working one: its task's work is closed and it starts again from the new brief, ahead of the waiting.
    host.reply(&bot, "STEER replace|Working|just say so").await;
    wait("both reports", async || (relayed(&host, &bot).await.len() == 2).then_some(())).await;
    assert_eq!(
        relayed(&host, &bot).await,
        ["Relayed: Report Working: forked; no chat", "Relayed: Report Waiting: forked; no chat"]
    );
    let again = item(&host, &bot, "Working", "done").await;
    assert_ne!(again["task"], working["task"]);
    assert_eq!(again["cutIn"], false);
    let prompts = task_prompts(&host, &bot, "Working");
    assert_eq!(prompts.len(), 2);
    let text = prompts[1]["text"].as_str().unwrap();
    assert!(text.contains("\n\njust say so\n\n") && !text.contains("HOLD:never\n"), "{text}");
    assert!(text.contains("[The user said since: «STEER replace|Working|just say so»]"), "{text}");
    let text = task_prompts(&host, &bot, "Waiting")[0]["text"].as_str().unwrap().to_owned();
    assert!(text.contains("\n\na quick look\n\n"), "{text}");

    host.reply(&bot, "STEER replace|Working|too late").await;
    let refused = reply(&host, &bot, "Refused:").await;
    assert_eq!(refused, "Refused: \"Working\" has ended: add new work instead");
}

#[tokio::test]
async fn an_item_that_follows_another_runs_in_its_session_when_it_is_done() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD First|HOLD:first").await;
    let first = item(&host, &bot, "First", "working").await;
    host.reply(&bot, "ADD Other|quick").await;
    host.reply(&bot, "ADD Second|more of the same|after=First").await;
    assert_eq!(item(&host, &bot, "Second", "waiting").await["wait"], json!({"kind": "after"}));

    release(&host, &bot, "first");
    wait("every report", async || (relayed(&host, &bot).await.join(" ").matches("Report ").count() == 3).then_some(()))
        .await;
    let reports = relayed(&host, &bot).await.join(" | ");
    assert!(reports.contains("Report First: forked; no chat"), "{reports}");
    assert!(reports.contains("Report Second: next; no chat"), "{reports}");

    // The same task and session took it, ahead of the ordinary item that was before it in the box.
    let second = item(&host, &bot, "Second", "done").await;
    assert_eq!(second["task"], first["task"]);
    assert_ne!(item(&host, &bot, "Other", "done").await["task"], first["task"]);
    let (first, second) = (&task_prompts(&host, &bot, "First")[0], &task_prompts(&host, &bot, "Second")[0]);
    assert_eq!(first["session"], second["session"]);
    let prompts = host.log(&bot, "prompts.jsonl");
    let at = |prompt: &Value| prompts.iter().position(|p| p == prompt).unwrap();
    assert!(at(second) < at(&task_prompts(&host, &bot, "Other")[0]), "it had the place its task kept");
    assert_eq!(host.log(&bot, "forks.jsonl").len(), 2, "one fork per task");

    // Once that task has ended, an item can't follow into it.
    host.reply(&bot, "ADD Third|more|after=First").await;
    let refused = reply(&host, &bot, "Refused:").await;
    assert_eq!(refused, "Refused: after names no item that is working or waiting: see work_status");
}

#[tokio::test]
async fn items_stop_one_or_all_without_a_report_and_a_stopped_one_is_tried_again() {
    let host = Host::start().await;
    let bot = bot_with(&host, 2).await;
    host.reply(&bot, "ADD One|HOLD:one").await;
    host.reply(&bot, "ADD Two|HOLD:two").await;
    host.reply(&bot, "ADD Three|HOLD:three|after=One").await;
    host.reply(&bot, "ADD Four|HOLD:four").await;
    let one = item(&host, &bot, "One", "working").await;
    item(&host, &bot, "Two", "working").await;

    // One item: its task stops, and what followed it becomes an ordinary item that takes the place.
    host.call("stopWork", json!({"botId": bot, "itemId": id(&one)})).await;
    let stopped = item(&host, &bot, "One", "stopped").await;
    assert!(stopped["endedAt"].is_i64() && stopped["result"].is_null(), "{stopped}");
    let three = item(&host, &bot, "Three", "working").await;
    assert_ne!(three["task"], one["task"]);
    let refused = host.refused("stopWork", json!({"botId": bot, "itemId": id(&one)})).await;
    assert_eq!(refused, "that work has already ended");

    // The bot stops one with its tool; then the user stops all, the waiting too.
    host.reply(&bot, "STEER stop|Two").await;
    item(&host, &bot, "Two", "stopped").await;
    item(&host, &bot, "Four", "working").await;
    host.reply(&bot, "ADD Five|quick").await;
    host.call("stopWork", json!({"botId": bot})).await;
    for title in ["Three", "Four", "Five"] {
        item(&host, &bot, title, "stopped").await;
    }
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(relayed(&host, &bot).await, [] as [String; 0], "the user knows: nothing is reported");

    // Tried again, it waits last and starts in a new task.
    let refused = host.refused("retryWork", json!({"botId": bot, "itemId": "nope"})).await;
    assert_eq!(refused, "no such work");
    release(&host, &bot, "one");
    host.call("retryWork", json!({"botId": bot, "itemId": id(&one)})).await;
    wait("its report", async || {
        relayed(&host, &bot).await.iter().any(|r| r == "Relayed: Report One: forked; no chat").then_some(())
    })
    .await;
    let done = item(&host, &bot, "One", "done").await;
    assert_ne!(done["task"], one["task"]);
    let gone = host.refused("taskEntries", json!({"botId": bot, "taskId": one["task"]})).await;
    assert_eq!(gone, "unknown task");
}

#[tokio::test]
async fn an_item_cut_off_by_a_host_restart_picks_up_and_still_reports() {
    let mut host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD Long one|HOLD:long").await;
    let long = item(&host, &bot, "Long one", "working").await;
    host.reply(&bot, "ADD Next|quick").await;

    host.restart(|_| {}).await;
    // Its task's actor resumes the turn that was cut off; what that turn says is the report. The box is as it was.
    let relayed = reply(&host, &bot, "Relayed:").await;
    assert!(relayed.contains("Codync restarted"), "{relayed}");
    assert_eq!(item(&host, &bot, "Long one", "done").await["task"], long["task"]);
    item(&host, &bot, "Next", "done").await;
}

#[tokio::test]
async fn an_item_whose_agent_cannot_start_says_so_and_frees_the_place_for_the_next() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "hello").await;
    let no_start = host.home.join("bots").join(&bot).join("workspace/no-start");
    std::fs::write(&no_start, "").unwrap();

    host.reply(&bot, "ADD Broken|anything").await;
    let relayed = reply(&host, &bot, "Relayed:").await;
    assert!(relayed.contains("\"Broken\" did not finish: it could not start"), "{relayed}");
    let failed = item(&host, &bot, "Broken", "failed").await;
    assert!(failed["result"].as_str().unwrap().starts_with("it could not start"), "{failed}");

    std::fs::remove_file(no_start).unwrap();
    host.reply(&bot, "ADD Next|HOLD:next").await;
    item(&host, &bot, "Next", "working").await;
    host.call("retryWork", json!({"botId": bot, "itemId": id(&failed)})).await;
    assert_eq!(item(&host, &bot, "Broken", "waiting").await["result"], Value::Null);
}

#[tokio::test]
async fn a_deleted_bot_takes_its_items_and_tasks_with_it() {
    let host = Host::start().await;
    let bot = bot_with(&host, 2).await;
    host.reply(&bot, "ADD One|HOLD:one").await;
    host.reply(&bot, "ADD Two|HOLD:two").await;
    host.reply(&bot, "ADD Three|HOLD:three").await;
    let live = [item(&host, &bot, "One", "working").await, item(&host, &bot, "Two", "working").await];
    item(&host, &bot, "Three", "waiting").await;
    let mut events = host.events().await;

    host.call("deleteBot", json!({"botId": bot})).await;
    for item in live {
        let gone = host.refused("taskEntries", json!({"botId": bot, "taskId": item["task"]})).await;
        assert_eq!(gone, "unknown task");
    }
    no_task_bot_reached(&mut events).await;
}

#[tokio::test]
async fn stopping_the_turn_that_added_an_item_drops_it_before_it_begins() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "task_agent.py", "").await;
    let nonce = uuid::Uuid::new_v4().to_string();
    host.call("send", json!({"botId": bot, "text": "ADD Never|anything|hold", "clientNonce": nonce})).await;
    let never = item(&host, &bot, "Never", "waiting").await;
    assert_eq!(never["wait"], json!({"kind": "next"}), "it starts when the turn ends");

    host.call("stop", json!({"botId": bot})).await;
    wait("the item to go", async || work(&host, &bot).await.is_empty().then_some(())).await;
    assert_eq!(task_prompts(&host, &bot, "Never"), [] as [Value; 0]);
}

/// A routine of `bot`'s that does what `instruction` says (`task_agent.py`); its id.
async fn routine(host: &Host, bot: &str, name: &str, instruction: &str) -> String {
    let body = json!({"botId": bot, "name": name, "instruction": instruction, "triggers": [{"type": "webhook"}]});
    host.call("saveRoutine", body).await["routine"]["id"].as_str().unwrap().to_owned()
}

/// A run of `bot`'s routine `routine` that reads `status`, once there is one.
async fn run(host: &Host, bot: &str, routine: &str, status: &str) -> Value {
    wait(&format!("a run to be {status}"), async || {
        let runs = host.call("routines", json!({"botId": bot})).await["runs"].as_array().unwrap().clone();
        runs.into_iter().find(|run| run["routineId"] == routine && run["status"] == status)
    })
    .await
}

/// The chat's entries of `kind` whose text starts with `prefix`.
async fn said(host: &Host, bot: &str, kind: &str, prefix: &str) -> Vec<Value> {
    let starts = |e: &Value| e["kind"] == kind && e["data"]["text"].as_str().is_some_and(|t| t.starts_with(prefix));
    history(host, bot).await.into_iter().filter(starts).collect()
}

#[tokio::test]
async fn a_routine_run_is_an_item_and_its_last_message_is_the_routines_message_in_the_chat() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "hello").await;
    let digest = routine(&host, &bot, "Digest", "HOLD:digest PROGRESS ASK SEND NEST").await;
    let mut events = host.events().await;

    host.call("runRoutine", json!({"botId": bot, "id": digest})).await;
    let working = item(&host, &bot, "Digest", "working").await;
    assert_eq!(working["source"], json!({"kind": "routine", "id": digest}), "the row says where it came from");
    // Its line is in the chat as a run's is, and the chat itself stays free.
    let line = wait("the run's line", async || said(&host, &bot, "notice", "Routine · Digest").await.pop()).await;
    host.reply(&bot, "are you there?").await;
    // A second routine waits for the bot's one place, like any item.
    let second = routine(&host, &bot, "Second", "PASS").await;
    host.call("runRoutine", json!({"botId": bot, "id": second})).await;
    assert_eq!(item(&host, &bot, "Second", "waiting").await["wait"], json!({"kind": "next"}));

    release(&host, &bot, "digest");
    let result = reply(&host, &bot, "Routine done:").await;
    // What a run may not do in the chat it may not do here, and its progress note is its routine's.
    assert!(result.contains("Progress: {\"ok\":true"), "{result}");
    assert!(result.contains("Refused: request_decision works only in the current turn of your own chat"), "{result}");
    assert!(result.contains("Refused: send_message isn't available in this turn"), "{result}");
    assert!(result.contains("Refused: add_work isn't available in a task"), "{result}");
    let posted = said(&host, &bot, "agent", "Routine done:").await.remove(0);
    assert_eq!(posted["turn"], line["turn"], "in the turn its line took");
    assert_eq!((&posted["data"]["author"], &posted["data"]["final"]), (&json!(bot), &json!(true)));
    // The silent one ran next, and said nothing.
    run(&host, &bot, &second, "succeeded").await;
    assert_eq!(item(&host, &bot, "Second", "done").await["result"], Value::Null);
    let done = item(&host, &bot, "Digest", "done").await;
    assert!(done["result"].as_str().unwrap().starts_with("Routine done:"), "{done}");

    // The bot took no turn for either: nothing was relayed, and its run's prompt reached a task as a routine's.
    assert_eq!(relayed(&host, &bot).await, [] as [String; 0]);
    assert_eq!(said(&host, &bot, "agent", "Routine done:").await.len(), 1, "posted once");
    let prompts = host.log(&bot, "prompts.jsonl");
    let ran = prompts.iter().find(|p| p["text"].as_str().unwrap().contains("Scheduled routine: Digest")).unwrap();
    assert!(ran["text"].as_str().unwrap().contains("Progress note from this routine's earlier runs"), "{ran}");
    assert!(!ran["text"].as_str().unwrap().contains("[Task]"), "a run's own prompt, not an item's");
    // It starts as a run in the chat did: a new session with only its own prompt, though the agent can fork.
    assert_eq!(host.log(&bot, "forks.jsonl"), [] as [Value; 0], "a run never forks the chat");
    assert_ne!(ran["session"], prompts[0]["session"], "nor works in the chat's session");
    assert!(!ran["text"].as_str().unwrap().contains("[The chat you come from, so far]"), "{ran}");
    let tasks = no_task_bot_reached(&mut events).await;
    assert!(tasks.iter().any(|task| Some(task.as_str()) == done["task"].as_str()), "its trace is its task's");

    // The next run reads what this one saved.
    host.call("runRoutine", json!({"botId": bot, "id": digest})).await;
    wait("the second Digest run", async || {
        let prompts = host.log(&bot, "prompts.jsonl");
        prompts.iter().any(|p| p["text"].as_str().unwrap().contains("instructions):\nhandled 1")).then_some(())
    })
    .await;
}

#[tokio::test]
async fn a_routine_item_is_stopped_dropped_and_tried_again_as_its_routine_is() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "hello").await;
    let held = routine(&host, &bot, "Held", "HOLD:held").await;
    let later = routine(&host, &bot, "Later", "HOLD:later").await;

    // Stopping a run's item cancels the run, with no failure line.
    host.call("runRoutine", json!({"botId": bot, "id": held})).await;
    let working = item(&host, &bot, "Held", "working").await;
    host.call("runRoutine", json!({"botId": bot, "id": later})).await;
    let waiting = item(&host, &bot, "Later", "waiting").await;
    // A routine deleted while its run waits takes the run out of the box.
    host.call("deleteRoutine", json!({"botId": bot, "id": later})).await;
    wait("the deleted routine's run to leave", async || {
        work(&host, &bot).await.iter().all(|i| i["id"] != waiting["id"]).then_some(())
    })
    .await;
    host.call("stopWork", json!({"botId": bot, "itemId": id(&working)})).await;
    let stopped = item(&host, &bot, "Held", "stopped").await;
    assert_eq!(run(&host, &bot, &held, "cancelled").await["detail"], "Stopped by user");
    assert_eq!(said(&host, &bot, "notice", "Routine could not finish").await.len(), 0);

    // Try again runs the routine again: a new run and a new item, in the old one's place.
    host.call("retryWork", json!({"botId": bot, "itemId": id(&stopped)})).await;
    let again = item(&host, &bot, "Held", "working").await;
    assert_ne!(again["id"], stopped["id"]);
    assert_eq!(work(&host, &bot).await.iter().filter(|i| i["title"] == "Held").count(), 1);
    // It can't give way to another item, nor take a new brief: it would run twice.
    host.reply(&bot, "ADD Urgent|quick").await;
    let urgent = item(&host, &bot, "Urgent", "waiting").await;
    let refused =
        host.refused("workCutIn", json!({"botId": bot, "itemId": id(&urgent), "insteadOf": id(&again)})).await;
    assert_eq!(refused, "\"Held\" is a routine run or another bot's request: it can't give way, stop it instead");
    host.call("workCutIn", json!({"botId": bot, "itemId": id(&urgent)})).await;
    item(&host, &bot, "Held", "working").await;

    // A run whose agent can't start fails as a run in the chat does, and the box frees its place.
    release(&host, &bot, "held");
    item(&host, &bot, "Urgent", "done").await;
    let no_start = host.home.join("bots").join(&bot).join("workspace/no-start");
    std::fs::write(&no_start, "").unwrap();
    let broken = routine(&host, &bot, "Broken", "anything").await;
    host.call("runRoutine", json!({"botId": bot, "id": broken})).await;
    let failed = item(&host, &bot, "Broken", "failed").await;
    assert!(failed["result"].as_str().unwrap().starts_with("it could not start"), "{failed}");
    let run = run(&host, &bot, &broken, "failed").await;
    assert!(run["detail"].as_str().unwrap().starts_with("Could not start routine:"), "{run}");
    wait("the failure's line", async || said(&host, &bot, "notice", "Routine could not finish").await.pop()).await;
    std::fs::remove_file(no_start).unwrap();
}

#[tokio::test]
async fn a_routine_run_cut_off_by_a_host_restart_picks_up_and_posts_once() {
    let mut host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "hello").await;
    let held = routine(&host, &bot, "Held", "HOLD:held").await;
    host.call("runRoutine", json!({"botId": bot, "id": held})).await;
    let working = item(&host, &bot, "Held", "working").await;

    host.restart(|_| {}).await;
    // Its task resumes the turn; what that turn says is the routine's message, once.
    let result = reply(&host, &bot, "reply: [Codync restarted").await;
    assert!(result.contains("interrupted you mid-task"), "{result}");
    assert_eq!(item(&host, &bot, "Held", "done").await["task"], working["task"]);
    run(&host, &bot, &held, "succeeded").await;
    assert_eq!(said(&host, &bot, "agent", "reply: [Codync restarted").await.len(), 1);
    assert_eq!(said(&host, &bot, "notice", "Routine could not finish").await.len(), 0);
}

#[tokio::test]
async fn another_bots_request_is_an_item_and_its_last_message_is_the_reply() {
    let host = Host::start().await;
    let asker = host.fixture_bot("Ada", "task_agent.py", "").await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "hello").await;

    host.reply(&asker, &format!("MESSAGE {bot}|Review the diff HOLD:review ASK SEND FOLLOW NEST")).await;
    let working = item(&host, &bot, "Review the diff HOLD:review ASK SEND FOLLOW NEST", "working").await;
    assert_eq!(working["source"], json!({"kind": "request", "id": asker}));
    let notice = said(&host, &bot, "notice", "Message from Ada").await.remove(0);
    assert_eq!(notice["data"]["status"], "sent", "the exchange reads as running");
    // The asked bot's chat stays free while its task answers.
    host.reply(&bot, "are you there?").await;

    release(&host, &bot, "review");
    item(&host, &bot, "Review the diff HOLD:review ASK SEND FOLLOW NEST", "done").await;
    // The reply went to the bot that asked, as a turn of its own, and not into the asked bot's chat.
    let prompt = wait("the reply turn", async || {
        let prompts = host.log(&asker, "prompts.jsonl");
        let reply = prompts
            .iter()
            .find(|p| p["text"].as_str().unwrap().contains("[Codync: Mika replied to your message_bot request"));
        reply.map(|p| p["text"].as_str().unwrap().to_owned())
    })
    .await;
    assert!(prompt.contains("Request done:"), "{prompt}");
    // What a request's turn may not do in the chat it may not do here.
    assert!(prompt.contains("Refused: request_decision works only in the current turn of your own chat"), "{prompt}");
    assert!(prompt.contains("Refused: send_message isn't available in a bot request"), "{prompt}");
    assert!(prompt.contains("Refused: follow_up works only in your own chat"), "{prompt}");
    assert!(prompt.contains("Refused: add_work isn't available in a task"), "{prompt}");
    let notice = said(&host, &bot, "notice", "Message from Ada").await.remove(0);
    assert_eq!(notice["data"]["status"], "completed");
    assert!(notice["data"]["botMessage"]["reply"].as_str().unwrap().starts_with("Request done:"), "{notice}");
    assert_eq!(said(&host, &bot, "agent", "Request done:").await.len(), 0, "no bubble in the asked bot's chat");
    assert_eq!(relayed(&host, &bot).await, [] as [String; 0], "and no turn of its own for it");

    // A reply of exactly `(pass)` goes back as it is, as it did from a turn in the chat.
    host.reply(&asker, &format!("MESSAGE {bot}|Quiet PASS")).await;
    assert_eq!(item(&host, &bot, "Quiet PASS", "done").await["result"], "(pass)");
    wait("the passed reply", async || {
        let notices = said(&host, &bot, "notice", "Message from Ada").await;
        notices.iter().any(|n| n["data"]["botMessage"]["reply"] == "(pass)").then_some(())
    })
    .await;

    // A request stopped in the box is cancelled for the bot that asked; it can't be tried again.
    host.reply(&asker, &format!("MESSAGE {bot}|Second HOLD:second")).await;
    let second = item(&host, &bot, "Second HOLD:second", "working").await;
    host.call("stopWork", json!({"botId": bot, "itemId": id(&second)})).await;
    let stopped = item(&host, &bot, "Second HOLD:second", "stopped").await;
    wait("the exchange to read cancelled", async || {
        let notices = said(&host, &bot, "notice", "Message from Ada").await;
        notices.iter().any(|n| n["data"]["status"] == "cancelled").then_some(())
    })
    .await;
    let refused = host.refused("retryWork", json!({"botId": bot, "itemId": id(&stopped)})).await;
    assert_eq!(refused, "another bot's request can't be tried again: that bot asks again");
}

#[tokio::test]
async fn a_task_asks_the_user_to_choose_and_shares_in_the_chat() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "task_agent.py", "").await;
    host.reply(&bot, "hello").await;
    host.reply(&bot, "ADD Pick one|ASK then SHARE QUOTE and ARTIFACT").await;

    let card = wait("the task's choice card in the chat", async || {
        let mut cards = history(&host, &bot).await.into_iter().filter(|e| e["data"]["decision"] == true);
        cards.rfind(|e| e["data"]["status"] == "pending")
    })
    .await;
    let waits = item(&host, &bot, "Pick one", "needsYou").await;
    assert_eq!(card["data"]["item"], waits["id"], "the card names the item");
    assert_eq!(card["data"]["task"], json!({"id": waits["task"], "title": "Pick one"}));
    assert_eq!(card["data"]["author"], bot, "to the user it is the bot asking");
    assert!(!card["data"]["text"].as_str().unwrap().contains("Reply with"), "only a press answers it");
    // What the user types meanwhile is for the bot in the chat: it neither answers the card nor ends it.
    host.reply(&bot, "A").await;
    let still = host.call("history", json!({"botId": bot})).await;
    let pending = still["entries"].as_array().unwrap().iter().find(|e| e["id"] == card["id"]).unwrap();
    assert_eq!(pending["data"]["status"], "pending");

    host.call("respondDecision", json!({"entryId": card["id"], "option": "B"})).await;
    let relayed = reply(&host, &bot, "Relayed:").await;
    assert!(relayed.contains("Asked: {\"choice\":\"B\"}"), "{relayed}");
    assert!(relayed.contains("Shared: {\"sent\":true}"), "{relayed}");
    assert!(relayed.contains("Published: {"), "{relayed}");
    let chat = history(&host, &bot).await;
    let answered = chat.iter().find(|e| e["id"] == card["id"]).unwrap();
    assert_eq!((&answered["data"]["status"], &answered["data"]["selected"]), (&json!("answered"), &json!("B")));
    let choice = chat.iter().find(|e| e["kind"] == "user" && e["data"]["decisionId"] == card["id"]).unwrap();
    assert_eq!(choice["data"]["text"], "B", "the user's choice is recorded where they made it");

    // The file and the artifact's card are the bot's messages in the chat, named by the item.
    let file = chat.iter().find(|e| e["data"]["files"].is_array()).expect("the file's message");
    assert_eq!(
        (&file["kind"], &file["data"]["item"], &file["data"]["author"]),
        (&json!("agent"), &waits["id"], &json!(bot))
    );
    assert_eq!(file["data"]["files"][0]["name"], "shared.txt");
    // Its `reply_to` quotes a message of the bot's chat, where the file shows, not of the task's own (issue #402).
    let asked = chat
        .iter()
        .find(|e| e["kind"] == "user" && e["data"]["text"] == "ADD Pick one|ASK then SHARE QUOTE and ARTIFACT");
    assert_eq!(file["data"]["replyTo"]["id"], asked.expect("the message that added the item")["id"]);
    let stored = host.home.join("bots").join(&bot).join("files").join(file["data"]["files"][0]["id"].as_str().unwrap());
    assert!(stored.exists(), "kept with the bot, not with the task: {}", stored.display());
    let page = chat.iter().find(|e| !e["data"]["artifact"].is_null()).expect("the artifact's card");
    assert_eq!((&page["data"]["item"], &page["data"]["author"]), (&waits["id"], &json!(bot)));
    let library = host.call("library", json!({"botId": bot})).await;
    assert!(library.to_string().contains("Page"), "the artifact is in the bot's Library: {library}");
}

#[tokio::test]
async fn a_tasks_choice_card_ends_with_its_item() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "task_agent.py", "").await;
    host.reply(&bot, "ADD Pick one|ASK").await;
    let pending = async || {
        let mut cards = history(&host, &bot).await.into_iter().filter(|e| e["data"]["decision"] == true);
        cards.next_back()
    };
    let card = wait("the card", async || pending().await.filter(|e| e["data"]["status"] == "pending")).await;
    let waits = item(&host, &bot, "Pick one", "needsYou").await;

    // Stopping the chat's own turn is not the task's: the card still waits. Stopping the item ends it.
    host.call("stop", json!({"botId": bot})).await;
    assert_eq!(pending().await.unwrap()["data"]["status"], "pending");
    host.call("stopWork", json!({"botId": bot, "itemId": id(&waits)})).await;
    wait("the card to expire", async || pending().await.filter(|e| e["data"]["status"] == "expired")).await;
    let late = host.refused("respondDecision", json!({"entryId": card["id"], "option": "A"})).await;
    assert_eq!(late, "that card is already answered or expired");
}

#[tokio::test]
async fn a_tasks_choice_card_expires_with_its_270_second_call() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "task_agent.py", "").await;
    host.reply(&bot, "ADD Pick one|ASK").await;
    let card = wait("the card", async || {
        let mut cards = history(&host, &bot).await.into_iter().filter(|e| e["data"]["decision"] == true);
        cards.rfind(|e| e["data"]["status"] == "pending")
    })
    .await;
    // Unlike a chat's card it doesn't stay open: a press there has no next message to become.
    common::eventually("the task's card to expire at its deadline", Duration::from_secs(290), async || {
        history(&host, &bot).await.iter().any(|e| e["id"] == card["id"] && e["data"]["status"] == "expired")
    })
    .await;
    let relayed = reply(&host, &bot, "Relayed:").await;
    assert!(relayed.contains("Refused: the card expired after four and a half minutes"), "{relayed}");
}

#[tokio::test]
async fn a_tasks_edit_memory_is_its_own_turns_so_it_adds_and_never_takes_out() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.write_memory(Some(&bot), "MEMORY.md", "- Likes tea\n- Has two cats\n");
    host.reply(&bot, "hello").await;
    host.reply(&bot, "ADD Notes|HOLD:notes REMEMBER FORGET").await;
    item(&host, &bot, "Notes", "working").await;
    // The chat runs a turn the user started, which may take lines out; the task's turn is its own, and may not.
    let nonce = uuid::Uuid::new_v4().to_string();
    host.call("send", json!({"botId": bot, "text": "ADD Other|x|hold", "clientNonce": nonce})).await;
    item(&host, &bot, "Other", "waiting").await;
    release(&host, &bot, "notes");
    let done = item(&host, &bot, "Notes", "done").await;
    let result = done["result"].as_str().unwrap();
    assert!(result.contains("Remembered: {\"status\":\"saved\""), "{result}");
    assert!(result.contains("Refused: Only in a turn the user started in your own chat"), "{result}");
    let main = host.memory_folder(Some(&bot)).join("MEMORY.md");
    assert_eq!(std::fs::read_to_string(main).unwrap(), "- Likes tea\n- Has two cats\n- Works at night\n");
    // The line it added is the bot's, as one added from the chat is.
    let detail = host.call("memoryFile", json!({"botId": bot, "file": "MEMORY.md"})).await;
    assert_eq!(detail["versions"][0]["author"], "bot", "{detail}");
    // It was written in the task's session, not the chat's: the chat hears of it with its next message, which is
    // the report's turn.
    host.call("stop", json!({"botId": bot})).await;
    let heard = relayed_for(&host, &bot, "Notes").await;
    assert!(heard.contains("<memory_update>") && heard.contains("- Works at night"), "{heard}");
}

/// git in `dir` alone: a pre-commit hook running these tests sets `GIT_DIR` and `GIT_INDEX_FILE` for the commit
/// being made, which would point every git here at the repository under test's own checkout. The host's own
/// (`host/src/git.rs` `command`) clears the same; this test can't reach it.
fn git_in(dir: &Path) -> std::process::Command {
    let mut cmd = std::process::Command::new("git");
    for (key, _) in std::env::vars_os().filter(|(k, _)| k.to_string_lossy().starts_with("GIT_")) {
        cmd.env_remove(key);
    }
    cmd.current_dir(dir);
    cmd
}

/// Makes `dir` a git repository with one commit, where the fixture's own files are ignored. False where git isn't
/// installed.
fn git_repo(dir: &Path) -> bool {
    std::fs::write(dir.join(".gitignore"), "*.jsonl\nrelease-*\n").unwrap();
    std::fs::write(dir.join("README.md"), "# Repo\n").unwrap();
    let git = |args: &[&str]| git_in(dir).args(args).output().is_ok_and(|out| out.status.success());
    git(&["init", "-q"])
        && git(&["add", "."])
        && git(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "first"])
}

/// A bot like [`bot_with`]'s that works in a project folder of its own (not its own folder, where tasks never get
/// worktrees), and that folder.
async fn project_bot(host: &Host, agents: u8) -> (String, PathBuf) {
    let dir = host.home.join("projects").join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&dir).unwrap();
    let bot = bot_with(host, agents).await;
    host.call("updateBot", json!({"id": bot, "cwd": dir})).await;
    (bot, dir)
}

/// The bot's row as a client holds it.
async fn row(host: &Host, bot: &str) -> Value {
    host.snapshot().await["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot).unwrap().clone()
}

/// The bot's working item called `title`, once it has a worktree.
async fn in_worktree(host: &Host, bot: &str, title: &str) -> Value {
    wait(&format!("{title}'s worktree"), async || {
        let found = work(host, bot).await.into_iter().find(|i| i["title"] == title && i["status"] == "working");
        found.filter(|i| i["worktree"].is_object())
    })
    .await
}

/// The chat's `Relayed:` reply for the item called `title`, once it is there.
async fn relayed_for(host: &Host, bot: &str, title: &str) -> String {
    let report = format!("Report {title}:");
    wait(&format!("{title}'s report"), async || relayed(host, bot).await.into_iter().find(|t| t.contains(&report)))
        .await
}

#[tokio::test]
async fn a_task_beside_another_in_one_repository_works_in_a_worktree_of_its_own() {
    let host = Host::start().await;
    let (bot, dir) = project_bot(&host, 3).await;
    assert_eq!(row(&host, &bot).await["gitRepo"], false);
    if !git_repo(&dir) {
        return;
    }
    host.reply(&bot, "hello").await;
    let settings = row(&host, &bot).await;
    assert_eq!((&settings["gitRepo"], &settings["taskWorktrees"]), (&json!(true), &json!(true)), "on unless set");
    let branches = || {
        let out = git_in(&dir).args(["branch", "--list", "codync/*"]).output();
        String::from_utf8(out.unwrap().stdout).unwrap()
    };

    // The first task alone keeps the bot's folder; each one that starts beside it gets a worktree and a branch.
    host.reply(&bot, "ADD First|HOLD:first").await;
    let first = item(&host, &bot, "First", "working").await;
    host.reply(&bot, "ADD Second task|HOLD:second").await;
    let second = in_worktree(&host, &bot, "Second task").await;
    host.reply(&bot, "ADD Third|TOUCH HOLD:third").await;
    let third = in_worktree(&host, &bot, "Third").await;
    assert!(item(&host, &bot, "First", "working").await["worktree"].is_null(), "{first}");
    let folder = |item: &Value| PathBuf::from(item["worktree"]["path"].as_str().unwrap());
    assert_eq!(second["worktree"]["branch"], "codync/second-task");
    assert_eq!(folder(&second), host.home.join("worktrees").join(&bot).join(id(&second)));
    assert_eq!(third["worktree"]["branch"], "codync/third");
    // Its agent runs there, from the folder's commit, and was told so.
    assert!(folder(&second).join("README.md").exists());
    let told = wait("the task's prompt in its worktree", async || {
        std::fs::read_to_string(folder(&second).join("prompts.jsonl")).ok()
    })
    .await;
    assert!(told.contains("you work in a git worktree of your own") && told.contains("codync/second-task"), "{told}");
    assert_eq!(task_prompts_in(&dir, "Second task"), [] as [Value; 0], "not in the bot's folder");
    assert_eq!(logged(&dir, "forks.jsonl").len(), 1, "only the first forked: a worktree's task starts anew");

    // Nothing changed in it: the worktree and its branch go, and the report says so.
    std::fs::write(folder(&second).join("release-second"), "").unwrap();
    let report = relayed_for(&host, &bot, "Second task").await;
    let done = item(&host, &bot, "Second task", "done").await;
    // On Windows a folder a process is still in can't be removed: the worktree stays and the report says where.
    if cfg!(windows) {
        assert!(report.contains("branch codync/second-task") && report.contains("nothing was merged"), "{report}");
    } else {
        let removed = "had no changes and no commits your folder doesn't have, and was removed with its branch";
        assert!(report.contains(removed), "{report}");
        assert!(done["worktree"].is_null() && !folder(&second).exists(), "{done}");
        assert!(!branches().contains("codync/second-task"), "{}", branches());
    }

    // A file it made: both stay, and the report names the branch and the folder for the bot to tell the user.
    std::fs::write(folder(&third).join("release-third"), "").unwrap();
    let report = relayed_for(&host, &bot, "Third").await;
    let place = format!("branch codync/third, in {}", folder(&third).display());
    assert!(report.contains(&place) && report.contains("nothing was merged"), "{report}");
    assert_eq!(item(&host, &bot, "Third", "done").await["worktree"], third["worktree"]);
    assert!(folder(&third).join("made.txt").exists() && branches().contains("codync/third"));
    assert!(!dir.join("made.txt").exists(), "not in the bot's folder");

    // With the switch off, a task beside another works in the bot's folder as before.
    host.call("updateBot", json!({"id": bot, "taskWorktrees": false})).await;
    host.reply(&bot, "ADD Plain|HOLD:plain").await;
    let plain = item(&host, &bot, "Plain", "working").await;
    wait("its prompt in the bot's folder", async || task_prompts_in(&dir, "Plain").pop()).await;
    assert!(item(&host, &bot, "Plain", "working").await["worktree"].is_null(), "{plain}");
    for name in ["plain", "first"] {
        std::fs::write(dir.join(format!("release-{name}")), "").unwrap();
    }
    item(&host, &bot, "First", "done").await;

    // A project folder that is no repository never gets one, and neither does a bot's own folder, a repository
    // Codync keeps versions in: all the bot's work stays there.
    let (plain, plain_dir) = project_bot(&host, 3).await;
    let own = bot_with(&host, 3).await;
    wait("its own folder to keep versions", async || (row(&host, &own).await["gitRepo"] == true).then_some(())).await;
    for (bot, dir) in [(&plain, plain_dir), (&own, host.workspace(&own))] {
        host.reply(bot, "ADD One|HOLD:one").await;
        item(&host, bot, "One", "working").await;
        host.reply(bot, "ADD Two|HOLD:two").await;
        item(&host, bot, "Two", "working").await;
        wait("its prompt in the bot's folder", async || task_prompts_in(&dir, "Two").pop()).await;
        assert!(work(&host, bot).await.iter().all(|i| i["worktree"].is_null()));
    }
    assert_eq!(row(&host, &plain).await["gitRepo"], false);
}

#[tokio::test]
async fn a_bots_own_folder_keeps_a_version_before_and_after_a_turn_and_a_project_folder_none() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "task_agent.py", "").await;
    let dir = host.workspace(&bot);
    if !git_in(&dir).arg("--version").output().is_ok_and(|out| out.status.success()) {
        return;
    }
    let log = || {
        let out = git_in(&dir).args(["log", "--format=%s"]).output().unwrap();
        String::from_utf8(out.stdout).unwrap().lines().map(str::to_owned).collect::<Vec<_>>()
    };
    let made = "Codync keeps versions of this folder";
    wait("its own folder to become a repository", async || (log() == [made]).then_some(())).await;

    // What changed since is saved before the turn, and what the turn changed (its agent's log) after it.
    std::fs::write(dir.join("notes.md"), "before the turn").unwrap();
    let (project, project_dir) = project_bot(&host, 1).await;
    host.reply(&project, "Tidy up\nthe notes").await;
    host.reply(&bot, "Tidy up\nthe notes").await;
    let saved = wait("the turn's versions", async || Some(log()).filter(|log| log.len() == 3)).await;
    assert_eq!(saved, ["Tidy up", "Before: Tidy up", made]);
    assert!(!project_dir.join(".git").exists(), "a project folder's history is the user's");
}

/// A bot on `task_agent.py` run as Kimi Code, a harness Codync knows (so a sign-in it needs gets a card) that no
/// machine running the tests has installed, with `fallbacks`.
async fn kimi_bot(host: &Host, name: &str, fallbacks: Value) -> String {
    let bot = json!({"name": name, "backend": "kimi", "command": common::fixture_command("task_agent.py", ""),
        "connectors": [], "notify": false, "fallbacks": fallbacks});
    host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned()
}

/// The fixture agent `relay_agent.py <name>`: it answers "<name> answered" and logs its prompts to `<name>.jsonl`.
fn relay_agent(name: &str) -> String {
    common::fixture_command("relay_agent.py", name)
}

/// Writes the flag file `name` in `bot`'s folder (see `task_agent.py`).
fn flag(host: &Host, bot: &str, name: &str) -> PathBuf {
    let path = host.workspace(bot).join(name);
    std::fs::write(&path, "").unwrap();
    path
}

/// The newest entry of `bot`'s chat that `found` picks, once it is there.
async fn entry(host: &Host, bot: &str, what: &str, found: impl Fn(&Value) -> bool) -> Value {
    wait(what, async || history(host, bot).await.into_iter().rev().find(|e| found(e))).await
}

/// The transcript of `item`'s task.
async fn transcript(host: &Host, bot: &str, item: &Value) -> Vec<Value> {
    let entries = host.call("taskEntries", json!({"botId": bot, "taskId": item["task"]})).await;
    entries["entries"].as_array().unwrap().clone()
}

#[tokio::test]
async fn a_tasks_sign_in_is_asked_in_the_chat_and_its_item_goes_on_in_the_same_task_once_signed_in() {
    let host = Host::start().await;
    let bot = kimi_bot(&host, "Mika", json!([])).await;
    host.reply(&bot, "hello").await;
    let signed_out = flag(&host, &bot, "task-signedout");

    host.reply(&bot, "ADD Login|anything").await;
    let waiting = item(&host, &bot, "Login", "needsYou").await;
    assert_eq!(waiting["activity"], "Needs Kimi Code sign-in");
    // The card is the user's to press, so it is in the chat, marked as the item's, holding its message.
    let card = entry(&host, &bot, "the sign-in card", |e| e["data"]["item"] == waiting["id"]).await;
    let request = &card["data"]["connectionRequest"];
    assert_eq!((request["kind"].as_str(), request["status"].as_str()), (Some("agent"), Some("pending")), "{card}");
    assert_eq!(request["botId"], bot, "the chat's bot, which the apps know");
    assert_eq!(request["entryIds"].as_array().unwrap().len(), 1, "{card}");
    assert_eq!(card["data"]["task"], json!({"id": waiting["task"], "title": "Login"}));
    // The task's own transcript only says where the ask went.
    let asked = transcript(&host, &bot, &waiting).await;
    assert!(asked.iter().any(|e| e["data"]["text"] == "Asked in the chat to sign in to Kimi Code."), "{asked:?}");
    assert!(!asked.iter().any(|e| e["data"]["connectionRequest"].is_object()), "{asked:?}");
    assert_eq!(relayed(&host, &bot).await, [] as [String; 0], "the item isn't over: no report");

    // Signed in (a turn of the agent that ends well says so: here the chat's own), the item's message goes out again
    // to the same task.
    std::fs::remove_file(signed_out).unwrap();
    host.reply(&bot, "hello again").await;
    let done = item(&host, &bot, "Login", "done").await;
    assert_eq!(done["task"], waiting["task"], "the same task");
    // A new agent process (the old one kept the old sign-in) goes on in the task's session: its fork of the chat
    // and what it did are still there.
    let tries = task_prompts(&host, &bot, "Login");
    assert_eq!(tries.len(), 2, "{tries:?}");
    assert_eq!(tries[0]["session"], tries[1]["session"], "{tries:?}");
    assert!(reply(&host, &bot, "Relayed:").await.contains("Login"));
    let card = entry(&host, &bot, "the card", |e| e["id"] == card["id"]).await;
    assert_eq!(card["data"]["connectionRequest"]["status"], "ready");
}

#[tokio::test]
async fn a_tasks_sign_in_on_an_agent_that_cant_reopen_a_session_tells_the_new_one_the_chat() {
    let host = Host::start().await;
    let command = common::fixture_command("task_agent.py", "no-load");
    let bot = json!({"name": "Mika", "backend": "kimi", "command": command, "connectors": [], "notify": false});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    host.reply(&bot, "Remember the blue theme").await;
    let signed_out = flag(&host, &bot, "task-signedout");

    host.reply(&bot, "ADD Login|anything").await;
    item(&host, &bot, "Login", "needsYou").await;
    std::fs::remove_file(signed_out).unwrap();
    host.reply(&bot, "hello again").await;
    item(&host, &bot, "Login", "done").await;
    // A new agent process can't take the task's session up again: a new one is told the chat it comes from.
    let tries = task_prompts(&host, &bot, "Login");
    assert_eq!(tries.len(), 2, "{tries:?}");
    assert_ne!(tries[0]["session"], tries[1]["session"], "{tries:?}");
    let text = tries[1]["text"].as_str().unwrap();
    assert!(text.contains("[The chat you come from, so far]") && text.contains("blue theme"), "{text}");
}

#[tokio::test]
async fn a_task_moves_past_a_signed_out_agent_asks_in_the_chat_and_its_next_agent_hears_the_chat() {
    let host = Host::start().await;
    let bot = kimi_bot(&host, "Mika", json!([{"backend": "codex", "command": relay_agent("standin")}])).await;
    host.reply(&bot, "Remember the blue theme").await;
    flag(&host, &bot, "task-signedout");

    host.reply(&bot, "ADD Login|anything").await;
    let done = item(&host, &bot, "Login", "done").await;
    assert_eq!(done["result"], "standin answered");
    // Nothing waits on the sign-in, and still the user is asked where they are.
    let card = entry(&host, &bot, "the sign-in card", |e| e["data"]["item"] == done["id"]).await;
    assert_eq!(card["data"]["connectionRequest"]["entryIds"], json!([]), "{card}");
    let moved = transcript(&host, &bot, &done).await;
    let texts: Vec<&str> = moved.iter().filter_map(|e| e["data"]["text"].as_str()).collect();
    assert!(texts.contains(&"Kimi Code is signed out. Codex carried on."), "{texts:?}");
    assert!(texts.contains(&"Asked in the chat to sign in to Kimi Code."), "{texts:?}");
    // The fork that carried the chat stays with the agent that failed: the next one is told the chat.
    let log = host.log(&bot, "standin.jsonl");
    let prompt = log.iter().rfind(|r| r["method"] == "session/prompt").unwrap()["params"]["prompt"][0]["text"].clone();
    let prompt = prompt.as_str().unwrap();
    assert!(prompt.contains("[The chat you come from, so far]") && prompt.contains("blue theme"), "{prompt}");
}

#[tokio::test]
async fn a_tasks_offer_to_add_a_fallback_is_in_the_chat_and_new_fallbacks_reach_a_running_task() {
    let host = Host::start().await;
    let bot = bot_with(&host, 2).await;
    host.reply(&bot, "hello").await;
    let limited = flag(&host, &bot, "task-limit");

    host.reply(&bot, "ADD Big|anything").await;
    let failed = item(&host, &bot, "Big", "failed").await;
    let offer = entry(&host, &bot, "the offer", |e| e["data"]["addFallback"] == true).await;
    assert_eq!((&offer["data"]["item"], &offer["data"]["author"]), (&failed["id"], &json!(bot)), "{offer}");
    assert!(offer["data"]["text"].as_str().unwrap().contains("You've hit your limit"), "{offer}");
    assert!(offer["data"]["stopReason"].is_null(), "it ends no turn of the bot's own: {offer}");
    let kept = transcript(&host, &bot, &failed).await;
    let error = kept.iter().find(|e| e["data"]["style"] == "error").unwrap();
    assert!(error["data"]["addFallback"].is_null(), "the task's transcript keeps only the failure: {error}");

    // Fallbacks set while a task works reach it: it falls back like the bot.
    std::fs::remove_file(&limited).unwrap();
    host.reply(&bot, "ADD Slow|HOLD:slow").await;
    item(&host, &bot, "Slow", "working").await;
    let fallbacks = json!([{"backend": "codex", "command": relay_agent("standin")}]);
    host.call("updateBot", json!({"id": bot, "fallbacks": fallbacks})).await;
    flag(&host, &bot, "task-limit");
    release(&host, &bot, "slow");
    assert_eq!(item(&host, &bot, "Slow", "done").await["result"], "standin answered");
}

#[tokio::test]
async fn a_task_that_starts_on_a_fallback_never_forks_the_chats_own_agents_session() {
    let host = Host::start().await;
    // The fallback can fork too (it would fork any session it's given).
    let fallbacks = json!([{"backend": "codex", "command": common::fixture_command("task_agent.py", "")}]);
    let bot = kimi_bot(&host, "Mika", fallbacks).await;
    host.reply(&bot, "hello").await;
    // Another bot on the same login finds it signed out: it rests, for the whole computer.
    let other = kimi_bot(&host, "Other", json!([])).await;
    flag(&host, &other, "signedout");
    host.call("send", json!({"botId": other, "text": "hi", "clientNonce": "n1"})).await;
    wait("the login to rest", async || {
        let snapshot = host.snapshot().await;
        let row = snapshot["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot).cloned().unwrap();
        row["resting"].as_array().is_some_and(|r| !r.is_empty()).then_some(())
    })
    .await;

    // A request starts an item with no turn of the chat's in between: the chat is still on its own agent.
    let asker = host.fixture_bot("Ada", "task_agent.py", "").await;
    host.reply(&asker, &format!("MESSAGE {bot}|Review the diff")).await;
    item(&host, &bot, "Review the diff", "done").await;
    let forks = host.workspace(&bot).join("forks.jsonl");
    assert!(!forks.exists(), "{}", std::fs::read_to_string(&forks).unwrap_or_default());
}

/// Writes `text` to the thread of `item`: what the host answers.
async fn tell(host: &Host, bot: &str, item: &Value, text: &str) -> Value {
    let nonce = uuid::Uuid::new_v4().to_string();
    host.call("send", json!({"botId": bot, "itemId": item["id"], "text": text, "clientNonce": nonce})).await
}

/// The entry `id` of the thread of `item`, as its transcript holds it; none once the user took it back.
async fn thread_entry(host: &Host, bot: &str, item: &Value, id: &Value) -> Option<Value> {
    transcript(host, bot, item).await.into_iter().find(|e| e["id"] == *id)
}

/// The entry `id` of `item`'s thread, once it reads `status`.
async fn thread_status(host: &Host, bot: &str, item: &Value, id: &Value, status: &str) -> Value {
    wait(&format!("the message to be {status}"), async || {
        thread_entry(host, bot, item, id).await.filter(|e| e["data"]["status"] == status)
    })
    .await
}

/// The prompts a task of `bot` got for what the user wrote to its thread, oldest first.
fn thread_prompts(host: &Host, bot: &str) -> Vec<Value> {
    let written = |p: &Value| p["text"].as_str().is_some_and(|t| t.contains("in this work's thread"));
    host.log(bot, "prompts.jsonl").into_iter().filter(written).collect()
}

/// The bot's item called `title`, once its result starts with `prefix`.
async fn result(host: &Host, bot: &str, title: &str, prefix: &str) -> Value {
    wait(&format!("{title} to report {prefix}"), async || {
        let found = work(host, bot).await.into_iter().find(|i| i["title"] == title);
        found.filter(|i| i["result"].as_str().is_some_and(|r| r.starts_with(prefix)))
    })
    .await
}

#[tokio::test]
async fn a_message_to_a_working_thread_waits_its_turn_and_the_item_reports_once_with_the_users_words() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD Slow|HOLD:slow").await;
    let slow = item(&host, &bot, "Slow", "working").await;
    assert_eq!(slow["talk"], true, "the bot's own work, whose task is there");
    assert!(slow["folder"].as_str().unwrap().ends_with("workspace"), "where its task works: {slow}");
    let mut events = host.events().await;

    // Its turn runs: the message waits in the thread, as one does in a chat.
    let sent = tell(&host, &bot, &slow, "also the tests").await;
    assert_eq!((&sent["reopened"], &sent["queued"]), (&json!(false), &json!(true)));
    let entry = &sent["entry"];
    assert_eq!((&entry["kind"], &entry["botId"]), (&json!("user"), &slow["task"]), "an entry of its transcript");
    assert_eq!((&entry["data"]["text"], &entry["data"]["status"]), (&json!("also the tests"), &json!("queued")));
    let nonce = &entry["data"]["clientNonce"];
    let twice = json!({"botId": bot, "itemId": slow["id"], "text": "also the tests", "clientNonce": nonce});
    assert_eq!(host.call("send", twice).await["entry"]["id"], entry["id"], "sent twice, it is one message");

    // The caller names the bot and its item, never a task; text only.
    let other = host.fixture_bot("Other", "task_agent.py", "").await;
    let (thread, id) = (&slow["id"], &entry["id"]);
    for (method, body, why) in [
        ("send", json!({"botId": bot, "itemId": "nope", "text": "x"}), "no such work"),
        ("send", json!({"botId": other, "itemId": thread, "text": "x"}), "no such work"),
        ("send", json!({"botId": slow["task"], "text": "x"}), "unknown bot"),
        ("send", json!({"botId": bot, "itemId": thread, "text": ""}), "empty message"),
        ("send", json!({"botId": bot, "itemId": thread, "text": "x", "replyTo": id}), "a thread takes text only"),
        ("steer", json!({"botId": other, "itemId": thread, "entryId": id}), "no such work"),
        ("unqueue", json!({"botId": slow["task"], "entryId": id}), "unknown bot"),
    ] {
        assert_eq!(host.refused(method, body.clone()).await, why, "{method} {body}");
    }

    // Removed, a waiting message never goes out and leaves the transcript.
    let second = tell(&host, &bot, &slow, "and the docs").await["entry"].clone();
    host.call("unqueue", json!({"botId": bot, "itemId": thread, "entryId": second["id"]})).await;
    wait("the removed message to go", async || {
        thread_entry(&host, &bot, &slow, &second["id"]).await.is_none().then_some(())
    })
    .await;

    // The item's own turn ends: the item isn't over while a message waits. That message's turn is its end, in the
    // session that did the work, and its one report ends with the user's words.
    release(&host, &bot, "slow");
    let done = result(&host, &bot, "Slow", "Thread").await;
    assert_eq!(done["result"], "Thread also the tests: same session");
    assert_eq!((&done["status"], &done["task"]), (&json!("done"), &slow["task"]));
    let relayed_once = reply(&host, &bot, "Relayed:").await;
    assert!(relayed_once.starts_with("Relayed: Thread also the tests: same session"), "{relayed_once}");
    let words = "the user wrote to it directly. These are the user's own words, not the task's: «also the tests»]";
    assert!(relayed_once.contains(words), "{relayed_once}");
    assert!(!relayed_once.contains("and the docs"), "what was taken back was never said: {relayed_once}");
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(relayed(&host, &bot).await.len(), 1, "the turn before the message reported nothing");

    let prompts = thread_prompts(&host, &bot);
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0]["session"], task_prompts(&host, &bot, "Slow")[0]["session"]);
    let text = prompts[0]["text"].as_str().unwrap();
    assert!(text.contains("also the tests\n\n[Codync: the user wrote this to you directly"), "{text}");
    assert_eq!(thread_status(&host, &bot, &slow, id, "sent").await["data"]["text"], "also the tests");
    let tasks = no_task_bot_reached(&mut events).await;
    assert!(tasks.iter().any(|task| slow["task"] == task.as_str()), "the thread's entries are taskEntry events");
}

#[tokio::test]
async fn send_now_stops_the_threads_turn_for_the_message_or_joins_it_where_the_agent_can() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD Slow|HOLD:slow").await;
    let slow = item(&host, &bot, "Slow", "working").await;
    let sent = tell(&host, &bot, &slow, "do this instead").await;
    host.call("steer", json!({"botId": bot, "itemId": slow["id"], "entryId": sent["entry"]["id"]})).await;
    // Never released: its turn was stopped for the message, whose turn ends the item.
    let done = result(&host, &bot, "Slow", "Thread").await;
    assert_eq!((&done["status"], &done["result"]), (&json!("done"), &json!("Thread do this instead: same session")));
    let transcript = host.call("taskEntries", json!({"botId": bot, "taskId": slow["task"]})).await;
    let stopped = transcript["entries"].as_array().unwrap().iter().any(|e| e["data"]["text"] == "Stopped.");
    assert!(!stopped, "no Stopped. line under the message it stopped for: {transcript}");

    // An agent that takes a message into its running turn: the turn goes on, with it.
    let steers = host.fixture_bot("Tess", "task_agent.py", "steer").await;
    host.reply(&steers, "ADD Slow|HOLD:slow").await;
    let slow = item(&host, &steers, "Slow", "working").await;
    let sent = tell(&host, &steers, &slow, "use tabs").await;
    host.call("steer", json!({"botId": steers, "itemId": slow["id"], "entryId": sent["entry"]["id"]})).await;
    thread_status(&host, &steers, &slow, &sent["entry"]["id"], "sent").await;
    assert_eq!(item(&host, &steers, "Slow", "working").await["task"], slow["task"]);
    release(&host, &steers, "slow");
    let done = item(&host, &steers, "Slow", "done").await;
    assert_eq!(done["result"], "Report Slow: forked; no chat; joined use tabs");
    let relayed = reply(&host, &steers, "Relayed:").await;
    assert!(relayed.contains("not the task's: «use tabs»]"), "{relayed}");
}

#[tokio::test]
async fn a_message_to_an_ended_item_reopens_it_and_it_reports_again() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD Quick|look around").await;
    assert_eq!(reply(&host, &bot, "Relayed:").await, "Relayed: Report Quick: forked; no chat");
    let done = item(&host, &bot, "Quick", "done").await;
    assert_eq!(done["talk"], true, "its task is kept with it");

    // It works again in the task it had, timed from now, and what a task may not do it still may not.
    let sent = tell(&host, &bot, &done, "one more thing HOLD:again NEST").await;
    assert_eq!((&sent["reopened"], &sent["queued"]), (&json!(true), &json!(false)));
    let again = item(&host, &bot, "Quick", "working").await;
    assert_eq!(again["task"], done["task"]);
    assert!(again["startedAt"].as_i64() > done["startedAt"].as_i64(), "{again}");
    assert!(again["endedAt"].is_null() && again["result"].is_null(), "{again}");
    // The user asked, so it runs whatever the ceiling; nothing else starts while the bot is over it.
    host.reply(&bot, "ADD Other|quick").await;
    assert_eq!(item(&host, &bot, "Other", "waiting").await["wait"], json!({"kind": "next"}));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(item(&host, &bot, "Quick", "working").await["task"], done["task"], "nothing ended it meanwhile");

    release(&host, &bot, "again");
    let over = result(&host, &bot, "Quick", "Thread").await;
    let said = over["result"].as_str().unwrap();
    assert!(said.starts_with("Thread one more thing HOLD:again NEST: same session; Refused: add_work isn't"), "{said}");
    assert_eq!(over["status"], "done");
    item(&host, &bot, "Other", "done").await;
    wait("every report", async || (relayed(&host, &bot).await.len() == 3).then_some(())).await;
    let reports = relayed(&host, &bot).await;
    let second = reports.iter().find(|r| r.starts_with("Relayed: Thread")).expect("it reported again");
    assert!(second.contains("not the task's: «one more thing HOLD:again NEST»]"), "{second}");
    assert_eq!(thread_prompts(&host, &bot)[0]["session"], task_prompts(&host, &bot, "Quick")[0]["session"]);

    // An agent that can't pick a session up again: a new one, told the work and what it reported.
    let fresh = host.fixture_bot("Nell", "task_agent.py", "no-load").await;
    host.reply(&fresh, "ADD Quick|look around").await;
    reply(&host, &fresh, "Relayed:").await;
    let done = item(&host, &fresh, "Quick", "done").await;
    assert_eq!(tell(&host, &fresh, &done, "and now?").await["reopened"], true);
    assert_eq!(result(&host, &fresh, "Quick", "Thread").await["result"], "Thread and now?: picked up");
    let prompt = thread_prompts(&host, &fresh).remove(0);
    assert_ne!(prompt["session"], task_prompts(&host, &fresh, "Quick")[0]["session"]);
    let text = prompt["text"].as_str().unwrap();
    assert!(text.contains("You worked on \"Quick\" before") && text.contains("look around"), "{text}");
    assert!(text.contains("[Codync: what you reported when it ended:]\nReport Quick: forked; no chat"), "{text}");
    assert!(text.contains("and now?\n\n[Codync: the user wrote this to you directly"), "{text}");
}

#[tokio::test]
async fn a_routines_run_takes_no_message_and_a_message_never_answers_a_threads_card() {
    let host = Host::start().await;
    let bot = bot_with(&host, 2).await;
    host.reply(&bot, "hello").await;
    let digest = routine(&host, &bot, "Digest", "HOLD:digest").await;
    host.call("runRoutine", json!({"botId": bot, "id": digest})).await;
    let run = item(&host, &bot, "Digest", "working").await;
    assert_eq!(run["talk"], false, "its result has an owner already");
    let refused = host.refused("send", json!({"botId": bot, "itemId": run["id"], "text": "hi"})).await;
    assert_eq!(refused, "\"Digest\" is a routine's run: it can be read and stopped, not written to");
    release(&host, &bot, "digest");

    // A thread that waits on a card: the message waits behind it, and only a press answers the card.
    host.reply(&bot, "ADD Pick one|ASK").await;
    let pending = async || {
        let mut cards = history(&host, &bot).await.into_iter().filter(|e| e["data"]["decision"] == true);
        cards.next_back()
    };
    let card = wait("the card", async || pending().await.filter(|e| e["data"]["status"] == "pending")).await;
    let waits = item(&host, &bot, "Pick one", "needsYou").await;
    let sent = tell(&host, &bot, &waits, "B").await;
    assert_eq!(sent["queued"], true);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(pending().await.unwrap()["data"]["status"], "pending", "a message neither answers nor ends it");
    assert_eq!(thread_status(&host, &bot, &waits, &sent["entry"]["id"], "queued").await["data"]["text"], "B");

    host.call("respondDecision", json!({"entryId": card["id"], "option": "A"})).await;
    assert_eq!(result(&host, &bot, "Pick one", "Thread").await["result"], "Thread B: same session");
    assert_eq!(pending().await.unwrap()["data"]["selected"], "A");
}

#[tokio::test]
async fn stopping_an_item_drops_what_waited_in_its_thread_and_a_host_restart_fails_it() {
    let mut host = Host::start().await;
    let bot = bot_with(&host, 2).await;
    host.reply(&bot, "ADD One|HOLD:one").await;
    let one = item(&host, &bot, "One", "working").await;
    let waited = tell(&host, &bot, &one, "never mind").await;
    host.call("stopWork", json!({"botId": bot, "itemId": one["id"]})).await;
    item(&host, &bot, "One", "stopped").await;
    thread_status(&host, &bot, &one, &waited["entry"]["id"], "cancelled").await;

    // A restart: the cut-off turn is picked up, the message that waited is not (as in a chat), and the item's
    // report holds none of it. The thread still takes a message afterwards.
    host.reply(&bot, "ADD Long|HOLD:long").await;
    let long = item(&host, &bot, "Long", "working").await;
    let waited = tell(&host, &bot, &long, "after the restart?").await;
    host.restart(|_| {}).await;
    thread_status(&host, &bot, &long, &waited["entry"]["id"], "failed").await;
    let relayed = reply(&host, &bot, "Relayed:").await;
    assert!(relayed.contains("Codync restarted") && !relayed.contains("after the restart?"), "{relayed}");
    let done = item(&host, &bot, "Long", "done").await;
    assert_eq!(tell(&host, &bot, &done, "still there?").await["reopened"], true);
    assert_eq!(result(&host, &bot, "Long", "Thread").await["result"], "Thread still there?: same session");
}

#[tokio::test]
async fn what_waited_in_a_thread_goes_with_an_item_that_gives_way() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD Slow|HOLD:slow").await;
    let slow = item(&host, &bot, "Slow", "working").await;
    host.reply(&bot, "ADD Urgent|quick").await;
    let urgent = item(&host, &bot, "Urgent", "waiting").await;
    let refused = host.refused("send", json!({"botId": bot, "itemId": urgent["id"], "text": "x"})).await;
    assert_eq!(refused, "\"Urgent\" hasn't started yet: there is nothing to write to");
    assert_eq!(urgent["talk"], false);

    tell(&host, &bot, &slow, "mind the cache").await;
    host.call("workCutIn", json!({"botId": bot, "itemId": urgent["id"], "insteadOf": slow["id"]})).await;
    item(&host, &bot, "Urgent", "done").await;
    // It starts again in a new session, which reads what the user wrote and no turn had taken.
    release(&host, &bot, "slow");
    item(&host, &bot, "Slow", "done").await;
    let prompts = task_prompts(&host, &bot, "Slow");
    assert_eq!(prompts.len(), 2);
    let again = prompts[1]["text"].as_str().unwrap();
    assert!(again.contains("[The user said since: «mind the cache»]"), "{again}");
}
