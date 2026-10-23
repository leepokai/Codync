//! A bot's model settings (reasoning effort) through a real host and a fixture agent that offers them the way
//! Codex does: on the session, beside the model. A bot on the agent's default model gets its own effort on new and
//! resumed sessions, a value the agent rejects says which setting failed, and choosing a model drops the old one's
//! settings (#177).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// These tests must never unlock the host's vault: that writes a key to the login keychain, which outlives the
/// test home. Bots here have no connectors, no agent env and call no credential API, so `vault::unlock` is never
/// reached; this checks it when the test ends, before the home is deleted (`vault.id` is stored before the key).
struct NoVault(std::path::PathBuf);

impl NoVault {
    fn of(host: &Host) -> Self {
        Self(host.home.join("codync.db"))
    }
}

impl Drop for NoVault {
    fn drop(&mut self) {
        // No database: nothing was written. One that can't be read proves nothing, so it fails the test too.
        let problem = if self.0.exists() {
            let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY;
            match rusqlite::Connection::open_with_flags(&self.0, flags)
                .and_then(|db| db.query_row("SELECT COUNT(*) FROM kv WHERE k = 'vault.id'", [], |r| r.get::<_, i64>(0)))
            {
                Ok(0) => None,
                Ok(_) => Some("the test unlocked the vault: a login-keychain item was written".to_owned()),
                Err(e) => Some(format!("couldn't check that the vault stayed locked: {e}")),
            }
        } else {
            None
        };
        match problem {
            Some(problem) if !std::thread::panicking() => panic!("{problem} ({})", self.0.display()),
            Some(problem) => eprintln!("{problem} ({})", self.0.display()),
            None => {}
        }
    }
}

fn python_missing() -> bool {
    let missing = std::process::Command::new(common::python()).arg("--version").output().is_err();
    if missing {
        eprintln!("skipping: Python not available");
    }
    missing
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

/// Sends `text` and waits for the agent's answer to it.
async fn answer(host: &Host, bot: &str, text: &str) -> String {
    host.call("send", json!({"botId": bot, "text": text})).await;
    let found = |e: &Value| {
        e["kind"] == "agent"
            && e["data"]["final"] == true
            && e["data"]["text"].as_str().is_some_and(|t| t.ends_with(text))
    };
    wait_for(host, bot, text, found).await["data"]["text"].as_str().unwrap().to_owned()
}

/// The session calls the agent logged, as `method` or `configId=value`.
fn calls(host: &Host, bot: &str) -> Vec<String> {
    host.log(bot, "calls.jsonl")
        .iter()
        .map(|c| match c["configId"].as_str() {
            Some(id) => format!("{id}={}", c["value"].as_str().unwrap()),
            None => c["method"].as_str().unwrap().to_owned(),
        })
        .collect()
}

#[tokio::test]
async fn a_bot_on_the_default_model_gets_its_effort_on_new_and_resumed_sessions() {
    if python_missing() {
        return;
    }
    let mut host = Host::start().await;
    let _vault = NoVault::of(&host);
    let bot = host.fixture_bot("Thinker", "effort_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "modelOptions": {"reasoning_effort": "high"}})).await;
    assert_eq!(answer(&host, &bot, "one").await, "sol high · one", "the agent's model, the bot's effort");
    assert_eq!(calls(&host, &bot), ["session/new", "reasoning_effort=high"], "no model is chosen for it");

    // A fresh agent process starts at its default again: the resumed session gets the effort back.
    host.restart(|_| {}).await;
    assert_eq!(answer(&host, &bot, "two").await, "sol high · two");
    assert_eq!(calls(&host, &bot)[2..], ["session/load", "reasoning_effort=high"]);

    // Like a new model, a new setting starts a new agent session after the turn (the chat is kept).
    host.call("updateBot", json!({"id": bot, "modelOptions": {"reasoning_effort": "low"}})).await;
    assert_eq!(answer(&host, &bot, "three").await, "sol low · three");
    assert_eq!(calls(&host, &bot)[4..], ["session/new", "reasoning_effort=low"]);
}

#[tokio::test]
async fn a_setting_the_agent_rejects_names_the_setting() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let _vault = NoVault::of(&host);
    let bot = host.fixture_bot("Rejected", "effort_agent.py", "").await;
    // Listed, so it is sent; the agent refuses it.
    host.call("updateBot", json!({"id": bot, "modelOptions": {"reasoning_effort": "max"}})).await;
    host.call("send", json!({"botId": bot, "text": "one"})).await;
    let failed = wait_for(&host, &bot, "failure", |e| e["data"]["style"] == "error").await;
    let text = failed["data"]["text"].as_str().unwrap();
    assert!(text.contains("Couldn't set reasoning_effort to max"), "{text}");
}

#[tokio::test]
async fn a_setting_no_schema_offers_is_never_sent() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let _vault = NoVault::of(&host);
    let bot = host.fixture_bot("Unlisted", "effort_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "modelOptions": {"verbosity": "terse"}})).await;
    assert_eq!(answer(&host, &bot, "one").await, "sol low · one");
    assert_eq!(calls(&host, &bot), ["session/new"], "never sent");
    let said = wait_for(&host, &bot, "notice", |e| e["kind"] == "notice").await;
    assert!(said["data"]["text"].as_str().unwrap().contains("verbosity terse isn't offered by this agent"));
}

#[tokio::test]
async fn a_value_the_model_doesnt_offer_is_left_out_and_said() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let _vault = NoVault::of(&host);
    let bot = host.fixture_bot("Stale", "effort_agent.py", "").await;
    // Saved for one model, the agent's offer changed: mini has no high effort.
    host.call("updateBot", json!({"id": bot, "model": "mini", "modelOptions": {"reasoning_effort": "high"}})).await;
    assert_eq!(answer(&host, &bot, "one").await, "mini low · one", "the session still starts, on the agent's own");
    assert_eq!(calls(&host, &bot), ["session/new", "model=mini"], "never sent");
    let said = wait_for(&host, &bot, "notice", |e| e["kind"] == "notice").await;
    let text = said["data"]["text"].as_str().unwrap();
    assert!(text.contains("Reasoning Effort") && text.contains("high") && text.contains("isn't offered"), "{text}");
}

