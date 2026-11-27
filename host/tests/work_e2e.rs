//! A bot's box against a real host and its `chat` MCP server, with a scripted ACP agent: the bot adds tasks, threads
//! take them up to `maxThreads` (each from its brief, in a session of its own), raise their approval cards in the
//! chat and report back as turns there; the user and the bot move, cut in, replace, stop and retry tasks.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::ffi::OsStr;
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

/// The bot's task called `title`, once it reads `status`.
async fn task(host: &Host, bot: &str, title: &str, status: &str) -> Value {
    wait(&format!("{title} to be {status}"), async || {
        work(host, bot).await.into_iter().find(|i| i["title"] == title && i["status"] == status)
    })
    .await
}

fn id(task: &Value) -> &str {
    task["id"].as_str().unwrap()
}

/// Lets the thread holding on `HOLD:<name>` finish.
fn release(host: &Host, bot: &str, name: &str) {
    std::fs::write(host.workspace(bot).join(format!("release-{name}")), "").unwrap();
}

/// A bot on `work_agent.py` that runs at most `agents` threads at once.
async fn bot_with(host: &Host, agents: u8) -> String {
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "maxThreads": agents})).await;
    bot
}

/// The chat-visible `Relayed:` replies, oldest first.
async fn relayed(host: &Host, bot: &str) -> Vec<String> {
    let texts = history(host, bot).await.into_iter().filter_map(|e| e["data"]["text"].as_str().map(str::to_owned));
    texts.filter(|t| t.starts_with("Relayed:")).collect()
}

/// The prompts the bot's threads got for `title`, oldest first.
fn task_prompts(host: &Host, bot: &str, title: &str) -> Vec<Value> {
    task_prompts_in(&host.workspace(bot), title)
}

/// The servers the session `prompt` went to was given, by name.
fn servers_of(host: &Host, bot: &str, prompt: &Value) -> Vec<String> {
    let started = host.log(bot, "servers.jsonl").into_iter().find(|s| s["session"] == prompt["session"]).unwrap();
    started["servers"].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_owned()).collect()
}

/// The prompts the threads working in `dir` got for `title`, oldest first.
fn task_prompts_in(dir: &Path, title: &str) -> Vec<Value> {
    let named = format!(" \"{title}\".");
    let of_title = |p: &Value| p["text"].as_str().is_some_and(|t| t.contains("[Task]") && t.contains(&named));
    logged(dir, "prompts.jsonl").into_iter().filter(of_title).collect()
}

/// The JSON lines a fixture agent working in `dir` logged to `file` there.
fn logged(dir: &Path, file: &str) -> Vec<Value> {
    let log = std::fs::read_to_string(dir.join(file)).unwrap_or_default();
    log.lines().map(|l| serde_json::from_str::<Value>(l).unwrap()).collect()
}

/// No client ever hears of a thread as a bot, or of its transcript as a chat's.
async fn no_thread_bot_reached(events: &mut tokio::sync::mpsc::UnboundedReceiver<Value>) -> Vec<String> {
    let mut threads = Vec::new();
    while let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(300), events.recv()).await {
        let listed = |id: &Value| !id.as_str().unwrap_or_default().starts_with("thread-");
        assert!(event["type"] != "entry" || listed(&event["entry"]["botId"]), "{event}");
        assert!(event["type"] != "bot" || listed(&event["bot"]["id"]), "{event}");
        if event["type"] == "threadEntry" {
            threads.extend(event["thread"].as_str().map(str::to_owned));
        }
    }
    threads
}

#[tokio::test]
async fn a_thread_that_makes_one_call_again_and_again_gets_its_bot_one_watchdog_turn() {
    // The watchdog looks every second here, not every five minutes.
    let host = Host::start_with(&[("CODYNC_WORK_WATCH_SECS", OsStr::new("1"))]).await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;
    host.reply(&bot, "ADD Tail the log|LOOP HOLD:tail").await;

    let looked = reply(&host, &bot, "Watchdog:").await;
    assert_eq!(looked, "Watchdog: it has called \"Read the log\" 6 times in a row");
    // It went on looking meanwhile: the bot is woken for a thread at most once an hour.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let woken: Vec<Value> = logged(&host.workspace(&bot), "prompts.jsonl")
        .into_iter()
        .filter(|p| p["text"].as_str().is_some_and(|t| t.contains("[Codync watchdog]")))
        .collect();
    assert_eq!(woken.len(), 1, "{woken:?}");
    let task = task(&host, &bot, "Tail the log", "working").await;
    assert!(woken[0]["text"].as_str().unwrap().contains(&format!("Task \"Tail the log\" ({})", id(&task))));

    release(&host, &bot, "tail");
    reply(&host, &bot, "Relayed: Report Tail the log").await;
}

#[tokio::test]
async fn a_watchdog_turn_that_passes_leaves_nothing_in_the_chat() {
    let host = Host::start_with(&[("CODYNC_WORK_WATCH_SECS", OsStr::new("1"))]).await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;
    host.reply(&bot, "ADD Tail PASS|LOOP HOLD:tail").await;
    let replies = || async {
        let entries = history(&host, &bot).await;
        entries.into_iter().filter(|e| e["kind"] == "agent" && e["data"]["final"] == true).count()
    };
    let before = replies().await;

    wait("the watchdog's turn", async || {
        let prompts = logged(&host.workspace(&bot), "prompts.jsonl");
        prompts.iter().any(|p| p["text"].as_str().is_some_and(|t| t.contains("[Codync watchdog]"))).then_some(())
    })
    .await;
    wait("the chat to be idle", async || common::steer::idle(&host, &bot).await.then_some(())).await;
    assert_eq!(replies().await, before, "(pass) says nothing");

    release(&host, &bot, "tail");
    reply(&host, &bot, "Relayed: Report Tail PASS").await;
}

#[tokio::test]
async fn a_task_starts_from_its_brief_alone_and_its_report_comes_back_as_a_turn() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;
    host.reply(&bot, "hello").await;
    let mut events = host.events().await;

    host.reply(&bot, "ADD Fix the login|Find why the redirect drops the path").await;
    let relayed = reply(&host, &bot, "Relayed:").await;
    assert_eq!(relayed, "Relayed: Report Fix the login: briefed; no chat", "its last message reached the chat's turn");

    // A new session of the thread's own, never a fork of the chat's, told the task and nothing of the chat.
    assert_eq!(host.log(&bot, "forks.jsonl"), [] as [Value; 0]);
    let prompts = host.log(&bot, "prompts.jsonl");
    let chat_session = prompts[0]["session"].as_str().unwrap();
    let thread_prompt = &task_prompts(&host, &bot, "Fix the login")[0];
    assert_ne!(thread_prompt["session"], chat_session);
    let text = thread_prompt["text"].as_str().unwrap();
    assert!(text.contains("Find why the redirect drops the path"));
    assert!(text.contains("[The user's own words: «ADD Fix the login|Find why the redirect drops the path»]"));
    assert!(text.contains("<bot-profile>") && text.contains("You are doing one task"), "its own instructions: {text}");
    assert!(!text.contains("Title: Mika") && !text.contains("add_task"), "nothing of the chat's: {text}");
    assert_eq!(
        servers_of(&host, &bot, thread_prompt),
        ["connectors", "chat", "memory", "workflows"],
        "nor its servers"
    );

    // No client lists the thread as a bot or mixes its transcript into a chat.
    let snapshot = host.snapshot().await;
    assert!(snapshot["bots"].as_array().unwrap().iter().all(|b| b["threadOf"].is_null()), "{}", snapshot["bots"]);
    let chat = history(&host, &bot).await;
    assert!(chat.iter().all(|e| !e["data"]["text"].as_str().unwrap_or_default().contains("[Task]")));

    // The task stays in the box as done, with what its thread reported and the thread that had it.
    let done = task(&host, &bot, "Fix the login", "done").await;
    assert_eq!(done["result"], "Report Fix the login: briefed; no chat");
    assert!(done["wait"].is_null() && done["cutIn"] == false && done["endedAt"].is_i64(), "{done}");
    let threads = no_thread_bot_reached(&mut events).await;
    assert_eq!(
        threads.first().map(String::as_str),
        done["thread"].as_str(),
        "its entries went out as threadEntry events"
    );

    // Its transcript is fetched by the task's thread.
    let details = host.call("threadEntries", json!({"botId": bot, "threadId": done["thread"]})).await;
    assert_eq!(details["thread"]["title"], "Fix the login");
    assert_eq!(details["thread"]["status"], "done");
    let finals: Vec<&str> = details["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "agent" && e["data"]["final"] == true)
        .filter_map(|e| e["data"]["text"].as_str())
        .collect();
    assert_eq!(finals, ["Report Fix the login: briefed; no chat"]);

    host.reply(&bot, "STATUS").await;
    let status = reply(&host, &bot, "Status:").await;
    assert!(status.contains("\"title\":\"Fix the login\"") && status.contains("\"status\":\"done\""), "{status}");
    host.call("dropWork", json!({"botId": bot, "taskId": id(&done)})).await;
    assert_eq!(work(&host, &bot).await, [] as [Value; 0]);
    let gone = host.refused("threadEntries", json!({"botId": bot, "threadId": done["thread"]})).await;
    assert_eq!(gone, "unknown thread", "a thread no task names goes with its transcript");
}

/// The thread's final messages, oldest first.
#[cfg(unix)]
async fn finals(host: &Host, bot: &str, thread: &Value) -> Vec<String> {
    let details = host.call("threadEntries", json!({"botId": bot, "threadId": thread})).await;
    let entries = details["entries"].as_array().cloned().unwrap_or_default();
    let finals = entries.iter().filter(|e| e["kind"] == "agent" && e["data"]["final"] == true);
    finals.map(|e| e["data"]["text"].as_str().unwrap_or_default().to_owned()).collect()
}

/// #634: a thread that ends its turn while something it started in the background still runs (a review, a CI run it
/// watches) hasn't finished its task: its progress line isn't the report, and the work it waits on isn't cut off. The
/// host asks it once to wait for that, and its next turn's end is the task's.
#[cfg(unix)]
#[tokio::test]
async fn a_task_whose_thread_ends_its_turn_while_its_background_work_runs_is_not_over() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;
    host.reply(&bot, "ADD Review|DETACH:review").await;
    let working = task(&host, &bot, "Review", "working").await;
    let thread = working["thread"].clone();
    let pid_file = host.workspace(&bot).join("detached.pid");
    let pid = wait("the background command", async || std::fs::read_to_string(&pid_file).ok()).await;
    let progress = "Report Review: briefed; no chat; started the review, waiting for it to finish";
    wait("the thread's turn to end", async || {
        finals(&host, &bot, &thread).await.contains(&progress.into()).then_some(())
    })
    .await;
    // The box looks again as soon as the turn ends; give it time to.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let now = work(&host, &bot).await.into_iter().find(|i| i["title"] == "Review").unwrap();
    let alive = std::process::Command::new("/bin/kill").args(["-0", pid.trim()]).status().unwrap().success();
    release(&host, &bot, "review");
    assert_eq!(now["status"], "working", "its background work still runs, so the task isn't over: {now}");
    assert_ne!(now["result"], progress, "a progress line isn't its report");
    assert!(alive, "the work it waits on isn't cut off with its agent");
    let done = task(&host, &bot, "Review", "done").await;
    assert_eq!(done["result"], "Report Review: it finished, all green", "its report came once the work had: {done}");
    assert!(host.workspace(&bot).join("detached-done").exists());
    let asked = host.log(&bot, "prompts.jsonl").into_iter().filter(|p| {
        p["text"].as_str().unwrap_or_default().contains("[Codync: you ended your turn while a command started")
    });
    assert_eq!(asked.count(), 1, "asked once to wait");
}

/// #634: a Claude subagent launched in the background holds its task the same way, and the thread waits for it in the
/// same agent process and session, which nothing closes or stops meanwhile (that would end the subagent).
#[tokio::test]
async fn a_background_subagent_holds_its_task_in_the_same_session() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;
    host.reply(&bot, "ADD Review|SUBAGENT").await;
    let done = task(&host, &bot, "Review", "done").await;
    assert_eq!(done["result"], "Report Review: it finished, all green", "{done}");
    let prompts = host.log(&bot, "prompts.jsonl");
    let turns: Vec<&Value> = prompts
        .iter()
        .filter(|p| {
            let text = p["text"].as_str().unwrap_or_default();
            text.contains("[Task]") || text.contains("[Codync: you ended your turn while a subagent")
        })
        .collect();
    assert_eq!(turns.len(), 2, "its task's turn and the one that waits: {prompts:?}");
    assert_eq!((&turns[0]["session"], &turns[0]["pid"]), (&turns[1]["session"], &turns[1]["pid"]), "same session");
    let closed = host.log(&bot, "closed.jsonl");
    let between = closed.iter().any(|c| c["session"] == turns[0]["session"] && c["prompts"].as_u64() < Some(2));
    assert!(!between, "nothing closed its session before the turn that waits: {closed:?}");
}

/// #634: a thread that ends its turn again while its background work still runs, after it was asked to wait, has
/// stopped early: its task fails saying what still ran, never done with a progress line.
#[cfg(unix)]
#[tokio::test]
async fn a_thread_that_ends_again_while_its_background_work_runs_stopped_early() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;
    host.reply(&bot, "ADD Review|DETACH:review AGAIN").await;
    let failed = task(&host, &bot, "Review", "failed").await;
    release(&host, &bot, "review");
    let why = "it ended its turn while a command started in the background still ran. Its last message:\n\n\
               Still waiting for it";
    assert_eq!(failed["result"], why, "what it said stays with why: {failed}");
}

/// #634: a message the user wrote to the thread while its turn ran takes the next turn first, with the work it left
/// running still its task's: that turn's end asks it to wait, once, and the task ends with the report.
#[cfg(unix)]
#[tokio::test]
async fn a_message_to_a_thread_that_left_work_running_goes_first_and_the_task_still_waits_for_it() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;
    host.reply(&bot, "ADD Review|DETACH:review HOLD:slow").await;
    let working = task(&host, &bot, "Review", "working").await;
    let sent = tell(&host, &bot, &working, "also the tests").await;
    assert_eq!(sent["queued"], true, "{sent}");
    release(&host, &bot, "slow");
    let asked = "[Codync: you ended your turn while a command started";
    let prompts = wait("the host to ask it to wait", async || {
        let prompts = host.log(&bot, "prompts.jsonl");
        prompts.iter().any(|p| p["text"].as_str().unwrap_or_default().contains(asked)).then_some(prompts)
    })
    .await;
    let now = work(&host, &bot).await.into_iter().find(|i| i["title"] == "Review").unwrap();
    release(&host, &bot, "review");
    assert_eq!(now["status"], "working", "{now}");
    let at = |what: &str| prompts.iter().position(|p| p["text"].as_str().unwrap_or_default().contains(what)).unwrap();
    assert!(at("in this task's thread") < at(asked), "the user's message first: {prompts:?}");
    let pid = |what: &str| &prompts[at(what)]["pid"];
    assert!(pid("[Task]") == pid(asked) && pid("in this task's thread") == pid(asked), "one agent: {prompts:?}");
    let done = task(&host, &bot, "Review", "done").await;
    assert_eq!(done["result"], "Report Review: it finished, all green", "{done}");
    let asked =
        host.log(&bot, "prompts.jsonl").into_iter().filter(|p| p["text"].as_str().unwrap_or_default().contains(asked));
    assert_eq!(asked.count(), 1, "asked once to wait");
}

