//! The one-time move of bots that worked in a project to their own folder (`hub::projects::migrate`), against a real
//! host whose store is set back to the old shape between two starts.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The backups the migration wrote.
fn backups(home: &Path) -> Vec<PathBuf> {
    let Ok(dir) = std::fs::read_dir(home.join("backups")) else { return Vec::new() };
    dir.map(|e| e.unwrap().path()).filter(|p| p.to_string_lossy().contains("bot-projects-")).collect()
}

async fn row(host: &Host, bot: &str) -> Value {
    common::bot(&host.snapshot().await, bot).clone()
}

#[tokio::test]
async fn a_bot_in_a_project_moves_home_once_keeping_its_project_routines_uploads_and_chat() {
    let mut host = Host::start().await;
    let bot = host.fixture_bot("Mika", "instructions_agent.py", "").await;
    host.reply(&bot, "Fix the login tests").await;
    let routine = json!({"botId": bot, "name": "Nightly", "instruction": "Check", "triggers": [{"type": "webhook"}]});
    let routine = host.call("saveRoutine", routine).await["routine"]["id"].as_str().unwrap().to_owned();
    let project = host.home.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let upload = uuid::Uuid::new_v4().to_string();
    assert!(backups(&host.home).is_empty(), "nothing to move on a new computer");

    // The store as an older host left it: the bot works in the project, its uploads in its data folder.
    host.restart(|home| {
        let db = rusqlite::Connection::open(home.join("codync.db")).unwrap();
        let old = "UPDATE bots SET config = json_remove(json_set(config, '$.cwd', ?2), '$.projects', '$.sharedMemory')
                   WHERE id = ?1";
        db.execute(old, rusqlite::params![bot, project.to_string_lossy()]).unwrap();
        db.execute("DELETE FROM kv WHERE k = 'projects.migrated'", []).unwrap();
        // A bot that never worked in a folder of its own has none yet.
        std::fs::remove_dir_all(home.join("bots").join(&bot).join("workspace")).unwrap();
        let sent = home.join("bots").join(&bot).join("uploads").join(&upload);
        std::fs::create_dir_all(&sent).unwrap();
        std::fs::write(sent.join("notes.txt"), "hi").unwrap();
    })
    .await;

    let home = host.workspace(&bot);
    let moved = row(&host, &bot).await;
    assert_eq!((&moved["cwd"], &moved["projects"]), (&json!(home), &json!([project])), "{moved}");
    assert_eq!((&moved["sharedMemory"], &moved["managedWorkspace"]), (&json!(false), &json!(true)));
    let routines = host.call("routines", json!({"botId": bot})).await;
    let saved = routines["routines"].as_array().unwrap().iter().find(|r| r["id"] == routine.as_str()).unwrap();
    assert_eq!(saved["folder"], json!(project), "its runs keep working where they did");
    assert!(home.join("uploads").join(&upload).join("notes.txt").exists(), "its uploads moved home");
    let git = std::process::Command::new("git").arg("--version").output().is_ok_and(|out| out.status.success());
    if git {
        let versioned = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !home.join(".git").exists() {
            assert!(std::time::Instant::now() < versioned, "its own folder keeps versions, as a new bot's does");
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
    assert!(!host.home.join("bots").join(&bot).join("uploads").exists());
    let written = backups(&host.home);
    assert_eq!(written.len(), 1);
    let backup: Value = serde_json::from_slice(&std::fs::read(&written[0]).unwrap()).unwrap();
    assert_eq!((&backup["bots"][0]["id"], &backup["bots"][0]["cwd"]), (&json!(bot), &json!(project)));
    assert_eq!(backup["routines"][0]["id"], json!(routine));

    // Its next message starts a new session at home, which hears what the chat was about.
    host.reply(&bot, "Go on").await;
    let told = host.prompts(&bot).pop().unwrap();
    assert!(told.contains("in another working folder") && told.contains("Fix the login tests"), "{told}");

    // Once: another start changes nothing.
    host.restart(|_| {}).await;
    assert_eq!(backups(&host.home).len(), 1);
    assert_eq!(row(&host, &bot).await["projects"], json!([project]));
}
