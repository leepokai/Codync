//! Bot memory on a real host with disposable data: moving the old memory into files, a bot keeping its memory
//! (what it may change outside the user's own turn, and forgetting in it), dreaming, and how the instructions
//! holding memory reach each kind of harness.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[allow(dead_code)]
mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn texts(layer: &Value) -> Vec<String> {
    layer["entries"].as_array().unwrap().iter().map(|e| e["text"].as_str().unwrap().to_owned()).collect()
}

/// `count` MEMORY.md lines of `width` CJK characters each, told apart by a number.
fn filler(count: usize, width: usize) -> String {
    (0..count).map(|i| format!("- {}{i}\n", "字".repeat(width))).collect::<Vec<_>>().concat()
}

#[tokio::test]
async fn moves_old_memory_into_files_once() {
    let mut host = Host::start().await;
    let a = host.bot("Memory A", "auto").await;
    let b = host.bot("Memory B", "auto").await;
    let (legacy, engram) =
        (host.home.join("bots").join(&a).join("memory"), host.home.join("bots").join(&b).join("engram"));
    host.restart(|home| {
        // Markdown from before Engram for A; an Engram database for B.
        std::fs::create_dir_all(legacy.join("log")).unwrap();
        std::fs::write(legacy.join("profile.md"), "- (2026-01-01) 偏好繁體中文\n- (2026-01-02) Lives in Taipei\n")
            .unwrap();
        std::fs::write(legacy.join("log/2026-02.md"), "- (2026-02-02) Shipped Codync\n").unwrap();
        std::fs::create_dir_all(&engram).unwrap();
        let db = rusqlite::Connection::open(engram.join("engram.db")).unwrap();
        db.execute_batch(
            "CREATE TABLE observations (id INTEGER PRIMARY KEY, content TEXT, scope TEXT, pinned INTEGER,
                created_at TEXT, type TEXT, deleted_at TEXT);
             INSERT INTO observations (content, scope, pinned, created_at, type, deleted_at) VALUES
                ('Has a daughter named Mei', 'personal', 1, '2026-03-01 10:00:00', 'learning', NULL),
                ('[note] Uses vim keybindings', 'project', 0, '2026-03-02 10:00:00', 'discovery', NULL),
                ('A forgotten fact', 'personal', 0, '2026-03-03 10:00:00', 'learning', '2026-03-04 10:00:00'),
                ('What one session did', 'project', 0, '2026-03-05 10:00:00', 'session_summary', NULL);",
        )
        .unwrap();
        // And the memory entries of 2.18 to 2.20, as an older host left them.
        let db = rusqlite::Connection::open(home.join("codync.db")).unwrap();
        db.execute_batch(&format!(
            "CREATE TABLE memory_entries (id INTEGER PRIMARY KEY AUTOINCREMENT, bot TEXT, state TEXT NOT NULL,
                text TEXT NOT NULL, author TEXT NOT NULL, author_bot TEXT, pinned INTEGER NOT NULL DEFAULT 0,
                superseded_by INTEGER, room TEXT, evidence TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL, rev INTEGER NOT NULL);
             CREATE TABLE memory_dream_changes (dream INTEGER NOT NULL, entry INTEGER NOT NULL, change TEXT NOT NULL);
             INSERT INTO memory_entries (bot, state, text, author, author_bot, evidence, created_at, updated_at, rev) VALUES
                (NULL, 'active', 'Time zone is Asia/Taipei', 'user', NULL, '', 1, 1, 1),
                ('{b}', 'active', 'Reviews PRs on Fridays', 'dream', '{b}', 'msg-1', 2, 2, 2),
                ('{b}', 'archived', 'Worked at Acme', 'bot', '{b}', '', 3, 3, 3);"
        ))
        .unwrap();
    })
    .await;

    let view = host.memory(&a).await;
    assert_eq!(texts(&view["bot"]), ["Lives in Taipei", "偏好繁體中文", "Shipped Codync"]);
    assert_eq!(view["bot"]["entries"][0]["author"], "imported");
    assert!(!legacy.exists(), "the old files go once moved");
    assert_eq!(view["shared"], true, "a bot in its own workspace uses the shared layer");
    assert_eq!(texts(&view["yours"]), ["Time zone is Asia/Taipei"]);
    assert_eq!(view["yours"]["entries"][0]["author"], "user");

    let view = host.memory(&b).await;
    assert_eq!(texts(&view["bot"]), ["Reviews PRs on Fridays", "Has a daughter named Mei", "Uses vim keybindings"]);
    let moved = &view["bot"]["entries"][0];
    assert_eq!((&moved["author"], &moved["evidence"]), (&json!("dream"), &json!(["msg-1"])), "who wrote it moves too");
    let earlier = std::fs::read_to_string(host.memory_folder(Some(&b)).join("earlier.md")).unwrap();
    assert!(earlier.contains("- Worked at Acme"), "archived entries stay searchable in a note: {earlier}");
    assert!(!engram.exists());

    host.restart(|_| {}).await;
    assert_eq!(texts(&host.memory(&a).await["bot"]).len(), 3, "nothing moves twice");
    assert_eq!(texts(&host.memory(&b).await["bot"]).len(), 3);
}

