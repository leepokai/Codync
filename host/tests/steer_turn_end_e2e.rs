//! A real host where a steered message meets the end of its turn: the end waits for the agent's answer, so a late yes
//! still makes the message that turn's (its reply, memory, a fallback's retry), and anything else, no answer included,
//! leaves it for the next turn.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use common::steer::{entries, entry, idle, queue_behind_a_turn, status};
use common::{eventually, python_missing};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;

/// What a nameless bot's finished turns recorded, for it to learn from.
async fn recorded_exchange(host: &Host, bot: &str) -> Value {
    let db = rusqlite::Connection::open(host.home.join("codync.db")).unwrap();
    let recorded = || -> Option<String> {
        db.query_row("SELECT v FROM kv WHERE k = ?", [format!("naming.turns.{bot}")], |r| r.get(0)).ok()
    };
    eventually("the turn's exchange was recorded", Duration::from_secs(20), async || recorded().is_some()).await;
    serde_json::from_str(&recorded().unwrap()).unwrap()
}

/// `args` make the agent end the turn and then answer without taking the message, or not answer in time: the bot stays
/// at the turn until then, and the message has the next one.
async fn it_goes_next(args: &str) {
    let host = Host::start().await;
    let bot = host.fixture_bot("Late", "steer_agent.py", args).await;
    let (first, second) = queue_behind_a_turn(&host, &bot).await;
    host.call("steer", json!({"botId": bot, "entryId": second})).await;
    eventually("the message started its own turn", Duration::from_secs(30), async || {
        host.requests(&bot, "session/prompt").len() == 2
    })
    .await;
    let prompt = &host.requests(&bot, "session/prompt")[1]["params"]["prompt"][0]["text"];
    assert!(prompt.as_str().unwrap().contains("second"), "{prompt}");
    let (first, second) = (entry(&host, &bot, &first).await, entry(&host, &bot, &second).await);
    assert_eq!(second["data"]["status"], "sent");
    assert_ne!(second["turn"], first["turn"], "a turn of its own");
    eventually("the bot finished", Duration::from_secs(20), async || idle(&host, &bot).await).await;
}

/// `args` make the agent's turn fail with a usage limit: the message goes along to the next agent.
async fn it_goes_with_its_turn_to_the_next_agent(args: &str) {
    let host = Host::start().await;
    let bot = host.fixture_bot("Carries", "steer_agent.py", args).await;
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
async fn a_turn_that_ends_before_the_agent_answers_still_answers_the_message() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("", "steer_agent.py", "sends-late 30").await;
    let (_, second) = queue_behind_a_turn(&host, &bot).await;
    eventually("the bot told the user something", Duration::from_secs(20), async || {
        entries(&host, &bot).await.iter().any(|e| e["data"]["text"].as_str() == Some("Before it joined"))
    })
    .await;
    host.call("steer", json!({"botId": bot, "entryId": second})).await;
    eventually("the turn ended", Duration::from_secs(20), async || {
        status(&host, &bot, &second).await == "sent" && idle(&host, &bot).await
    })
    .await;

    let reply = entries(&host, &bot)
        .await
        .into_iter()
        .find(|e| e["data"]["text"].as_str().is_some_and(|t| t.contains("Got: ")));
    assert_eq!(reply.unwrap()["data"]["final"], true, "its reply reaches the chat");
    let turns = recorded_exchange(&host, &bot).await;
    assert!(turns[0]["user"].as_str().unwrap().contains("second"), "{turns}");
}

#[tokio::test]
async fn a_late_answer_that_leaves_the_message_for_later_gives_it_the_next_turn() {
    if !python_missing() {
        it_goes_next("next 3 6").await;
    }
}

#[tokio::test]
async fn a_late_refusal_gives_the_message_the_next_turn() {
    if !python_missing() {
        it_goes_next("refuse 3 6").await;
    }
}

#[tokio::test]
async fn an_agent_that_never_answers_only_holds_the_turn_until_the_message_goes_next() {
    if !python_missing() {
        it_goes_next("steer 3 60").await;
    }
}

#[tokio::test]
async fn a_steered_message_goes_with_its_turn_to_the_next_agent() {
    if !python_missing() {
        it_goes_with_its_turn_to_the_next_agent("limit 30").await;
    }
}

#[tokio::test]
async fn a_message_the_agent_takes_after_its_turn_failed_goes_on_to_the_next_agent() {
    if !python_missing() {
        it_goes_with_its_turn_to_the_next_agent("limit 3 6").await;
    }
}

#[tokio::test]
async fn a_steered_message_is_in_the_exchange_the_bot_learns_from() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("", "steer_agent.py", "steer 30").await;
    let (_, second) = queue_behind_a_turn(&host, &bot).await;
    host.call("steer", json!({"botId": bot, "entryId": second})).await;
    let turns = recorded_exchange(&host, &bot).await;
    let user = turns[0]["user"].as_str().unwrap();
    assert!(user.contains("first") && user.contains("second"), "{user}");
}