#[tokio::test]
async fn a_thread_hears_nothing_of_the_chat_it_was_handed_out_in() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;
    host.reply(&bot, "Remember the blue theme").await;
    host.reply(&bot, "ADD Update the docs|Rewrite the install steps").await;
    assert_eq!(reply(&host, &bot, "Relayed:").await, "Relayed: Report Update the docs: briefed; no chat");
    let text = task_prompts(&host, &bot, "Update the docs")[0]["text"].as_str().unwrap().to_owned();
    assert!(text.contains("<bot-profile>"), "a new session gets its instructions");
    assert!(!text.contains("blue theme"), "but not what the chat said: {text}");
}

#[tokio::test]
async fn a_threads_approval_card_is_answered_in_the_chat_and_it_cannot_add_task() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "permission": "ask"})).await;
    host.reply(&bot, "ADD Run the suite|PERMISSION then NEST").await;

    let card = wait("the thread's card in the chat", async || {
        history(&host, &bot).await.into_iter().find(|e| e["kind"] == "permission" && e["data"]["status"] == "pending")
    })
    .await;
    let waits = task(&host, &bot, "Run the suite", "needsYou").await;
    assert_eq!(card["data"]["task"], waits["id"], "the card names the task");
    assert_eq!(card["data"]["thread"], json!({"id": waits["thread"], "title": "Run the suite"}));
    assert_eq!(card["data"]["title"], "Run the tests");
    // The chat itself stays free while the thread waits.
    host.reply(&bot, "are you there?").await;

    // What a thread may not do from where it is, whichever server it asks.
    for (method, tool) in [
        ("chatCall", "set_name"),
        ("chatCall", "steer_task"),
        ("chatCall", "tell_task"),
        ("chatCall", "work_status"),
        ("chatCall", "recall_task"),
        ("teamCall", "message_bot"),
        ("routineCall", "follow_up"),
        ("manageCall", "create_bot"),
    ] {
        let refused = host.refused(method, json!({"botId": waits["thread"], "name": tool, "arguments": {}})).await;
        assert!(refused.contains(&format!("{tool} isn't available in a thread")), "{method} {tool}: {refused}");
    }

    host.call("respondPermission", json!({"entryId": card["id"], "optionId": "allow"})).await;
    let relayed = reply(&host, &bot, "Relayed:").await;
    assert!(relayed.contains("Refused: add_task isn't available in a thread"), "{relayed}");
    assert!(relayed.ends_with("approved"), "{relayed}");
    // The chat's `chat` server lists `add_task`; the thread's leaves it out, and the host refused the call anyway.
    let listed: Vec<bool> = host
        .log(&bot, "listed.jsonl")
        .iter()
        .filter(|l| l["tool"] == "add_task")
        .map(|l| l["listed"].as_bool().unwrap())
        .collect();
    assert_eq!(listed, [true, false]);
}

#[tokio::test]
async fn a_thread_waiting_on_the_user_gives_its_seat_to_the_next_task_and_both_report() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.call("updateBot", json!({"id": bot, "permission": "ask"})).await;
    host.reply(&bot, "ADD Asks|PERMISSION:asks").await;
    task(&host, &bot, "Asks", "working").await;
    host.reply(&bot, "ADD Next|HOLD:next").await;
    task(&host, &bot, "Next", "waiting").await;

    // Its card raised, the thread lends its seat and the next task starts beside it.
    release(&host, &bot, "asks");
    let card = wait("the thread's card", async || {
        history(&host, &bot).await.into_iter().find(|e| e["kind"] == "permission" && e["data"]["status"] == "pending")
    })
    .await;
    task(&host, &bot, "Next", "working").await;
    let seats: Vec<(Value, Value)> =
        work(&host, &bot).await.iter().map(|i| (i["title"].clone(), i["seat"].clone())).collect();
    assert_eq!(seats, [(json!("Asks"), json!(false)), (json!("Next"), json!(true))], "one seat in use");

    // Answered, it goes on past the ceiling, and each task reports once.
    host.call("respondPermission", json!({"entryId": card["id"], "optionId": "allow"})).await;
    release(&host, &bot, "next");
    task(&host, &bot, "Asks", "done").await;
    task(&host, &bot, "Next", "done").await;
    let reports = wait("both reports", async || {
        let relayed = relayed(&host, &bot).await.join(" | ");
        (relayed.matches("Report ").count() == 2).then_some(relayed)
    })
    .await;
    assert!(reports.contains("Report Asks:") && reports.contains("Report Next:"), "{reports}");
}

#[tokio::test]
async fn tasks_wait_their_turn_in_order_and_one_moved_first_starts_first() {
    let host = Host::start().await;
    for agents in [0, 7] {
        let bot = host.fixture_bot("Odd", "work_agent.py", "").await;
        let refused = host.refused("updateBot", json!({"id": bot, "maxThreads": agents})).await;
        assert_eq!(refused, "maxThreads is 1 to 6");
    }
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD One|HOLD:one").await;
    task(&host, &bot, "One", "working").await;
    host.reply(&bot, "ADD Two|HOLD:two").await;
    host.reply(&bot, "ADD Three|HOLD:three").await;
    host.reply(&bot, "ADD Four|quick").await;
    host.reply(&bot, "ADD Five|quick").await;

    let titles = |work: &[Value]| work.iter().map(|i| i["title"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
    let listed = work(&host, &bot).await;
    assert_eq!(titles(&listed), ["One", "Two", "Three", "Four", "Five"], "the working one, then the waiting in order");
    assert!(listed[1..].iter().all(|i| i["status"] == "waiting" && i["thread"].is_null()), "{listed:?}");
    assert_eq!(listed[1]["wait"], json!({"kind": "next"}));
    assert_eq!(listed[2]["wait"], json!({"kind": "behind", "ahead": 1}));
    assert_eq!(listed[4]["wait"], json!({"kind": "behind", "ahead": 3}));

    // The user moves one first; the bot another, with its tool; a waiting one is dropped.
    host.call("workFirst", json!({"botId": bot, "taskId": id(&listed[2])})).await;
    host.reply(&bot, "STEER first|Four").await;
    host.call("dropWork", json!({"botId": bot, "taskId": id(&listed[4])})).await;
    let refused = host.refused("dropWork", json!({"botId": bot, "taskId": id(&listed[0])})).await;
    assert_eq!(refused, "\"One\" is being worked on: stop it first");
    let refused = host.refused("workFirst", json!({"botId": bot, "taskId": id(&listed[0])})).await;
    assert_eq!(refused, "\"One\" isn't waiting");
    assert_eq!(titles(&work(&host, &bot).await), ["One", "Four", "Three", "Two"]);

    // A place comes free: the first in order takes it, and only that one.
    release(&host, &bot, "one");
    task(&host, &bot, "One", "done").await;
    task(&host, &bot, "Four", "done").await;
    task(&host, &bot, "Three", "working").await;
    assert_eq!(task(&host, &bot, "Two", "waiting").await["wait"], json!({"kind": "next"}));

    // A higher ceiling starts what waits, at once.
    host.call("updateBot", json!({"id": bot, "maxThreads": 2})).await;
    task(&host, &bot, "Two", "working").await;
    release(&host, &bot, "two");
    release(&host, &bot, "three");
    wait("every report", async || (relayed(&host, &bot).await.join(" ").matches("Report ").count() == 4).then_some(()))
        .await;
}

#[tokio::test]
async fn a_task_that_could_not_start_anyway_cuts_nothing_out() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD Slow|HOLD:slow").await;
    let slow = task(&host, &bot, "Slow", "working").await;
    host.reply(&bot, "ADD More|HOLD:more|after=Slow").await;
    let more = task(&host, &bot, "More", "waiting").await;
    assert_eq!(more["wait"], json!({"kind": "after"}));

    // It follows what works: closing that session would only make both start over.
    host.call("workCutIn", json!({"botId": bot, "taskId": id(&more), "insteadOf": id(&slow)})).await;
    host.call("workCutIn", json!({"botId": bot, "taskId": id(&more)})).await;
    let listed = work(&host, &bot).await;
    let still = listed.iter().find(|i| i["title"] == "Slow").unwrap();
    assert!(still["status"] == "working" && still["thread"] == slow["thread"] && still["cutIn"] == false, "{still}");
    assert_eq!(task(&host, &bot, "More", "waiting").await["wait"], json!({"kind": "after"}));

    release(&host, &bot, "slow");
    let joined = task(&host, &bot, "More", "working").await;
    assert_eq!(joined["thread"], slow["thread"], "in the same session, as it asked");
    release(&host, &bot, "more");
    wait("both reports", async || (relayed(&host, &bot).await.len() == 2).then_some(())).await;
}

#[tokio::test]
async fn a_task_cuts_in_and_the_one_that_gave_way_starts_again_in_a_new_thread() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD Slow|HOLD:slow").await;
    let slow = task(&host, &bot, "Slow", "working").await;
    host.reply(&bot, "ADD Urgent|HOLD:urgent").await;
    let urgent = task(&host, &bot, "Urgent", "waiting").await;
    let mut events = host.events().await;

    let refused = host.refused("workCutIn", json!({"botId": bot, "taskId": id(&urgent), "insteadOf": "nope"})).await;
    assert_eq!(refused, "no such task");
    host.call("workCutIn", json!({"botId": bot, "taskId": id(&urgent), "insteadOf": id(&slow)})).await;
    task(&host, &bot, "Urgent", "working").await;
    let aside = task(&host, &bot, "Slow", "waiting").await;
    assert!(aside["cutIn"] == true && aside["thread"].is_null() && aside["startedAt"].is_null(), "{aside}");
    assert_eq!(aside["wait"], json!({"kind": "next"}));
    wait("the thread that gave way to go", async || {
        let gone = host.post("threadEntries", &json!({"botId": bot, "threadId": slow["thread"]})).await;
        (!gone.0).then_some(())
    })
    .await;

    // The place comes free again: it starts over in a new thread and session, told it was begun before.
    release(&host, &bot, "urgent");
    let again = task(&host, &bot, "Slow", "working").await;
    assert_ne!(again["thread"], slow["thread"]);
    release(&host, &bot, "slow");
    wait("both reports", async || (relayed(&host, &bot).await.len() == 2).then_some(())).await;
    assert_eq!(
        relayed(&host, &bot).await,
        ["Relayed: Report Urgent: briefed; no chat", "Relayed: Report Slow: briefed; no chat; again"]
    );
    let prompts = task_prompts(&host, &bot, "Slow");
    assert_eq!(prompts.len(), 2);
    assert_ne!(prompts[0]["session"], prompts[1]["session"]);
    assert_eq!(task(&host, &bot, "Slow", "done").await["cutIn"], true);
    no_thread_bot_reached(&mut events).await;

    // With a place free, cutting in is moving first; the bot's tool picks who gives way when none is named.
    host.call("updateBot", json!({"id": bot, "maxThreads": 2})).await;
    host.reply(&bot, "ADD A|HOLD:a").await;
    host.reply(&bot, "ADD B|HOLD:b").await;
    task(&host, &bot, "B", "working").await;
    host.reply(&bot, "ADD C|HOLD:c").await;
    host.reply(&bot, "STEER now|C").await;
    task(&host, &bot, "C", "working").await;
    assert!(task(&host, &bot, "B", "waiting").await["cutIn"] == true, "the one that started last gave way");
    task(&host, &bot, "A", "working").await;
}

#[tokio::test]
async fn a_replaced_brief_is_what_the_task_starts_from_waiting_or_working() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD Working|HOLD:never").await;
    let working = task(&host, &bot, "Working", "working").await;
    host.reply(&bot, "ADD Waiting|HOLD:never").await;
    task(&host, &bot, "Waiting", "waiting").await;

    // A waiting task: its brief changes and nothing else.
    host.reply(&bot, "STEER replace|Waiting|a quick look").await;
    reply(&host, &bot, "Steered:").await;
    assert_eq!(task(&host, &bot, "Working", "working").await["thread"], working["thread"]);

    // A working one: its thread's work is closed and it starts again from the new brief, ahead of the waiting.
    host.reply(&bot, "STEER replace|Working|just say so").await;
    wait("both reports", async || (relayed(&host, &bot).await.len() == 2).then_some(())).await;
    assert_eq!(
        relayed(&host, &bot).await,
        ["Relayed: Report Working: briefed; no chat", "Relayed: Report Waiting: briefed; no chat"]
    );
    let again = task(&host, &bot, "Working", "done").await;
    assert_ne!(again["thread"], working["thread"]);
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
async fn the_chat_passes_words_on_to_a_task_and_its_one_report_comes_after_them() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "steer").await;
    host.call("updateBot", json!({"id": bot, "maxThreads": 1})).await;
    host.reply(&bot, "ADD Working|HOLD:working").await;
    task(&host, &bot, "Working", "working").await;
    host.reply(&bot, "ADD Waiting|x").await;
    task(&host, &bot, "Waiting", "waiting").await;

    // A waiting task reads it when it starts.
    host.reply(&bot, "TELL Waiting|the API page too").await;
    assert!(reply(&host, &bot, "Told:").await.contains("It reads this when it starts."));
    // A working one: now, into its running turn; or after it, as a turn of its own before it reports.
    host.reply(&bot, "TELL Working|mind the docs|now").await;
    host.reply(&bot, "TELL Working|and the tests").await;
    let told: Vec<String> =
        history(&host, &bot).await.into_iter().filter_map(|e| e["data"]["text"].as_str().map(str::to_owned)).collect();
    assert!(told.iter().any(|t| t.contains("before it reports")), "{told:?}");
    release(&host, &bot, "working");
    task(&host, &bot, "Working", "done").await;
    task(&host, &bot, "Waiting", "done").await;
    wait("both reports", async || (relayed(&host, &bot).await.len() == 2).then_some(())).await;
    assert_eq!(
        relayed(&host, &bot).await,
        ["Relayed: Heard and the tests", "Relayed: Report Waiting: briefed; no chat"],
        "one report a task: the last turn's"
    );
    let heard = logged(&host.workspace(&bot), "prompts.jsonl");
    let in_thread = |t: &&str| t.contains("and the tests") && !t.contains("TELL");
    let relay = heard.iter().find_map(|p| p["text"].as_str().filter(in_thread)).unwrap();
    assert!(
        relay.contains("[Codync: your chat passed this on to you") && !relay.contains("[Codync: message "),
        "{relay}"
    );
    let waiting = task_prompts(&host, &bot, "Waiting")[0]["text"].as_str().unwrap().to_owned();
    assert!(waiting.contains("the API page too"), "{waiting}");
}

#[tokio::test]
async fn in_advisor_mode_tasks_run_on_the_threads_agent() {
    let host = Host::start().await;
    let bot = bot_with(&host, 2).await;
    let worker = common::fixture_command("work_agent.py", "as=worker");
    host.call("updateBot", json!({"id": bot, "threadAgent": {"backend": "custom", "command": worker}})).await;
    host.reply(&bot, "ADD Fix|x").await;
    wait("its report", async || (relayed(&host, &bot).await.len() == 1).then_some(())).await;
    assert_eq!(relayed(&host, &bot).await, ["Relayed: Report Fix: briefed; no chat; on worker"]);

    // Off: the chat's own agent again.
    host.call("updateBot", json!({"id": bot, "threadAgent": null})).await;
    host.reply(&bot, "ADD Again|x").await;
    wait("its report", async || (relayed(&host, &bot).await.len() == 2).then_some(())).await;
    assert_eq!(relayed(&host, &bot).await[1], "Relayed: Report Again: briefed; no chat");
}