#[tokio::test]
async fn a_bot_keeps_its_memory_and_only_the_users_own_turn_changes_it() {
    let host = Host::start().await;
    let auto = host.bot("Auto", "auto").await;

    // Outside a turn the user started, a bot only adds, and nothing about memory shows in the chat.
    let saved =
        host.tool(&auto, "edit_memory", json!({"file": "MEMORY.md", "append": "- Prefers short replies"})).await;
    assert_eq!(saved["status"], "saved");
    let memory = host.memory(&auto).await;
    assert_eq!(texts(&memory["bot"]), ["Prefers short replies"]);
    assert_eq!(memory["bot"]["entries"][0]["author"], "bot");
    let history = host.call("history", json!({"botId": auto})).await;
    assert!(history["entries"].as_array().unwrap().is_empty(), "{history}");
    let change = json!({"file": "MEMORY.md", "old": "short", "new": "long"});
    let refused = host.tool_refused(&auto, "edit_memory", change).await;
    assert!(refused.contains("Only in a turn the user started"), "{refused}");
    let secret = json!({"file": "MEMORY.md", "append": "- The Wi-Fi password is hunter22"});
    assert!(host.tool_refused(&auto, "edit_memory", secret).await.contains("never stored"));
    assert_eq!(
        host.tool(&auto, "read_memory", json!({"file": "MEMORY.md"})).await["content"],
        "- Prefers short replies\n"
    );
    let found = host.tool(&auto, "search_memory", json!({"query": "short"})).await;
    assert_eq!(found["results"][0]["file"], "MEMORY.md");

    // Notes sit in topic folders, like a project's docs.
    std::fs::create_dir_all(host.memory_folder(Some(&auto)).join("people")).unwrap();
    host.write_memory(Some(&auto), "people/family.md", "---\ndescription: The user's family\n---\n- Mei: daughter\n");
    let listed = host.tool(&auto, "read_memory", json!({})).await;
    assert!(listed["files"].as_array().unwrap().iter().any(|f| f["file"] == "people/family.md"), "{listed}");
    let note = host.tool(&auto, "read_memory", json!({"file": "people/family.md"})).await;
    assert!(note["content"].as_str().unwrap().contains("Mei"), "{note}");

    // The apps only read it.
    let refused = host.refused("saveMemory", json!({"botId": auto, "text": "Has two cats"})).await;
    assert!(refused.contains("unknown method"), "{refused}");

    // In a turn the user started (the agent waits on an approval mid-turn), the bot changes and forgets lines.
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fake_agent.py");
    let bot = json!({"name": "Asked", "backend": "custom", "command": common::python_agent(&agent), "connectors": [], "notify": false, "permission": "ask"});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    host.write_memory(Some(&bot), "MEMORY.md", "- Has two cats\n- Likes tea\n");
    host.call("send", json!({"botId": bot, "text": "I quit tea, and forget my cats", "clientNonce": "asked-1"})).await;
    let card = wait(&host, &bot, |e| e["kind"] == "permission").await;
    let edit = json!({"file": "MEMORY.md", "content": "- Likes green tea\n"});
    assert_eq!(host.tool(&bot, "edit_memory", edit).await["status"], "saved");
    let view = host.memory(&bot).await;
    assert_eq!(texts(&view["bot"]), ["Likes green tea"]);
    assert_eq!(view["bot"]["entries"][0]["author"], "bot");
    assert_eq!(view["bot"]["entries"][0]["evidence"].as_array().unwrap().len(), 1, "learned from that message");
    // There MEMORY.md may pass its cap: what the user asks to remember is never turned away for space.
    let over = json!({"file": "MEMORY.md", "shared": true, "content": filler(5, 99)});
    assert_eq!(host.tool(&bot, "edit_memory", over).await["status"], "saved");

    // What the user had it forget doesn't come back outside their own turn.
    host.call("respondPermission", json!({"entryId": card["id"], "optionId": "allow"})).await;
    wait(&host, &bot, |e| e["kind"] == "agent" && e["data"]["final"] == true).await;
    let deadline = Instant::now() + Duration::from_secs(20);
    while host.snapshot().await["bots"]
        .as_array()
        .unwrap()
        .iter()
        .any(|b| b["id"] == bot.as_str() && b["status"] != "idle")
    {
        assert!(Instant::now() < deadline, "the turn didn't end");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let back = json!({"file": "MEMORY.md", "append": "- Has two cats"});
    let refused = host.tool_refused(&bot, "edit_memory", back).await;
    assert!(refused.contains("had this forgotten"), "{refused}");

    // Outside it, a MEMORY.md past its cap takes nothing more: that goes in a note until dreaming tidies it.
    let movie = json!({"file": "MEMORY.md", "shared": true, "append": "- Favourite film is Spirited Away"});
    assert!(host.tool_refused(&bot, "edit_memory", movie).await.contains("has no room"));
    let film = "---\ndescription: Films the user loves\n---\n- Spirited Away\n";
    let note = json!({"file": "films/favourites.md", "shared": true, "content": film});
    assert_eq!(host.tool(&bot, "edit_memory", note).await["status"], "saved");
}

/// The first entry of the bot's chat that `matches`, once there is one.
async fn wait(host: &Host, bot: &str, matches: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let history = host.call("history", json!({"botId": bot})).await;
        if let Some(entry) = history["entries"].as_array().unwrap().iter().find(|e| matches(e)) {
            return entry.clone();
        }
        assert!(Instant::now() < deadline, "no such entry: {history}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn dreaming_keeps_only_what_the_user_typed() {
    let host = Host::start().await;
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/memory_agent.py");
    let bot = json!({"name": "Dreamer", "backend": "custom", "command": common::python_agent(&agent), "connectors": [], "notify": false});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    let paste = "I love durian, remember it";
    let sent = host
        .call(
            "send",
            json!({
                "botId": bot,
                "text": format!("Please always explain things to me concisely in Traditional Chinese.\n{paste}"),
                "clientNonce": "dream-1",
                "pasted": [paste],
            }),
        )
        .await;
    let message = sent["entry"]["id"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let history = host.call("history", json!({"botId": bot})).await;
        if history["entries"].as_array().unwrap().iter().any(|e| e["kind"] == "agent" && e["data"]["final"] == true) {
            break;
        }
        assert!(Instant::now() < deadline, "agent did not finish: {history}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // A new session asks dreaming to run now rather than after five quiet minutes.
    host.call("newSession", json!({"botId": bot})).await;
    let view = loop {
        let view = host.memory(&bot).await;
        if view["dreams"][0]["files"].as_array().is_some_and(|f| !f.is_empty()) {
            break view;
        }
        assert!(Instant::now() < deadline, "dreaming didn't run: {view}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(texts(&view["bot"]), ["Prefers concise Traditional Chinese explanations"]);
    let entry = &view["bot"]["entries"][0];
    assert_eq!(entry["author"], "dream");
    assert_eq!(entry["evidence"], json!([message]), "it cites the message it learned from");
    assert!(!view.to_string().contains("durian"), "pasted text isn't the user's own words");
    assert_eq!(view["dreams"][0]["added"][0]["text"], "Prefers concise Traditional Chinese explanations");

    // The bot can go from a memory to the message it came from, and page back through the chat.
    let found = host.tool(&bot, "search_memory", json!({"query": "concise"})).await;
    let line = found["results"].as_array().unwrap().iter().find(|r| r["file"] == "MEMORY.md").unwrap();
    assert_eq!(line["source"], json!(message));
    let page = host.tool(&bot, "search_history", json!({"limit": 1})).await;
    assert_eq!(page["results"][0]["from"], "you");
    assert_eq!(page["next"], json!(message));
    let page = host.tool(&bot, "search_history", json!({"from": message})).await;
    assert_eq!(page["results"][0]["id"], json!(message));
    assert_eq!(page["next"], Value::Null);
    let other = host.bot("Other", "auto").await;
    assert!(host.tool_refused(&other, "search_history", json!({"from": message})).await.contains("no message"));

    // Detail goes to a note, as a line under new frontmatter.
    let note = host.memory_folder(Some(&bot)).join("style.md");
    assert_eq!(
        std::fs::read_to_string(&note).unwrap(),
        "---\ndescription: How the user likes answers\n---\n- Wants things explained concisely, in Traditional Chinese\n"
    );
    assert_eq!(view["dreams"][0]["files"][0]["name"], "style.md");
    assert!(
        host.refused("undoDream", json!({"botId": bot, "id": view["dreams"][0]["id"]}))
            .await
            .contains("unknown method")
    );
}

#[tokio::test]
async fn dreaming_tidies_a_nearly_full_memory_md_once_and_stops_when_nothing_moves() {
    let host = Host::start().await;
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/memory_agent.py");
    let mut bots = Vec::new();
    for name in ["Tidy A", "Tidy B"] {
        let bot = json!({"name": name, "backend": "custom", "command": common::python_agent(&agent), "connectors": [], "notify": false});
        bots.push(host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned());
    }
    let (a, b) = (bots[0].as_str(), bots[1].as_str());
    let log = host.home.join("background/dreams.log");
    let tidies = || std::fs::read_to_string(&log).unwrap_or_default().matches("tidy").count();

    // Both bots share a MEMORY.md nine-tenths full of the user's own lines: one run moves the detail into a note,
    // with no new conversation, and the other finds it tidied.
    let main = format!("- Allergic to peanuts and shellfish {}\n{}", "敏".repeat(100), filler(3, 90));
    host.write_memory(None, "MEMORY.md", &main);
    host.memory(a).await;
    for bot in [a, b] {
        host.call("newSession", json!({"botId": bot})).await;
    }
    let shared = host.memory_folder(None);
    let deadline = Instant::now() + Duration::from_secs(30);
    while std::fs::read_to_string(shared.join("MEMORY.md")).unwrap().contains("Allergic") {
        assert!(Instant::now() < deadline, "dreaming didn't tidy");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    let notes: Vec<PathBuf> = std::fs::read_dir(shared.join("health")).unwrap().map(|e| e.unwrap().path()).collect();
    assert_eq!(notes.len(), 1, "one run moved it: {notes:?}");
    assert_eq!(std::fs::read_to_string(&notes[0]).unwrap().matches("Allergic").count(), 1);
    let main = std::fs::read_to_string(shared.join("MEMORY.md")).unwrap();
    assert_eq!(main.matches("- Has food allergies → health/allergies-").count(), 1, "one line points to it: {main}");
    assert_eq!(tidies(), 1, "the other bot found it tidied");
    let view = host.memory(b).await;
    assert!(view["yours"]["used"].as_u64() < view["yours"]["cap"].as_u64(), "{view}");
    let listed = view["files"]["yours"]["files"].as_array().unwrap();
    assert!(listed.iter().any(|f| f["name"].as_str().unwrap().starts_with("health/allergies-")), "{view}");

    // A run that moves nothing out waits for MEMORY.md to change.
    host.write_memory(Some(a), "MEMORY.md", &filler(8, 140));
    host.memory(a).await;
    host.call("newSession", json!({"botId": a})).await;
    let deadline = Instant::now() + Duration::from_secs(30);
    while tidies() < 2 {
        assert!(Instant::now() < deadline, "dreaming didn't try to tidy the bot's own");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    host.call("newSession", json!({"botId": a})).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(tidies(), 2, "nothing moved, so it doesn't try again");
}

#[tokio::test]
async fn instructions_reach_every_harness_and_survive_compaction() {
    let host = Host::start().await;
    let profile_again = "replaces any earlier bot profile";

    // An agent with ACP's systemPrompt gets the instructions there, not in a message.
    let standard = host.fixture_bot("Standard", "instructions_agent.py", "standard").await;
    host.write_memory(Some(&standard), "MEMORY.md", "- Allergic to peanuts\n");
    host.reply(&standard, "hello").await;
    let new = &host.requests(&standard, "session/new")[0]["params"];
    assert_eq!(new["systemPrompt"]["mode"], "append");
    let system = new["systemPrompt"]["content"][0]["text"].as_str().unwrap();
    assert!(system.contains("Standard") && system.contains("Allergic to peanuts"), "{system}");
    assert!(!host.prompts(&standard)[0].contains("<bot-profile>"));
    // A change reaches the session as a note on its next message; the opening stays as it was.
    host.write_memory(Some(&standard), "MEMORY.md", "- Allergic to peanuts\n- Plays the piano\n");
    host.reply(&standard, "again").await;
    host.reply(&standard, "and again").await;
    let prompts = host.prompts(&standard);
    assert!(prompts[1].starts_with("<memory_update>") && prompts[1].contains("Plays the piano"), "{}", prompts[1]);
    assert!(!prompts[2].contains("<memory_update>"), "told once");
    assert_eq!(host.requests(&standard, "session/new").len(), 1);

    // Others get it in the first message, and again once after the agent reports a compaction.
    let reporting = host.fixture_bot("Reporting", "instructions_agent.py", "reporting").await;
    host.reply(&reporting, "hello").await;
    host.reply(&reporting, "COMPACT_NOW").await;
    host.reply(&reporting, "after").await;
    host.reply(&reporting, "later").await;
    assert!(host.requests(&reporting, "session/new")[0]["params"].get("systemPrompt").is_none());
    let prompts = host.prompts(&reporting);
    assert!(prompts[0].contains("<bot-profile>") && !prompts[0].contains(profile_again), "{}", prompts[0]);
    assert!(!prompts[1].contains("<bot-profile>"));
    assert!(prompts[2].contains("<bot-profile>") && prompts[2].contains(profile_again), "{}", prompts[2]);
    assert!(!prompts[3].contains("<bot-profile>"), "only once");

    // An agent that doesn't report compactions shows them as its context use halving.
    let silent = host.fixture_bot("Silent", "instructions_agent.py", "silent").await;
    host.reply(&silent, "USAGE 90000").await;
    host.reply(&silent, "USAGE 70000").await;
    host.reply(&silent, "steady").await;
    host.reply(&silent, "USAGE 9000").await;
    host.reply(&silent, "after").await;
    let prompts = host.prompts(&silent);
    assert!(!prompts[2].contains("<bot-profile>"), "a small fall isn't a compaction");
    assert!(prompts[4].contains(profile_again), "{}", prompts[4]);
}

/// A first prompt that failed may never have reached the agent: its retry (even after a host restart) still
/// carries the instructions.
async fn retry_after_failed_first_prompt(restart: bool) {
    let mut host = Host::start().await;
    let bot = host.fixture_bot("Flaky", "instructions_agent.py", "flaky").await;
    host.call("send", json!({"botId": bot, "text": "first"})).await;
    let deadline = Instant::now() + Duration::from_secs(30);
    while !host.call("history", json!({"botId": bot})).await.to_string().contains("The agent failed") {
        assert!(Instant::now() < deadline, "the first prompt never failed");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if restart {
        host.restart(|_| {}).await;
    }
    host.reply(&bot, "second").await;
    let prompts = host.prompts(&bot);
    assert_eq!(prompts.len(), 2, "{prompts:?}");
    assert!(prompts[0].contains("<bot-profile>") && prompts[1].contains("<bot-profile>"), "{prompts:?}");
}

#[tokio::test]
async fn a_retry_after_a_failed_first_prompt_still_carries_the_instructions() {
    retry_after_failed_first_prompt(false).await;
}

#[tokio::test]
async fn a_retry_after_a_restart_still_carries_the_instructions() {
    retry_after_failed_first_prompt(true).await;
}