#[tokio::test]
async fn choosing_a_model_drops_the_default_models_effort() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let _vault = NoVault::of(&host);
    let bot = host.fixture_bot("Switcher", "effort_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "modelOptions": {"reasoning_effort": "high"}})).await;
    assert_eq!(answer(&host, &bot, "one").await, "sol high · one");

    // mini has no high effort: an app that only changes the model doesn't carry it over.
    host.call("updateBot", json!({"id": bot, "model": "mini"})).await;
    let snapshot = host.snapshot().await;
    let saved = snapshot["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot.as_str()).unwrap();
    assert_eq!(saved["modelOptions"], json!({}));
    assert_eq!(answer(&host, &bot, "two").await, "mini low · two");
    assert_eq!(calls(&host, &bot)[2..], ["session/new", "model=mini"]);
}

#[tokio::test]
async fn clients_see_what_the_running_session_uses() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let _vault = NoVault::of(&host);
    let bot = host.fixture_bot("Shown", "effort_agent.py", "").await;
    let in_use = || async {
        let snapshot = host.snapshot().await;
        snapshot["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot.as_str()).unwrap()["sessionOptions"].clone()
    };
    assert_eq!(in_use().await, Value::Null, "nothing is confirmed before the agent runs");

    host.call("updateBot", json!({"id": bot, "modelOptions": {"reasoning_effort": "high"}})).await;
    answer(&host, &bot, "one").await;
    assert_eq!(in_use().await, json!({"model": "sol", "reasoning_effort": "high"}), "what the agent reports");
}

#[tokio::test]
async fn a_chosen_models_settings_come_from_the_session_running_it() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let _vault = NoVault::of(&host);
    let bot = host.fixture_bot("Chosen", "effort_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "model": "mini"})).await;
    answer(&host, &bot, "one").await;
    let snapshot = host.snapshot().await;
    let shown = snapshot["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot.as_str()).unwrap().clone();
    // Discovery only sees the agent's default model; the bot's own session lists what mini offers (what it is set
    // to is `sessionOptions`).
    assert_eq!(
        shown["sessionSettings"],
        json!([{"id": "reasoning_effort", "name": "Reasoning Effort", "values": [{"id": "low", "name": "Low"}]}])
    );
}

#[tokio::test]
async fn a_bot_left_on_default_sends_nothing_and_another_bot_is_untouched() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let _vault = NoVault::of(&host);
    let set = host.fixture_bot("Set", "effort_agent.py", "").await;
    let plain = host.fixture_bot("Plain", "effort_agent.py", "").await;
    host.call("updateBot", json!({"id": set, "modelOptions": {"reasoning_effort": "high"}})).await;
    assert_eq!(answer(&host, &set, "one").await, "sol high · one");
    assert_eq!(answer(&host, &plain, "two").await, "sol low · two", "the agent's own default");
    assert_eq!(calls(&host, &plain), ["session/new"], "no model and no setting is forced on it");

    // Read back, then cleared to inherit again.
    let saved = |snapshot: &Value, id: &str| {
        snapshot["bots"].as_array().unwrap().iter().find(|b| b["id"] == id).unwrap()["modelOptions"].clone()
    };
    let snapshot = host.snapshot().await;
    assert_eq!(saved(&snapshot, &set), json!({"reasoning_effort": "high"}));
    assert_eq!(saved(&snapshot, &plain), json!({}));
    host.call("updateBot", json!({"id": set, "modelOptions": {}})).await;
    assert_eq!(saved(&host.snapshot().await, &set), json!({}));
    assert_eq!(answer(&host, &set, "three").await, "sol low · three");
    assert_eq!(calls(&host, &set)[2..], ["session/new"], "inheriting again sets nothing");
}

/// Turns on one of the fixture's behaviours (see `effort_agent.py`) for `bot`'s agent.
fn fixture_flag(host: &Host, bot: &str, flag: &str) {
    let workspace = host.home.join("bots").join(bot).join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::write(workspace.join(flag), "").unwrap();
}

async fn session_options(host: &Host, bot: &str) -> Value {
    let snapshot = host.snapshot().await;
    snapshot["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot).unwrap()["sessionOptions"].clone()
}

#[tokio::test]
async fn a_setter_answering_without_its_settings_leaves_the_value_unknown() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let _vault = NoVault::of(&host);
    let bot = host.fixture_bot("Silent", "effort_agent.py", "").await;
    fixture_flag(&host, &bot, "silent");
    host.call("updateBot", json!({"id": bot, "modelOptions": {"reasoning_effort": "high"}})).await;
    answer(&host, &bot, "one").await;
    // The agent took high but didn't say so: what it listed before the change (low) is no confirmation.
    assert_eq!(session_options(&host, &bot).await["reasoning_effort"], Value::Null);
}

#[tokio::test]
async fn another_sessions_update_does_not_change_the_bots_report() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let _vault = NoVault::of(&host);
    let bot = host.fixture_bot("Foreign", "effort_agent.py", "").await;
    fixture_flag(&host, &bot, "foreign");
    host.call("updateBot", json!({"id": bot, "modelOptions": {"reasoning_effort": "high"}})).await;
    answer(&host, &bot, "one").await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(session_options(&host, &bot).await["reasoning_effort"], "high", "not another session's max");
}