#[tokio::test]
async fn a_task_that_follows_another_runs_in_its_session_when_it_is_done() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD First|HOLD:first").await;
    let first = task(&host, &bot, "First", "working").await;
    host.reply(&bot, "ADD Other|quick").await;
    host.reply(&bot, "ADD Second|more of the same|after=First").await;
    assert_eq!(task(&host, &bot, "Second", "waiting").await["wait"], json!({"kind": "after"}));

    release(&host, &bot, "first");
    wait("every report", async || (relayed(&host, &bot).await.join(" ").matches("Report ").count() == 3).then_some(()))
        .await;
    let reports = relayed(&host, &bot).await.join(" | ");
    assert!(reports.contains("Report First: briefed; no chat"), "{reports}");
    assert!(reports.contains("Report Second: next; no chat"), "{reports}");

    // The same thread and session took it, ahead of the ordinary task that was before it in the box.
    let second = task(&host, &bot, "Second", "done").await;
    assert_eq!(second["thread"], first["thread"]);
    assert_ne!(task(&host, &bot, "Other", "done").await["thread"], first["thread"]);
    let (first, second) = (&task_prompts(&host, &bot, "First")[0], &task_prompts(&host, &bot, "Second")[0]);
    assert_eq!(first["session"], second["session"]);
    let prompts = host.log(&bot, "prompts.jsonl");
    let at = |prompt: &Value| prompts.iter().position(|p| p == prompt).unwrap();
    assert!(at(second) < at(&task_prompts(&host, &bot, "Other")[0]), "it had the place its thread kept");
    assert_eq!(host.log(&bot, "forks.jsonl"), [] as [Value; 0], "no thread forks the chat");

    // Once that thread has ended, a task can't follow into it.
    host.reply(&bot, "ADD Third|more|after=First").await;
    let refused = reply(&host, &bot, "Refused:").await;
    assert_eq!(refused, "Refused: after names no task that is working or waiting: see work_status");
}

#[tokio::test]
async fn tasks_stop_one_or_all_without_a_report_and_a_stopped_one_is_tried_again() {
    let host = Host::start().await;
    let bot = bot_with(&host, 2).await;
    host.reply(&bot, "ADD One|HOLD:one").await;
    host.reply(&bot, "ADD Two|HOLD:two").await;
    host.reply(&bot, "ADD Three|HOLD:three|after=One").await;
    host.reply(&bot, "ADD Four|HOLD:four").await;
    let one = task(&host, &bot, "One", "working").await;
    task(&host, &bot, "Two", "working").await;

    // One task: its thread stops, and what followed it becomes an ordinary task that takes the place.
    host.call("stopWork", json!({"botId": bot, "taskId": id(&one)})).await;
    let stopped = task(&host, &bot, "One", "stopped").await;
    assert!(stopped["endedAt"].is_i64() && stopped["result"].is_null(), "{stopped}");
    let three = task(&host, &bot, "Three", "working").await;
    assert_ne!(three["thread"], one["thread"]);
    let refused = host.refused("stopWork", json!({"botId": bot, "taskId": id(&one)})).await;
    assert_eq!(refused, "that work has already ended");

    // The bot stops one with its tool; then the user stops all, the waiting too.
    host.reply(&bot, "STEER stop|Two").await;
    task(&host, &bot, "Two", "stopped").await;
    task(&host, &bot, "Four", "working").await;
    host.reply(&bot, "ADD Five|quick").await;
    host.call("stopWork", json!({"botId": bot})).await;
    for title in ["Three", "Four", "Five"] {
        task(&host, &bot, title, "stopped").await;
    }
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(relayed(&host, &bot).await, [] as [String; 0], "the user knows: nothing is reported");

    // Tried again, it waits last and starts in a new thread.
    let refused = host.refused("retryWork", json!({"botId": bot, "taskId": "nope"})).await;
    assert_eq!(refused, "no such task");
    release(&host, &bot, "one");
    host.call("retryWork", json!({"botId": bot, "taskId": id(&one)})).await;
    wait("its report", async || {
        relayed(&host, &bot).await.iter().any(|r| r == "Relayed: Report One: briefed; no chat").then_some(())
    })
    .await;
    let done = task(&host, &bot, "One", "done").await;
    assert_ne!(done["thread"], one["thread"]);
    let gone = host.refused("threadEntries", json!({"botId": bot, "threadId": one["thread"]})).await;
    assert_eq!(gone, "unknown thread");
}

#[tokio::test]
async fn a_task_cut_off_by_a_host_restart_picks_up_and_still_reports() {
    let mut host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD Long one|HOLD:long").await;
    let long = task(&host, &bot, "Long one", "working").await;
    host.reply(&bot, "ADD Next|quick").await;

    host.restart(|_| {}).await;
    // Its thread's actor resumes the turn that was cut off; what that turn says is the report. The box is as it was.
    let relayed = reply(&host, &bot, "Relayed:").await;
    assert!(relayed.contains("Codync restarted"), "{relayed}");
    assert_eq!(task(&host, &bot, "Long one", "done").await["thread"], long["thread"]);
    task(&host, &bot, "Next", "done").await;
}

#[tokio::test]
async fn a_task_whose_agent_cannot_start_says_so_and_lets_the_next_start() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "hello").await;
    let no_start = host.home.join("bots").join(&bot).join("workspace/no-start");
    std::fs::write(&no_start, "").unwrap();

    host.reply(&bot, "ADD Broken|anything").await;
    let relayed = reply(&host, &bot, "Relayed:").await;
    assert!(relayed.contains("\"Broken\" did not finish: it could not start"), "{relayed}");
    let failed = task(&host, &bot, "Broken", "failed").await;
    assert!(failed["result"].as_str().unwrap().starts_with("it could not start"), "{failed}");

    std::fs::remove_file(no_start).unwrap();
    host.reply(&bot, "ADD Next|HOLD:next").await;
    task(&host, &bot, "Next", "working").await;
    host.call("retryWork", json!({"botId": bot, "taskId": id(&failed)})).await;
    assert_eq!(task(&host, &bot, "Broken", "waiting").await["result"], Value::Null);
}

#[tokio::test]
async fn a_deleted_bot_takes_its_tasks_and_threads_with_it() {
    let host = Host::start().await;
    let bot = bot_with(&host, 2).await;
    host.reply(&bot, "ADD One|HOLD:one").await;
    host.reply(&bot, "ADD Two|HOLD:two").await;
    host.reply(&bot, "ADD Three|HOLD:three").await;
    let live = [task(&host, &bot, "One", "working").await, task(&host, &bot, "Two", "working").await];
    task(&host, &bot, "Three", "waiting").await;
    let mut events = host.events().await;

    host.call("deleteBot", json!({"botId": bot})).await;
    for task in live {
        let gone = host.refused("threadEntries", json!({"botId": bot, "threadId": task["thread"]})).await;
        assert_eq!(gone, "unknown thread");
    }
    no_thread_bot_reached(&mut events).await;
}

#[tokio::test]
async fn stopping_the_turn_that_added_a_task_drops_it_before_it_begins() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;
    let nonce = uuid::Uuid::new_v4().to_string();
    host.call("send", json!({"botId": bot, "text": "ADD Never|anything|hold", "clientNonce": nonce})).await;
    let never = task(&host, &bot, "Never", "waiting").await;
    assert_eq!(never["wait"], json!({"kind": "next"}), "it starts when the turn ends");

    host.call("stop", json!({"botId": bot})).await;
    wait("the task to go", async || work(&host, &bot).await.is_empty().then_some(())).await;
    assert_eq!(task_prompts(&host, &bot, "Never"), [] as [Value; 0]);
}

/// A routine of `bot`'s that does what `instruction` says (`work_agent.py`); its id.
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
async fn a_routine_with_a_folder_runs_there_and_fails_once_it_is_gone() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    let folder = std::env::temp_dir().join(format!("codync-routine-folder-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&folder).unwrap();
    let relative = json!({"botId": bot, "name": "Nope", "instruction": "PASS", "triggers": [{"type": "webhook"}],
        "folder": "relative/path"});
    assert_eq!(host.refused("saveRoutine", relative).await, "folder must be the full path of an existing folder");
    let body = json!({"botId": bot, "name": "Elsewhere", "instruction": "PASS", "triggers": [{"type": "webhook"}],
        "folder": folder});
    let saved = host.call("saveRoutine", body).await;
    assert_eq!(saved["routine"]["folder"], json!(folder));
    let id = saved["routine"]["id"].as_str().unwrap().to_owned();

    host.call("runRoutine", json!({"botId": bot, "id": id})).await;
    run(&host, &bot, &id, "succeeded").await;
    let there = logged(&folder, "prompts.jsonl");
    assert!(there.iter().any(|p| p["text"].as_str().unwrap().contains("Scheduled routine: Elsewhere")), "{there:?}");

    common::remove_dir_all_when_free(&folder).unwrap();
    host.call("runRoutine", json!({"botId": bot, "id": id})).await;
    let failed = run(&host, &bot, &id, "failed").await;
    assert!(failed["detail"].as_str().unwrap().contains("is gone"), "{failed}");
    // Gone, it is kept by a save that leaves it out or sends it unchanged (a drive not mounted); a new one must exist.
    let edit = |folder: Option<Value>| {
        let mut body = json!({"botId": bot, "id": id, "name": "Elsewhere", "instruction": "PASS",
            "triggers": [{"type": "webhook"}]});
        if let Some(folder) = folder {
            body["folder"] = folder;
        }
        body
    };
    assert_eq!(host.call("saveRoutine", edit(None)).await["routine"]["folder"], json!(folder));
    assert_eq!(host.call("saveRoutine", edit(Some(json!(folder)))).await["routine"]["folder"], json!(folder));
    let elsewhere = folder.with_extension("other");
    let refused = host.refused("saveRoutine", edit(Some(json!(elsewhere)))).await;
    assert_eq!(refused, "folder must be the full path of an existing folder");
    // Cleared, it runs in the bot's folder again.
    assert_eq!(host.call("saveRoutine", edit(Some(Value::Null))).await["routine"]["folder"], Value::Null);
}

#[tokio::test]
async fn a_routine_with_a_folder_runs_in_a_thread_even_for_a_bot_that_runs_routines_in_its_chat() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.call("updateBot", json!({"id": bot, "manageTeam": true})).await;
    let folder = std::env::temp_dir().join(format!("codync-routine-folder-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&folder).unwrap();
    let body = json!({"botId": bot, "name": "Team elsewhere", "instruction": "PASS",
        "triggers": [{"type": "webhook"}], "folder": folder});
    let id = host.call("saveRoutine", body).await["routine"]["id"].as_str().unwrap().to_owned();

    host.call("runRoutine", json!({"botId": bot, "id": id})).await;
    run(&host, &bot, &id, "succeeded").await;
    let there = logged(&folder, "prompts.jsonl");
    assert!(
        there.iter().any(|p| p["text"].as_str().unwrap().contains("Scheduled routine: Team elsewhere")),
        "{there:?}"
    );
    common::remove_dir_all_when_free(&folder).unwrap();
}

#[tokio::test]
async fn a_routine_run_is_a_task_and_its_last_message_is_the_routines_message_in_the_chat() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "hello").await;
    let digest = routine(&host, &bot, "Digest", "HOLD:digest PROGRESS ASK SEND NEST").await;
    let mut events = host.events().await;

    host.call("runRoutine", json!({"botId": bot, "id": digest})).await;
    let working = task(&host, &bot, "Digest", "working").await;
    assert_eq!(working["source"], json!({"kind": "routine", "id": digest}), "the row says where it came from");
    // Its line is in the chat as a run's is, and the chat itself stays free.
    let line = wait("the run's line", async || said(&host, &bot, "notice", "Routine · Digest").await.pop()).await;
    host.reply(&bot, "are you there?").await;
    // A second routine waits for the bot's one place, like any task.
    let second = routine(&host, &bot, "Second", "PASS").await;
    host.call("runRoutine", json!({"botId": bot, "id": second})).await;
    assert_eq!(task(&host, &bot, "Second", "waiting").await["wait"], json!({"kind": "next"}));

    release(&host, &bot, "digest");
    let result = reply(&host, &bot, "Routine done:").await;
    // What a run may not do in the chat it may not do here, and its progress note is its routine's.
    assert!(result.contains("Progress: {\"ok\":true"), "{result}");
    assert!(result.contains("Refused: request_decision works only in the current turn of your own chat"), "{result}");
    assert!(result.contains("Refused: send_message isn't available in a thread"), "{result}");
    assert!(result.contains("Refused: add_task isn't available in a thread"), "{result}");
    let posted = said(&host, &bot, "agent", "Routine done:").await.remove(0);
    assert_eq!(posted["turn"], line["turn"], "in the turn its line took");
    assert_eq!((&posted["data"]["author"], &posted["data"]["final"]), (&json!(bot), &json!(true)));
    // The silent one ran next, and said nothing.
    run(&host, &bot, &second, "succeeded").await;
    assert_eq!(task(&host, &bot, "Second", "done").await["result"], Value::Null);
    let done = task(&host, &bot, "Digest", "done").await;
    assert!(done["result"].as_str().unwrap().starts_with("Routine done:"), "{done}");

    // The bot took no turn for either: nothing was relayed, and its run's prompt reached a thread as a routine's.
    assert_eq!(relayed(&host, &bot).await, [] as [String; 0]);
    assert_eq!(said(&host, &bot, "agent", "Routine done:").await.len(), 1, "posted once");
    let prompts = host.log(&bot, "prompts.jsonl");
    let ran = prompts.iter().find(|p| p["text"].as_str().unwrap().contains("Scheduled routine: Digest")).unwrap();
    assert!(ran["text"].as_str().unwrap().contains("Progress note from this routine's earlier runs"), "{ran}");
    assert!(!ran["text"].as_str().unwrap().contains("[Task]"), "a run's own prompt, not a task's");
    // It starts as a run in the chat did: a new session with only its own prompt, though the agent can fork.
    assert_eq!(host.log(&bot, "forks.jsonl"), [] as [Value; 0], "a run never forks the chat");
    assert_ne!(ran["session"], prompts[0]["session"], "nor works in the chat's session");
    assert!(ran["text"].as_str().unwrap().contains("running one of your routines"), "a run's instructions: {ran}");
    let servers = servers_of(&host, &bot, ran);
    assert!(servers.contains(&"routines".to_owned()) && servers.contains(&"team".to_owned()), "{servers:?}");
    let threads = no_thread_bot_reached(&mut events).await;
    assert!(threads.iter().any(|thread| Some(thread.as_str()) == done["thread"].as_str()), "its trace is its thread's");

    // The next run reads what this one saved.
    host.call("runRoutine", json!({"botId": bot, "id": digest})).await;
    wait("the second Digest run", async || {
        let prompts = host.log(&bot, "prompts.jsonl");
        prompts.iter().any(|p| p["text"].as_str().unwrap().contains("instructions):\nhandled 1")).then_some(())
    })
    .await;
}

