//! A bot's box against a real host and its `chat` MCP server, with a scripted ACP agent: the bot adds items, tasks
//! take them up to `maxAgents` (a fork of the chat's session, or from a brief), raise their approval cards in the
//! chat and report back as turns there; the user and the bot move, cut in, replace, stop and retry items.

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
    std::fs::write(host.home.join("bots").join(bot).join("workspace").join(format!("release-{name}")), "").unwrap();
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
    let named = format!(": \"{title}\".");
    let of_title = |p: &Value| p["text"].as_str().is_some_and(|t| t.contains("[Task]") && t.contains(&named));
    host.log(bot, "prompts.jsonl").into_iter().filter(of_title).collect()
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
async fn a_tasks_approval_card_is_answered_in_the_chat_and_it_cannot_ask_or_add_work() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "task_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "permission": "ask"})).await;
    host.reply(&bot, "ADD Run the suite|PERMISSION then ASK and NEST").await;

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
        ("chatCall", "send_file"),
        ("chatCall", "publish_artifact"),
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
    assert!(relayed.contains("Refused: request_decision isn't available in a task"), "{relayed}");
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
