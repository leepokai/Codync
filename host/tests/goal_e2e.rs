//! Goal mode on a real host and a scripted agent: the host goes on with the goal after a turn that ended without
//! saying it is met, and stops at the goal's ends.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;

impl Host {
    async fn goal_bot(&self) -> String {
        let cwd = self.home.join("ada");
        std::fs::create_dir_all(&cwd).unwrap();
        let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
        self.call(
            "createBot",
            json!({
                "name": "Ada", "backend": "custom", "cwd": cwd, "permission": "ask", "notify": false,
                "command": common::python_agent(&agent),
            }),
        )
        .await["bot"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    /// The bot's prompts, as its agent was told them.
    fn goal_prompts(&self, bot: &str) -> Vec<String> {
        std::fs::read_to_string(self.home.join(bot).join("prompts.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    async fn until(&self, bot: &str, check: impl Fn(&Value) -> bool) -> Value {
        tokio::time::timeout(Duration::from_secs(25), async {
            loop {
                let history = self.call("history", json!({"botId": bot})).await;
                if check(&history) {
                    return history;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("the goal flow did not finish")
    }
}

fn notice<'a>(history: &'a Value, field: &str) -> Vec<&'a Value> {
    history["entries"].as_array().unwrap().iter().filter(|e| e["data"][field].is_string()).collect()
}

#[tokio::test]
async fn goal_continues_until_complete() {
    let host = Host::start().await;
    let bot = host.goal_bot().await;
    host.call("setGoal", json!({"botId": bot, "goal": "GOAL_DONE Ship notes", "maxTurns": 5})).await;
    let history = host.until(&bot, |h| !notice(h, "goalEnded").is_empty()).await;
    assert_eq!(notice(&history, "goalEnded")[0]["data"]["goalEnded"], "done");
    assert_eq!(host.goal_prompts("ada").len(), 2);
    assert_eq!(notice(&history, "goalStep").len(), 2);
    let snapshot = host.snapshot().await;
    let state = snapshot["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot).unwrap();
    assert_eq!(state["goal"]["stopped"], "done");
    assert_eq!(state["goal"]["turns"], 2);
}

#[tokio::test]
async fn restarting_the_host_marks_an_active_goal_stopped_without_resuming() {
    let mut host = Host::start().await;
    let bot = host.goal_bot().await;
    host.call("setGoal", json!({"botId": bot, "goal": "GOAL_BLOCK wait"})).await;
    tokio::time::timeout(Duration::from_secs(15), async {
        while host.goal_prompts("ada").is_empty() {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    host.restart(|_| {}).await;
    let snapshot = host.snapshot().await;
    let state = snapshot["bots"].as_array().unwrap().iter().find(|b| b["id"] == bot).unwrap();
    assert_eq!(state["goal"]["stopped"], "failed");
    assert_eq!(state["status"], "idle");
    assert_eq!(host.goal_prompts("ada").len(), 1);
}

#[tokio::test]
async fn asking_the_user_and_approval_cards_end_automatic_continuation() {
    let host = Host::start().await;
    let bot = host.goal_bot().await;
    host.call("setGoal", json!({"botId": bot, "goal": "GOAL_ASK publish"})).await;
    let history = host.until(&bot, |h| !notice(h, "goalEnded").is_empty()).await;
    assert_eq!(notice(&history, "goalEnded")[0]["data"]["goalEnded"], "needsUser");
    host.call("setGoal", json!({"botId": bot, "goal": "PERMISSION edit"})).await;
    let history = host.until(&bot, |h| notice(h, "goalEnded").len() == 2).await;
    assert_eq!(notice(&history, "goalEnded")[1]["data"]["goalEnded"], "needsUser");
    host.call("stop", json!({"botId": bot})).await;
}

#[tokio::test]
async fn decision_card_ends_automatic_continuation_after_answer() {
    let host = Host::start().await;
    let bot = host.goal_bot().await;
    host.call("setGoal", json!({"botId": bot, "goal": "GOAL_DECISION choose", "maxTurns": 4})).await;
    let history =
        host.until(&bot, |h| h["entries"].as_array().unwrap().iter().any(|e| e["data"]["decision"] == true)).await;
    let card = history["entries"].as_array().unwrap().iter().find(|e| e["data"]["decision"] == true).unwrap();
    assert_eq!(common::bot(&host.snapshot().await, &bot)["goal"]["stopped"], "needsUser");
    host.call("respondDecision", json!({"entryId": card["id"], "option": "Left"})).await;
    common::eventually("decision turn ends", Duration::from_secs(20), async || {
        common::bot(&host.snapshot().await, &bot)["status"] == "idle"
    })
    .await;
    assert_eq!(host.goal_prompts("ada").len(), 1);
}

#[tokio::test]
async fn turn_limit_stops_the_goal() {
    let host = Host::start().await;
    let bot = host.goal_bot().await;
    host.call("setGoal", json!({"botId": bot, "goal": "Keep going", "maxTurns": 2})).await;
    let history = host.until(&bot, |h| !notice(h, "goalEnded").is_empty()).await;
    assert_eq!(notice(&history, "goalEnded")[0]["data"]["goalEnded"], "limit");
    assert_eq!(host.goal_prompts("ada").len(), 2);
}

#[tokio::test]
async fn an_agent_error_stops_the_goal() {
    let host = Host::start().await;
    let bot = host.goal_bot().await;
    host.call("setGoal", json!({"botId": bot, "goal": "FAIL this step"})).await;
    let history = host.until(&bot, |h| !notice(h, "goalEnded").is_empty()).await;
    assert_eq!(notice(&history, "goalEnded")[0]["data"]["goalEnded"], "failed");
    assert_eq!(host.goal_prompts("ada").len(), 1);
}

#[tokio::test]
async fn a_completion_marker_after_send_message_still_stops() {
    let host = Host::start().await;
    let bot = host.goal_bot().await;
    host.call("setGoal", json!({"botId": bot, "goal": "GOAL_SEND Ship notes"})).await;
    let h = host.until(&bot, |h| !notice(h, "goalEnded").is_empty()).await;
    assert_eq!(notice(&h, "goalEnded")[0]["data"]["goalEnded"], "done");
    assert_eq!(host.goal_prompts("ada").len(), 1);
}

#[tokio::test]
async fn stopping_during_session_start_prevents_the_prompt() {
    let host = Host::start().await;
    let bot = host.goal_bot().await;
    std::fs::write(host.home.join("ada/slow-start"), "").unwrap();
    host.call("setGoal", json!({"botId": bot, "goal": "GOAL_BLOCK should not start"})).await;
    tokio::time::timeout(Duration::from_secs(15), async {
        while !host.home.join("ada/servers.json").exists() {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    host.call("stopGoal", json!({"botId": bot})).await;
    host.until(&bot, |h| !notice(h, "stopReason").is_empty()).await;
    assert_eq!(host.goal_prompts("ada").len(), 0);
}

#[tokio::test]
async fn stopping_a_running_goal_cancels_the_agent_turn() {
    let host = Host::start().await;
    let bot = host.goal_bot().await;
    host.call("setGoal", json!({"botId": bot, "goal": "GOAL_BLOCK wait"})).await;
    tokio::time::timeout(Duration::from_secs(15), async {
        while host.goal_prompts("ada").is_empty() {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    host.call("stopGoal", json!({"botId": bot})).await;
    let h = host.until(&bot, |h| !notice(h, "stopReason").is_empty()).await;
    assert_eq!(notice(&h, "stopReason")[0]["data"]["stopReason"], "cancelled");
    assert_eq!(notice(&h, "goalEnded")[0]["data"]["goalEnded"], "stopped");
    assert_eq!(host.goal_prompts("ada").len(), 1);
}

#[tokio::test]
async fn a_running_goal_is_cancelled_at_its_time_limit() {
    let host = Host::start().await;
    let bot = host.goal_bot().await;
    host.call("setGoal", json!({"botId": bot, "goal": "GOAL_BLOCK wait", "maxMinutes": 1})).await;
    tokio::time::timeout(Duration::from_secs(75), async {
        loop {
            let h = host.call("history", json!({"botId": bot})).await;
            if !notice(&h, "stopReason").is_empty() {
                assert_eq!(notice(&h, "stopReason")[0]["data"]["stopReason"], "cancelled");
                assert_eq!(notice(&h, "goalEnded")[0]["data"]["goalEnded"], "limit");
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(host.goal_prompts("ada").len(), 1);
}

#[tokio::test]
async fn a_user_message_does_not_disable_the_running_goal_step_deadline() {
    let host = Host::start().await;
    let bot = host.goal_bot().await;
    host.call("setGoal", json!({"botId": bot, "goal": "GOAL_BLOCK wait", "maxMinutes": 1})).await;
    common::eventually("goal prompt", Duration::from_secs(20), async || host.goal_prompts("ada").len() == 1).await;
    host.call("send", json!({"botId": bot, "text": "Only do this new request"})).await;
    common::eventually("goal stopped by the user", Duration::from_secs(20), async || {
        common::bot(&host.snapshot().await, &bot)["goal"]["stopped"] == "user"
    })
    .await;
    // Replacing or clearing the goal would cancel this generation independently of its deadline.
    // Keep the stopped goal in place and wait for the running step's original limit to expire.
    let started = std::time::Instant::now();
    common::eventually("interrupted goal step deadline", Duration::from_secs(75), async || {
        let h = host.call("history", json!({"botId": bot})).await;
        !notice(&h, "stopReason").is_empty()
            && h["entries"].as_array().unwrap().iter().any(|e| {
                e["kind"] == "agent"
                    && e["data"]["text"].as_str().is_some_and(|t| t.contains("Only do this new request"))
            })
    })
    .await;
    let user_turn = host.call("history", json!({"botId": bot})).await;
    assert!(
        started.elapsed() >= Duration::from_secs(55),
        "the interrupted step was cancelled before its original deadline"
    );
    assert_eq!(notice(&user_turn, "stopReason")[0]["data"]["stopReason"], "cancelled");
    let prompts = host.goal_prompts("ada");
    assert_eq!(
        prompts.iter().filter(|p| p.contains("Only do this new request")).count(),
        1,
        "the queued user message runs once"
    );
    assert_eq!(prompts.iter().filter(|p| p.contains("GOAL_BLOCK wait")).count(), 1);
    let history = host.call("history", json!({"botId": bot})).await;
    assert_eq!(notice(&history, "stopReason").len(), 1, "the expired step is cancelled once");
    assert_eq!(
        notice(&history, "goalEnded").iter().filter(|e| e["data"]["goalEnded"] == "user").count(),
        1,
        "the user stop is recorded once"
    );
}

#[tokio::test]
async fn user_message_invalidates_a_goal_queued_behind_an_existing_turn() {
    let host = Host::start().await;
    let bot = host.goal_bot().await;
    host.call("send", json!({"botId": bot, "text": "BLOCK existing work"})).await;
    tokio::time::timeout(Duration::from_secs(15), async {
        while host.goal_prompts("ada").is_empty() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    host.call("setGoal", json!({"botId": bot, "goal": "Must never run"})).await;
    host.call("send", json!({"botId": bot, "text": "I changed my mind"})).await;
    std::fs::write(host.home.join("ada/release"), "").unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        while host.goal_prompts("ada").len() < 2 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let prompts = host.goal_prompts("ada");
    assert_eq!(prompts.len(), 2);
    assert!(!prompts.iter().any(|p| p.contains("[Codync: goal step ")));
    let history = host.call("history", json!({"botId": bot})).await;
    assert_eq!(notice(&history, "goalEnded")[0]["data"]["goalEnded"], "user");
}

#[tokio::test]
async fn explicit_stop_invalidates_queued_goal() {
    let host = Host::start().await;
    let bot = host.goal_bot().await;
    host.call("send", json!({"botId": bot, "text": "BLOCK existing"})).await;
    tokio::time::timeout(Duration::from_secs(15), async {
        while host.goal_prompts("ada").is_empty() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    host.call("setGoal", json!({"botId": bot, "goal": "Must never run"})).await;
    host.call("stopGoal", json!({"botId": bot})).await;
    std::fs::write(host.home.join("ada/release"), "").unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(host.goal_prompts("ada").len(), 1);
}

#[tokio::test]
async fn reported_usage_limit_stops_before_starting_the_agent() {
    let host = Host::start().await;
    let bot = host
        .call(
            "createBot",
            json!({
                "name": "Budget", "backend": "claude", "command": "must-not-run-goal-fixture",
                "connectors": [], "notify": false,
            }),
        )
        .await["bot"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let response = reqwest::Client::new()
        .post(format!("{}/ingest/statusline", host.base))
        .bearer_auth(&host.token)
        .json(&json!({"rate_limits": {"five_hour": {"used_percentage": 80.0}}}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    host.call("setGoal", json!({"botId": bot, "goal": "Do not spend past the usage reserve"})).await;
    let history = host.until(&bot, |h| !notice(h, "goalEnded").is_empty()).await;
    assert_eq!(notice(&history, "goalEnded")[0]["data"]["goalEnded"], "usage");
    assert_eq!(notice(&history, "goalStep"), [] as [&Value; 0]);
    let snapshot = host.snapshot().await;
    assert_eq!(common::bot(&snapshot, &bot)["goal"]["turns"], 0);
    assert_eq!(common::bot(&snapshot, &bot)["status"], "idle");
}

#[tokio::test]
async fn a_goal_checks_the_agent_it_would_use_after_its_own_agent_recovers() {
    let host = Host::start().await;
    let cwd = host.home.join("ada");
    std::fs::create_dir_all(&cwd).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/relay_agent.py");
    let primary = format!("{} primary", common::python_agent(&fixture));
    let fallback = format!("{} standin", common::python_agent(&fixture));
    let bot = host
        .call(
            "createBot",
            json!({
                "name": "Budget", "backend": "custom", "command": primary, "cwd": cwd,
                "fallbacks": [{"backend": "claude", "command": fallback}],
                "connectors": [], "notify": false,
            }),
        )
        .await["bot"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    std::fs::write(cwd.join("primary.limit"), "").unwrap();
    host.call("send", json!({"botId": bot, "text": "Park on the fallback"})).await;
    common::eventually("the fallback answers", Duration::from_secs(20), async || {
        common::bot(&host.snapshot().await, &bot)["status"] == "idle"
            && common::bot(&host.snapshot().await, &bot)["agentStep"]["agent"] == "claude"
    })
    .await;

    let response = reqwest::Client::new()
        .post(format!("{}/ingest/statusline", host.base))
        .bearer_auth(&host.token)
        .json(&json!({"rate_limits": {"five_hour": {"used_percentage": 80.0}}}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());

    // The own agent is healthy again without first switching the idle bot back to it.
    let db = rusqlite::Connection::open(host.home.join("codync.db")).unwrap();
    let cleared = db.execute("DELETE FROM kv WHERE k LIKE 'rest.custom#%'", []).unwrap();
    assert_eq!(cleared, 1);
    drop(db);
    std::fs::remove_file(cwd.join("primary.limit")).unwrap();

    host.call("setGoal", json!({"botId": bot, "goal": "One resumed step", "maxTurns": 1})).await;
    let history = host.until(&bot, |h| !notice(h, "goalEnded").is_empty()).await;
    assert_eq!(notice(&history, "goalEnded")[0]["data"]["goalEnded"], "limit");
    assert_eq!(common::bot(&host.snapshot().await, &bot)["goal"]["turns"], 1);
    assert_eq!(
        common::bot(&host.snapshot().await, &bot)["agentStep"],
        Value::Null,
        "the goal runs on the recovered own agent"
    );

    let prompts = |name: &str| {
        std::fs::read_to_string(cwd.join(name))
            .unwrap_or_default()
            .lines()
            .filter(|line| line.contains("\"session/prompt\""))
            .count()
    };
    assert_eq!(prompts("primary.jsonl"), 2, "the parked turn and the goal use the own agent");
    assert_eq!(prompts("standin.jsonl"), 1, "the high-usage fallback gets no goal work");
}

#[tokio::test]
async fn replacing_a_running_goal_cancels_only_the_old_generation() {
    let host = Host::start().await;
    let bot = host.goal_bot().await;
    host.call("setGoal", json!({"botId": bot, "goal": "GOAL_BLOCK old work"})).await;
    common::eventually("old goal prompt", Duration::from_secs(20), async || host.goal_prompts("ada").len() == 1).await;
    host.call("setGoal", json!({"botId": bot, "goal": "GOAL_DONE replacement", "maxTurns": 4})).await;
    host.until(&bot, |h| notice(h, "goalEnded").iter().any(|e| e["data"]["goalEnded"] == "done")).await;
    let prompts = host.goal_prompts("ada");
    assert_eq!(prompts.len(), 3, "old work once, replacement twice");
    assert!(prompts[0].contains("GOAL_BLOCK old work"));
    assert!(prompts[1].contains("[Codync: goal step 1/4]"));
    assert!(prompts[2].contains("[Codync: goal step 2/4]"));
    let snapshot = host.snapshot().await;
    assert_eq!(common::bot(&snapshot, &bot)["goal"]["text"], "GOAL_DONE replacement");
    assert_eq!(common::bot(&snapshot, &bot)["goal"]["stopped"], "done");
}

#[tokio::test]
async fn clearing_a_running_goal_cancels_it_without_repeating_work() {
    let host = Host::start().await;
    let bot = host.goal_bot().await;
    host.call("setGoal", json!({"botId": bot, "goal": "GOAL_BLOCK old work"})).await;
    common::eventually("goal prompt", Duration::from_secs(20), async || host.goal_prompts("ada").len() == 1).await;
    host.call("setGoal", json!({"botId": bot, "goal": ""})).await;
    host.until(&bot, |h| !notice(h, "stopReason").is_empty()).await;
    let snapshot = host.snapshot().await;
    assert!(common::bot(&snapshot, &bot)["goal"].is_null());
    assert_eq!(host.goal_prompts("ada").len(), 1);
}

#[tokio::test]
async fn user_message_during_a_goal_runs_once_and_prevents_continuation() {
    let host = Host::start().await;
    let bot = host.goal_bot().await;
    host.call("setGoal", json!({"botId": bot, "goal": "GOAL_BLOCK old work"})).await;
    common::eventually("goal prompt", Duration::from_secs(20), async || host.goal_prompts("ada").len() == 1).await;
    host.call("send", json!({"botId": bot, "text": "Only do this new request"})).await;
    std::fs::write(host.home.join("ada/release"), "").unwrap();
    host.until(&bot, |h| {
        h["entries"].as_array().unwrap().iter().any(|e| {
            e["kind"] == "agent" && e["data"]["text"].as_str().is_some_and(|t| t.contains("Only do this new request"))
        })
    })
    .await;
    let snapshot = host.snapshot().await;
    assert_eq!(common::bot(&snapshot, &bot)["goal"]["stopped"], "user");
    let prompts = host.goal_prompts("ada");
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1].contains("Only do this new request"));
    assert!(!prompts[1].contains("[Codync: goal step "));
}

#[tokio::test]
async fn a_queued_goal_expires_without_cancelling_the_existing_user_turn() {
    let host = Host::start().await;
    let bot = host.goal_bot().await;
    host.call("send", json!({"botId": bot, "text": "BLOCK existing user work"})).await;
    common::eventually("user prompt", Duration::from_secs(20), async || host.goal_prompts("ada").len() == 1).await;
    host.call("setGoal", json!({"botId": bot, "goal": "Must never run", "maxMinutes": 1})).await;
    common::eventually("queued goal deadline", Duration::from_secs(75), async || {
        let snapshot = host.snapshot().await;
        common::bot(&snapshot, &bot)["goal"]["stopped"] == "limit"
    })
    .await;
    let snapshot = host.snapshot().await;
    assert_eq!(common::bot(&snapshot, &bot)["status"], "working");
    assert_eq!(common::bot(&snapshot, &bot)["goal"]["turns"], 0);
    assert_eq!(host.goal_prompts("ada").len(), 1);
    host.call("stop", json!({"botId": bot})).await;
}

#[tokio::test]
async fn an_approval_stops_continuation_even_when_saving_the_stop_fails() {
    let host = Host::start().await;
    let bot = host.goal_bot().await;
    host.call("setGoal", json!({"botId": bot, "goal": "WAIT_PERMISSION edit", "maxTurns": 3})).await;
    common::eventually("goal prompt", Duration::from_secs(20), async || host.goal_prompts("ada").len() == 1).await;
    let db = rusqlite::Connection::open(host.home.join("codync.db")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_goal_update BEFORE UPDATE ON kv WHEN OLD.k LIKE 'goal.%' BEGIN SELECT RAISE(FAIL, 'goal write refused'); END;").unwrap();
    std::fs::write(host.home.join("ada/permission-ready"), "ready").unwrap();
    let history =
        host.until(&bot, |h| h["entries"].as_array().unwrap().iter().any(|e| e["kind"] == "permission")).await;
    let card = history["entries"].as_array().unwrap().iter().find(|e| e["kind"] == "permission").unwrap();
    assert_eq!(notice(&history, "goalEnded"), [] as [&Value; 0], "no false claim that the stop was saved");
    // Writes recover before the approved turn ends: the stale persisted goal must not authorize more work.
    db.execute_batch("DROP TRIGGER reject_goal_update;").unwrap();
    host.call("respondPermission", json!({"entryId": card["id"], "optionId": "allow"})).await;
    common::eventually("approved goal turn ends", Duration::from_secs(20), async || {
        common::bot(&host.snapshot().await, &bot)["status"] == "idle"
    })
    .await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(host.goal_prompts("ada").len(), 1);
    assert_eq!(common::bot(&host.snapshot().await, &bot)["goal"]["stopped"], "needsUser");
}

#[tokio::test]
async fn clearing_or_replacing_a_goal_waiting_for_approval_cancels_its_old_step() {
    for replacement in ["", "GOAL_DONE replacement"] {
        let host = Host::start().await;
        let bot = host.goal_bot().await;
        host.call("setGoal", json!({"botId": bot, "goal": "PERMISSION BLOCK old work"})).await;
        host.until(&bot, |h| h["entries"].as_array().unwrap().iter().any(|e| e["kind"] == "permission")).await;
        host.call("setGoal", json!({"botId": bot, "goal": replacement, "maxTurns": 4})).await;
        common::eventually("old approval turn and replacement end", Duration::from_secs(20), async || {
            let snapshot = host.snapshot().await;
            let current = common::bot(&snapshot, &bot);
            current["status"] == "idle"
                && (if replacement.is_empty() {
                    current["goal"].is_null()
                } else {
                    current["goal"]["stopped"] == "done"
                })
        })
        .await;
        assert_eq!(host.goal_prompts("ada").len(), if replacement.is_empty() { 1 } else { 3 });
    }
}

#[tokio::test]
async fn an_approved_goal_error_does_not_replay_the_turn_on_a_fallback() {
    let host = Host::start().await;
    let bot = host.goal_bot().await;
    let fallback = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/relay_agent.py");
    let command = format!("{} standin", common::python_agent(&fallback));
    host.call("updateBot", json!({"id": bot, "fallbacks": [{"backend": "codex", "command": command}]})).await;
    host.call("setGoal", json!({"botId": bot, "goal": "PERMISSION_RATE_LIMIT edit"})).await;
    let history =
        host.until(&bot, |h| h["entries"].as_array().unwrap().iter().any(|e| e["kind"] == "permission")).await;
    let card = history["entries"].as_array().unwrap().iter().find(|e| e["kind"] == "permission").unwrap();
    host.call("respondPermission", json!({"entryId": card["id"], "optionId": "allow"})).await;
    common::eventually("failed goal turn ends", Duration::from_secs(20), async || {
        common::bot(&host.snapshot().await, &bot)["status"] == "error"
    })
    .await;
    let history = host.call("history", json!({"botId": bot})).await;
    assert!(!history["entries"].as_array().unwrap().iter().any(|e| e["data"]["agentSwitch"].is_object()));
    assert!(!host.home.join("ada/standin.jsonl").exists(), "the fallback must not even start");
    assert_eq!(host.goal_prompts("ada").len(), 1);
    assert_eq!(common::bot(&host.snapshot().await, &bot)["goal"]["stopped"], "needsUser");
}

#[tokio::test]
async fn failed_goal_persistence_does_not_acknowledge_stop_or_accept_a_new_message() {
    let host = Host::start().await;
    let bot = host.goal_bot().await;
    host.call("setGoal", json!({"botId": bot, "goal": "GOAL_BLOCK working"})).await;
    common::eventually("goal prompt", Duration::from_secs(20), async || host.goal_prompts("ada").len() == 1).await;
    let db = rusqlite::Connection::open(host.home.join("codync.db")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_goal_update BEFORE UPDATE ON kv WHEN OLD.k LIKE 'goal.%' BEGIN SELECT RAISE(FAIL, 'goal write refused'); END;").unwrap();
    assert!(host.refused("stopGoal", json!({"botId": bot})).await.contains("goal write refused"));
    assert!(host.refused("send", json!({"botId": bot, "text": "must not queue"})).await.contains("goal write refused"));
    let history = host.call("history", json!({"botId": bot})).await;
    assert_eq!(notice(&history, "goalEnded"), [] as [&Value; 0]);
    assert!(!history["entries"].as_array().unwrap().iter().any(|e| e["data"]["text"] == "must not queue"));
    db.execute_batch("DROP TRIGGER reject_goal_update;").unwrap();
    host.call("stopGoal", json!({"botId": bot})).await;
    host.until(&bot, |h| !notice(h, "stopReason").is_empty()).await;
    assert_eq!(host.goal_prompts("ada").len(), 1);
}
