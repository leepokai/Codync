//! A bot's fallback chain through a real host and two fixture agents: the bot's own agent runs out of usage
//! mid-turn, the fallback carries the same turn on with what it did, the bot goes back on request with a catch-up (a
//! recap when its session is gone), a new session carries nothing over, and a chain that's used up says so.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn agent(name: &str) -> String {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/relay_agent.py");
    format!("{} {name}", common::python_agent(&script))
}

/// The newest entry of `bot`'s chat that `found` picks, within 30 s.
async fn wait_for(host: &Host, bot: &str, what: &str, found: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let history = host.call("history", json!({"botId": bot})).await;
        if let Some(e) = history["entries"].as_array().unwrap().iter().rev().find(|e| found(e)) {
            return e.clone();
        }
        assert!(Instant::now() < deadline, "no {what}: {history}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The last prompt `agent` got in `bot`'s workspace.
fn last_prompt(host: &Host, bot: &str, agent: &str) -> String {
    let log = host.log(bot, &format!("{agent}.jsonl"));
    let prompt = log.iter().rfind(|r| r["method"] == "session/prompt").unwrap();
    prompt["params"]["prompt"][0]["text"].as_str().unwrap().to_owned()
}

async fn bot_value(host: &Host, bot: &str) -> Value {
    let snapshot = host.snapshot().await;
    snapshot["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot).cloned().unwrap()
}

#[tokio::test]
async fn the_next_agent_carries_the_turn_on_and_the_bot_goes_back() {
    let host = Host::start().await;
    let bot =
        json!({"name": "Relay", "backend": "custom", "command": agent("primary"), "connectors": [], "notify": false});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    let fallbacks = json!([{"backend": "codex", "command": agent("standin")}]);
    let saved = host.call("updateBot", json!({"id": bot, "fallbacks": fallbacks})).await["bot"].clone();
    assert_eq!(saved["selfFallbacks"], true, "a bot may set its own fallbacks unless the user says no");
    let folder = host.home.join("bots").join(&bot).join("workspace");
    std::fs::write(folder.join("primary.limit"), "").unwrap();

    host.reply(&bot, "Fix the login tests").await;
    let reply = wait_for(&host, &bot, "reply", |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    assert_eq!(reply["data"]["text"], "standin answered");
    let line = wait_for(&host, &bot, "switch line", |e| e["data"]["agentSwitch"].is_object()).await;
    let switch = &line["data"]["agentSwitch"];
    assert_eq!((switch["from"].as_str(), switch["to"].as_str()), (Some("custom"), Some("codex")));
    assert_eq!(switch["reason"], "usageLimit");
    assert_eq!(line["data"]["text"], "Custom command reached its usage limit (resets in 1 h). Codex carried on.");
    assert!(switch["carried"].as_str().is_some_and(|c| c.contains("partway through this turn")), "{switch}");
    let prompt = last_prompt(&host, &bot, "standin");
    assert!(prompt.contains("Custom command reached its usage limit partway through this turn"), "{prompt}");
    assert!(prompt.contains("- Edit a.txt (completed)") && prompt.contains("Fix the login tests"), "{prompt}");

    let on = bot_value(&host, &bot).await;
    assert_eq!(on["agentStep"]["agent"], "codex");
    assert_eq!(on["resting"][0]["agent"], "custom");
    assert_eq!(on["resting"][0]["reason"], "usageLimit");

    // Still resting: the next message goes straight to the fallback, which resumes its own session.
    host.reply(&bot, "And the signup tests").await;
    let standin = host.log(&bot, "standin.jsonl");
    assert_eq!(standin.iter().filter(|r| r["method"] == "session/new").count(), 1);

    std::fs::remove_file(folder.join("primary.limit")).unwrap();
    host.call("switchBack", json!({"botId": bot})).await;
    wait_for(&host, &bot, "back line", |e| e["data"]["text"] == "Back on Custom command.").await;
    assert!(bot_value(&host, &bot).await["agentStep"].is_null());
    host.reply(&bot, "Thanks").await;
    let last = last_prompt(&host, &bot, "primary");
    assert!(last.contains("<catch_up>") && last.contains("\"standin answered\""), "{last}");
    assert!(last.contains("And the signup tests"), "{last}");
    assert!(last.find("</catch_up>") < last.find("Thanks"), "the catch-up comes before the message: {last}");
    let primary = host.log(&bot, "primary.jsonl");
    assert!(primary.iter().any(|r| r["method"] == "session/load"), "it resumes the session it had");

    // Both out of usage: the turn fails, and says how each agent is.
    std::fs::write(folder.join("primary.limit"), "").unwrap();
    std::fs::write(folder.join("standin.limit"), "").unwrap();
    host.call("send", json!({"botId": bot, "text": "Once more", "clientNonce": "n-last"})).await;
    let failed = wait_for(&host, &bot, "failure", |e| e["data"]["style"] == "error").await;
    let text = failed["data"]["text"].as_str().unwrap();
    assert!(
        text.contains("No agent in this bot's list could answer: Custom command reached its usage limit"),
        "{text}"
    );
    assert!(text.contains("Codex reached its usage limit (resets in 1 h)"), "{text}");
}

#[tokio::test]
async fn an_agent_back_in_a_new_session_hears_the_conversation_unless_the_user_started_over() {
    let host = Host::start().await;
    let bot =
        json!({"name": "Relay", "backend": "custom", "command": agent("primary"), "connectors": [], "notify": false});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    let fallbacks = json!([{"backend": "codex", "command": agent("standin")}]);
    host.call("updateBot", json!({"id": bot, "fallbacks": fallbacks})).await;
    let folder = host.home.join("bots").join(&bot).join("workspace");
    std::fs::write(folder.join("primary.limit"), "").unwrap();
    host.reply(&bot, "Fix the login tests").await;

    // Its own session won't load: the new one gets a recap, not a catch-up.
    std::fs::remove_file(folder.join("primary.limit")).unwrap();
    std::fs::write(folder.join("primary.noload"), "").unwrap();
    host.call("switchBack", json!({"botId": bot})).await;
    host.reply(&bot, "Thanks").await;
    let last = last_prompt(&host, &bot, "primary");
    assert!(last.contains("<handoff>") && !last.contains("<catch_up>"), "{last}");
    assert!(last.contains("Fix the login tests") && last.contains("\"standin answered\""), "{last}");

    // After New session, no agent is told what came before it.
    std::fs::write(folder.join("primary.limit"), "").unwrap();
    host.reply(&bot, "One more").await;
    host.call("newSession", json!({"botId": bot})).await;
    std::fs::remove_file(folder.join("primary.limit")).unwrap();
    host.call("switchBack", json!({"botId": bot})).await;
    host.reply(&bot, "Fresh start").await;
    let last = last_prompt(&host, &bot, "primary");
    assert!(last.contains("Fresh start"), "{last}");
    assert!(!last.contains("<handoff>") && !last.contains("Fix the login tests"), "{last}");
}

#[tokio::test]
async fn an_agent_without_the_bots_model_is_skipped_until_the_settings_change() {
    let host = Host::start().await;
    let bot = json!({
        "name": "Relay", "backend": "custom", "command": agent("primary"), "model": "missing", "connectors": [],
        "notify": false, "fallbacks": [{"backend": "codex", "command": agent("standin")}],
    });
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    host.reply(&bot, "Hello").await;
    host.reply(&bot, "Again").await;
    let new_sessions =
        |host: &Host| host.log(&bot, "primary.jsonl").iter().filter(|r| r["method"] == "session/new").count();
    assert_eq!(new_sessions(&host), 1, "the second message stays on the fallback");
    assert!(last_prompt(&host, &bot, "standin").contains("Again"));

    host.call("updateBot", json!({"id": bot, "model": null})).await;
    host.reply(&bot, "Now").await;
    assert!(last_prompt(&host, &bot, "primary").contains("Now"), "the bot's own agent again");
}

#[tokio::test]
async fn a_bot_without_fallbacks_offers_to_add_one() {
    let host = Host::start().await;
    let bot =
        json!({"name": "Solo", "backend": "custom", "command": agent("primary"), "connectors": [], "notify": false});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    std::fs::write(host.home.join("bots").join(&bot).join("workspace").join("primary.limit"), "").unwrap();
    host.call("send", json!({"botId": bot, "text": "Hello", "clientNonce": "n1"})).await;
    let failed = wait_for(&host, &bot, "failure", |e| e["data"]["style"] == "error").await;
    assert_eq!(failed["data"]["addFallback"], true);
    assert!(failed["data"]["text"].as_str().unwrap().contains("You've hit your limit"));
}

#[tokio::test]
async fn a_signed_out_agent_hands_the_turn_over_and_asks_to_sign_in() {
    let host = Host::start().await;
    // A harness Codync knows (it gets a sign-in card) that no machine running the tests has installed.
    let bot =
        json!({"name": "Signed", "backend": "kimi", "command": agent("primary"), "connectors": [], "notify": false});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    let fallbacks = json!([{"backend": "codex", "command": agent("standin")}]);
    host.call("updateBot", json!({"id": bot, "fallbacks": fallbacks})).await;
    let folder = host.home.join("bots").join(&bot).join("workspace");
    std::fs::write(folder.join("primary.signedout"), "").unwrap();

    host.reply(&bot, "Fix the build").await;
    let reply = wait_for(&host, &bot, "reply", |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    assert_eq!(reply["data"]["text"], "standin answered");
    let card = wait_for(&host, &bot, "sign-in card", |e| e["data"]["connectionRequest"]["kind"] == "agent").await;
    let request = &card["data"]["connectionRequest"];
    assert_eq!((request["backend"].as_str(), request["status"].as_str()), (Some("kimi"), Some("pending")));
    assert_eq!(request["entryIds"], json!([]), "the fallback answered: nothing waits for the sign-in");
    let line = wait_for(&host, &bot, "switch line", |e| e["data"]["agentSwitch"].is_object()).await;
    assert_eq!(line["data"]["text"], "Kimi Code is signed out. Codex carried on.");
    assert_eq!(bot_value(&host, &bot).await["resting"][0]["reason"], "signedOut");

    // Another bot on the same agent skips it while it rests, and asks to sign in too.
    let second = json!({"name": "Second", "backend": "kimi", "command": agent("primary"), "connectors": [],
        "notify": false, "fallbacks": fallbacks});
    let second = host.call("createBot", second).await["bot"]["id"].as_str().unwrap().to_owned();
    host.reply(&second, "Fix the tests").await;
    let line = wait_for(&host, &second, "switch line", |e| e["data"]["agentSwitch"].is_object()).await;
    assert_eq!(line["data"]["text"], "Kimi Code is signed out. Codex carried on.");
    let card = wait_for(&host, &second, "sign-in card", |e| e["data"]["connectionRequest"]["kind"] == "agent").await;
    assert_eq!(card["data"]["connectionRequest"]["backend"], "kimi");

    // One that starts on an agent before it skips it when that agent fails, and asks to sign in too.
    let chain = json!([{"backend": "kimi", "command": agent("primary")}, fallbacks[0]]);
    let third = json!({"name": "Third", "backend": "custom", "command": agent("limited"), "connectors": [],
        "notify": false, "fallbacks": chain});
    let third = host.call("createBot", third).await["bot"]["id"].as_str().unwrap().to_owned();
    std::fs::write(host.home.join("bots").join(&third).join("workspace").join("limited.limit"), "").unwrap();
    host.reply(&third, "Fix the docs").await;
    let card = wait_for(&host, &third, "sign-in card", |e| e["data"]["connectionRequest"]["kind"] == "agent").await;
    assert_eq!(card["data"]["connectionRequest"]["backend"], "kimi");

    // A turn of the agent elsewhere means it's signed in (with a command of its own, too): the bot goes back.
    let solo =
        json!({"name": "Solo", "backend": "kimi", "command": agent("helper"), "connectors": [], "notify": false});
    let solo = host.call("createBot", solo).await["bot"]["id"].as_str().unwrap().to_owned();
    host.reply(&solo, "Hello").await;
    wait_for(&host, &bot, "the card to complete", |e| e["data"]["connectionRequest"]["status"] == "ready").await;
    std::fs::remove_file(folder.join("primary.signedout")).unwrap();
    host.reply(&bot, "Thanks").await;
    wait_for(&host, &bot, "back line", |e| e["data"]["text"] == "Back on Kimi Code.").await;
    assert!(last_prompt(&host, &bot, "primary").contains("Thanks"));
}

/// How `CodeBuddy`'s CLI (`WorkBuddy`'s) says it's signed out: a refusal with the error in its meta.
const SIGNED_OUT_REFUSAL: &str = r#"{"code":-32000,"message":"Authentication required","data":{"details":"Please use /login command to sign in to your account","category":"auth"}}"#;

#[tokio::test]
async fn a_sign_in_error_inside_a_refusal_hands_the_turn_over() {
    let host = Host::start().await;
    let bot = json!({"name": "Hidden", "backend": "kimi", "command": agent("primary"), "connectors": [],
        "notify": false, "fallbacks": [{"backend": "codex", "command": agent("standin")}]});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    std::fs::write(host.home.join("bots").join(&bot).join("workspace").join("primary.refusal"), SIGNED_OUT_REFUSAL)
        .unwrap();

    host.reply(&bot, "Fix the build").await;
    let reply = wait_for(&host, &bot, "reply", |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    assert_eq!(reply["data"]["text"], "standin answered");
    let line = wait_for(&host, &bot, "switch line", |e| e["data"]["agentSwitch"].is_object()).await;
    assert_eq!(line["data"]["text"], "Kimi Code is signed out. Codex carried on.");
    wait_for(&host, &bot, "sign-in card", |e| e["data"]["connectionRequest"]["kind"] == "agent").await;
    assert_eq!(bot_value(&host, &bot).await["resting"][0]["reason"], "signedOut");
}

#[tokio::test]
async fn an_error_inside_a_refusal_holds_the_message_or_says_what_it_is() {
    let host = Host::start().await;
    let bot =
        json!({"name": "Solo", "backend": "kimi", "command": agent("primary"), "connectors": [], "notify": false});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    let refusal = host.home.join("bots").join(&bot).join("workspace").join("primary.refusal");
    std::fs::write(&refusal, SIGNED_OUT_REFUSAL).unwrap();

    // Signed out: the sign-in card holds the message and sends it again once the agent is signed in.
    host.call("send", json!({"botId": bot, "text": "Fix the build", "clientNonce": "n1"})).await;
    let card = wait_for(&host, &bot, "sign-in card", |e| e["data"]["connectionRequest"]["kind"] == "agent").await;
    assert_eq!(card["data"]["connectionRequest"]["entryIds"].as_array().map(Vec::len), Some(1), "{card}");
    std::fs::remove_file(&refusal).unwrap();
    let helper =
        json!({"name": "Helper", "backend": "kimi", "command": agent("helper"), "connectors": [], "notify": false});
    let helper = host.call("createBot", helper).await["bot"]["id"].as_str().unwrap().to_owned();
    host.reply(&helper, "Hello").await;
    let reply = wait_for(&host, &bot, "the held message's reply", |e| e["data"]["final"] == true).await;
    assert_eq!(reply["data"]["text"], "primary answered");
    assert!(last_prompt(&host, &bot, "primary").contains("Fix the build"));

    // Any other error is shown as the agent words it, with no sign-in card: signing in doesn't fix a bad key.
    let custom_model = r#"{"code":-32603,"message":"Internal error","data":{"details":"Authentication failed for custom model my-model. Check if the apiKey in your models.json is correct","category":"custom_model_auth"}}"#;
    std::fs::write(&refusal, custom_model).unwrap();
    host.call("send", json!({"botId": bot, "text": "Again", "clientNonce": "n2"})).await;
    let failed = wait_for(&host, &bot, "failure", |e| e["data"]["style"] == "error").await;
    assert!(failed["data"]["text"].as_str().unwrap().contains("Check if the apiKey in your models.json"), "{failed}");
    let history = host.call("history", json!({"botId": bot})).await;
    let cards = history["entries"].as_array().unwrap().iter().filter(|e| e["data"]["connectionRequest"].is_object());
    assert_eq!(cards.count(), 1, "no new sign-in card: {history}");

    // A refusal with nothing in it is still the agent declining.
    std::fs::write(&refusal, "").unwrap();
    host.call("send", json!({"botId": bot, "text": "Once more", "clientNonce": "n3"})).await;
    wait_for(&host, &bot, "declined notice", |e| e["data"]["text"] == "The agent declined to continue.").await;
}
