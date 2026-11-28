use super::{Host, entry, result, row, task, task_prompts, tell, wait};
use serde_json::{Value, json};

const TITLE: &str = "Ship it";
const SKILL: &str = "- B: Release steps.";

async fn setup_task(host: &Host) -> (String, Value, Value) {
    let bot = host.fixture_bot("Mika", "work_agent.py", "standard defer-system track-skills").await;
    host.reply(&bot, "ADD Ship it|Finish the release").await;
    let done = task(host, &bot, TITLE, "done").await;
    let [opening] = task_prompts(host, &bot, TITLE).try_into().unwrap();
    assert!(!text(&opening).contains(SKILL), "{opening}");
    (bot, done, opening)
}

async fn enable_skill(host: &Host, bot: &str) {
    host.call(
        "chatCall",
        json!({"botId": bot, "name": "save_skill",
            "arguments": {"name": "B", "description": "Release steps.", "instructions": "1. Build."}}),
    )
    .await;
}

async fn disable_skills(host: &Host, bot: &str) {
    assert_eq!(host.call("updateBot", json!({"id": bot, "skills": []})).await["bot"]["skills"], json!([]));
}

fn text(prompt: &Value) -> &str {
    prompt["text"].as_str().unwrap()
}

async fn prompt(host: &Host, bot: &str, words: &str) -> Value {
    wait(words, async || host.log(bot, "prompts.jsonl").into_iter().find(|p| text(p).contains(words))).await
}

#[tokio::test]
async fn a_task_retries_the_skill_update_after_refusal() {
    let host = Host::start().await;
    let (bot, done, opening) = setup_task(&host).await;
    enable_skill(&host, &bot).await;
    let refused = "read B RETURN_REFUSAL";
    assert_eq!(tell(&host, &bot, &done, refused).await["reopened"], true);
    let received = prompt(&host, &bot, refused).await;
    assert_eq!(received["session"], opening["session"]);
    assert!(text(&received).contains("<skills_update>") && text(&received).contains(SKILL), "{received}");

    let failed = task(&host, &bot, TITLE, "failed").await;
    let [outcome] = host.log(&bot, "prompt_outcomes.jsonl").try_into().unwrap();
    assert_eq!(outcome, json!({"session": opening["session"], "stopReason": "refusal", "cancelReceived": false}));

    let retry = "retry after refusal";
    assert_eq!(tell(&host, &bot, &failed, retry).await["reopened"], true);
    let finished = result(&host, &bot, TITLE, &format!("Thread {retry}:")).await;
    let retried = prompt(&host, &bot, retry).await;
    assert_eq!(retried["session"], opening["session"]);
    let again = "check after successful retry";
    assert_eq!(tell(&host, &bot, &finished, again).await["reopened"], true);
    result(&host, &bot, TITLE, &format!("Thread {again}:")).await;
    let checked = prompt(&host, &bot, again).await;
    assert_eq!(checked["session"], opening["session"]);
    let counts = [&received, &retried, &checked].map(|p| text(p).matches("<skills_update>").count());
    eprintln!("task refusal: {outcome}, skill updates {counts:?}");
    assert_eq!(counts, [1, 1, 0]);
    assert!(text(&retried).contains(SKILL), "{retried}");
}

#[tokio::test]
async fn a_chat_revokes_a_skill_seen_before_stop_when_its_agent_returns_end_turn() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "standard").await;
    let hello = "open the chat for the skill test";
    host.reply(&bot, hello).await;
    let opening = prompt(&host, &bot, hello).await;
    enable_skill(&host, &bot).await;

    let stopped = "STOP_CHAT_WITH_END_TURN";
    host.call("send", json!({"botId": bot, "text": stopped})).await;
    let received = prompt(&host, &bot, stopped).await;
    assert_eq!(received["session"], opening["session"]);
    assert!(text(&received).contains("<agent_profile_update>") && text(&received).contains(SKILL), "{received}");
    host.call("stop", json!({"botId": bot})).await;
    entry(&host, &bot, "the chat stop notice", |e| e["data"]["stopReason"] == "cancelled").await;
    let [outcome] = host.log(&bot, "prompt_outcomes.jsonl").try_into().unwrap();
    assert_eq!(outcome, json!({"session": opening["session"], "stopReason": "end_turn", "cancelReceived": true}));
    wait("the stopped chat to be idle", async || (row(&host, &bot).await["status"] == "idle").then_some(())).await;

    disable_skills(&host, &bot).await;
    let retry = "continue the chat without B";
    host.reply(&bot, retry).await;
    let retried = prompt(&host, &bot, retry).await;
    assert_eq!(retried["session"], opening["session"]);
    let again = "check the chat after successful retry";
    host.reply(&bot, again).await;
    let checked = prompt(&host, &bot, again).await;
    assert_eq!(checked["session"], opening["session"]);
    let counts = [&received, &retried, &checked].map(|p| text(p).matches("<agent_profile_update>").count());
    eprintln!("chat stop: {outcome}, profile updates {counts:?}");
    assert_eq!(counts, [1, 1, 0]);
    assert!(text(&retried).contains("\nSkills: none.\n"), "{retried}");
}

