//! A real host with the user's messages queued behind a running turn: steered into it (and on to a fallback with it),
//! interrupting a turn whose agent can't steer, first in line behind other work, left for the next turn when the agent
//! says so, or taken back.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use common::python_missing;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{Duration, Instant};

async fn entries(host: &Host, bot: &str) -> Vec<Value> {
    host.call("history", json!({"botId": bot})).await["entries"].as_array().unwrap().clone()
}

async fn entry(host: &Host, bot: &str, id: &str) -> Value {
    entries(host, bot).await.into_iter().find(|e| e["id"] == id).unwrap()
}

async fn eventually(what: &str, within: Duration, mut done: impl AsyncFnMut() -> bool) {
    let deadline = Instant::now() + within;
    while !done().await {
        assert!(Instant::now() < deadline, "timed out waiting until {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Sends "first" (its turn running at the agent) and then "second", which waits: the ids of both.
async fn queue_behind_a_turn(host: &Host, bot: &str) -> (String, String) {
    let first =
        host.call("send", json!({"botId": bot, "text": "first"})).await["entry"]["id"].as_str().unwrap().to_owned();
    eventually("the agent got the first prompt", Duration::from_mins(1), async || {
        host.requests(bot, "session/prompt").len() == 1
    })
    .await;
    let second = host.call("send", json!({"botId": bot, "text": "second"})).await["entry"].clone();
    assert_eq!(second["data"]["status"], "queued");
    (first, second["id"].as_str().unwrap().to_owned())
}

async fn idle(host: &Host, bot: &str) -> bool {
    common::bot(&host.snapshot().await, bot)["status"] == "idle"
}

async fn status(host: &Host, bot: &str, id: &str) -> Value {
    entry(host, bot, id).await["data"]["status"].clone()
}

#[tokio::test]
async fn a_steered_message_joins_the_running_turn() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Steers", "steer_agent.py", "steer 30 1").await;
    let (first, second) = queue_behind_a_turn(&host, &bot).await;
    host.call("steer", json!({"botId": bot, "entryId": second})).await;
    // Pressed twice while the agent decides: it goes to the agent once.
    host.call("steer", json!({"botId": bot, "entryId": second})).await;
    eventually("the turn answered both", Duration::from_secs(20), async || {
        status(&host, &bot, &second).await == "sent" && idle(&host, &bot).await
    })
    .await;

    let steered = host.requests(&bot, "_session/steering");
    assert_eq!(steered.len(), 1);
    assert!(steered[0]["params"]["prompt"][0]["text"].as_str().unwrap().contains("second"));
    assert_eq!(steered[0]["params"]["_meta"]["steering"]["idleBehavior"], "promptRequired");
    assert_eq!(host.requests(&bot, "session/prompt").len(), 1, "no turn of its own");
    let (first, second) = (entry(&host, &bot, &first).await, entry(&host, &bot, &second).await);
    assert_eq!(second["turn"], first["turn"], "it is the running turn's");
    let reply = entries(&host, &bot).await.into_iter().rev().find(|e| e["data"]["final"] == true).unwrap();
    assert!(reply["data"]["text"].as_str().unwrap().contains("Got: "), "{reply}");
    assert!(reply["seq"].as_i64() > second["seq"].as_i64(), "the reply comes after the message it answers");
}

#[tokio::test]
async fn a_steered_message_keeps_a_turn_that_ends_before_the_agent_answers() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Late", "steer_agent.py", "late 30").await;
    let (first, second) = queue_behind_a_turn(&host, &bot).await;
    host.call("steer", json!({"botId": bot, "entryId": second})).await;
    eventually("the agent took it", Duration::from_secs(20), async || status(&host, &bot, &second).await == "sent")
        .await;
    assert_eq!(host.requests(&bot, "session/prompt").len(), 1, "no turn of its own");
    let (first, second) = (entry(&host, &bot, &first).await, entry(&host, &bot, &second).await);
    assert_eq!(second["turn"], first["turn"], "it stays the turn's");
}

#[tokio::test]
async fn a_steered_message_goes_with_its_turn_to_the_next_agent() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Carries", "steer_agent.py", "limit 30").await;
    let standin = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/relay_agent.py");
    let standin = format!("{} standin", common::python_agent(&standin));
    let fallbacks = json!([{"backend": "codex", "command": standin}]);
    host.call("updateBot", json!({"id": bot, "fallbacks": fallbacks})).await;
    let (_, second) = queue_behind_a_turn(&host, &bot).await;
    host.call("steer", json!({"botId": bot, "entryId": second})).await;
    let prompt = async || {
        let log = host.log(&bot, "standin.jsonl");
        log.iter().find(|r| r["method"] == "session/prompt").map(|r| r["params"]["prompt"][0]["text"].clone())
    };
    eventually("the next agent took the turn on", Duration::from_secs(30), async || prompt().await.is_some()).await;
    let prompt = prompt().await.unwrap();
    assert!(prompt.as_str().unwrap().contains("first") && prompt.as_str().unwrap().contains("second"), "{prompt}");
    assert_eq!(status(&host, &bot, &second).await, "sent");
}