#[tokio::test]
async fn a_routine_task_is_stopped_dropped_and_tried_again_as_its_routine_is() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "hello").await;
    let held = routine(&host, &bot, "Held", "HOLD:held").await;
    let later = routine(&host, &bot, "Later", "HOLD:later").await;

    // Stopping a run's task cancels the run, with no failure line.
    host.call("runRoutine", json!({"botId": bot, "id": held})).await;
    let working = task(&host, &bot, "Held", "working").await;
    host.call("runRoutine", json!({"botId": bot, "id": later})).await;
    let waiting = task(&host, &bot, "Later", "waiting").await;
    // A routine deleted while its run waits takes the run out of the box.
    host.call("deleteRoutine", json!({"botId": bot, "id": later})).await;
    wait("the deleted routine's run to leave", async || {
        work(&host, &bot).await.iter().all(|i| i["id"] != waiting["id"]).then_some(())
    })
    .await;
    host.call("stopWork", json!({"botId": bot, "taskId": id(&working)})).await;
    let stopped = task(&host, &bot, "Held", "stopped").await;
    assert_eq!(run(&host, &bot, &held, "cancelled").await["detail"], "Stopped by user");
    assert_eq!(said(&host, &bot, "notice", "Routine could not finish").await.len(), 0);

    // Try again runs the routine again: a new run and a new task, in the old one's place.
    host.call("retryWork", json!({"botId": bot, "taskId": id(&stopped)})).await;
    let again = task(&host, &bot, "Held", "working").await;
    assert_ne!(again["id"], stopped["id"]);
    assert_eq!(work(&host, &bot).await.iter().filter(|i| i["title"] == "Held").count(), 1);
    // It can't give way to another task, nor take a new brief: it would run twice.
    host.reply(&bot, "ADD Urgent|quick").await;
    let urgent = task(&host, &bot, "Urgent", "waiting").await;
    let refused =
        host.refused("workCutIn", json!({"botId": bot, "taskId": id(&urgent), "insteadOf": id(&again)})).await;
    assert_eq!(refused, "\"Held\" is a routine run or another bot's request: it can't give way, stop it instead");
    host.call("workCutIn", json!({"botId": bot, "taskId": id(&urgent)})).await;
    task(&host, &bot, "Held", "working").await;

    // A run whose agent can't start fails as a run in the chat does, and the box frees its place.
    release(&host, &bot, "held");
    task(&host, &bot, "Urgent", "done").await;
    let no_start = host.home.join("bots").join(&bot).join("workspace/no-start");
    std::fs::write(&no_start, "").unwrap();
    let broken = routine(&host, &bot, "Broken", "anything").await;
    host.call("runRoutine", json!({"botId": bot, "id": broken})).await;
    let failed = task(&host, &bot, "Broken", "failed").await;
    assert!(failed["result"].as_str().unwrap().starts_with("it could not start"), "{failed}");
    let run = run(&host, &bot, &broken, "failed").await;
    assert!(run["detail"].as_str().unwrap().starts_with("Could not start routine:"), "{run}");
    wait("the failure's line", async || said(&host, &bot, "notice", "Routine could not finish").await.pop()).await;
    std::fs::remove_file(no_start).unwrap();
}

#[tokio::test]
async fn pausing_a_bot_stops_its_working_thread_and_cancels_a_run_that_comes_due() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "hello").await;
    let held = routine(&host, &bot, "Held", "HOLD:held").await;
    host.call("runRoutine", json!({"botId": bot, "id": held})).await;
    let working = task(&host, &bot, "Held", "working").await;
    let ran = |prompts: &[Value]| {
        prompts.iter().filter(|p| p["text"].as_str().is_some_and(|t| t.contains("Scheduled routine: Held"))).count()
    };
    wait("the held run's prompt", async || (ran(&logged(&host.workspace(&bot), "prompts.jsonl")) == 1).then_some(()))
        .await;

    host.call("updateBot", json!({"id": bot, "paused": true})).await;
    // The thread that was working stops, and its run says why. No report is posted as a success.
    assert_eq!(task(&host, &bot, "Held", "stopped").await["thread"], working["thread"]);
    let cancelled = run(&host, &bot, &held, "cancelled").await;
    assert_eq!(cancelled["detail"], "the bot is paused");
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(ran(&logged(&host.workspace(&bot), "prompts.jsonl")), 1, "the stopped thread spent nothing more");

    // A run that comes due while paused is cancelled before it reaches the box or a thread.
    let later = routine(&host, &bot, "Later", "HOLD:later").await;
    host.call("runRoutine", json!({"botId": bot, "id": later})).await;
    run(&host, &bot, &later, "cancelled").await;
    assert!(work(&host, &bot).await.iter().all(|task| task["title"] != "Later"), "{:?}", work(&host, &bot).await);

    // Resuming starts neither the stopped task nor the cancelled run; new work does run.
    host.call("updateBot", json!({"id": bot, "paused": false})).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(ran(&logged(&host.workspace(&bot), "prompts.jsonl")), 1);
    assert_eq!(task(&host, &bot, "Held", "stopped").await["thread"], working["thread"]);
    let after = routine(&host, &bot, "After", "PASS").await;
    host.call("runRoutine", json!({"botId": bot, "id": after})).await;
    task(&host, &bot, "After", "done").await;
    run(&host, &bot, &after, "succeeded").await;
}

#[tokio::test]
async fn a_pause_during_session_setup_writes_no_prompt_and_the_message_does_not_run_after_resume() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    let workspace = host.workspace(&bot);
    let barrier = workspace.join("setup-barrier");
    let waiting = workspace.join("setup-barrier-waiting");
    std::fs::write(&barrier, "").unwrap();
    let sent = json!({"botId": bot, "text": "PAUSED_DURING_SETUP", "clientNonce": "pause-setup"});
    host.call("send", sent).await;
    wait("the agent to wait in session setup", async || waiting.exists().then_some(())).await;

    // The pause returns while the turn is still setting up. Releasing setup afterwards must not submit its prompt.
    host.call("updateBot", json!({"id": bot, "paused": true})).await;
    std::fs::remove_file(&barrier).unwrap();
    wait("the waiting session setup to finish", async || (!waiting.exists()).then_some(())).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let prompts = logged(&workspace, "prompts.jsonl");
    assert!(
        prompts.iter().all(|p| !p["text"].as_str().unwrap().contains("PAUSED_DURING_SETUP")),
        "a prompt was written after the pause returned: {prompts:?}"
    );

    // Resume does not revive it; a new message is a new turn.
    host.call("updateBot", json!({"id": bot, "paused": false})).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    host.call("send", json!({"botId": bot, "text": "AFTER_RESUME", "clientNonce": "after-resume"})).await;
    assert_eq!(reply(&host, &bot, "reply: AFTER_RESUME").await, "reply: AFTER_RESUME");
    let prompts = logged(&workspace, "prompts.jsonl");
    assert_eq!(prompts.iter().filter(|p| p["text"].as_str().unwrap().contains("PAUSED_DURING_SETUP")).count(), 0);
    assert_eq!(prompts.iter().filter(|p| p["text"].as_str().unwrap().contains("AFTER_RESUME")).count(), 1);
}

#[tokio::test]
async fn a_request_queued_before_pause_is_refused_after_the_resume() {
    let host = Host::start().await;
    let asker = host.fixture_bot("Ada", "work_agent.py", "").await;
    let bot = bot_with(&host, 1).await;
    let workspace = host.workspace(&bot);
    let barrier = workspace.join("setup-barrier");
    let waiting = workspace.join("setup-barrier-waiting");
    std::fs::write(&barrier, "").unwrap();
    host.call("send", json!({"botId": bot, "text": "SETUP_FOR_REQUEST", "clientNonce": "request-setup"})).await;
    wait("the agent to wait in session setup", async || waiting.exists().then_some(())).await;

    // The request is made while the target is unpaused, then its actor is kept in setup while pause and resume both
    // finish. It must not become a turn after the resume.
    host.reply(&asker, &format!("MESSAGE {bot}|SHOULD_NOT_RUN_REQUEST")).await;
    host.call("updateBot", json!({"id": bot, "paused": true})).await;
    std::fs::remove_file(&barrier).unwrap();
    host.call("updateBot", json!({"id": bot, "paused": false})).await;
    let answer = wait("the refused request's reply turn", async || {
        let prompts = host.log(&asker, "prompts.jsonl");
        prompts.into_iter().find(|p| {
            let text = p["text"].as_str().unwrap();
            text.contains("message_bot request")
                && text.contains("couldn't finish")
                && text.contains("couldn't start recipient: this bot is paused")
        })
    })
    .await;
    let text = answer["text"].as_str().unwrap();
    assert!(text.contains("couldn't start recipient"), "{text}");
    assert!(
        logged(&workspace, "prompts.jsonl")
            .iter()
            .all(|p| !p["text"].as_str().unwrap().contains("SHOULD_NOT_RUN_REQUEST")),
        "the paused request reached the agent"
    );
}

#[tokio::test]
async fn a_stuck_prompt_write_does_not_freeze_pause_or_another_bot() {
    let host = Host::start().await;
    let stuck = host.fixture_bot("Stuck input", "stuck_input_agent.py", "").await;
    let other = host.fixture_bot("Other", "work_agent.py", "").await;
    host.call("send", json!({"botId": stuck, "text": "x".repeat(512 * 1024)})).await;
    wait("the agent to stop reading its input", async || {
        host.workspace(&stuck).join("session-ready").exists().then_some(())
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let base = host.base.clone();
    let token = host.token.clone();
    let pausing = tokio::spawn(async move {
        reqwest::Client::new()
            .post(format!("{base}/api/updateBot"))
            .bearer_auth(token)
            .json(&json!({"id": stuck, "paused": true}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    tokio::time::timeout(
        Duration::from_secs(8),
        host.call("updateBot", json!({"id": other, "description": "still responsive"})),
    )
    .await
    .expect("another bot froze behind the stuck prompt write");
    tokio::time::timeout(Duration::from_secs(15), pausing)
        .await
        .expect("the pause did not stop the stuck agent")
        .unwrap();
    let snapshot = host.snapshot().await;
    assert_eq!(common::bot(&snapshot, &other)["description"], "still responsive");
}

#[tokio::test]
async fn a_quick_pause_and_resume_during_setup_is_cancelled_not_a_start_failure() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    let workspace = host.workspace(&bot);
    let barrier = workspace.join("setup-barrier");
    let waiting = workspace.join("setup-barrier-waiting");
    std::fs::write(&barrier, "").unwrap();
    host.call("send", json!({"botId": bot, "text": "QUICK_PAUSE_RESUME", "clientNonce": "quick-pause-resume"})).await;
    wait("the agent to wait in session setup", async || waiting.exists().then_some(())).await;

    host.call("updateBot", json!({"id": bot, "paused": true})).await;
    host.call("updateBot", json!({"id": bot, "paused": false})).await;
    std::fs::remove_file(&barrier).unwrap();

    wait("the old turn to settle", async || {
        history(&host, &bot)
            .await
            .into_iter()
            .any(|e| {
                let text = e["data"]["text"].as_str().unwrap_or_default();
                e["kind"] == "notice"
                    && (text == "This bot is paused. Resume it to respond."
                        || text.contains("QUICK_PAUSE_RESUME")
                        || text.contains("Couldn't start the agent"))
            })
            .then_some(())
    })
    .await;
    let entries = history(&host, &bot).await;
    assert!(
        entries.iter().any(|e| e["data"]["text"] == "This bot is paused. Resume it to respond."),
        "the old turn was not classified as paused: {entries:?}"
    );
    assert!(
        entries.iter().all(|e| !e["data"]["text"].as_str().unwrap_or_default().contains("Couldn't start the agent")),
        "a paused setup became a start failure: {entries:?}"
    );
    let sent = entries.iter().find(|e| e["data"]["text"] == "QUICK_PAUSE_RESUME").expect("the sent message");
    assert_eq!(sent["data"]["status"], "cancelled");
    assert!(
        logged(&workspace, "prompts.jsonl").iter().all(|p| !p["text"].as_str().unwrap().contains("QUICK_PAUSE_RESUME"))
    );
}

#[tokio::test]
async fn an_old_pause_notification_does_not_cancel_a_fresh_routine_after_resume() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    let workspace = host.workspace(&bot);
    let barrier = workspace.join("setup-barrier");
    let waiting = workspace.join("setup-barrier-waiting");
    std::fs::write(&barrier, "").unwrap();
    host.call("send", json!({"botId": bot, "text": "KEEP_ACTOR_IN_SETUP", "clientNonce": "keep-actor-in-setup"})).await;
    wait("the agent to wait in session setup", async || waiting.exists().then_some(())).await;
    let fresh = routine(&host, &bot, "Fresh after resume", "PASS").await;

    host.call("updateBot", json!({"id": bot, "paused": true})).await;
    host.call("updateBot", json!({"id": bot, "paused": false})).await;
    host.call("runRoutine", json!({"botId": bot, "id": fresh})).await;
    std::fs::remove_file(&barrier).unwrap();

    let finished = wait("the fresh run to settle", async || {
        host.call("routines", json!({"botId": bot})).await["runs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|run| {
                run["routineId"] == fresh
                    && !matches!(run["status"].as_str(), Some("pending" | "starting" | "running" | "recovering"))
            })
            .cloned()
    })
    .await;
    assert_eq!(finished["status"], "succeeded", "old pause cleanup cancelled a fresh run: {finished}");
    assert_eq!(finished["detail"], Value::Null);
}

#[tokio::test]
async fn a_restart_while_paused_neither_resumes_cut_off_work_nor_spends_after_resume() {
    let mut host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "hello").await;
    let held = routine(&host, &bot, "Held", "HOLD:held").await;
    host.call("runRoutine", json!({"botId": bot, "id": held})).await;
    let working = task(&host, &bot, "Held", "working").await;
    let ran = |prompts: &[Value]| {
        prompts.iter().filter(|p| p["text"].as_str().is_some_and(|t| t.contains("Scheduled routine: Held"))).count()
    };
    wait("the held run's prompt", async || (ran(&logged(&host.workspace(&bot), "prompts.jsonl")) == 1).then_some(()))
        .await;

    host.call("updateBot", json!({"id": bot, "paused": true})).await;
    host.restart(|_| {}).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(ran(&logged(&host.workspace(&bot), "prompts.jsonl")), 1, "the paused restart resumed the run");
    let after = work(&host, &bot).await.into_iter().find(|task| task["id"] == working["id"]).unwrap();
    assert_ne!(after["status"], "working", "{after}");
    assert_ne!(after["status"], "done", "{after}");

    // Resume does not bring the cut-off routine back, and a new chat turn does run.
    host.call("updateBot", json!({"id": bot, "paused": false})).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(ran(&logged(&host.workspace(&bot), "prompts.jsonl")), 1, "resume replayed a paused run");
    host.call("send", json!({"botId": bot, "text": "AFTER_PAUSED_RESTART", "clientNonce": "after-paused-restart"}))
        .await;
    assert_eq!(reply(&host, &bot, "reply: AFTER_PAUSED_RESTART").await, "reply: AFTER_PAUSED_RESTART");
}

#[tokio::test]
async fn a_routine_run_cut_off_by_a_host_restart_picks_up_and_posts_once() {
    let mut host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "hello").await;
    let held = routine(&host, &bot, "Held", "HOLD:held").await;
    host.call("runRoutine", json!({"botId": bot, "id": held})).await;
    let working = task(&host, &bot, "Held", "working").await;

    host.restart(|_| {}).await;
    // Its thread resumes the turn; what that turn says is the routine's message, once.
    let result = reply(&host, &bot, "reply: [Codync restarted").await;
    assert!(result.contains("interrupted you mid-turn"), "{result}");
    assert_eq!(task(&host, &bot, "Held", "done").await["thread"], working["thread"]);
    run(&host, &bot, &held, "succeeded").await;
    assert_eq!(said(&host, &bot, "agent", "reply: [Codync restarted").await.len(), 1);
    assert_eq!(said(&host, &bot, "notice", "Routine could not finish").await.len(), 0);
}

#[tokio::test]
async fn another_bots_request_is_a_task_and_its_last_message_is_the_reply() {
    let host = Host::start().await;
    let asker = host.fixture_bot("Ada", "work_agent.py", "").await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "hello").await;

    host.reply(&asker, &format!("MESSAGE {bot}|Review the diff HOLD:review ASK SEND FOLLOW NEST")).await;
    let working = task(&host, &bot, "Review the diff HOLD:review ASK SEND FOLLOW NEST", "working").await;
    assert_eq!(working["source"], json!({"kind": "request", "id": asker}));
    let notice = said(&host, &bot, "notice", "Message from Ada").await.remove(0);
    assert_eq!(notice["data"]["status"], "sent", "the exchange reads as running");
    // The asked bot's chat stays free while its thread answers.
    host.reply(&bot, "are you there?").await;

    release(&host, &bot, "review");
    task(&host, &bot, "Review the diff HOLD:review ASK SEND FOLLOW NEST", "done").await;
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
    assert!(prompt.contains("Refused: send_message isn't available in a thread"), "{prompt}");
    assert!(prompt.contains("Refused: follow_up works only in your own chat"), "{prompt}");
    assert!(prompt.contains("Refused: add_task isn't available in a thread"), "{prompt}");
    let prompts = host.log(&bot, "prompts.jsonl");
    let answered = prompts.iter().find(|p| p["text"].as_str().unwrap().contains("sent you this request")).unwrap();
    assert!(
        answered["text"].as_str().unwrap().contains("another bot asked you"),
        "a request's instructions: {answered}"
    );
    let notice = said(&host, &bot, "notice", "Message from Ada").await.remove(0);
    assert_eq!(notice["data"]["status"], "completed");
    assert!(notice["data"]["botMessage"]["reply"].as_str().unwrap().starts_with("Request done:"), "{notice}");
    assert_eq!(said(&host, &bot, "agent", "Request done:").await.len(), 0, "no bubble in the asked bot's chat");
    assert_eq!(relayed(&host, &bot).await, [] as [String; 0], "and no turn of its own for it");

    // A reply of exactly `(pass)` goes back as it is, as it did from a turn in the chat.
    host.reply(&asker, &format!("MESSAGE {bot}|Quiet PASS")).await;
    assert_eq!(task(&host, &bot, "Quiet PASS", "done").await["result"], "(pass)");
    wait("the passed reply", async || {
        let notices = said(&host, &bot, "notice", "Message from Ada").await;
        notices.iter().any(|n| n["data"]["botMessage"]["reply"] == "(pass)").then_some(())
    })
    .await;

    // A request stopped in the box is cancelled for the bot that asked; it can't be tried again.
    host.reply(&asker, &format!("MESSAGE {bot}|Second HOLD:second")).await;
    let second = task(&host, &bot, "Second HOLD:second", "working").await;
    host.call("stopWork", json!({"botId": bot, "taskId": id(&second)})).await;
    let stopped = task(&host, &bot, "Second HOLD:second", "stopped").await;
    wait("the exchange to read cancelled", async || {
        let notices = said(&host, &bot, "notice", "Message from Ada").await;
        notices.iter().any(|n| n["data"]["status"] == "cancelled").then_some(())
    })
    .await;
    let refused = host.refused("retryWork", json!({"botId": bot, "taskId": id(&stopped)})).await;
    assert_eq!(refused, "another bot's request can't be tried again: that bot asks again");
}