#[tokio::test]
async fn a_standard_chat_resends_a_skill_revocation_dropped_by_compaction() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Mika", "work_agent.py", "standard report-compaction track-skills").await;
    enable_skill(&host, &bot).await;
    let hello = "open the chat with B";
    host.reply(&bot, hello).await;
    let opening = prompt(&host, &bot, hello).await;
    let session = &opening["session"];
    let [created] = host.log(&bot, "standard_sessions.jsonl").try_into().unwrap();
    assert_eq!(created["method"], "session/new");
    assert_eq!(&created["session"], session);
    assert!(created["system"].as_str().unwrap().contains(SKILL), "{created}");

    disable_skills(&host, &bot).await;
    let compact = "revoke B and COMPACT_AGAIN";
    host.reply(&bot, compact).await;
    let received = prompt(&host, &bot, compact).await;
    let notice = entry(&host, &bot, "the agent's compaction notice", |e| e["data"]["compaction"].is_object()).await;
    assert_eq!(notice["data"]["compaction"]["byHost"], false);
    let [outcome] = host.log(&bot, "prompt_outcomes.jsonl").try_into().unwrap();
    assert_eq!(outcome, json!({"session": session, "stopReason": "end_turn", "cancelReceived": false}));
    let [dropped] = host.log(&bot, "compacted_skills.jsonl").try_into().unwrap();
    assert_eq!(&dropped["session"], session);
    assert!(dropped["dropped"].as_str().unwrap().contains("\nSkills: none.\n"), "{dropped}");
    let overlay = host.workspace(&bot).join(format!("fixture-skills-{}", session.as_str().unwrap()));
    assert!(!overlay.exists(), "compaction discarded the revocation");

    let retry = "continue the compacted chat";
    host.reply(&bot, retry).await;
    let retried = prompt(&host, &bot, retry).await;
    let again = "check the chat after compaction recovery";
    host.reply(&bot, again).await;
    let checked = prompt(&host, &bot, again).await;
    assert_eq!(host.log(&bot, "standard_sessions.jsonl"), [created]);
    let counts = [&received, &retried, &checked].map(|p| {
        assert_eq!(&p["session"], session);
        assert_eq!(p["pid"], opening["pid"]);
        text(p).matches("<agent_profile_update>").count()
    });
    eprintln!(
        "standard chat {session} kept B in its system prompt and dropped the Skills: none. update before {outcome}, profile updates {counts:?}"
    );
    assert_eq!(counts, [1, 1, 0], "repeat the revocation once after its turn compacted");
    assert!(text(&retried).contains("\nSkills: none.\n"), "{retried}");
    assert!(std::fs::read_to_string(overlay).unwrap().contains("\nSkills: none.\n"));
}

#[tokio::test]
async fn a_task_revokes_an_unacknowledged_skill_after_a_host_crash() {
    let mut host = Host::start().await;
    let (bot, done, opening) = setup_task(&host).await;
    let session = &opening["session"];
    enable_skill(&host, &bot).await;
    let interrupted = "read B HOLD:crash_delivery";
    assert_eq!(tell(&host, &bot, &done, interrupted).await["reopened"], true);
    let received = prompt(&host, &bot, interrupted).await;
    assert_eq!(&received["session"], session);
    assert!(text(&received).contains("<skills_update>") && text(&received).contains(SKILL), "{received}");
    let overlay = host.workspace(&bot).join(format!("fixture-skills-{}", session.as_str().unwrap()));
    wait("the fixture to apply B", async || {
        std::fs::read_to_string(&overlay).ok().filter(|skills| skills.contains(SKILL))
    })
    .await;
    disable_skills(&host, &bot).await;
    let working = task(&host, &bot, TITLE, "working").await;
    assert_eq!(working["thread"], done["thread"]);
    assert!(!host.workspace(&bot).join("release-crash_delivery").exists());
    host.crash();
    host.restart(|_| {}).await;

    let resumed = prompt(&host, &bot, "Codync restarted on this computer").await;
    assert_eq!(&resumed["session"], session);
    assert_ne!(resumed["pid"], received["pid"]);
    assert!(host.log(&bot, "standard_sessions.jsonl").iter().any(|event| {
        event["method"] == "session/load" && &event["session"] == session && event["pid"] == resumed["pid"]
    }));
    let finished = task(&host, &bot, TITLE, "done").await;
    assert_eq!(finished["thread"], done["thread"]);
    let again = "check after the host restart";
    assert_eq!(tell(&host, &bot, &finished, again).await["reopened"], true);
    result(&host, &bot, TITLE, &format!("Thread {again}:")).await;
    let checked = prompt(&host, &bot, again).await;
    assert_eq!(&checked["session"], session);
    let counts = [&received, &resumed, &checked].map(|p| text(p).matches("<skills_update>").count());
    eprintln!("task crash: reapplied session {session} on a new fixture process, skill updates {counts:?}");
    assert_eq!(counts, [1, 1, 0]);
    assert!(text(&resumed).contains("\nSkills: none.\n</skills_update>"), "{resumed}");
    assert!(std::fs::read_to_string(overlay).unwrap().ends_with("\nSkills: none."));
}
