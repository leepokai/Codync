//! An agent's own question (`elicitation/create`, as Hermes sends its clarify prompts) through a real host and a
//! fixture agent: it waits on a choice card even when the bot approves tool requests automatically, the user's answer
//! goes back as `accept`, Stop as `cancel`, and a form no card can show faithfully is declined. A conversation goal
//! stops at that question, and answering it does not hand the goal its next step.

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

/// The goal the snapshot reports for `bot`.
async fn goal(host: &Host, bot: &str) -> Value {
    common::bot(&host.snapshot().await, bot)["goal"].clone()
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

#[tokio::test]
async fn a_goal_stops_for_the_agents_own_question_and_its_answer_does_not_resume_it() {
    let host = Host::start().await;
    let bot = auto_bot(&host).await;

    // The goal's step asks the agent to continue; the agent stops to ask the user instead.
    host.call("setGoal", json!({"botId": bot, "goal": "ASK_CHOICE pick the database", "maxTurns": 4})).await;
    let asked = card(&host, &bot).await;
    assert_eq!(asked["data"]["choices"], json!(["sqlite", "postgres"]), "{asked}");
    common::eventually("the goal stops for the user", Duration::from_secs(20), async || {
        goal(&host, &bot).await["stopped"] == "needsUser"
    })
    .await;

    // The press answers the agent's question, and the turn ends there: the goal prompts no next step.
    host.call("respondDecision", json!({"entryId": asked["id"], "option": "sqlite"})).await;
    assert_eq!(reply(&host, &bot, 0).await["result"], json!({"action": "accept", "content": {"answer": "sqlite"}}));
    idle(&host, &bot).await;
    common::eventually("no second goal prompt", Duration::from_secs(10), async || host.prompts(&bot).len() == 1).await;

    // The goal stays stopped at the step that asked, and its end is what the chat shows.
    let state = goal(&host, &bot).await;
    assert_eq!(state["stopped"], "needsUser", "{state}");
    assert_eq!(state["turns"], 1, "{state}");
    let history = host.call("history", json!({"botId": bot})).await;
    let ended: Vec<&Value> =
        history["entries"].as_array().unwrap().iter().filter(|e| e["data"]["goalEnded"].is_string()).collect();
    assert_eq!(ended.len(), 1, "{history}");
    assert_eq!(ended[0]["data"]["goalEnded"], "needsUser", "{history}");
    assert_eq!(host.prompts(&bot).len(), 1, "only the goal's step prompted the agent");
}