#[tokio::test]
async fn a_thread_asks_the_user_to_choose_and_shares_in_the_chat() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;
    host.reply(&bot, "hello").await;
    host.reply(&bot, "ADD Pick one|ASK then SHARE QUOTE and ARTIFACT").await;

    let card = wait("the thread's choice card in the chat", async || {
        let mut cards = history(&host, &bot).await.into_iter().filter(|e| e["data"]["decision"] == true);
        cards.rfind(|e| e["data"]["status"] == "pending")
    })
    .await;
    let waits = task(&host, &bot, "Pick one", "needsYou").await;
    assert_eq!(card["data"]["task"], waits["id"], "the card names the task");
    assert_eq!(card["data"]["thread"], json!({"id": waits["thread"], "title": "Pick one"}));
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

    // The file and the artifact's card are the bot's messages in the chat, named by the task.
    let file = chat.iter().find(|e| e["data"]["files"].is_array()).expect("the file's message");
    assert_eq!(
        (&file["kind"], &file["data"]["task"], &file["data"]["author"]),
        (&json!("agent"), &waits["id"], &json!(bot))
    );
    assert_eq!(file["data"]["files"][0]["name"], "shared.txt");
    // Its `reply_to` quotes a message of the bot's chat, where the file shows, not of the thread's own (issue #402).
    let asked = chat
        .iter()
        .find(|e| e["kind"] == "user" && e["data"]["text"] == "ADD Pick one|ASK then SHARE QUOTE and ARTIFACT");
    assert_eq!(file["data"]["replyTo"]["id"], asked.expect("the message that added the task")["id"]);
    let stored = host.home.join("bots").join(&bot).join("files").join(file["data"]["files"][0]["id"].as_str().unwrap());
    assert!(stored.exists(), "kept with the bot, not with the thread: {}", stored.display());
    let page = chat.iter().find(|e| !e["data"]["artifact"].is_null()).expect("the artifact's card");
    assert_eq!((&page["data"]["task"], &page["data"]["author"]), (&waits["id"], &json!(bot)));
    let library = host.call("library", json!({"botId": bot})).await;
    assert!(library.to_string().contains("Page"), "the artifact is in the bot's Library: {library}");
}

#[tokio::test]
async fn a_threads_choice_card_ends_with_its_task() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;
    host.reply(&bot, "ADD Pick one|ASK").await;
    let pending = async || {
        let mut cards = history(&host, &bot).await.into_iter().filter(|e| e["data"]["decision"] == true);
        cards.next_back()
    };
    let card = wait("the card", async || pending().await.filter(|e| e["data"]["status"] == "pending")).await;
    let waits = task(&host, &bot, "Pick one", "needsYou").await;

    // Stopping the chat's own turn is not the thread's: the card still waits. Stopping the task ends it.
    host.call("stop", json!({"botId": bot})).await;
    assert_eq!(pending().await.unwrap()["data"]["status"], "pending");
    host.call("stopWork", json!({"botId": bot, "taskId": id(&waits)})).await;
    wait("the card to expire", async || pending().await.filter(|e| e["data"]["status"] == "expired")).await;
    let late = host.refused("respondDecision", json!({"entryId": card["id"], "option": "A"})).await;
    assert_eq!(late, "that card is already answered or expired");
}

#[tokio::test]
async fn a_threads_choice_card_expires_with_its_270_second_call() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;
    host.reply(&bot, "ADD Pick one|ASK").await;
    let card = wait("the card", async || {
        let mut cards = history(&host, &bot).await.into_iter().filter(|e| e["data"]["decision"] == true);
        cards.rfind(|e| e["data"]["status"] == "pending")
    })
    .await;
    // Unlike a chat's card it doesn't stay open: a press there has no next message to become.
    common::eventually("the thread's card to expire at its deadline", Duration::from_secs(290), async || {
        history(&host, &bot).await.iter().any(|e| e["id"] == card["id"] && e["data"]["status"] == "expired")
    })
    .await;
    let relayed = reply(&host, &bot, "Relayed:").await;
    assert!(relayed.contains("Refused: the card expired after four and a half minutes"), "{relayed}");
}

#[tokio::test]
async fn a_threads_edit_memory_is_its_own_turns_so_it_adds_and_never_takes_out() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.write_memory(Some(&bot), "MEMORY.md", "- Likes tea\n- Has two cats\n");
    host.reply(&bot, "hello").await;
    host.reply(&bot, "ADD Notes|HOLD:notes REMEMBER FORGET").await;
    task(&host, &bot, "Notes", "working").await;
    // The chat runs a turn the user started, which may take lines out; the thread's turn is its own, and may not.
    let nonce = uuid::Uuid::new_v4().to_string();
    host.call("send", json!({"botId": bot, "text": "ADD Other|x|hold", "clientNonce": nonce})).await;
    task(&host, &bot, "Other", "waiting").await;
    release(&host, &bot, "notes");
    let done = task(&host, &bot, "Notes", "done").await;
    let result = done["result"].as_str().unwrap();
    assert!(result.contains("Remembered: {\"status\":\"saved\""), "{result}");
    assert!(result.contains("Refused: Only in a turn the user started in your own chat"), "{result}");
    let main = host.memory_folder(Some(&bot)).join("MEMORY.md");
    assert_eq!(std::fs::read_to_string(main).unwrap(), "- Likes tea\n- Has two cats\n- Works at night\n");
    // The line it added is the bot's, as one added from the chat is.
    let detail = host.call("memoryFile", json!({"botId": bot, "file": "MEMORY.md"})).await;
    assert_eq!(detail["versions"][0]["author"], "bot", "{detail}");
    // It was written in the thread's session, not the chat's: the chat hears of it with its next message, which is
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

/// A bot like [`bot_with`]'s that works in a project folder of its own (not its own folder, where threads never get
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

/// The bot's working task called `title`, once it has a worktree.
async fn in_worktree(host: &Host, bot: &str, title: &str) -> Value {
    wait(&format!("{title}'s worktree"), async || {
        let found = work(host, bot).await.into_iter().find(|i| i["title"] == title && i["status"] == "working");
        found.filter(|i| i["worktree"].is_object())
    })
    .await
}

/// The chat's `Relayed:` reply for the task called `title`, once it is there.
async fn relayed_for(host: &Host, bot: &str, title: &str) -> String {
    let report = format!("Report {title}:");
    wait(&format!("{title}'s report"), async || relayed(host, bot).await.into_iter().find(|t| t.contains(&report)))
        .await
}

#[tokio::test]
async fn a_thread_beside_another_in_one_repository_works_in_a_worktree_of_its_own() {
    let host = Host::start().await;
    let (bot, dir) = project_bot(&host, 3).await;
    assert_eq!(row(&host, &bot).await["gitRepo"], false);
    if !git_repo(&dir) {
        return;
    }
    host.reply(&bot, "hello").await;
    let settings = row(&host, &bot).await;
    assert_eq!((&settings["gitRepo"], &settings["threadWorktrees"]), (&json!(true), &json!(true)), "on unless set");
    let branches = || {
        let out = git_in(&dir).args(["branch", "--list", "codync/*"]).output();
        String::from_utf8(out.unwrap().stdout).unwrap()
    };

    // The first thread alone keeps the bot's folder; each one that starts beside it gets a worktree and a branch.
    host.reply(&bot, "ADD First|HOLD:first").await;
    let first = task(&host, &bot, "First", "working").await;
    host.reply(&bot, "ADD Second thread|HOLD:second").await;
    let second = in_worktree(&host, &bot, "Second thread").await;
    host.reply(&bot, "ADD Third|TOUCH HOLD:third").await;
    let third = in_worktree(&host, &bot, "Third").await;
    assert!(task(&host, &bot, "First", "working").await["worktree"].is_null(), "{first}");
    let folder = |task: &Value| PathBuf::from(task["worktree"]["path"].as_str().unwrap());
    assert_eq!(second["worktree"]["branch"], "codync/second-thread");
    assert_eq!(folder(&second), host.home.join("worktrees").join(&bot).join(id(&second)));
    assert_eq!(third["worktree"]["branch"], "codync/third");
    // Its agent runs there, from the folder's commit, and was told so.
    assert!(folder(&second).join("README.md").exists());
    let told = wait("the thread's prompt in its worktree", async || {
        std::fs::read_to_string(folder(&second).join("prompts.jsonl")).ok()
    })
    .await;
    assert!(told.contains("you work in a git worktree of your own") && told.contains("codync/second-thread"), "{told}");
    assert_eq!(task_prompts_in(&dir, "Second thread"), [] as [Value; 0], "not in the bot's folder");
    assert!(logged(&dir, "forks.jsonl").is_empty(), "no thread forks the chat");

    // Nothing changed in it: the worktree and its branch go, and the report says so.
    std::fs::write(folder(&second).join("release-second"), "").unwrap();
    let report = relayed_for(&host, &bot, "Second thread").await;
    let done = task(&host, &bot, "Second thread", "done").await;
    // On Windows a folder a process is still in can't be removed: the worktree stays and the report says where.
    if cfg!(windows) {
        assert!(report.contains("branch codync/second-thread") && report.contains("nothing was merged"), "{report}");
    } else {
        let removed = "had no changes and no commits your folder doesn't have, and was removed with its branch";
        assert!(report.contains(removed), "{report}");
        assert!(done["worktree"].is_null() && !folder(&second).exists(), "{done}");
        assert!(!branches().contains("codync/second-thread"), "{}", branches());
    }

    // A file it made: both stay, and the report names the branch and the folder for the bot to tell the user.
    std::fs::write(folder(&third).join("release-third"), "").unwrap();
    let report = relayed_for(&host, &bot, "Third").await;
    let place = format!("branch codync/third, in {}", folder(&third).display());
    assert!(report.contains(&place) && report.contains("nothing was merged"), "{report}");
    assert_eq!(task(&host, &bot, "Third", "done").await["worktree"], third["worktree"]);
    assert!(folder(&third).join("made.txt").exists() && branches().contains("codync/third"));
    assert!(!dir.join("made.txt").exists(), "not in the bot's folder");

    // With the switch off, a thread beside another works in the bot's folder as before.
    host.call("updateBot", json!({"id": bot, "threadWorktrees": false})).await;
    host.reply(&bot, "ADD Plain|HOLD:plain").await;
    let plain = task(&host, &bot, "Plain", "working").await;
    wait("its prompt in the bot's folder", async || task_prompts_in(&dir, "Plain").pop()).await;
    assert!(task(&host, &bot, "Plain", "working").await["worktree"].is_null(), "{plain}");
    for name in ["plain", "first"] {
        std::fs::write(dir.join(format!("release-{name}")), "").unwrap();
    }
    task(&host, &bot, "First", "done").await;

    // A project folder that is no repository never gets one, and neither does a bot's own folder, a repository
    // Codync keeps versions in: all the bot's work stays there.
    let (plain, plain_dir) = project_bot(&host, 3).await;
    let own = bot_with(&host, 3).await;
    wait("its own folder to keep versions", async || host.workspace(&own).join(".git").exists().then_some(())).await;
    for (bot, dir) in [(&plain, plain_dir), (&own, host.workspace(&own))] {
        host.reply(bot, "ADD One|HOLD:one").await;
        task(&host, bot, "One", "working").await;
        host.reply(bot, "ADD Two|HOLD:two").await;
        task(&host, bot, "Two", "working").await;
        wait("its prompt in the bot's folder", async || task_prompts_in(&dir, "Two").pop()).await;
        assert!(work(&host, bot).await.iter().all(|i| i["worktree"].is_null()));
    }
    assert_eq!(row(&host, &plain).await["gitRepo"], false);
}

