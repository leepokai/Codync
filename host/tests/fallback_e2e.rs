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
    common::fixture_command("relay_agent.py", name)
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

/// Real host: a fresh session's handoff carries the mechanical Context Manifest and respects a new-session boundary.
#[tokio::test]
async fn a_fresh_handoff_carries_the_context_manifest_and_names_budget_omissions() {
    let host = Host::start().await;
    let bot = host
        .call(
            "createBot",
            json!({"name":"Ledger", "backend":"custom", "command":agent("primary"), "connectors":[], "notify":false,
        "fallbacks":[{"backend":"codex", "command":agent("standin")}]}),
        )
        .await["bot"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let folder = host.home.join("bots").join(&bot).join("workspace");
    std::fs::write(folder.join("primary.limit"), "").unwrap();
    std::fs::write(folder.join("ledger-files.json"), r#"["old-secret.txt"]"#).unwrap();
    host.reply(&bot, "Never change old/private.txt").await;
    host.call("newSession", json!({"botId":bot})).await;
    std::fs::write(folder.join("ledger-files.json"), r#"["current.txt"]"#).unwrap();
    host.reply(&bot, "Only work in new/current.txt").await;
    std::fs::remove_file(folder.join("primary.limit")).unwrap();
    std::fs::write(folder.join("primary.noload"), "").unwrap();
    host.call("switchBack", json!({"botId":bot})).await;
    host.reply(&bot, "Continue").await;
    let prompt = last_prompt(&host, &bot, "primary");
    assert!(prompt.contains("<handoff>") && prompt.contains("Only work in new/current.txt"), "{prompt}");
    assert!(
        prompt.contains(
            "current.txt (created; 5 B; sha256 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        ),
        "{prompt}"
    );
    assert!(prompt.contains("synthetic-test") && prompt.contains("exit 3"), "{prompt}");
    assert!(
        !prompt.contains("old/private.txt") && !prompt.contains("old-secret.txt"),
        "reset scopes the manifest: {prompt}"
    );
}

#[tokio::test]
async fn a_bounded_handoff_names_each_item_it_could_not_expand() {
    let host = Host::start().await;
    let bot = host
        .call(
            "createBot",
            json!({"name":"Budget", "backend":"custom", "command":agent("primary"), "connectors":[], "notify":false,
        "fallbacks":[{"backend":"codex", "command":agent("standin")}]}),
        )
        .await["bot"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let folder = host.home.join("bots").join(&bot).join("workspace");
    // The primary agent reports a known 100k-token window; carry::room gives the handoff 60_000 bytes.
    std::fs::write(folder.join("primary.window"), "").unwrap();
    host.reply(&bot, "Report your window").await;
    let paths: Vec<_> = (0..24).map(|i| format!("file_{i}_{}.txt", "x".repeat(90))).collect();
    std::fs::write(folder.join("ledger-files.json"), serde_json::to_vec(&paths).unwrap()).unwrap();
    std::fs::write(folder.join("primary.limit"), "").unwrap();
    host.reply(&bot, &format!("Never lose these constraints {}", "a".repeat(12_000))).await;
    std::fs::remove_file(folder.join("primary.limit")).unwrap();
    std::fs::write(folder.join("primary.noload"), "").unwrap();
    host.call("switchBack", json!({"botId":bot})).await;
    host.reply(&bot, "Continue").await;
    let prompt = last_prompt(&host, &bot, "primary");
    let from = prompt.find("<handoff>").unwrap();
    let end = prompt[from..].find("</handoff>").unwrap() + from + "</handoff>".len();
    let handoff = &prompt[from..end];
    assert!(handoff.len() <= 60_000, "{} bytes", handoff.len());
    assert!(handoff.contains("collection limit") && handoff.contains("[truncated;"), "{handoff}");
    assert!(handoff.contains("context budget") || handoff.contains("labels not expanded"), "{handoff}");
    assert!(handoff.contains("created") && handoff.contains("5 B"), "{handoff}");
    assert!(
        handoff.contains("Omitted due to budget/collection limit") && handoff.contains("search_history"),
        "every unexpanded item must be disclosed: {handoff}"
    );
    let disclosure = handoff.split("Omitted due to budget/collection limit:").nth(1).unwrap_or("");
    for index in 0..24 {
        let prefix = format!("file_{index}_");
        let expanded = handoff.contains(&format!("File this turn: {prefix}"));
        let omitted = disclosure.lines().find(|line| line.contains(&prefix));
        assert!(expanded || omitted.is_some(), "file {index} must be expanded or itemized: {handoff}");
        if let Some(label) = omitted {
            assert!(
                label.contains(" B") && (label.contains("collection limit") || label.contains("context budget")),
                "an omitted file must name its size and reason: {label}"
            );
        }
    }
    let skipped: usize = handoff
        .split("Omitted: ")
        .nth(1)
        .and_then(|text| text.split(" items").next())
        .and_then(|text| text.parse().ok())
        .unwrap_or_default();
    let labels = disclosure.lines().filter(|line| line.contains(" B") || line.contains("unknown bytes")).count();
    assert_eq!(labels, skipped, "every unexpanded item needs its own label: {handoff}");
}

/// The agent a turn switches to hears the manifest in its very first message, and every item the manifest could not
/// expand carries its own label there.
#[tokio::test]
async fn a_switch_hands_the_manifest_over_in_the_new_agents_first_message() {
    let host = Host::start().await;
    let bot = host
        .call(
            "createBot",
            json!({"name":"SwitchManifest", "backend":"custom", "command":agent("primary"), "connectors":[], "notify":false,
        "fallbacks":[{"backend":"codex", "command":agent("standin")}]}),
        )
        .await["bot"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let folder = host.home.join("bots").join(&bot).join("workspace");
    // More files than a manifest expands, each one created by the agent the turn starts on.
    let paths: Vec<_> = (0..24).map(|i| format!("note_{i:02}_{}.txt", "y".repeat(30))).collect();
    std::fs::write(folder.join("ledger-files.json"), serde_json::to_vec(&paths).unwrap()).unwrap();
    std::fs::write(folder.join("primary.limit"), "").unwrap();
    host.reply(&bot, "Never run the suite twice").await;
    let reply = wait_for(&host, &bot, "reply", |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    assert_eq!(reply["data"]["text"], "standin answered");

    // The switched-to agent's first message is the handed-over turn itself.
    let log = host.log(&bot, "standin.jsonl");
    let prompts: Vec<_> = log.iter().filter(|r| r["method"] == "session/prompt").collect();
    assert_eq!(prompts.len(), 1, "one prompt, its first: {log:#?}");
    let prompt = prompts[0]["params"]["prompt"][0]["text"].as_str().unwrap();
    assert!(prompt.contains("<ledger>\nGathered by rule"), "the first message carries the manifest: {prompt}");
    assert!(prompt.contains("a.txt (created;"), "{prompt}");

    // Every file is either expanded or named by its own label, with its size and the reason it stayed out.
    let disclosure = prompt
        .split("Omitted due to budget/collection limit:")
        .nth(1)
        .unwrap_or_else(|| panic!("the manifest names what it could not expand: {prompt}"));
    let disclosure = disclosure.split("</ledger>").next().unwrap_or(disclosure);
    let labels: Vec<&str> = disclosure.lines().filter(|line| !line.trim().is_empty()).collect();
    for index in 0..24 {
        let prefix = format!("note_{index:02}_");
        let expanded = prompt.contains(&format!("File this turn: {prefix}"));
        let labeled = labels.iter().find(|label| label.contains(&prefix));
        assert!(expanded || labeled.is_some(), "file {index} must be expanded or itemized: {prompt}");
        if let Some(label) = labeled {
            assert!(
                label.contains(" B; ") && label.contains("collection limit"),
                "an unexpanded file names its size and why: {label}"
            );
        }
    }
    let skipped: usize = prompt
        .split("Omitted: ")
        .nth(1)
        .and_then(|text| text.split(" items").next())
        .and_then(|text| text.parse().ok())
        .unwrap_or_default();
    assert_eq!(labels.len(), skipped, "every unexpanded item needs its own label: {prompt}");
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
    std::fs::write(folder.join("standin.work"), "").unwrap();
    host.reply(&bot, "And the signup tests").await;
    std::fs::remove_file(folder.join("standin.work")).unwrap();
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
    // What the fallback changed meanwhile is named; what this agent did itself before it left is not news to it.
    assert!(last.contains("Files changed, newest first:\n- standin.txt (modified)\n</ledger>"), "{last}");
    assert!(!last.contains("a.txt (created)") && !last.contains("npm test"), "{last}");
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
async fn a_fallback_without_the_bots_mode_doesnt_take_the_turn() {
    let host = Host::start().await;
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mode_agent.py");
    let bot = json!({"name": "Moded", "backend": "custom", "command": common::python_agent(&script),
        "connectors": [], "notify": false, "permission": "ask", "mode": "read-only"});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    host.call("updateBot", json!({"id": bot, "fallbacks": [{"backend": "codex", "command": agent("standin")}]})).await;
    let folder = host.home.join("bots").join(&bot).join("workspace");
    std::fs::write(folder.join("limit"), "").unwrap();

    host.call("send", json!({"botId": bot, "text": "Fix the login tests"})).await;
    let failed = wait_for(&host, &bot, "failure", |e| e["data"]["style"] == "error").await;
    let text = failed["data"]["text"].as_str().unwrap();
    assert!(text.contains("Codex doesn't have read-only, the mode this bot works in"), "{text}");
    let standin = host.log(&bot, "standin.jsonl");
    assert!(!standin.iter().any(|r| r["method"] == "session/prompt"), "the fallback never works in its own mode");
}

fn moded(args: &str) -> String {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/named_mode_agent.py");
    format!("{} {args}", common::python_agent(&script))
}

/// A bot in `mode` on agent `primary` (`named_mode_agent.py` args) with `fallbacks`, the files in `blocked` made in
/// its folder, after its first message.
async fn moded_bot(host: &Host, mode: &str, primary: &str, fallbacks: Value, blocked: &[&str]) -> String {
    let bot = json!({"name": "Moded", "backend": "custom", "command": moded(primary), "connectors": [],
        "notify": false, "permission": "ask", "mode": mode});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    host.call("updateBot", json!({"id": bot, "fallbacks": fallbacks})).await;
    let folder = host.home.join("bots").join(&bot).join("workspace");
    for file in blocked {
        std::fs::write(folder.join(file), "").unwrap();
    }
    host.call("send", json!({"botId": bot, "text": "Fix the login tests"})).await;
    bot
}

fn prompted(host: &Host, bot: &str, agent: &str) -> bool {
    host.log(bot, &format!("{agent}.jsonl")).iter().any(|r| r["method"] == "session/prompt")
}

#[tokio::test]
async fn a_step_without_the_bots_mode_lets_the_next_one_answer() {
    let host = Host::start().await;
    let fallbacks = json!([{"backend": "codex", "command": agent("standin")},
        {"backend": "gemini", "command": moded("third read-only agent")}]);
    let bot = moded_bot(&host, "read-only", "primary read-only agent", fallbacks, &["primary.limit"]).await;
    wait_for(&host, &bot, "the third agent's answer", |e| e["data"]["text"] == "third mode read-only").await;
    assert!(!prompted(&host, &bot, "standin"));
}

#[tokio::test]
async fn a_step_without_the_bots_mode_runs_in_its_freest_one_no_freer() {
    let host = Host::start().await;
    let fallbacks = json!([{"backend": "codex", "command": moded("standin agent read-only agent-full-access")}]);
    let bot = moded_bot(&host, "plan", "primary plan agent", fallbacks, &["primary.limit"]).await;
    wait_for(&host, &bot, "the stand-in's answer", |e| e["data"]["text"] == "standin mode read-only").await;
    let notice = wait_for(&host, &bot, "a notice", |e| {
        e["data"]["text"].as_str().is_some_and(|t| t.contains("doesn't have plan") && t.contains("as read-only"))
    })
    .await;
    assert_eq!(notice["data"]["style"], "info", "{notice}");
}

#[tokio::test]
async fn a_step_with_only_freer_modes_moves_on_to_the_next() {
    let host = Host::start().await;
    let fallbacks = json!([{"backend": "codex", "command": moded("standin build agent")},
        {"backend": "gemini", "command": moded("third agent plan")}]);
    let bot = moded_bot(&host, "plan", "primary plan agent", fallbacks, &["primary.limit"]).await;
    wait_for(&host, &bot, "the third agent's answer", |e| e["data"]["text"] == "third mode plan").await;
    assert!(!prompted(&host, &bot, "standin"), "the fallback never works more freely than the bot");
}

#[tokio::test]
async fn a_step_that_refuses_the_switch_moves_on_to_the_next() {
    let host = Host::start().await;
    let fallbacks = json!([{"backend": "codex", "command": moded("standin agent read-only")},
        {"backend": "gemini", "command": moded("third agent plan")}]);
    let bot = moded_bot(&host, "plan", "primary plan agent", fallbacks, &["primary.limit", "standin.stuck"]).await;
    wait_for(&host, &bot, "the third agent's answer", |e| e["data"]["text"] == "third mode plan").await;
    assert!(!prompted(&host, &bot, "standin"));
    let history = host.call("history", json!({"botId": bot})).await;
    assert!(!history.to_string().contains("runs this bot as"), "no stand-in notice for a refused switch: {history}");
}

#[tokio::test]
async fn the_bots_own_agent_without_its_mode_fails_the_turn() {
    let host = Host::start().await;
    let fallbacks = json!([{"backend": "codex", "command": moded("standin plan")}]);
    let bot = moded_bot(&host, "plan", "primary read-only agent", fallbacks, &[]).await;
    let failed = wait_for(&host, &bot, "failure", |e| e["data"]["style"] == "error").await;
    let text = failed["data"]["text"].as_str().unwrap();
    assert!(text.contains("doesn't have plan, the mode this bot works in"), "{text}");
    assert!(!prompted(&host, &bot, "primary") && !prompted(&host, &bot, "standin"), "the chain doesn't move on");
}

#[tokio::test]
async fn a_command_right_after_going_back_leaves_the_catch_up_for_the_next_message() {
    let host = Host::start().await;
    let bot =
        json!({"name": "Relay", "backend": "custom", "command": agent("primary"), "connectors": [], "notify": false});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    let fallbacks = json!([{"backend": "codex", "command": agent("standin")}]);
    host.call("updateBot", json!({"id": bot, "fallbacks": fallbacks})).await;
    let folder = host.home.join("bots").join(&bot).join("workspace");
    std::fs::write(folder.join("primary.commands"), "").unwrap();
    std::fs::write(folder.join("primary.limit"), "").unwrap();
    host.reply(&bot, "Fix the login tests").await;
    wait_for(&host, &bot, "reply", |e| e["data"]["text"] == "standin answered").await;

    std::fs::remove_file(folder.join("primary.limit")).unwrap();
    host.call("switchBack", json!({"botId": bot})).await;
    wait_for(&host, &bot, "back line", |e| e["data"]["text"] == "Back on Custom command.").await;
    host.reply(&bot, "/review the login tests").await;
    assert_eq!(last_prompt(&host, &bot, "primary"), "/review the login tests", "the command goes as typed");
    host.reply(&bot, "Thanks").await;
    let last = last_prompt(&host, &bot, "primary");
    assert!(last.contains("<catch_up>") && last.contains("\"standin answered\""), "{last}");
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
    // The handoff carries the ledger of the turns before: an index of what changed and ran, never the contents.
    let ledger = &last[last.find("<ledger>").unwrap()..last.find("</ledger>").unwrap()];
    assert!(ledger.contains("Files changed, newest first:\n- a.txt (created)"), "{ledger}");
    assert!(ledger.contains("Commands run, failed ones first:\n- npm test (failed)"), "{ledger}");
    assert!(!last.contains("the edit's own text"), "paths only: {last}");

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
async fn a_recap_for_an_agent_whose_window_is_known_reaches_back_to_the_users_earlier_words() {
    let host = Host::start().await;
    let bot =
        json!({"name": "Relay", "backend": "custom", "command": agent("primary"), "connectors": [], "notify": false});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    host.call("updateBot", json!({"id": bot, "fallbacks": [{"backend": "codex", "command": agent("standin")}]})).await;
    let folder = host.home.join("bots").join(&bot).join("workspace");
    // The bot's agent reports its window while it works through a chat longer than a recap repeats in full, in
    // messages longer than a chat line carries.
    std::fs::write(folder.join("primary.window"), "").unwrap();
    std::fs::write(folder.join("primary.work"), "").unwrap();
    for n in 0..8 {
        host.reply(&bot, &format!("Step {n} of the migration{}", ", then the next table".repeat(70))).await;
    }
    std::fs::remove_file(folder.join("primary.work")).unwrap();
    std::fs::write(folder.join("primary.limit"), "").unwrap();
    host.reply(&bot, "Fix the login tests").await;

    // The fallback never said how wide its window is: it gets the latest messages, as ever.
    let standin = last_prompt(&host, &bot, "standin");
    assert!(standin.contains("<handoff>") && standin.contains("Step 7 of the migration"), "{standin}");
    assert!(
        !standin.contains("Step 0 of the migration") && !standin.contains("What the user said before"),
        "{standin}"
    );
    assert!(standin.contains("Not every earlier message is repeated here"), "{standin}");
    // The ledger is of the turns before; what the agent did in this one before it stopped is in the takeover alone.
    let ledger = &standin[standin.find("<ledger>").unwrap()..standin.find("</ledger>").unwrap()];
    assert!(ledger.contains("Files changed, newest first:\n- primary.txt (modified)\n"), "{ledger}");
    assert!(!ledger.contains("a.txt") && !ledger.contains("npm test"), "{ledger}");
    assert_eq!(standin.matches("- Edit a.txt (completed)").count(), 1, "{standin}");
    // The chat line unfolds to the start and the end of a handoff too long for it: what only it says stays.
    let line = wait_for(&host, &bot, "switch line", |e| e["data"]["agentSwitch"].is_object()).await;
    let carried = line["data"]["agentSwitch"]["carried"].as_str().unwrap();
    assert!(carried.len() < 8_100 && carried.starts_with("<handoff>") && carried.contains("\n…\n"), "{carried}");
    assert!(carried.contains("<ledger>") && carried.contains("partway through this turn"), "{carried}");

    // The bot's own agent did: in a new session it also hears what the user said before those, shortened.
    std::fs::remove_file(folder.join("primary.limit")).unwrap();
    std::fs::write(folder.join("primary.noload"), "").unwrap();
    host.call("switchBack", json!({"botId": bot})).await;
    host.reply(&bot, "Thanks").await;
    let last = last_prompt(&host, &bot, "primary");
    let earlier = last.find("What the user said before the latest messages").expect(&last);
    let step = last.find("\"Step 0 of the migration").expect(&last);
    assert!(earlier < step && step < last.find("The latest messages").unwrap(), "{last}");
    assert!(last.contains("- Bot: \"standin answered\""), "{last}");
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

#[tokio::test]
async fn a_process_with_a_replaced_sign_in_starts_again_before_asking_to_sign_in() {
    let host = Host::start().await;
    let bot =
        json!({"name": "Solo", "backend": "kimi", "command": agent("primary"), "connectors": [], "notify": false});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    let folder = host.home.join("bots").join(&bot).join("workspace");
    host.reply(&bot, "Hello").await;
    wait_for(&host, &bot, "first reply", |e| e["data"]["final"] == true).await;
    let prompts = || host.log(&bot, "primary.jsonl").iter().filter(|r| r["method"] == "session/prompt").count();
    let answered = |history: &Value| {
        history["entries"].as_array().unwrap().iter().filter(|e| e["data"]["text"] == "primary answered").count()
    };

    // The running process holds a sign-in replaced since (Claude's revoked token): a new process answers, no card.
    std::fs::write(folder.join("primary.revoked"), "").unwrap();
    host.call("send", json!({"botId": bot, "text": "Fix the build", "clientNonce": "n1"})).await;
    let deadline = Instant::now() + Duration::from_secs(30);
    while answered(&host.call("history", json!({"botId": bot})).await) < 2 {
        assert!(Instant::now() < deadline, "the message wasn't answered again");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(last_prompt(&host, &bot, "primary").contains("Fix the build"));
    assert_eq!(prompts(), 3, "the old process's try and the new one's");
    let opened = |method: &str| host.log(&bot, "primary.jsonl").iter().filter(|r| r["method"] == method).count();
    assert_eq!(
        (opened("session/new"), opened("session/load")),
        (1, 1),
        "the new process goes on in the chat's session"
    );
    let history = host.call("history", json!({"botId": bot})).await;
    assert!(!history["entries"].as_array().unwrap().iter().any(|e| e["data"]["connectionRequest"].is_object()));

    // Signed out for real: the new process is refused too, and only then the card holds the message.
    std::fs::write(folder.join("primary.signedout"), "").unwrap();
    host.call("send", json!({"botId": bot, "text": "Again", "clientNonce": "n2"})).await;
    let card = wait_for(&host, &bot, "sign-in card", |e| e["data"]["connectionRequest"]["kind"] == "agent").await;
    assert_eq!(card["data"]["connectionRequest"]["entryIds"].as_array().map(Vec::len), Some(1), "{card}");
    assert_eq!(prompts(), 5, "tried once more on a new process, then no more");
}

#[tokio::test]
async fn a_new_process_that_cant_start_signed_out_keeps_the_chats_session() {
    let host = Host::start().await;
    let bot =
        json!({"name": "Solo", "backend": "kimi", "command": agent("primary"), "connectors": [], "notify": false});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    let folder = host.home.join("bots").join(&bot).join("workspace");
    host.reply(&bot, "Hello").await;

    // The running process holds a sign-in replaced since, and the new one quits as it starts: no process is left.
    std::fs::write(folder.join("primary.revoked"), "").unwrap();
    std::fs::write(folder.join("primary.nostart"), "").unwrap();
    host.call("send", json!({"botId": bot, "text": "Fix the build", "clientNonce": "n1"})).await;
    wait_for(&host, &bot, "sign-in card", |e| e["data"]["connectionRequest"]["kind"] == "agent").await;

    // Signed in, the next process goes on in the chat's session.
    std::fs::remove_file(folder.join("primary.nostart")).unwrap();
    host.reply(&bot, "Again").await;
    let opened = |method: &str| host.log(&bot, "primary.jsonl").iter().filter(|r| r["method"] == method).count();
    assert_eq!((opened("session/new"), opened("session/load")), (1, 1));
}

/// #470: pi's model request fails (a 503 after a provider queue), and pi-acp ends the turn as `end_turn` with no
/// reply; pi's error comes only as Codync's extension's notification. The turn still fails: the next agent takes it.
#[tokio::test]
async fn a_turn_pi_ended_in_an_error_moves_on_to_the_next_agent() {
    let host = Host::start().await;
    let bot = json!({"name": "Pi", "backend": "pi", "command": agent("primary"), "connectors": [], "notify": false});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    host.call("updateBot", json!({"id": bot, "fallbacks": [{"backend": "codex", "command": agent("standin")}]})).await;
    let folder = host.home.join("bots").join(&bot).join("workspace");
    std::fs::write(folder.join("primary.errored"), "").unwrap();

    host.call("send", json!({"botId": bot, "text": "Fix the login tests", "clientNonce": "n-470"})).await;
    let reply = wait_for(&host, &bot, "fallback reply", |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    assert_eq!(reply["data"]["text"], "standin answered");
    wait_for(&host, &bot, "switch line", |e| e["data"]["agentSwitch"].is_object()).await;
}

/// #470 without a fallback: the turn pi ended in an error says so in the chat instead of going silent.
#[tokio::test]
async fn a_turn_pi_ended_in_an_error_says_so() {
    let host = Host::start().await;
    let bot = json!({"name": "Pi", "backend": "pi", "command": agent("primary"), "connectors": [], "notify": false});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    let folder = host.home.join("bots").join(&bot).join("workspace");
    std::fs::write(folder.join("primary.errored"), "").unwrap();

    host.call("send", json!({"botId": bot, "text": "Fix the login tests", "clientNonce": "n-470"})).await;
    let notice = wait_for(&host, &bot, "error notice", |e| e["data"]["style"] == "error").await;
    assert!(notice["data"]["text"].as_str().unwrap().contains("upstream busy"), "{notice}");
}

#[tokio::test]
async fn a_switch_the_user_makes_carries_the_conversation_and_says_so() {
    let host = Host::start().await;
    let bot = relay_bot(&host).await;
    host.reply(&bot, "Fix the login tests").await;

    host.call("updateBot", json!({"id": bot, "backend": "codex", "command": agent("standin")})).await;
    let line = wait_for(&host, &bot, "pending line", manual).await;
    assert_eq!(line["data"]["agentSwitch"]["state"], "pending", "{line}");
    assert_eq!(line["data"]["text"], "Context handoff pending: Custom command → Codex");

    host.reply(&bot, "And the signup tests").await;
    let prompt = last_prompt(&host, &bot, "standin");
    assert!(prompt.contains("<handoff>") && prompt.contains("Fix the login tests"), "{prompt}");
    let done = wait_for(&host, &bot, "handed-off line", |e| state(e, "handedOff")).await;
    assert_eq!(done["id"], line["id"], "the same line moves on");
    assert_eq!(done["data"]["text"], "Context handed off: Custom command → Codex");
    assert!(done["data"]["agentSwitch"]["carried"].as_str().is_some_and(|c| c.contains("Fix the login tests")));

    // Switched back, then a new session: nothing is carried, and the line says so.
    host.call("updateBot", json!({"id": bot, "backend": "custom", "command": agent("primary")})).await;
    let back = wait_for(&host, &bot, "second line", |e| manual(e) && e["id"] != line["id"]).await;
    host.call("newSession", json!({"botId": bot})).await;
    host.reply(&bot, "Fresh start").await;
    let last = last_prompt(&host, &bot, "primary");
    assert!(!last.contains("<handoff>") && !last.contains("Fix the login tests"), "{last}");
    let dropped = wait_for(&host, &bot, "dropped line", |e| e["id"] == back["id"] && state(e, "switched")).await;
    assert_eq!(dropped["data"]["text"], "Switched: Codex → Custom command");
}

#[tokio::test]
async fn a_switch_the_user_makes_outlives_a_second_switch_and_a_host_restart() {
    let mut host = Host::start().await;
    let bot = relay_bot(&host).await;
    host.reply(&bot, "Fix the login tests").await;

    host.call("updateBot", json!({"id": bot, "backend": "codex", "command": agent("standin")})).await;
    let line = wait_for(&host, &bot, "pending line", manual).await;
    // Only the model: the same login, so nothing more is said, and the recap stays owed.
    host.call("updateBot", json!({"id": bot, "model": "bigger"})).await;
    host.call("updateBot", json!({"id": bot, "model": null})).await;
    // Back before the next message: the same line names the login it just left.
    host.call("updateBot", json!({"id": bot, "backend": "custom", "command": agent("primary")})).await;
    let moved =
        wait_for(&host, &bot, "moved line", |e| e["data"]["text"] == "Context handoff pending: Codex → Custom command")
            .await;
    assert_eq!(moved["id"], line["id"], "one line per handoff");
    let history = host.call("history", json!({"botId": bot})).await;
    assert_eq!(history["entries"].as_array().unwrap().iter().filter(|e| manual(e)).count(), 1, "{history}");

    host.restart(|_| {}).await;
    host.reply(&bot, "And the signup tests").await;
    let prompt = last_prompt(&host, &bot, "primary");
    assert!(prompt.contains("<handoff>") && prompt.contains("Fix the login tests"), "{prompt}");
    let done = wait_for(&host, &bot, "handed-off line", |e| state(e, "handedOff")).await;
    assert_eq!(done["id"], line["id"]);
}

#[tokio::test]
async fn a_command_right_after_the_users_switch_leaves_the_recap_for_the_next_message() {
    let host = Host::start().await;
    let bot = relay_bot(&host).await;
    std::fs::write(host.workspace(&bot).join("standin.commands"), "").unwrap();
    host.reply(&bot, "Fix the login tests").await;

    host.call("updateBot", json!({"id": bot, "backend": "codex", "command": agent("standin")})).await;
    wait_for(&host, &bot, "pending line", manual).await;
    host.reply(&bot, "/review the login tests").await;
    assert_eq!(last_prompt(&host, &bot, "standin"), "/review the login tests", "the command goes as typed");
    let line = wait_for(&host, &bot, "line", manual).await;
    assert_eq!(line["data"]["agentSwitch"]["state"], "pending", "{line}");
    host.reply(&bot, "Thanks").await;
    let last = last_prompt(&host, &bot, "standin");
    assert!(last.contains("<handoff>") && last.contains("Fix the login tests"), "{last}");
    wait_for(&host, &bot, "handed-off line", |e| state(e, "handedOff")).await;
}

/// The host moves `bot`, which works in a project, to its own folder (`hub::projects::migrate`, at a host start):
/// its store is set back to before projects, and the host starts again.
async fn moved_home(host: &mut Host, bot: &str) {
    let project = host.home.join("project");
    std::fs::create_dir_all(&project).unwrap();
    host.restart(|home| {
        let db = rusqlite::Connection::open(home.join("codync.db")).unwrap();
        let old = "UPDATE bots SET config = json_set(config, '$.cwd', ?2, '$.projects', json('[]')) WHERE id = ?1";
        db.execute(old, rusqlite::params![bot, project.to_string_lossy()]).unwrap();
        db.execute("DELETE FROM kv WHERE k = 'projects.migrated'", []).unwrap();
    })
    .await;
}

#[tokio::test]
async fn a_move_to_its_own_folder_carries_the_conversation_once() {
    let mut host = Host::start().await;
    let bot = relay_bot(&host).await;
    host.reply(&bot, "Fix the login tests").await;

    // The agent starts again in its own folder, in a new session: the next message carries the chat.
    moved_home(&mut host, &bot).await;
    host.reply(&bot, "And the signup tests").await;
    let prompt = last_prompt(&host, &bot, "primary");
    assert!(prompt.contains("<handoff>") && prompt.contains("Fix the login tests"), "{prompt}");
    assert!(prompt.contains("in another working folder") && !prompt.contains("another agent"), "{prompt}");
    host.reply(&bot, "Thanks").await;
    assert!(!last_prompt(&host, &bot, "primary").contains("<handoff>"), "carried once");
    let history = host.call("history", json!({"botId": bot})).await;
    assert!(!history["entries"].as_array().unwrap().iter().any(manual), "a move shows no switch line: {history}");
}

#[tokio::test]
async fn a_switch_after_a_move_to_its_own_folder_says_so_and_names_the_agent_left() {
    let mut host = Host::start().await;
    let bot = relay_bot(&host).await;
    host.reply(&bot, "Fix the login tests").await;

    moved_home(&mut host, &bot).await;
    host.call("updateBot", json!({"id": bot, "backend": "codex", "command": agent("standin")})).await;
    wait_for(&host, &bot, "pending line", manual).await;
    host.reply(&bot, "And the signup tests").await;
    let prompt = last_prompt(&host, &bot, "standin");
    assert!(prompt.contains("<handoff>") && prompt.contains("Fix the login tests"), "{prompt}");
    assert!(prompt.contains("another agent") && !prompt.contains("another working folder"), "{prompt}");
    let history = host.call("history", json!({"botId": bot})).await;
    assert_eq!(history["entries"].as_array().unwrap().iter().filter(|e| manual(e)).count(), 1, "{history}");
}

#[tokio::test]
async fn a_new_session_after_a_move_to_its_own_folder_carries_nothing() {
    let mut host = Host::start().await;
    let bot = relay_bot(&host).await;
    host.reply(&bot, "Fix the login tests").await;

    moved_home(&mut host, &bot).await;
    host.call("newSession", json!({"botId": bot})).await;
    host.reply(&bot, "Fresh start").await;
    let prompt = last_prompt(&host, &bot, "primary");
    assert!(!prompt.contains("<handoff>") && !prompt.contains("Fix the login tests"), "{prompt}");
}

#[tokio::test]
async fn a_move_to_its_own_folder_carries_the_context_manifest_past_a_restart() {
    let mut host = Host::start().await;
    let bot = relay_bot(&host).await;
    let home_ws = host.workspace(&bot);
    std::fs::write(home_ws.join("ledger-files.json"), serde_json::to_vec(&["moved_from_home.txt"]).unwrap()).unwrap();
    host.reply(&bot, "Never run the suite twice").await;

    moved_home(&mut host, &bot).await;
    host.reply(&bot, "Continue in own folder").await;

    let prompt = last_prompt(&host, &bot, "primary");
    assert!(prompt.contains("<handoff>") && prompt.contains("in another working folder"), "{prompt}");
    assert!(!prompt.contains("another agent"), "{prompt}");
    assert!(prompt.contains("<ledger>\nGathered by rule"), "move carries the context manifest: {prompt}");
    assert!(prompt.contains("Never run the suite twice"), "move preserves user constraints: {prompt}");
    assert!(prompt.contains("synthetic-test") && prompt.contains("exit 3"), "{prompt}");
    assert!(prompt.contains("moved_from_home.txt"), "{prompt}");
}

async fn relay_bot(host: &Host) -> String {
    let bot =
        json!({"name": "Relay", "backend": "custom", "command": agent("primary"), "connectors": [], "notify": false});
    host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned()
}

/// A line of a switch the user made.
fn manual(e: &Value) -> bool {
    e["data"]["agentSwitch"]["manual"] == true
}

/// A line of a switch the user made, in `state`.
fn state(e: &Value, state: &str) -> bool {
    manual(e) && e["data"]["agentSwitch"]["state"] == state
}
