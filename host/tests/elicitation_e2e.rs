//! An agent's own question (`elicitation/create`, as Hermes sends its clarify prompts) through a real host and a
//! fixture agent: it waits on a choice card even when the bot approves tool requests automatically, the user's answer
//! goes back as `accept`, Stop as `cancel`, and a form no card can show faithfully is declined.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// A bot running the fixture that approves tool requests by itself.
async fn auto_bot(host: &Host) -> String {
    let bot = host.fixture_bot("Hermes", "elicitation_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "permission": "auto"})).await;
    bot
}

async fn send(host: &Host, bot: &str, text: &str) {
    host.call("send", json!({"botId": bot, "text": text, "clientNonce": uuid::Uuid::new_v4().to_string()})).await;
}

/// The pending choice card in `bot`'s chat, once it shows.
async fn card(host: &Host, bot: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let history = host.call("history", json!({"botId": bot})).await;
        let found = history["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == "notice" && e["data"]["decision"] == true && e["data"]["status"] == "pending");
        if let Some(card) = found {
            return card.clone();
        }
        assert!(Instant::now() < deadline, "no card came: {history}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The `nth` reply the agent got to one of its questions, once it comes.
async fn reply(host: &Host, bot: &str, nth: usize) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(response) = host.log(bot, "responses.jsonl").into_iter().nth(nth) {
            return response;
        }
        assert!(Instant::now() < deadline, "the agent's question got no reply");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn idle(host: &Host, bot: &str) {
    common::eventually("the bot is idle", Duration::from_secs(20), async || {
        common::bot(&host.snapshot().await, bot)["status"] == "idle"
    })
    .await;
}

#[tokio::test]
async fn an_agents_question_waits_for_the_user_whatever_the_approval_setting() {
    let host = Host::start().await;
    let bot = auto_bot(&host).await;

    // Free text: a card with no options, which waits for the user although the bot approves by itself.
    send(&host, &bot, "ASK_TEXT").await;
    let asked = card(&host, &bot).await;
    assert_eq!(asked["data"]["question"], "Which database?", "{asked}");
    assert_eq!(asked["data"]["choices"], json!([]));
    assert_eq!(common::bot(&host.snapshot().await, &bot)["status"], "needsInput");
    let init = host.requests(&bot, "initialize");
    assert_eq!(init[0]["params"]["clientCapabilities"]["elicitation"], json!({"form": {}}), "{init:?}");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(host.log(&bot, "responses.jsonl").is_empty(), "nothing answers it for the user");
    // The user's own words, typed as a message, are the answer.
    send(&host, &bot, "Postgres, the one we already run").await;
    let answered = reply(&host, &bot, 0).await;
    assert_eq!(
        answered["result"],
        json!({"action": "accept", "content": {"answer": "Postgres, the one we already run"}}),
        "{answered}"
    );
    idle(&host, &bot).await;

    // One of a few options: the press sends the option's value.
    send(&host, &bot, "ASK_CHOICE").await;
    let asked = card(&host, &bot).await;
    assert_eq!(asked["data"]["choices"], json!(["sqlite", "postgres"]));
    assert_eq!(asked["data"]["recommended"], 1, "the default is suggested");
    host.call("respondDecision", json!({"entryId": asked["id"], "option": "sqlite"})).await;
    assert_eq!(reply(&host, &bot, 1).await["result"], json!({"action": "accept", "content": {"answer": "sqlite"}}));
    idle(&host, &bot).await;

    // Yes or no sends a boolean.
    send(&host, &bot, "ASK_YESNO").await;
    let asked = card(&host, &bot).await;
    assert_eq!(asked["data"]["choices"], json!(["Yes", "No"]));
    host.call("respondDecision", json!({"entryId": asked["id"], "option": "No"})).await;
    assert_eq!(reply(&host, &bot, 2).await["result"], json!({"action": "accept", "content": {"answer": false}}));
    idle(&host, &bot).await;
}

#[tokio::test]
async fn a_question_no_card_can_answer_never_hangs() {
    let host = Host::start().await;
    let bot = auto_bot(&host).await;

    // Several options at once can't be picked on a card: declined, not answered for the user.
    send(&host, &bot, "ASK_MULTI").await;
    assert_eq!(reply(&host, &bot, 0).await["result"], json!({"action": "decline"}));
    idle(&host, &bot).await;
    // Nor a session the host never opened for this bot.
    send(&host, &bot, "ASK_TEXT ELSEWHERE").await;
    assert_eq!(reply(&host, &bot, 1).await["result"], json!({"action": "decline"}));
    idle(&host, &bot).await;

    // Words that name no option don't answer a question whose answer must be one: it is cancelled, and the words go
    // to the bot as the user's next message.
    send(&host, &bot, "ASK_CHOICE").await;
    let asked = card(&host, &bot).await;
    send(&host, &bot, "Neither, use what we have").await;
    assert_eq!(reply(&host, &bot, 2).await["result"], json!({"action": "cancel"}));
    common::eventually("the words reach the bot", Duration::from_secs(20), async || {
        host.prompts(&bot).last().is_some_and(|p| p.contains("Neither, use what we have"))
    })
    .await;
    let history = host.call("history", json!({"botId": bot})).await;
    let expired = history["entries"].as_array().unwrap().iter().find(|e| e["id"] == asked["id"]).unwrap();
    assert_eq!(expired["data"]["status"], "expired", "{expired}");
    idle(&host, &bot).await;

    // Stop cancels a question still waiting.
    send(&host, &bot, "ASK_TEXT").await;
    card(&host, &bot).await;
    host.call("stop", json!({"botId": bot})).await;
    assert_eq!(reply(&host, &bot, 3).await["result"], json!({"action": "cancel"}));
    idle(&host, &bot).await;
}