#[tokio::test]
async fn a_bot_lives_at_home_and_sends_work_to_its_projects_by_its_worktree_mode() {
    let host = Host::start().await;
    assert_eq!(host.call("hello", json!({})).await["projects"], true);
    let bot = bot_with(&host, 3).await;
    let projects = host.home.join("projects");
    let (a, b) = (projects.join("a"), projects.join("b"));
    for dir in [&a, &b] {
        std::fs::create_dir_all(dir).unwrap();
        if !git_repo(dir) {
            return;
        }
    }
    // An older app picking a project folder: it becomes the first project, and the bot stays home.
    host.call("updateBot", json!({"id": bot, "cwd": a, "name": "Mika"})).await;
    let settings = row(&host, &bot).await;
    let home = host.workspace(&bot);
    assert_eq!((&settings["cwd"], &settings["projects"]), (&json!(home), &json!([a])));
    assert_eq!((&settings["gitRepo"], &settings["projectGitRepos"]), (&json!(true), &json!([true])));
    assert_eq!((&settings["worktreeMode"], &settings["threadWorktrees"]), (&json!("sideBySide"), &json!(true)));
    host.call("updateBot", json!({"id": bot, "projects": [a, b, a]})).await;
    assert_eq!(row(&host, &bot).await["projects"], json!([a, b]), "each once");
    let missing = projects.join("missing");
    assert!(host.refused("updateBot", json!({"id": bot, "projects": [a, missing]})).await.contains("does not exist"));

    // Side by side counts the threads in one checkout: one in another repository doesn't give it a worktree.
    host.reply(&bot, "ADD In A|HOLD:a").await;
    task(&host, &bot, "In A", "working").await;
    wait("its prompt in the default project", async || task_prompts_in(&a, "In A").pop()).await;
    host.reply(&bot, &format!("ADD In B|HOLD:b|folder={}", b.display())).await;
    wait("its prompt in the folder it names", async || task_prompts_in(&b, "In B").pop()).await;
    assert!(work(&host, &bot).await.iter().all(|i| i["worktree"].is_null()), "one thread in each checkout");
    host.reply(&bot, "ADD Bad|x|folder=projects/b").await;
    reply(&host, &bot, "Refused: folder must be the full path").await;

    // Always: the first thread in a checkout gets one too. An older app's switch, saved with another field, keeps it.
    host.call("updateBot", json!({"id": bot, "worktreeMode": "always"})).await;
    host.call("updateBot", json!({"id": bot, "description": "Ships", "threadWorktrees": true})).await;
    assert_eq!(row(&host, &bot).await["worktreeMode"], "always");
    std::fs::write(a.join("release-a"), "").unwrap();
    task(&host, &bot, "In A", "done").await;
    host.reply(&bot, "ADD First in A|HOLD:c").await;
    let first = in_worktree(&host, &bot, "First in A").await;
    assert_eq!(first["worktree"]["branch"], "codync/first-in-a");
    assert!(task_prompts_in(&a, "First in A").is_empty(), "not in the user's checkout");
    host.call("updateBot", json!({"id": bot, "threadWorktrees": false})).await;
    assert_eq!(row(&host, &bot).await["worktreeMode"], "off");
}

#[tokio::test]
async fn a_bots_own_folder_keeps_a_version_before_and_after_a_turn_and_a_project_folder_none() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "").await;
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

/// A bot on `work_agent.py` run as Kimi Code, a harness Codync knows (so a sign-in it needs gets a card) that no
/// machine running the tests has installed, with `fallbacks`.
async fn kimi_bot(host: &Host, name: &str, fallbacks: Value) -> String {
    let bot = json!({"name": name, "backend": "kimi", "command": common::fixture_command("work_agent.py", ""),
        "connectors": [], "notify": false, "fallbacks": fallbacks});
    host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned()
}

/// The fixture agent `relay_agent.py <name>`: it answers "<name> answered" and logs its prompts to `<name>.jsonl`.
fn relay_agent(name: &str) -> String {
    common::fixture_command("relay_agent.py", name)
}

/// Writes the flag file `name` in `bot`'s folder (see `work_agent.py`).
fn flag(host: &Host, bot: &str, name: &str) -> PathBuf {
    let path = host.workspace(bot).join(name);
    std::fs::write(&path, "").unwrap();
    path
}

/// The newest entry of `bot`'s chat that `found` picks, once it is there.
async fn entry(host: &Host, bot: &str, what: &str, found: impl Fn(&Value) -> bool) -> Value {
    wait(what, async || history(host, bot).await.into_iter().rev().find(|e| found(e))).await
}

/// The transcript of `task`'s thread.
async fn transcript(host: &Host, bot: &str, task: &Value) -> Vec<Value> {
    let entries = host.call("threadEntries", json!({"botId": bot, "threadId": task["thread"]})).await;
    entries["entries"].as_array().unwrap().clone()
}

#[tokio::test]
async fn a_threads_sign_in_is_asked_in_the_chat_and_its_task_goes_on_in_the_same_thread_once_signed_in() {
    let host = Host::start().await;
    let bot = kimi_bot(&host, "Mika", json!([])).await;
    host.reply(&bot, "hello").await;
    let signed_out = flag(&host, &bot, "thread-signedout");

    host.reply(&bot, "ADD Login|anything").await;
    let waiting = task(&host, &bot, "Login", "needsYou").await;
    assert_eq!(waiting["activity"], "Needs Kimi Code sign-in");
    // The card is the user's to press, so it is in the chat, marked as the task's, holding its message.
    let card = entry(&host, &bot, "the sign-in card", |e| e["data"]["task"] == waiting["id"]).await;
    let request = &card["data"]["connectionRequest"];
    assert_eq!((request["kind"].as_str(), request["status"].as_str()), (Some("agent"), Some("pending")), "{card}");
    assert_eq!(request["botId"], bot, "the chat's bot, which the apps know");
    assert_eq!(request["entryIds"].as_array().unwrap().len(), 1, "{card}");
    assert_eq!(card["data"]["thread"], json!({"id": waiting["thread"], "title": "Login"}));
    // The thread's own transcript only says where the ask went.
    let asked = transcript(&host, &bot, &waiting).await;
    assert!(asked.iter().any(|e| e["data"]["text"] == "Asked in the chat to sign in to Kimi Code."), "{asked:?}");
    assert!(!asked.iter().any(|e| e["data"]["connectionRequest"].is_object()), "{asked:?}");
    assert_eq!(relayed(&host, &bot).await, [] as [String; 0], "the task isn't over: no report");

    // Signed in (a turn of the agent that ends well says so: here the chat's own), the task's message goes out again
    // to the same thread.
    std::fs::remove_file(signed_out).unwrap();
    host.reply(&bot, "hello again").await;
    let done = task(&host, &bot, "Login", "done").await;
    assert_eq!(done["thread"], waiting["thread"], "the same thread");
    // A new agent process (the old one kept the old sign-in) goes on in the thread's session: what it did is still
    // there.
    let tries = task_prompts(&host, &bot, "Login");
    assert_eq!(tries.len(), 2, "{tries:?}");
    assert_eq!(tries[0]["session"], tries[1]["session"], "{tries:?}");
    assert!(reply(&host, &bot, "Relayed:").await.contains("Login"));
    let card = entry(&host, &bot, "the card", |e| e["id"] == card["id"]).await;
    assert_eq!(card["data"]["connectionRequest"]["status"], "ready");
}

#[tokio::test]
async fn a_threads_sign_in_on_an_agent_that_cant_reopen_a_session_tells_the_new_one_nothing_of_the_chat() {
    let host = Host::start().await;
    let command = common::fixture_command("work_agent.py", "no-load");
    let bot = json!({"name": "Mika", "backend": "kimi", "command": command, "connectors": [], "notify": false});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    host.reply(&bot, "Remember the blue theme").await;
    let signed_out = flag(&host, &bot, "thread-signedout");

    host.reply(&bot, "ADD Login|anything").await;
    task(&host, &bot, "Login", "needsYou").await;
    std::fs::remove_file(signed_out).unwrap();
    host.reply(&bot, "hello again").await;
    task(&host, &bot, "Login", "done").await;
    // A new agent process can't take the thread's session up again: a new one, still nothing of the chat.
    let tries = task_prompts(&host, &bot, "Login");
    assert_eq!(tries.len(), 2, "{tries:?}");
    assert_ne!(tries[0]["session"], tries[1]["session"], "{tries:?}");
    let text = tries[1]["text"].as_str().unwrap();
    assert!(!text.contains("blue theme"), "{text}");
}

#[tokio::test]
async fn a_thread_moves_past_a_signed_out_agent_asks_in_the_chat_and_its_next_agent_hears_its_work_alone() {
    let host = Host::start().await;
    let bot = kimi_bot(&host, "Mika", json!([{"backend": "codex", "command": relay_agent("standin")}])).await;
    host.reply(&bot, "Remember the blue theme").await;
    flag(&host, &bot, "thread-signedout");

    host.reply(&bot, "ADD Login|anything").await;
    let done = task(&host, &bot, "Login", "done").await;
    assert_eq!(done["result"], "standin answered");
    // Nothing waits on the sign-in, and still the user is asked where they are.
    let card = entry(&host, &bot, "the sign-in card", |e| e["data"]["task"] == done["id"]).await;
    assert_eq!(card["data"]["connectionRequest"]["entryIds"], json!([]), "{card}");
    let moved = transcript(&host, &bot, &done).await;
    let texts: Vec<&str> = moved.iter().filter_map(|e| e["data"]["text"].as_str()).collect();
    assert!(texts.contains(&"Kimi Code is signed out. Codex carried on."), "{texts:?}");
    assert!(texts.contains(&"Asked in the chat to sign in to Kimi Code."), "{texts:?}");
    // The next agent is told the thread's own work, nothing of the chat.
    let log = host.log(&bot, "standin.jsonl");
    let prompt = log.iter().rfind(|r| r["method"] == "session/prompt").unwrap()["params"]["prompt"][0]["text"].clone();
    let prompt = prompt.as_str().unwrap();
    assert!(!prompt.contains("blue theme"), "{prompt}");
}

#[tokio::test]
async fn a_threads_offer_to_add_a_fallback_is_in_the_chat_and_new_fallbacks_reach_a_running_thread() {
    let host = Host::start().await;
    let bot = bot_with(&host, 2).await;
    host.reply(&bot, "hello").await;
    let limited = flag(&host, &bot, "thread-limit");

    host.reply(&bot, "ADD Big|anything").await;
    let failed = task(&host, &bot, "Big", "failed").await;
    let offer = entry(&host, &bot, "the offer", |e| e["data"]["addFallback"] == true).await;
    assert_eq!((&offer["data"]["task"], &offer["data"]["author"]), (&failed["id"], &json!(bot)), "{offer}");
    assert!(offer["data"]["text"].as_str().unwrap().contains("You've hit your limit"), "{offer}");
    assert!(offer["data"]["stopReason"].is_null(), "it ends no turn of the bot's own: {offer}");
    let kept = transcript(&host, &bot, &failed).await;
    let error = kept.iter().find(|e| e["data"]["style"] == "error").unwrap();
    assert!(error["data"]["addFallback"].is_null(), "the thread's transcript keeps only the failure: {error}");

    // Fallbacks set while a thread works reach it: it falls back like the bot.
    std::fs::remove_file(&limited).unwrap();
    host.reply(&bot, "ADD Slow|HOLD:slow").await;
    task(&host, &bot, "Slow", "working").await;
    let fallbacks = json!([{"backend": "codex", "command": relay_agent("standin")}]);
    host.call("updateBot", json!({"id": bot, "fallbacks": fallbacks})).await;
    flag(&host, &bot, "thread-limit");
    release(&host, &bot, "slow");
    assert_eq!(task(&host, &bot, "Slow", "done").await["result"], "standin answered");
}

#[tokio::test]
async fn a_thread_that_starts_on_a_fallback_never_forks_the_chats_own_agents_session() {
    let host = Host::start().await;
    // The fallback can fork too (it would fork any session it's given).
    let fallbacks = json!([{"backend": "codex", "command": common::fixture_command("work_agent.py", "")}]);
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

    // A request starts a task with no turn of the chat's in between: the chat is still on its own agent.
    let asker = host.fixture_bot("Ada", "work_agent.py", "").await;
    host.reply(&asker, &format!("MESSAGE {bot}|Review the diff")).await;
    task(&host, &bot, "Review the diff", "done").await;
    let forks = host.workspace(&bot).join("forks.jsonl");
    assert!(!forks.exists(), "{}", std::fs::read_to_string(&forks).unwrap_or_default());
}

/// Writes `text` to the thread of `task`: what the host answers.
async fn tell(host: &Host, bot: &str, task: &Value, text: &str) -> Value {
    let nonce = uuid::Uuid::new_v4().to_string();
    host.call("send", json!({"botId": bot, "taskId": task["id"], "text": text, "clientNonce": nonce})).await
}

/// The entry `id` of the thread of `task`, as its transcript holds it; none once the user took it back.
async fn thread_entry(host: &Host, bot: &str, task: &Value, id: &Value) -> Option<Value> {
    transcript(host, bot, task).await.into_iter().find(|e| e["id"] == *id)
}

/// The entry `id` of `task`'s thread, once it reads `status`.
async fn thread_status(host: &Host, bot: &str, task: &Value, id: &Value, status: &str) -> Value {
    wait(&format!("the message to be {status}"), async || {
        thread_entry(host, bot, task, id).await.filter(|e| e["data"]["status"] == status)
    })
    .await
}

/// The prompts a thread of `bot` got for what the user wrote to its thread, oldest first.
fn thread_prompts(host: &Host, bot: &str) -> Vec<Value> {
    let written = |p: &Value| p["text"].as_str().is_some_and(|t| t.contains("in this task's thread"));
    host.log(bot, "prompts.jsonl").into_iter().filter(written).collect()
}

/// The bot's task called `title`, once its result starts with `prefix`.
async fn result(host: &Host, bot: &str, title: &str, prefix: &str) -> Value {
    wait(&format!("{title} to report {prefix}"), async || {
        let found = work(host, bot).await.into_iter().find(|i| i["title"] == title);
        found.filter(|i| i["result"].as_str().is_some_and(|r| r.starts_with(prefix)))
    })
    .await
}

#[tokio::test]
async fn a_message_to_a_working_thread_waits_its_turn_and_the_task_reports_once_with_the_users_words() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD Slow|HOLD:slow").await;
    let slow = task(&host, &bot, "Slow", "working").await;
    assert_eq!(slow["talk"], true, "the bot's own work, whose thread is there");
    assert!(slow["folder"].as_str().unwrap().ends_with("workspace"), "where its thread works: {slow}");
    let mut events = host.events().await;

    // Its turn runs: the message waits in the thread, as one does in a chat.
    let sent = tell(&host, &bot, &slow, "also the tests").await;
    assert_eq!((&sent["reopened"], &sent["queued"]), (&json!(false), &json!(true)));
    let entry = &sent["entry"];
    assert_eq!((&entry["kind"], &entry["botId"]), (&json!("user"), &slow["thread"]), "an entry of its transcript");
    assert_eq!((&entry["data"]["text"], &entry["data"]["status"]), (&json!("also the tests"), &json!("queued")));
    let nonce = &entry["data"]["clientNonce"];
    let twice = json!({"botId": bot, "taskId": slow["id"], "text": "also the tests", "clientNonce": nonce});
    assert_eq!(host.call("send", twice).await["entry"]["id"], entry["id"], "sent twice, it is one message");

    // The caller names the bot and its task, never a thread; no quote or call.
    let other = host.fixture_bot("Other", "work_agent.py", "").await;
    let (thread, id) = (&slow["id"], &entry["id"]);
    for (method, body, why) in [
        ("send", json!({"botId": bot, "taskId": "nope", "text": "x"}), "no such task"),
        ("send", json!({"botId": other, "taskId": thread, "text": "x"}), "no such task"),
        ("send", json!({"botId": slow["thread"], "text": "x"}), "unknown bot"),
        ("send", json!({"botId": bot, "taskId": thread, "text": ""}), "empty message"),
        (
            "send",
            json!({"botId": bot, "taskId": thread, "text": "x", "replyTo": id}),
            "a thread takes no quotes or calls",
        ),
        ("steer", json!({"botId": other, "taskId": thread, "entryId": id}), "no such task"),
        ("unqueue", json!({"botId": slow["thread"], "entryId": id}), "unknown bot"),
    ] {
        assert_eq!(host.refused(method, body.clone()).await, why, "{method} {body}");
    }

    // Removed, a waiting message never goes out and leaves the transcript.
    let second = tell(&host, &bot, &slow, "and the docs").await["entry"].clone();
    host.call("unqueue", json!({"botId": bot, "taskId": thread, "entryId": second["id"]})).await;
    wait("the removed message to go", async || {
        thread_entry(&host, &bot, &slow, &second["id"]).await.is_none().then_some(())
    })
    .await;

    // The task's own turn ends: the task isn't over while a message waits. That message's turn is its end, in the
    // session that did the work, and its one report ends with the user's words.
    release(&host, &bot, "slow");
    let done = result(&host, &bot, "Slow", "Thread").await;
    assert_eq!(done["result"], "Thread also the tests: same session");
    assert_eq!((&done["status"], &done["thread"]), (&json!("done"), &slow["thread"]));
    let relayed_once = reply(&host, &bot, "Relayed:").await;
    assert!(relayed_once.starts_with("Relayed: Thread also the tests: same session"), "{relayed_once}");
    let words = "the user wrote to it directly. These are the user's own words, not the thread's: «also the tests»]";
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
    let threads = no_thread_bot_reached(&mut events).await;
    assert!(
        threads.iter().any(|thread| slow["thread"] == thread.as_str()),
        "the thread's entries are threadEntry events"
    );
}