#[tokio::test]
async fn behind_other_work_a_steered_message_goes_first_in_line() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Behind", "steer_agent.py", "steer 5").await;
    let mut routines = vec![];
    for name in ["The running routine", "The waiting routine"] {
        let body = json!({"botId": bot, "name": name, "instruction": name, "triggers": [{"type": "webhook"}], "timeoutSeconds": 60});
        routines.push(host.call("saveRoutine", body).await["routine"]["id"].as_str().unwrap().to_owned());
    }
    host.call("runRoutine", json!({"botId": bot, "id": routines[0]})).await;
    eventually("the routine's turn runs", Duration::from_mins(1), async || {
        host.requests(&bot, "session/prompt").len() == 1
    })
    .await;
    host.call("runRoutine", json!({"botId": bot, "id": routines[1]})).await;
    let second = host.call("send", json!({"botId": bot, "text": "second"})).await["entry"].clone();
    assert_eq!(second["data"]["status"], "queued");
    host.call("steer", json!({"botId": bot, "entryId": second["id"]})).await;
    eventually("all three had their turns", Duration::from_secs(30), async || {
        host.requests(&bot, "session/prompt").len() == 3
    })
    .await;

    let prompts: Vec<String> =
        host.requests(&bot, "session/prompt").iter().map(|r| r["params"]["prompt"][0]["text"].to_string()).collect();
    for (prompt, said) in prompts.iter().zip(["The running routine", "second", "The waiting routine"]) {
        assert!(prompt.contains(said), "{said}: {prompts:?}");
    }
    assert_eq!(host.requests(&bot, "_session/steering"), Vec::<Value>::new(), "not the user's turn to join");
    assert_eq!(host.requests(&bot, "session/cancel"), Vec::<Value>::new(), "not the user's turn to stop");
}

#[tokio::test]
async fn steering_an_agent_that_cannot_stops_the_turn_for_the_message() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Plain", "steer_agent.py", "plain 30").await;
    let (_, second) = queue_behind_a_turn(&host, &bot).await;
    host.call("steer", json!({"botId": bot, "entryId": second})).await;
    eventually("the message started its own turn", Duration::from_secs(20), async || {
        host.requests(&bot, "session/prompt").len() == 2
    })
    .await;
    assert_eq!(host.requests(&bot, "_session/steering"), Vec::<Value>::new());
    assert_eq!(host.requests(&bot, "session/cancel").len(), 1);
    let prompt = &host.requests(&bot, "session/prompt")[1]["params"]["prompt"][0]["text"];
    assert!(prompt.as_str().unwrap().contains("second"), "{prompt}");
    assert_eq!(status(&host, &bot, &second).await, "sent");
    host.call("stop", json!({"botId": bot})).await;
}

#[tokio::test]
async fn a_message_the_agent_leaves_for_later_is_the_next_turn() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Later", "steer_agent.py", "next 5").await;
    let (first, second) = queue_behind_a_turn(&host, &bot).await;
    host.call("steer", json!({"botId": bot, "entryId": second})).await;
    eventually("the message started its own turn", Duration::from_secs(20), async || {
        host.requests(&bot, "session/prompt").len() == 2
    })
    .await;
    assert_eq!(host.requests(&bot, "_session/steering").len(), 1);
    assert_eq!(host.requests(&bot, "session/cancel"), Vec::<Value>::new(), "the first turn ran to its end");
    let prompt = &host.requests(&bot, "session/prompt")[1]["params"]["prompt"][0]["text"];
    assert!(prompt.as_str().unwrap().contains("second"), "{prompt}");
    let (first, second) = (entry(&host, &bot, &first).await, entry(&host, &bot, &second).await);
    assert_eq!(second["data"]["status"], "sent");
    assert_ne!(second["turn"], first["turn"], "a turn of its own");
    host.call("stop", json!({"botId": bot})).await;
}

#[tokio::test]
async fn a_queued_message_taken_back_never_goes_out() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Takes back", "steer_agent.py", "plain 5").await;
    let (first, second) = queue_behind_a_turn(&host, &bot).await;
    host.call("unqueue", json!({"botId": bot, "entryId": second})).await;
    eventually("the roster stopped previewing it", Duration::from_secs(20), async || {
        !common::bot(&host.snapshot().await, &bot)["lastMessage"].as_str().unwrap().contains("second")
    })
    .await;
    eventually("the first turn ended", Duration::from_secs(20), async || idle(&host, &bot).await).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    assert_eq!(host.requests(&bot, "session/prompt").len(), 1, "the second never went out");
    let removed = entry(&host, &bot, &second).await;
    assert_eq!(removed["data"]["status"], "cancelled");
    assert_eq!(removed["data"]["removed"], true);
    // Only a message still waiting can be steered or taken back.
    for method in ["steer", "unqueue"] {
        for id in [&first, &second] {
            let error = host.refused(method, json!({"botId": bot, "entryId": id})).await;
            assert!(error.contains("isn't waiting"), "{method} {id}: {error}");
        }
    }
}

#[tokio::test]
async fn a_message_taken_back_while_the_agent_decides_goes_unless_the_agent_took_it() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    // The agent answers each steer a second later: it leaves one for later, and takes the other.
    let (left, taken) = (
        host.fixture_bot("Leaves it", "steer_agent.py", "next 30 1").await,
        host.fixture_bot("Takes it", "steer_agent.py", "steer 30 1").await,
    );
    let (_, gone) = queue_behind_a_turn(&host, &left).await;
    let (_, joined) = queue_behind_a_turn(&host, &taken).await;
    for (bot, id) in [(&left, &gone), (&taken, &joined)] {
        host.call("steer", json!({"botId": bot, "entryId": id})).await;
        host.call("unqueue", json!({"botId": bot, "entryId": id})).await;
    }
    eventually("the agent left it, and it went", Duration::from_secs(20), async || {
        entry(&host, &left, &gone).await["data"]["removed"] == true
    })
    .await;
    eventually("the agent took the other", Duration::from_secs(20), async || {
        status(&host, &taken, &joined).await == "sent"
    })
    .await;
    assert_eq!(status(&host, &left, &gone).await, "cancelled");
    assert_eq!(entry(&host, &taken, &joined).await["data"]["removed"], Value::Null, "the chat shows it again");
    host.call("stop", json!({"botId": left})).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(host.requests(&left, "session/prompt").len(), 1, "it never went out");
}
