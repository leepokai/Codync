//! Work an agent goes on with after its turn ended (like Hermes' background subagents), through a real host and a
//! fixture agent: a permission request it makes then gets the answer the bot's permission setting gives without a
//! card, only for the bot's own session and not after the user stopped the turn.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// The host's answer to the permission request the agent makes after answering `text`, the `nth` it got.
async fn answer_after(host: &Host, bot: &str, text: &str, nth: usize) -> Value {
    host.reply(bot, text).await;
    nth_answer(host, bot, nth).await
}

/// The `nth` permission answer the agent got, once it comes.
async fn nth_answer(host: &Host, bot: &str, nth: usize) -> Value {
    let file = host.home.join("bots").join(bot).join("workspace").join("responses.jsonl");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(line) = std::fs::read_to_string(&file).ok().and_then(|log| log.lines().nth(nth).map(str::to_owned))
        {
            return serde_json::from_str(&line).unwrap();
        }
        assert!(Instant::now() < deadline, "the host never answered the permission request");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn a_permission_asked_after_the_turn_gets_the_bots_automatic_answer() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Hermes", "late_permission_agent.py", "").await;

    // A bot that asks first: no card waits for it, so it's declined (the way ACP says it), not refused as unsupported.
    let asked = answer_after(&host, &bot, "write the notes", 0).await;
    assert_eq!(asked["result"]["outcome"]["outcome"], "cancelled", "{asked}");
    // Asked just before the turn ends: whether its card or the end of the turn answers it, the agent isn't left waiting.
    let early = answer_after(&host, &bot, "write the notes EARLY", 1).await;
    assert_eq!(early["result"]["outcome"]["outcome"], "cancelled", "{early}");

    host.call("updateBot", json!({"id": bot, "permission": "auto"})).await;
    let approved = answer_after(&host, &bot, "write the notes", 2).await;
    assert_eq!(approved["result"]["outcome"], json!({"outcome": "selected", "optionId": "yes"}), "{approved}");

    // Never for a session the host didn't open for this bot.
    let elsewhere = answer_after(&host, &bot, "write the notes elsewhere", 3).await;
    assert_eq!(elsewhere["result"]["outcome"]["outcome"], "cancelled", "{elsewhere}");

    // Nor after the user stopped the turn that started the work, which can't message them either.
    host.call("send", json!({"botId": bot, "text": "write the notes SLOW", "clientNonce": "slow"})).await;
    let deadline = Instant::now() + Duration::from_secs(20);
    while common::bot(&host.snapshot().await, &bot)["status"] == "idle" {
        assert!(Instant::now() < deadline, "the turn never started");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    host.call("stop", json!({"botId": bot})).await;
    let stopped = nth_answer(&host, &bot, 4).await;
    assert_eq!(stopped["result"]["outcome"]["outcome"], "cancelled", "{stopped}");
    let send = json!({"botId": bot, "name": "send_message", "arguments": {"text": "Done in the background."}});
    assert!(host.refused("chatCall", send).await.contains("stopped"));
}