/// Uploads a small file for a message to the thread of `task`: its upload id.
async fn upload_to_thread(host: &Host, bot: &str, task: &Value, name: &str) -> String {
    let upload = uuid::Uuid::new_v4().to_string();
    let chunk = json!({"botId": bot, "taskId": task["id"], "uploadId": upload, "name": name, "offset": 0,
        "data": "aGk=", "done": true});
    host.call("upload", chunk).await;
    upload
}

#[tokio::test]
async fn files_sent_to_a_thread_are_its_own_and_go_with_it() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD Slow|HOLD:slow").await;
    let slow = task(&host, &bot, "Slow", "working").await;
    assert_eq!(slow["talkFiles"], true, "its thread takes files");

    // A file alone waits behind the running turn, read back by the thread's id; taken back, the file goes too.
    let upload = upload_to_thread(&host, &bot, &slow, "shot.png").await;
    let body = json!({"botId": bot, "taskId": slow["id"], "text": "", "attachments": [upload], "clientNonce": "n1"});
    let sent = host.call("send", body).await;
    assert_eq!(sent["queued"], true);
    let entry = &sent["entry"];
    assert_eq!((&entry["botId"], &entry["data"]["attachments"][0]["name"]), (&slow["thread"], &json!("shot.png")));
    let read = json!({"botId": entry["botId"], "uploadId": upload, "offset": 0});
    assert!(host.post("readUpload", &read).await.0, "the file is there while it waits");
    let file = host.home.join("bots").join(slow["thread"].as_str().unwrap()).join("uploads").join(&upload);
    assert!(file.exists(), "in the thread's own uploads");
    host.call("unqueue", json!({"botId": bot, "taskId": slow["id"], "entryId": entry["id"]})).await;
    wait("its file to leave the thread", async || (!file.exists()).then_some(())).await;
    assert!(!host.post("readUpload", &read).await.0, "nor can a client read it");

    // With words: the task's report names the file with them.
    let upload = upload_to_thread(&host, &bot, &slow, "log.txt").await;
    let body = json!({"botId": bot, "taskId": slow["id"], "text": "see this", "attachments": [upload]});
    host.call("send", body).await;
    release(&host, &bot, "slow");
    let done = result(&host, &bot, "Slow", "Thread").await;
    let relayed = reply(&host, &bot, "Relayed:").await;
    assert!(relayed.contains("«see this [sent files: log.txt]»"), "{relayed}");

    // A file to the ended task reopens it.
    let upload = upload_to_thread(&host, &bot, &done, "more.png").await;
    let body = json!({"botId": bot, "taskId": done["id"], "text": "", "attachments": [upload]});
    assert_eq!(host.call("send", body).await["reopened"], true);
    wait("the reopened task to end again", async || {
        let task = work(&host, &bot).await.into_iter().find(|t| t["title"] == "Slow")?;
        (task["status"] == "done" && task["endedAt"].as_i64() > done["endedAt"].as_i64()).then_some(())
    })
    .await;

    // Its files go with the thread.
    let uploads = host.home.join("bots").join(done["thread"].as_str().unwrap()).join("uploads");
    assert!(uploads.join(&upload).exists(), "{}", uploads.display());
    host.call("dropWork", json!({"botId": bot, "taskId": done["id"]})).await;
    wait("the thread's files to go with it", async || (!uploads.exists()).then_some(())).await;
}

#[tokio::test]
async fn send_now_stops_the_threads_turn_for_the_message_or_joins_it_where_the_agent_can() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD Slow|HOLD:slow").await;
    let slow = task(&host, &bot, "Slow", "working").await;
    let sent = tell(&host, &bot, &slow, "do this instead").await;
    host.call("steer", json!({"botId": bot, "taskId": slow["id"], "entryId": sent["entry"]["id"]})).await;
    // Never released: its turn was stopped for the message, whose turn ends the task.
    let done = result(&host, &bot, "Slow", "Thread").await;
    assert_eq!((&done["status"], &done["result"]), (&json!("done"), &json!("Thread do this instead: same session")));
    let transcript = host.call("threadEntries", json!({"botId": bot, "threadId": slow["thread"]})).await;
    let stopped = transcript["entries"].as_array().unwrap().iter().any(|e| e["data"]["text"] == "Stopped.");
    assert!(!stopped, "no Stopped. line under the message it stopped for: {transcript}");

    // An agent that takes a message into its running turn: the turn goes on, with it.
    let steers = host.fixture_bot("Tess", "work_agent.py", "steer").await;
    host.reply(&steers, "ADD Slow|HOLD:slow").await;
    let slow = task(&host, &steers, "Slow", "working").await;
    let sent = tell(&host, &steers, &slow, "use tabs").await;
    host.call("steer", json!({"botId": steers, "taskId": slow["id"], "entryId": sent["entry"]["id"]})).await;
    thread_status(&host, &steers, &slow, &sent["entry"]["id"], "sent").await;
    assert_eq!(task(&host, &steers, "Slow", "working").await["thread"], slow["thread"]);
    release(&host, &steers, "slow");
    let done = task(&host, &steers, "Slow", "done").await;
    assert_eq!(done["result"], "Report Slow: briefed; no chat; joined use tabs");
    let relayed = reply(&host, &steers, "Relayed:").await;
    assert!(relayed.contains("not the thread's: «use tabs»]"), "{relayed}");
}

#[tokio::test]
async fn a_message_to_an_ended_task_reopens_it_and_it_reports_again() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD Quick|look around").await;
    assert_eq!(reply(&host, &bot, "Relayed:").await, "Relayed: Report Quick: briefed; no chat");
    let done = task(&host, &bot, "Quick", "done").await;
    assert_eq!(done["talk"], true, "its thread is kept with it");

    // It works again in the thread it had, timed from now, and what a thread may not do it still may not.
    let sent = tell(&host, &bot, &done, "one more thing HOLD:again NEST").await;
    assert_eq!((&sent["reopened"], &sent["queued"]), (&json!(true), &json!(false)));
    let again = task(&host, &bot, "Quick", "working").await;
    assert_eq!(again["thread"], done["thread"]);
    assert!(again["startedAt"].as_i64() > done["startedAt"].as_i64(), "{again}");
    assert!(again["endedAt"].is_null() && again["result"].is_null(), "{again}");
    // The user asked, so it runs whatever the ceiling; nothing else starts while the bot is over it.
    host.reply(&bot, "ADD Other|quick").await;
    assert_eq!(task(&host, &bot, "Other", "waiting").await["wait"], json!({"kind": "next"}));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(task(&host, &bot, "Quick", "working").await["thread"], done["thread"], "nothing ended it meanwhile");

    release(&host, &bot, "again");
    let over = result(&host, &bot, "Quick", "Thread").await;
    let said = over["result"].as_str().unwrap();
    assert!(said.starts_with("Thread one more thing HOLD:again NEST: same session; Refused: add_task isn't"), "{said}");
    assert_eq!(over["status"], "done");
    task(&host, &bot, "Other", "done").await;
    wait("every report", async || (relayed(&host, &bot).await.len() == 3).then_some(())).await;
    let reports = relayed(&host, &bot).await;
    let second = reports.iter().find(|r| r.starts_with("Relayed: Thread")).expect("it reported again");
    assert!(second.contains("not the thread's: «one more thing HOLD:again NEST»]"), "{second}");
    assert_eq!(thread_prompts(&host, &bot)[0]["session"], task_prompts(&host, &bot, "Quick")[0]["session"]);

    // An agent that can't pick a session up again: a new one, told the work and what it reported.
    let fresh = host.fixture_bot("Nell", "work_agent.py", "no-load").await;
    host.reply(&fresh, "ADD Quick|look around").await;
    reply(&host, &fresh, "Relayed:").await;
    let done = task(&host, &fresh, "Quick", "done").await;
    assert_eq!(tell(&host, &fresh, &done, "and now?").await["reopened"], true);
    assert_eq!(result(&host, &fresh, "Quick", "Thread").await["result"], "Thread and now?: picked up");
    let prompt = thread_prompts(&host, &fresh).remove(0);
    assert_ne!(prompt["session"], task_prompts(&host, &fresh, "Quick")[0]["session"]);
    let text = prompt["text"].as_str().unwrap();
    assert!(text.contains("You worked on \"Quick\" before") && text.contains("look around"), "{text}");
    assert!(text.contains("[Codync: what you reported when it ended:]\nReport Quick: briefed; no chat"), "{text}");
    assert!(text.contains("and now?\n\n[Codync: the user wrote this to you directly"), "{text}");
}

#[tokio::test]
async fn a_routines_run_takes_no_message_and_a_message_never_answers_a_threads_card() {
    let host = Host::start().await;
    let bot = bot_with(&host, 2).await;
    host.reply(&bot, "hello").await;
    let digest = routine(&host, &bot, "Digest", "HOLD:digest").await;
    host.call("runRoutine", json!({"botId": bot, "id": digest})).await;
    let run = task(&host, &bot, "Digest", "working").await;
    assert_eq!(run["talk"], false, "its result has an owner already");
    let refused = host.refused("send", json!({"botId": bot, "taskId": run["id"], "text": "hi"})).await;
    assert_eq!(refused, "\"Digest\" is a routine's run: it can be read and stopped, not written to");
    release(&host, &bot, "digest");

    // A thread that waits on a card: the message waits behind it, and only a press answers the card.
    host.reply(&bot, "ADD Pick one|ASK").await;
    let pending = async || {
        let mut cards = history(&host, &bot).await.into_iter().filter(|e| e["data"]["decision"] == true);
        cards.next_back()
    };
    let card = wait("the card", async || pending().await.filter(|e| e["data"]["status"] == "pending")).await;
    let waits = task(&host, &bot, "Pick one", "needsYou").await;
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
async fn stopping_a_task_drops_what_waited_in_its_thread_and_a_host_restart_fails_it() {
    let mut host = Host::start().await;
    let bot = bot_with(&host, 2).await;
    host.reply(&bot, "ADD One|HOLD:one").await;
    let one = task(&host, &bot, "One", "working").await;
    let waited = tell(&host, &bot, &one, "never mind").await;
    host.call("stopWork", json!({"botId": bot, "taskId": one["id"]})).await;
    task(&host, &bot, "One", "stopped").await;
    thread_status(&host, &bot, &one, &waited["entry"]["id"], "cancelled").await;

    // A restart: the cut-off turn is picked up, the message that waited is not (as in a chat), and the task's
    // report holds none of it. The thread still takes a message afterwards.
    host.reply(&bot, "ADD Long|HOLD:long").await;
    let long = task(&host, &bot, "Long", "working").await;
    let waited = tell(&host, &bot, &long, "after the restart?").await;
    host.restart(|_| {}).await;
    thread_status(&host, &bot, &long, &waited["entry"]["id"], "failed").await;
    let relayed = reply(&host, &bot, "Relayed:").await;
    assert!(relayed.contains("Codync restarted") && !relayed.contains("after the restart?"), "{relayed}");
    let done = task(&host, &bot, "Long", "done").await;
    assert_eq!(tell(&host, &bot, &done, "still there?").await["reopened"], true);
    assert_eq!(result(&host, &bot, "Long", "Thread").await["result"], "Thread still there?: same session");
}

#[tokio::test]
async fn what_waited_in_a_thread_goes_with_a_task_that_gives_way() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD Slow|HOLD:slow").await;
    let slow = task(&host, &bot, "Slow", "working").await;
    host.reply(&bot, "ADD Urgent|quick").await;
    let urgent = task(&host, &bot, "Urgent", "waiting").await;
    let refused = host.refused("send", json!({"botId": bot, "taskId": urgent["id"], "text": "x"})).await;
    assert_eq!(refused, "\"Urgent\" hasn't started yet: there is nothing to write to");
    assert_eq!(urgent["talk"], false);

    tell(&host, &bot, &slow, "mind the cache").await;
    host.call("workCutIn", json!({"botId": bot, "taskId": urgent["id"], "insteadOf": slow["id"]})).await;
    task(&host, &bot, "Urgent", "done").await;
    // It starts again in a new session, which reads what the user wrote and no turn had taken.
    release(&host, &bot, "slow");
    task(&host, &bot, "Slow", "done").await;
    let prompts = task_prompts(&host, &bot, "Slow");
    assert_eq!(prompts.len(), 2);
    let again = prompts[1]["text"].as_str().unwrap();
    assert!(again.contains("[The user said since: «mind the cache»]"), "{again}");
}

#[tokio::test]
async fn a_users_thread_starts_past_the_ceiling_and_the_bot_never_sees_it() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD Busy|HOLD:busy").await;
    task(&host, &bot, "Busy", "working").await;
    // The bot's one seat is taken; the user's thread starts anyway.
    host.call("startThread", json!({"botId": bot, "text": "Mine HOLD:mine"})).await;
    task(&host, &bot, "Mine HOLD:mine", "working").await;
    host.reply(&bot, "STATUS").await;
    let status = reply(&host, &bot, "Status:").await;
    assert!(status.contains("Busy") && !status.contains("Mine"), "{status}");
    release(&host, &bot, "busy");
    release(&host, &bot, "mine");
    task(&host, &bot, "Busy", "done").await;
    task(&host, &bot, "Mine HOLD:mine", "done").await;
}

#[tokio::test]
async fn a_thread_the_user_starts_opens_with_their_message_and_takes_no_turn_in_the_chat() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    let started = host.call("startThread", json!({"botId": bot, "text": "Look at the tests\nall of them"})).await;

    // Its first line names it; its turn is the user's message, in a session that starts without the chat.
    let done = result(&host, &bot, "Look at the tests", "Thread").await;
    assert_eq!((&done["id"], &done["status"]), (&started["taskId"], &json!("done")));
    assert_eq!(done["result"], "Thread Look at the tests: same session");
    assert_eq!((&done["source"], &done["talk"]), (&Value::Null, &json!(true)), "the bot's own to every client");
    let opened = transcript(&host, &bot, &done).await;
    let first = (&opened[0]["kind"], &opened[0]["data"]["text"]);
    assert_eq!(first, (&json!("user"), &json!("Look at the tests\nall of them")));
    let prompts = thread_prompts(&host, &bot);
    let text = prompts[0]["text"].as_str().unwrap();
    assert!(text.contains("the user opened this thread to work with you"), "{text}");
    assert!(!host.workspace(&bot).join("forks.jsonl").exists(), "it forks nothing of the chat");
    let servers = servers_of(&host, &bot, &prompts[0]);
    assert!(!servers.iter().any(|s| s == "routines" || s == "team"), "the chat's own servers: {servers:?}");

    // What it came to is read in the thread: the chat takes no turn for it.
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(history(&host, &bot).await.is_empty(), "{:?}", history(&host, &bot).await);

    // It takes more messages, as any thread of the bot's own work does, and still tells the chat nothing.
    tell(&host, &bot, &done, "and the docs").await;
    let again = result(&host, &bot, "Look at the tests", "Thread and").await;
    assert_eq!(again["result"], "Thread and the docs: same session");
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(relayed(&host, &bot).await, Vec::<String>::new());

    // Only a bot starts one, with something to say.
    let empty = "a thread starts with a message of at most 32000 bytes";
    assert_eq!(host.refused("startThread", json!({"botId": bot, "text": "  "})).await, empty);
    assert_eq!(host.refused("startThread", json!({"botId": done["thread"], "text": "x"})).await, "unknown bot");
}