#[tokio::test]
async fn an_agent_that_exited_reports_nothing() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let _vault = NoVault::of(&host);
    let bot = host.fixture_bot("Gone", "effort_agent.py", "").await;
    fixture_flag(&host, &bot, "exit");
    answer(&host, &bot, "one").await;
    let deadline = Instant::now() + Duration::from_secs(10);
    while session_options(&host, &bot).await != Value::Null {
        assert!(Instant::now() < deadline, "still reports the exited agent's settings");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn a_change_during_a_reply_waits_for_it_and_says_so() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let _vault = NoVault::of(&host);
    let bot = host.fixture_bot("Patient", "effort_agent.py", "").await;
    let workspace = host.home.join("bots").join(&bot).join("workspace");
    host.call("updateBot", json!({"id": bot, "modelOptions": {"reasoning_effort": "high"}})).await;
    assert_eq!(answer(&host, &bot, "one").await, "sol high · one");

    fixture_flag(&host, &bot, "hold");
    host.call("send", json!({"botId": bot, "text": "two"})).await;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !workspace.join("prompted").exists() {
        assert!(Instant::now() < deadline, "the reply never started");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // Mid-reply: saved, waiting, and the running session still says high.
    host.call("updateBot", json!({"id": bot, "modelOptions": {"reasoning_effort": "low"}})).await;
    let state = |snapshot: Value| {
        snapshot["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot.as_str()).cloned().unwrap()
    };
    let during = state(host.snapshot().await);
    assert_eq!(during["restartPending"], true);
    assert_eq!(during["sessionOptions"]["reasoning_effort"], "high");
    assert_eq!(during["modelOptions"], json!({"reasoning_effort": "low"}));

    // The reply finishes as it began, then the agent restarts on the new setting.
    std::fs::remove_file(workspace.join("hold")).unwrap();
    std::fs::write(workspace.join("release"), "").unwrap();
    wait_for(&host, &bot, "the held reply", |e| {
        e["kind"] == "agent" && e["data"]["final"] == true && e["data"]["text"] == "sol high · two"
    })
    .await;
    let deadline = Instant::now() + Duration::from_secs(10);
    while state(host.snapshot().await)["restartPending"] == true {
        assert!(Instant::now() < deadline, "still waiting after the reply");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(answer(&host, &bot, "three").await, "sol low · three");
    // A new agent session (context starts fresh), not the held one resumed.
    let log = calls(&host, &bot);
    assert_eq!(log[log.len() - 2..], ["session/new", "reasoning_effort=low"], "{log:?}");
    assert!(!log.contains(&"session/load".to_owned()), "{log:?}");
}

fn bot_state(snapshot: &Value, bot: &str) -> Value {
    snapshot["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot).cloned().unwrap()
}

#[tokio::test]
async fn a_session_that_lists_nothing_is_running_with_its_settings_unknown() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let _vault = NoVault::of(&host);
    let bot = host.fixture_bot("Bare", "effort_agent.py", "").await;
    fixture_flag(&host, &bot, "bare");
    host.call("updateBot", json!({"id": bot, "modelOptions": {"reasoning_effort": "high"}})).await;
    answer(&host, &bot, "one").await;
    let state = bot_state(&host.snapshot().await, &bot);
    assert_eq!(state["sessionLive"], true, "running");
    assert_eq!(state["sessionOptions"], json!({}), "nothing it said, so nothing known");
    assert_eq!(state["sessionSettings"], Value::Null);
    assert_eq!(calls(&host, &bot), ["session/new"], "no list, nothing set");
}

#[tokio::test]
async fn an_issue_is_about_its_value_and_a_new_choice_clears_it() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let _vault = NoVault::of(&host);
    let bot = host.fixture_bot("Retry", "effort_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "modelOptions": {"reasoning_effort": "max"}})).await;
    host.call("send", json!({"botId": bot, "text": "one"})).await;
    wait_for(&host, &bot, "failure", |e| e["data"]["style"] == "error").await;
    let state = bot_state(&host.snapshot().await, &bot);
    assert_eq!(state["settingIssues"], json!({"reasoning_effort": {"issue": "rejected", "value": "max"}}));
    assert_eq!(state["sessionLive"], false, "the session never started");

    // Choosing high clears the refusal of max, and high applies.
    host.call("updateBot", json!({"id": bot, "modelOptions": {"reasoning_effort": "high"}})).await;
    assert_eq!(bot_state(&host.snapshot().await, &bot)["settingIssues"], json!({}));
    assert_eq!(answer(&host, &bot, "two").await, "sol high · two");

    // Back to Default: nothing saved, nothing sent, the agent's own low.
    host.call("updateBot", json!({"id": bot, "modelOptions": {}})).await;
    assert_eq!(answer(&host, &bot, "three").await, "sol low · three");
    let state = bot_state(&host.snapshot().await, &bot);
    assert_eq!(
        (state["settingIssues"].clone(), state["sessionOptions"]["reasoning_effort"].clone()),
        (json!({}), json!("low"))
    );
}

#[tokio::test]
async fn going_back_to_default_during_a_reply_is_pending_too() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let _vault = NoVault::of(&host);
    let bot = host.fixture_bot("Undo", "effort_agent.py", "").await;
    let workspace = host.home.join("bots").join(&bot).join("workspace");
    host.call("updateBot", json!({"id": bot, "modelOptions": {"reasoning_effort": "high"}})).await;
    answer(&host, &bot, "one").await;
    fixture_flag(&host, &bot, "hold");
    host.call("send", json!({"botId": bot, "text": "two"})).await;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !workspace.join("prompted").exists() {
        assert!(Instant::now() < deadline, "the reply never started");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    host.call("updateBot", json!({"id": bot, "modelOptions": {}})).await;
    let during = bot_state(&host.snapshot().await, &bot);
    assert_eq!((during["restartPending"].clone(), during["modelOptions"].clone()), (json!(true), json!({})));
    std::fs::remove_file(workspace.join("hold")).unwrap();
    std::fs::write(workspace.join("release"), "").unwrap();
    wait_for(&host, &bot, "the held reply", |e| e["kind"] == "agent" && e["data"]["text"] == "sol high · two").await;
    assert_eq!(answer(&host, &bot, "three").await, "sol low · three", "the agent's own again, in a new session");
    let log = calls(&host, &bot);
    assert_eq!(log.last().map(String::as_str), Some("session/new"), "{log:?}");
    assert!(!log.contains(&"session/load".to_owned()), "{log:?}");
}
