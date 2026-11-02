//! A real host driving scripted ACP agents: Stop on an agent that ignores it, room turns
//! that a newer group message replaced, a reaction as the whole reply, Fast mode across session resumes, and the
//! agent mode a bot works in.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

struct Host {
    child: Child,
    base: String,
    token: String,
    home: PathBuf,
}

impl Drop for Host {
    fn drop(&mut self) {
        common::stop(&mut self.child);
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

impl Host {
    async fn start() -> Self {
        let home = std::env::temp_dir().join(format!("codync-turns-e2e-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let (child, base) = common::serve(&home).await;
        let token = std::fs::read_to_string(home.join("token")).unwrap().trim().to_owned();
        Self { child, base, token, home }
    }

    async fn call(&self, method: &str, body: Value) -> Value {
        let res = reqwest::Client::new()
            .post(format!("{}/api/{method}", self.base))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = res.status();
        let v: Value = res.json().await.unwrap();
        assert!(status.is_success(), "{method} failed: {v}");
        v
    }

    /// A bot running `fixture` in its own folder (`home/<name>`).
    async fn bot(&self, name: &str, fixture: &str) -> String {
        let cwd = self.home.join(name);
        std::fs::create_dir_all(&cwd).unwrap();
        let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fixture);
        let bot = json!({
            "name": name, "backend": "custom", "command": common::python_agent(&agent),
            "cwd": cwd, "permission": "ask", "notify": false,
        });
        self.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned()
    }

    async fn entries(&self, chat: &str) -> Vec<Value> {
        self.call("history", json!({"botId": chat})).await["entries"].as_array().unwrap().clone()
    }

    /// Polls `chat`'s history until `pred` matches an entry.
    async fn wait_for(&self, chat: &str, within: Duration, pred: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + within;
        loop {
            if let Some(e) = self.entries(chat).await.into_iter().rev().find(|e| pred(e)) {
                return e;
            }
            assert!(Instant::now() < deadline, "timed out; history: {:?}", self.entries(chat).await);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Polls until every bot is idle.
    async fn wait_idle(&self) {
        let deadline = Instant::now() + Duration::from_mins(1);
        while common::snapshot(&self.base, &self.token).await["bots"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["status"] != "idle")
        {
            assert!(Instant::now() < deadline, "a turn didn't finish");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

fn python_missing() -> bool {
    let missing = Command::new(common::python()).arg("--version").output().is_err();
    if missing {
        eprintln!("skipping: Python not available");
    }
    missing
}

#[tokio::test]
async fn stop_ends_a_turn_whose_agent_ignores_the_cancel() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.bot("Stuck", "tests/fixtures/stubborn_agent.py").await;
    host.call("send", json!({"botId": bot, "text": "go"})).await;
    host.wait_for(&bot, Duration::from_mins(1), |e| e["data"]["text"] == "Working on it").await;
    host.call("stop", json!({"botId": bot})).await;
    // The agent never answers: after the grace period its process is stopped and the turn ends.
    host.wait_for(&bot, Duration::from_secs(40), |e| e["kind"] == "notice" && e["data"]["text"] == "Stopped.").await;
    let sync = common::snapshot(&host.base, &host.token).await;
    assert_eq!(common::bot(&sync, &bot)["status"], "idle");
}

#[tokio::test]
async fn a_member_busy_elsewhere_answers_only_the_latest_room_turn() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let member = host.bot("Member", "tests/fixtures/team_agent.py").await;
    let folder = host.home.join("Member");
    let group =
        host.call("createBot", json!({"name": "Room", "kind": "group", "members": [member]})).await["bot"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
    // Busy in its own chat until `release` exists.
    host.call("send", json!({"botId": member, "text": "BLOCK"})).await;
    let started = Instant::now();
    while !std::fs::read_to_string(folder.join("prompts.jsonl")).is_ok_and(|p| p.contains("BLOCK")) {
        assert!(started.elapsed() < Duration::from_mins(1), "the member never started");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    host.call("send", json!({"botId": group, "text": "first"})).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    host.call("send", json!({"botId": group, "text": "second"})).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    std::fs::write(folder.join("release"), "").unwrap();

    let reply =
        host.wait_for(&group, Duration::from_secs(30), |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    assert_eq!(reply["data"]["text"], "reply: group");
    tokio::time::sleep(Duration::from_secs(2)).await;
    // The room turn "second" replaced was skipped: no room prompt saw "first" without "second".
    let prompts = std::fs::read_to_string(folder.join("prompts.jsonl")).unwrap();
    let rooms: Vec<&str> = prompts.lines().filter(|p| p.contains("[Group chat")).collect();
    assert!(rooms.iter().any(|p| p.contains("User: second")), "{rooms:?}");
    assert!(!rooms.iter().any(|p| p.contains("User: first") && !p.contains("User: second")), "{rooms:?}");
    let finals = host.entries(&group).await.into_iter().filter(|e| e["kind"] == "agent" && e["data"]["final"] == true);
    assert_eq!(finals.count(), 1);
}

#[tokio::test]
async fn a_reaction_can_be_the_whole_reply() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.bot("Warm", "tests/fixtures/team_agent.py").await;
    let finals = |entries: Vec<Value>| {
        entries.into_iter().filter(|e| e["kind"] == "agent" && e["data"]["final"] == true).count()
    };

    // In its own chat: the heart on the user's message is the reply; what the agent wrote stays in the trace.
    let thanks = host.call("send", json!({"botId": bot, "text": "THANKS"})).await;
    let heart = json!([{"bot": bot, "emoji": "❤️"}]);
    host.wait_for(&bot, Duration::from_mins(1), |e| {
        e["id"] == thanks["entry"]["id"] && e["data"]["botReactions"] == heart
    })
    .await;
    host.wait_idle().await;
    let entries = host.entries(&bot).await;
    assert!(entries.iter().any(|e| e["data"]["text"] == "Reacted with a heart" && e["data"]["final"] == false));
    assert_eq!(finals(entries), 0);

    // Reactions stay rare: thanked again right away, it holds back, and still says nothing.
    let again = host.call("send", json!({"botId": bot, "text": "THANKS"})).await;
    host.wait_for(&bot, Duration::from_mins(1), |e| e["data"]["text"] == "Held back a reaction").await;
    host.wait_idle().await;
    let entries = host.entries(&bot).await;
    assert!(entries.iter().any(|e| e["id"] == again["entry"]["id"] && e["data"]["botReactions"].is_null()));
    assert_eq!(finals(entries), 0);

    // A message that needs neither words nor a reaction: the bot passes, and the user sees nothing.
    host.call("send", json!({"botId": bot, "text": "OK"})).await;
    host.wait_for(&bot, Duration::from_mins(1), |e| e["data"]["text"] == "(pass)").await;
    host.wait_idle().await;
    assert_eq!(finals(host.entries(&bot).await), 0);

    // In a room: the member reacts to the user's message and passes.
    let group = host.call("createBot", json!({"name": "Room", "kind": "group", "members": [bot]})).await["bot"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let asked = host.call("send", json!({"botId": group, "text": "REACT if you agree"})).await;
    let thumb = json!([{"bot": bot, "emoji": "👍"}]);
    host.wait_for(&group, Duration::from_mins(1), |e| {
        e["id"] == asked["entry"]["id"] && e["data"]["botReactions"] == thumb
    })
    .await;
    host.wait_idle().await;
    assert_eq!(finals(host.entries(&group).await), 0);
}

/// A final reply that starts with `speed` and ends with `word`, the user's message: the instructions before it in
/// the prompt may hold the word too.
fn speed_reply(speed: &'static str, word: &'static str) -> impl Fn(&Value) -> bool {
    move |e: &Value| {
        e["kind"] == "agent"
            && e["data"]["final"] == true
            && e["data"]["text"].as_str().is_some_and(|t| t.starts_with(speed) && t.trim_end().ends_with(word))
    }
}

const FAST_ON: &str = r#""configId": "fast", "value": "on""#;
const FAST_OFF: &str = r#""configId": "fast", "value": "off""#;

#[tokio::test]
async fn fast_mode_switches_live_sessions_without_a_restart() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.bot("Quick", "tests/fixtures/fast_mode_agent.py").await;
    let calls = host.home.join("Quick").join("calls.jsonl");
    let log = || std::fs::read_to_string(&calls).unwrap();

    host.call("updateBot", json!({"id": bot, "fastMode": true})).await;
    host.call("send", json!({"botId": bot, "text": "one"})).await;
    host.wait_for(&bot, Duration::from_mins(1), speed_reply("fast on", "one")).await;
    assert_eq!(log().matches(FAST_ON).count(), 1);
    let sync = common::snapshot(&host.base, &host.token).await;
    assert_eq!(common::bot(&sync, &bot)["fastModeSupport"], "offered");

    // Off and on again: the same agent process switches the live session before each turn.
    host.call("updateBot", json!({"id": bot, "fastMode": false})).await;
    host.call("send", json!({"botId": bot, "text": "two"})).await;
    host.wait_for(&bot, Duration::from_mins(1), speed_reply("fast off", "two")).await;
    host.call("updateBot", json!({"id": bot, "fastMode": true})).await;
    host.call("send", json!({"botId": bot, "text": "three"})).await;
    host.wait_for(&bot, Duration::from_mins(1), speed_reply("fast on", "three")).await;
    let log = log();
    assert!(!log.contains("session/load"), "{log}");
    assert_eq!((log.matches(FAST_ON).count(), log.matches(FAST_OFF).count()), (2, 1), "{log}");
}

#[tokio::test]
async fn an_agent_without_config_options_has_no_fast_mode() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.bot("Plain", "tests/fixtures/fast_mode_agent.py").await;
    std::fs::write(host.home.join("Plain").join("plain"), "").unwrap();
    let support =
        || async { common::bot(&common::snapshot(&host.base, &host.token).await, &bot)["fastModeSupport"].clone() };
    assert_eq!(support().await, "unknown", "clients offer /fast until a session says");

    host.call("send", json!({"botId": bot, "text": "one"})).await;
    host.wait_for(&bot, Duration::from_mins(1), speed_reply("fast off", "one")).await;
    assert_eq!(support().await, "unsupported");
}

#[tokio::test]
async fn a_refused_fast_mode_is_reported_and_not_asked_for_again() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.bot("Refused", "tests/fixtures/fast_mode_agent.py").await;
    let folder = host.home.join("Refused");
    std::fs::write(folder.join("refuse"), "").unwrap();
    let log = || std::fs::read_to_string(folder.join("calls.jsonl")).unwrap();

    host.call("updateBot", json!({"id": bot, "fastMode": true})).await;
    host.call("send", json!({"botId": bot, "text": "one"})).await;
    host.wait_for(&bot, Duration::from_mins(1), speed_reply("fast off", "one")).await;
    let sync = common::snapshot(&host.base, &host.token).await;
    let state = common::bot(&sync, &bot);
    assert_eq!(state["fastModeSupport"], "refused");
    assert_eq!(state["fastModeIssue"], "requires extra usage");
    assert_eq!(state["fastMode"], true, "the user's choice stays");

    host.call("send", json!({"botId": bot, "text": "two"})).await;
    host.wait_for(&bot, Duration::from_mins(1), speed_reply("fast off", "two")).await;
    assert_eq!(log().matches(FAST_ON).count(), 1, "asked again after a refusal");

    // Changing the setting asks again.
    host.call("updateBot", json!({"id": bot, "fastMode": false})).await;
    host.call("updateBot", json!({"id": bot, "fastMode": true})).await;
    host.call("send", json!({"botId": bot, "text": "three"})).await;
    host.wait_for(&bot, Duration::from_mins(1), speed_reply("fast off", "three")).await;
    assert_eq!(log().matches(FAST_ON).count(), 2);
}

#[tokio::test]
async fn a_new_bot_works_in_its_agents_auto_mode_until_the_user_picks_another() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let cwd = host.home.join("Moded");
    std::fs::create_dir_all(&cwd).unwrap();
    // The agent leaves the mode by itself during a turn, as Claude leaves Plan.
    std::fs::write(cwd.join("planless"), "").unwrap();
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mode_agent.py");
    let bot = json!({"name": "Moded", "backend": "custom", "command": common::python_agent(&agent), "cwd": cwd, "notify": false});
    let created = host.call("createBot", bot).await["bot"].clone();
    assert_eq!(
        (&created["permission"], &created["mode"]),
        (&json!("ask"), &json!("auto")),
        "the default: Auto, then ask"
    );
    let bot = created["id"].as_str().unwrap().to_owned();
    let log = || std::fs::read_to_string(cwd.join("calls.jsonl")).unwrap();

    // `auto` is the agent's Auto mode under its own id.
    host.call("send", json!({"botId": bot, "text": "one"})).await;
    host.wait_for(&bot, Duration::from_mins(1), speed_reply("mode agent ", "one")).await;
    // Leaving it stands for the rest of the session: the mode is set once per session.
    host.call("send", json!({"botId": bot, "text": "two"})).await;
    host.wait_for(&bot, Duration::from_mins(1), speed_reply("mode agent-full-access ", "two")).await;
    assert_eq!(log().matches(r#""configId": "mode""#).count(), 1, "{}", log());

    // The user's pick switches the live session before its next turn.
    host.call("updateBot", json!({"id": bot, "mode": "read-only"})).await;
    host.call("send", json!({"botId": bot, "text": "three"})).await;
    host.wait_for(&bot, Duration::from_mins(1), speed_reply("mode read-only ", "three")).await;
    // Left again (planless); without a mode of its own, the bot goes back to the one its session opened in.
    host.call("updateBot", json!({"id": bot, "permission": "ask", "mode": null})).await;
    host.call("send", json!({"botId": bot, "text": "four"})).await;
    host.wait_for(&bot, Duration::from_mins(1), speed_reply("mode read-only ", "four")).await;
    let log = log();
    assert!(!log.contains("session/load"), "{log}");
    assert_eq!(log.matches(r#""value": "read-only""#).count(), 2, "{log}");
}

#[tokio::test]
async fn a_mode_switch_leaves_fast_mode_to_its_own() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.bot("Both", "tests/fixtures/fast_mode_agent.py").await;
    let folder = host.home.join("Both");
    std::fs::write(folder.join("moded"), "").unwrap();
    host.call("updateBot", json!({"id": bot, "fastMode": true, "mode": "auto"})).await;
    host.call("send", json!({"botId": bot, "text": "one"})).await;
    host.wait_for(&bot, Duration::from_mins(1), speed_reply("fast on", "one")).await;
    let sync = common::snapshot(&host.base, &host.token).await;
    assert_eq!(common::bot(&sync, &bot)["fastModeSupport"], "offered", "the mode switch's answer isn't a refusal");
    let log = std::fs::read_to_string(folder.join("calls.jsonl")).unwrap();
    assert!(log.contains(r#""configId": "mode", "value": "agent""#) && log.contains(FAST_ON), "{log}");
}

#[tokio::test]
async fn a_bot_never_works_in_a_mode_its_agent_refused_to_switch_to() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.bot("Stuck", "tests/fixtures/mode_agent.py").await;
    let folder = host.home.join("Stuck");
    std::fs::write(folder.join("stuck"), "").unwrap();
    host.call("updateBot", json!({"id": bot, "mode": "auto"})).await;

    host.call("send", json!({"botId": bot, "text": "one"})).await;
    let refused = |e: &Value| e["data"]["text"].as_str().is_some_and(|t| t.contains("mode switch refused"));
    let notice = host.wait_for(&bot, Duration::from_mins(1), refused).await;
    assert_eq!(notice["kind"], "notice", "{notice}");
    // Once the agent switches, the next turn runs in the bot's mode.
    std::fs::remove_file(folder.join("stuck")).unwrap();
    host.call("send", json!({"botId": bot, "text": "two"})).await;
    host.wait_for(&bot, Duration::from_mins(1), speed_reply("mode agent ", "two")).await;
    let entries = host.entries(&bot).await;
    assert!(
        !entries
            .iter()
            .any(|e| e["kind"] == "agent" && e["data"]["text"].as_str().is_some_and(|t| t.contains("· one")))
    );
}

#[tokio::test]
async fn a_session_that_ended_in_the_agent_is_closed_and_loaded_again() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.bot("Ended", "tests/fixtures/ended_session_agent.py").await;
    host.call("send", json!({"botId": bot, "text": "hello"})).await;
    host.wait_for(&bot, Duration::from_mins(1), |e| e["kind"] == "agent" && e["data"]["text"] == "pong").await;
    host.call("send", json!({"botId": bot, "text": "end-this-session"})).await;
    host.wait_for(&bot, Duration::from_secs(30), |e| {
        e["kind"] == "notice" && e["data"]["text"].as_str().is_some_and(|t| t.contains("session has ended"))
    })
    .await;
    // Left open, the ended session would refuse this one too.
    host.call("send", json!({"botId": bot, "text": "still there?"})).await;
    host.wait_for(&bot, Duration::from_secs(30), |e| e["kind"] == "agent" && e["data"]["text"] == "still here").await;
}

#[tokio::test]
async fn reasoning_is_per_bot_and_applies_with_default_and_legacy_models() {
    if python_missing() {
        return;
    }
    let mut host = Host::start().await;
    let bot = host.bot("Reasoning", "tests/fixtures/reasoning_agent.py").await;
    let other = host.bot("Other", "tests/fixtures/reasoning_agent.py").await;
    host.call("updateBot", json!({"id": bot, "modelOptions": {"effort": "high"}})).await;
    host.call("send", json!({"botId": bot, "text": "one"})).await;
    host.wait_for(&bot, Duration::from_mins(1), speed_reply("default high", "one")).await;
    host.call("send", json!({"botId": other, "text": "other"})).await;
    host.wait_for(&other, Duration::from_mins(1), speed_reply("default medium", "other")).await;
    host.wait_idle().await;

    // The saved preference survives the host and agent process restarting.
    common::stop(&mut host.child);
    let (child, base) = common::serve(&host.home).await;
    host.child = child;
    host.base = base;
    host.call("send", json!({"botId": bot, "text": "resumed"})).await;
    host.wait_for(&bot, Duration::from_mins(1), speed_reply("default high", "resumed")).await;
    host.wait_idle().await;
    host.call("updateBot", json!({"id": bot, "model": "explicit", "modelOptions": {"effort": "low"}})).await;
    host.call("send", json!({"botId": bot, "text": "explicit"})).await;
    host.wait_for(&bot, Duration::from_mins(1), speed_reply("explicit low", "explicit")).await;
    host.wait_idle().await;
    host.call("updateBot", json!({"id": bot, "modelOptions": {}})).await;
    host.call("send", json!({"botId": bot, "text": "reset"})).await;
    host.wait_for(&bot, Duration::from_mins(1), speed_reply("explicit medium", "reset")).await;
}