#[tokio::test]
async fn a_thread_the_user_starts_opens_with_their_files() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    // Uploaded to the bot first, then named by the thread: its first message carries them, read from the thread.
    let upload = uuid::Uuid::new_v4().to_string();
    let chunk =
        json!({"botId": bot, "uploadId": upload, "name": "shot.png", "offset": 0, "data": "aGk=", "done": true});
    host.call("upload", chunk).await;
    host.call("startThread", json!({"botId": bot, "text": "Look at this", "attachments": [upload]})).await;
    let done = result(&host, &bot, "Look at this", "Thread").await;
    let opened = transcript(&host, &bot, &done).await;
    assert_eq!(opened[0]["data"]["attachments"][0]["name"], "shot.png");
    let read = json!({"botId": done["thread"], "uploadId": upload, "offset": 0});
    assert!(host.post("readUpload", &read).await.0, "the thread has the file");
    let text = thread_prompts(&host, &bot)[0]["text"].as_str().unwrap().to_owned();
    assert!(text.contains("[Attached files") && text.contains("shot.png"), "{text}");

    // Files alone start one too, named by the first; one that was never uploaded starts nothing.
    let alone = uuid::Uuid::new_v4().to_string();
    let chunk = json!({"botId": bot, "uploadId": alone, "name": "log.txt", "offset": 0, "data": "aGk=", "done": true});
    host.call("upload", chunk).await;
    host.call("startThread", json!({"botId": bot, "text": "", "attachments": [alone]})).await;
    result(&host, &bot, "log.txt", "Thread").await;
    let missing = json!({"botId": bot, "text": "x", "attachments": [uuid::Uuid::new_v4().to_string()]});
    assert!(host.refused("startThread", missing).await.starts_with("unknown upload"));
}

#[tokio::test]
async fn a_thread_the_user_starts_works_where_its_setup_says() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    let elsewhere = host.workspace(&bot).join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    // Its model and approvals are the thread bot's own (`tasks::Setup::over`): the scripted agent takes neither.
    host.call("startThread", json!({"botId": bot, "text": "Elsewhere", "setup": {"cwd": elsewhere}})).await;
    let done = result(&host, &bot, "Elsewhere", "Thread").await;
    assert_eq!(done["folder"], json!(elsewhere), "its thread works in the folder the user chose");
    // Without one, in the bot's default project.
    let project = host.home.join("projects").join("mika");
    std::fs::create_dir_all(&project).unwrap();
    host.call("updateBot", json!({"id": bot, "projects": [project]})).await;
    host.call("startThread", json!({"botId": bot, "text": "In the project"})).await;
    let done = result(&host, &bot, "In the project", "Thread").await;
    assert_eq!(done["folder"], json!(project), "the bot's default project");

    // A setup is checked as a bot's is, and one that can't be read starts nothing.
    let start = |setup: Value| host.refused("startThread", json!({"botId": bot, "text": "x", "setup": setup}));
    assert_eq!(start(json!({"cwd": "elsewhere"})).await, "a thread's folder is a full path");
    assert!(start(json!({"cwd": host.workspace(&bot).join("missing")})).await.contains("does not exist"));
    assert!(start(json!({"approval": {"permission": "yolo", "mode": null}})).await.contains("a thread's setup"));

    // Its login is one of its agent's: one this computer doesn't have, or another agent's, starts nothing; `""` is the
    // agent's default login.
    let other = host.call("addLoginProfile", json!({"backend": "cursor", "kind": "apiKey"})).await;
    assert_eq!(start(json!({"loginProfile": "no-such-profile"})).await, "unknown login profile");
    assert_eq!(start(json!({"loginProfile": other["loginProfile"]["id"]})).await, "unknown login profile");
    host.call("startThread", json!({"botId": bot, "text": "At home", "setup": {"loginProfile": ""}})).await;
    result(&host, &bot, "At home", "Thread").await;
}

/// What the scripted agent heard in `folder`, oldest first.
fn heard_in(folder: &Path) -> Vec<Value> {
    let log = std::fs::read_to_string(folder.join("prompts.jsonl")).unwrap_or_default();
    log.lines().map(|l| serde_json::from_str::<Value>(l).unwrap()["text"].clone()).collect()
}

#[tokio::test]
async fn a_thread_without_a_bot_runs_its_agent_on_its_own() {
    let host = Host::start().await;
    let refused = "a thread without a bot names its agent";
    assert_eq!(host.refused("startThread", json!({"text": "Hello"})).await, refused);
    // The first one makes the computer's box, hidden, before its setup is checked: this one's folder isn't there.
    let gone = json!({"backend": "claude", "cwd": std::env::temp_dir().join("codync-no-such-folder")});
    assert!(host.refused("startThread", json!({"text": "Hello", "setup": gone})).await.contains("does not exist"));
    let snapshot = host.snapshot().await;
    let boxes: Vec<&Value> = snapshot["bots"].as_array().unwrap().iter().filter(|b| b["plain"] == true).collect();
    assert_eq!(boxes.len(), 1, "one box");
    let owner = boxes[0]["id"].as_str().unwrap().to_owned();
    assert_eq!((&boxes[0]["hidden"], &boxes[0]["connectors"]), (&json!(true), &json!([])));
    // Only the host makes it, and it stays out of sight.
    let made = host.call("createBot", json!({"name": "B", "backend": "custom", "command": "x", "plain": true})).await;
    assert!(made["bot"]["plain"].is_null(), "{made}");
    let shown = host.refused("updateBot", json!({"id": owner, "hidden": false})).await;
    assert_eq!(shown, "the box of threads without a bot stays hidden");

    // Here it runs the scripted agent, which hears the user's words and nothing of Codync's.
    let agent = common::fixture_command("work_agent.py", "");
    host.call("updateBot", json!({"id": owner, "backend": "custom", "command": agent})).await;
    let folder = host.home.join("project");
    std::fs::create_dir_all(&folder).unwrap();
    let setup = json!({"backend": "custom", "cwd": folder});
    let started = host.call("startThread", json!({"text": "Look around", "setup": setup})).await;
    let done = task(&host, &owner, "Look around", "done").await;
    assert_eq!((&done["id"], &done["folder"]), (&started["taskId"], &json!(folder)));
    assert_eq!((&started["botId"], &done["agent"]), (&json!(owner), &json!("custom")), "where it went, on what");
    assert_eq!(heard_in(&folder), [json!("Look around")]);
    // It takes more messages, as they are written.
    tell(&host, &owner, &done, "and the docs").await;
    wait("the second message", async || heard_in(&folder).get(1).cloned()).await;
    assert_eq!(heard_in(&folder)[1], "and the docs");

    // Its files go to the box, uploaded without a bot, and on to its thread.
    let upload = uuid::Uuid::new_v4().to_string();
    let chunk = json!({"uploadId": upload, "name": "notes.md", "offset": 0, "data": "aGk=", "done": true});
    host.call("upload", chunk).await;
    let with = json!({"text": "Read it", "setup": setup, "attachments": [upload]});
    host.call("startThread", with).await;
    let read = task(&host, &owner, "Read it", "done").await;
    let opened = transcript(&host, &owner, &read).await;
    assert_eq!(opened[0]["data"]["attachments"][0]["name"], "notes.md");
}

#[tokio::test]
async fn the_bot_asks_a_working_task_and_gets_its_answer_apart_from_its_report() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "steer").await;
    host.reply(&bot, "ADD Working|HOLD:working").await;
    task(&host, &bot, "Working", "working").await;
    host.reply(&bot, "ASKTASK Working|How far along are you?").await;
    let asked = reply(&host, &bot, "Asked:").await;
    assert!(asked.contains("\"answer\":\"Answer: How far along are you?\""), "{asked}");
    // The thread heard it as its bot's question.
    let heard = logged(&host.workspace(&bot), "prompts.jsonl");
    assert!(heard.iter().all(|p| !p["text"].as_str().unwrap().contains("asks you this")), "it joined the turn");

    // It goes on with its task, and its report is its last message, not the answer.
    release(&host, &bot, "working");
    let report = relayed_for(&host, &bot, "Working").await;
    assert!(!report.contains("Answer:"), "{report}");
    task(&host, &bot, "Working", "done").await;
    // An ended task is refused: its report came already.
    host.reply(&bot, "ASKTASK Working|And now?").await;
    let refused = reply(&host, &bot, "Refused:").await;
    assert!(refused.contains("has ended"), "{refused}");
}

/// The first message in `bot`'s chat that starts with `prefix`.
async fn text_starting(host: &Host, bot: &str, prefix: &str) -> Option<String> {
    let texts = history(host, bot).await.into_iter().filter_map(|e| e["data"]["text"].as_str().map(str::to_owned));
    texts.into_iter().find(|t| t.starts_with(prefix))
}

#[tokio::test]
async fn an_agent_that_cannot_take_the_question_mid_turn_answers_it_in_a_turn_of_its_own() {
    let host = Host::start().await;
    let bot = bot_with(&host, 1).await;
    host.reply(&bot, "ADD Working|HOLD:working").await;
    task(&host, &bot, "Working", "working").await;
    host.reply(&bot, "ASKTASK Working|Which file?").await;
    let asked = reply(&host, &bot, "Asked:").await;
    assert!(asked.contains("\"answer\":\"Answer: Which file?\""), "{asked}");
    // Its running turn stopped for the question, and its answer, the turn's last message, is its report too.
    task(&host, &bot, "Working", "done").await;
    let report = wait("its report", async || relayed(&host, &bot).await.pop()).await;
    assert_eq!(report, "Relayed: Answer: Which file?");
}

#[tokio::test]
async fn an_answer_after_the_minute_comes_to_the_bot_as_a_turn_of_its_own() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "steer").await;
    host.reply(&bot, "ADD Working|HOLD:working").await;
    task(&host, &bot, "Working", "working").await;
    host.call("send", json!({"botId": bot, "text": "ASKTASK Working|SLOW how far?"})).await;
    let said = async |prefix: &str| text_starting(&host, &bot, prefix).await;
    common::eventually("the call to give up", Duration::from_secs(90), async || said("Asked:").await.is_some()).await;
    assert!(said("Asked:").await.unwrap().contains("no answer yet"));
    common::eventually("the late answer's turn", Duration::from_secs(30), async || said("Late:").await.is_some()).await;
    assert_eq!(said("Late:").await.unwrap(), "Late: Answer: SLOW how far?");
    release(&host, &bot, "working");
    let report = relayed_for(&host, &bot, "Working").await;
    assert!(!report.contains("Answer:"), "{report}");
}

#[tokio::test]
async fn a_task_names_its_place_and_a_new_worktree_gets_the_ignored_files_and_shared_folders() {
    let host = Host::start().await;
    let (bot, dir) = project_bot(&host, 3).await;
    if !git_repo(&dir) {
        return;
    }
    // `.env` and `node_modules` are ignored, and `.worktreeinclude` lists the one to copy.
    std::fs::write(dir.join(".gitignore"), "*.jsonl\nrelease-*\n.env\nnode_modules/\n").unwrap();
    std::fs::write(dir.join(".worktreeinclude"), ".env\nREADME.md\n").unwrap();
    let commit = |args: &[&str]| assert!(git_in(&dir).args(args).output().unwrap().status.success());
    commit(&["add", "."]);
    commit(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "ignores"]);
    std::fs::write(dir.join(".env"), "KEY=1").unwrap();
    std::fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    std::fs::write(dir.join("uncommitted.txt"), "the user's").unwrap();
    let project = dir.to_string_lossy().into_owned();
    let mut shared = serde_json::Map::new();
    shared.insert(project.clone(), json!(["node_modules"]));
    host.call("updateBot", json!({"id": bot, "worktreeMode": "off", "sharedFolders": shared})).await;
    assert_eq!(row(&host, &bot).await["sharedFolders"][&project], json!(["node_modules"]));

    // Off mode, but the task asks for a worktree: it gets one, filled before its agent starts.
    host.reply(&bot, "ADD Alone|HOLD:alone|place=worktree").await;
    let alone = in_worktree(&host, &bot, "Alone").await;
    let folder = PathBuf::from(alone["worktree"]["path"].as_str().unwrap());
    assert_eq!(std::fs::read_to_string(folder.join(".env")).unwrap(), "KEY=1");
    // A link on Windows needs Developer Mode; without it the thread starts all the same.
    if cfg!(unix) {
        assert!(folder.join("node_modules").symlink_metadata().unwrap().file_type().is_symlink());
    }
    assert!(!folder.join("uncommitted.txt").exists(), "never the user's uncommitted changes");
    let told = wait("its prompt", async || std::fs::read_to_string(folder.join("prompts.jsonl")).ok()).await;
    assert!(told.contains("from the project's last commit"), "{told}");

    // Always mode, but the task asks to stay: it works in the folder itself.
    host.call("updateBot", json!({"id": bot, "worktreeMode": "always"})).await;
    host.reply(&bot, "ADD Here|HOLD:here|place=here").await;
    wait("its prompt in the folder", async || task_prompts_in(&dir, "Here").pop()).await;
    assert!(task(&host, &bot, "Here", "working").await["worktree"].is_null());
    // The bot's own folder never gets one: the call says why.
    let own = host.workspace(&bot).to_string_lossy().into_owned();
    host.reply(&bot, &format!("ADD Mine|x|folder={own}|place=worktree")).await;
    wait("why it gets no worktree", async || {
        let texts =
            history(&host, &bot).await.into_iter().filter_map(|e| e["data"]["text"].as_str().map(str::to_owned));
        texts.into_iter().find(|t| t.starts_with("Added:") && t.contains("your own folder never gets a worktree"))
    })
    .await;

    // Untouched, it goes with its link, and the project keeps its own folder.
    std::fs::write(folder.join("release-alone"), "").unwrap();
    std::fs::write(dir.join("release-here"), "").unwrap();
    let report = relayed_for(&host, &bot, "Alone").await;
    if !cfg!(windows) {
        assert!(report.contains("was removed with its branch"), "{report}");
        assert!(!folder.exists() && dir.join("node_modules/pkg").is_dir());
    }
}

#[tokio::test]
async fn a_thread_without_a_bot_and_no_folder_works_in_one_of_its_own() {
    let host = Host::start().await;
    let agent = common::fixture_command("work_agent.py", "");
    // The first start makes the computer's box; then it runs the scripted agent.
    let gone = json!({"backend": "claude", "cwd": std::env::temp_dir().join("codync-no-such-folder")});
    host.refused("startThread", json!({"text": "x", "setup": gone})).await;
    let snapshot = host.snapshot().await;
    let owner = snapshot["bots"].as_array().unwrap().iter().find(|b| b["plain"] == true).unwrap()["id"].clone();
    let owner = owner.as_str().unwrap().to_owned();
    host.call("updateBot", json!({"id": owner, "backend": "custom", "command": agent})).await;
    let started = host.call("startThread", json!({"text": "Look around", "setup": {"backend": "custom"}})).await;
    let done = task(&host, &owner, "Look around", "done").await;
    let own = host.home.join("threads").join(started["taskId"].as_str().unwrap());
    assert_eq!(done["folder"], json!(own), "not the home folder");
    assert_eq!(heard_in(&own), [json!("Look around")]);
    // Versioned as a bot's own folder is, and never given a worktree of itself.
    wait("its versions", async || own.join(".git").exists().then_some(())).await;
    assert!(done["worktree"].is_null());
}
