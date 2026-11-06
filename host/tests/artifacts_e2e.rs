//! Artifacts through real ACP/MCP and the apps' API: publish, card, update in a reply to the card, read, rename, the live
//! `artifact` events, a routine's artifact (no card), the chat's Library, and what deleting the bot removes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use base64::Engine as _;
use common::host::Host;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;

/// The bot's chat once its turn has ended with `Done.` and it holds `cards` artifact cards.
async fn cards(host: &Host, bot: &str, cards: usize) -> Vec<Value> {
    let waited = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let entries = host.call("history", json!({"botId": bot})).await["entries"].as_array().unwrap().clone();
            let artifacts: Vec<Value> = entries.iter().filter(|e| e["data"]["artifact"].is_object()).cloned().collect();
            let done = entries.iter().filter(|e| e["data"]["text"] == "Done." && e["data"]["final"] == true).count();
            if artifacts.len() == cards && done == cards {
                return artifacts;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    waited.unwrap()
}

async fn html(host: &Host, bot: &str, artifact: &str) -> String {
    let read = host.call("readArtifact", json!({"botId": bot, "artifactId": artifact, "offset": 0})).await;
    let bytes = base64::engine::general_purpose::STANDARD.decode(read["data"].as_str().unwrap()).unwrap();
    assert_eq!(read["size"], bytes.len());
    String::from_utf8(bytes).unwrap()
}

fn results(cwd: &Path) -> Vec<Value> {
    serde_json::from_str(&std::fs::read_to_string(cwd.join("artifact-results.json")).unwrap()).unwrap()
}

/// The titles of the live `artifact` events, up to one titled `last`.
async fn artifact_titles(events: &mut UnboundedReceiver<Value>, last: &str) -> Vec<String> {
    let mut titles = vec![];
    let waited = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(event) = events.recv().await {
            if event["type"] == "artifact" {
                titles.push(event["artifact"]["title"].as_str().unwrap().to_owned());
                if titles.last().is_some_and(|t| t == last) {
                    return;
                }
            }
        }
    })
    .await;
    waited.unwrap_or_else(|_| panic!("no artifact event titled {last:?}: {titles:?}"));
    titles
}

/// Waits until `ready` holds for the bot's Library, and returns it.
async fn library_until(host: &Host, bot: &str, ready: impl Fn(&[Value]) -> bool) -> Vec<Value> {
    let waited = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let items = library(host, bot).await;
            if ready(&items) {
                return items;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    waited.unwrap()
}

#[tokio::test]
async fn bots_publish_update_and_the_apps_read_and_rename_artifacts() {
    let host = Host::start().await;
    let cwd = host.home.join("workspace");
    std::fs::create_dir_all(&cwd).unwrap();
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/artifact_agent.py");
    let created = host.call("createBot", json!({"name":"Artifacts", "backend":"custom", "cwd":cwd, "command":common::python_agent(&agent), "connectors":[], "notify":false})).await;
    let bot = created["bot"]["id"].as_str().unwrap();
    let mut events = host.events().await;

    host.call("send", json!({"botId": bot, "text": "make-artifact"})).await;
    let card = cards(&host, bot, 1).await.remove(0);
    let made = results(&cwd);
    assert!(made[1]["isError"] == true, "a new artifact needs a title: {}", made[1]);
    let id = card["data"]["artifact"]["id"].as_str().unwrap();
    assert_eq!(card["data"]["artifact"]["title"], "Weekly report");
    assert_eq!(card["data"]["text"], "Artifact: Weekly report", "older apps show the text");
    let artifacts = library(&host, bot).await;
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0]["kind"], "artifact");
    assert_eq!(artifacts[0]["artifact"]["entryId"], card["id"], "messages about the artifact reply to its card");
    assert_eq!(html(&host, bot, id).await, "<h1>Weekly report</h1>");

    // A reply to the card tells the bot which artifact and where its HTML is; it updates that artifact.
    host.call("send", json!({"botId": bot, "text": "update-artifact", "replyTo": card["id"]})).await;
    let update = cards(&host, bot, 2).await.remove(1);
    let prompt = std::fs::read_to_string(cwd.join("last-prompt.txt")).unwrap();
    assert!(prompt.contains(&format!("this is about your artifact \"Weekly report\" (id {id})")), "{prompt}");
    assert_eq!(update["data"]["artifact"]["id"], id, "the same artifact, not a second one");
    assert_eq!(html(&host, bot, id).await, "<h1>Weekly report</h1><p>Updated</p>");
    let artifacts = library(&host, bot).await;
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0]["artifact"]["entryId"], update["id"]);

    let renamed = host.call("renameArtifact", json!({"botId": bot, "artifactId": id, "title": " Week 43 "})).await;
    assert_eq!(renamed["artifact"]["title"], "Week 43");
    let titles = artifact_titles(&mut events, "Week 43").await;
    assert_eq!(titles.first().map(String::as_str), Some("Weekly report"), "publishing tells the apps: {titles:?}");
    host.refused("renameArtifact", json!({"botId": bot, "artifactId": id, "title": " "})).await;
    let reply = host.call("send", json!({"botId": bot, "text": "thanks", "replyTo": card["id"]})).await;
    assert_eq!(
        reply["entry"]["data"]["replyTo"]["text"], "Artifact: Week 43",
        "a quote names the artifact as it's called now"
    );
    host.refused("readArtifact", json!({"botId": bot, "artifactId": id, "offset": 1_000_000})).await;
    host.refused("readArtifact", json!({"botId": bot, "artifactId": "../../token", "offset": 0})).await;
    host.refused("readArtifact", json!({"botId": "someone-else", "artifactId": id, "offset": 0})).await;

    // The user's attachments join the Library, newest first.
    let upload = uuid::Uuid::new_v4().to_string();
    let data = base64::engine::general_purpose::STANDARD.encode(b"plan");
    host.call(
        "upload",
        json!({"botId": bot, "uploadId": upload, "name": "plan.txt", "offset": 0, "data": data, "done": true}),
    )
    .await;
    host.call("send", json!({"botId": bot, "text": "", "attachments": [upload]})).await;
    let items = library(&host, bot).await;
    assert_eq!(items.len(), 2);
    assert_eq!((items[0]["kind"].as_str(), items[0]["file"]["name"].as_str()), (Some("attachment"), Some("plan.txt")));
    assert_eq!(items[1]["artifact"]["title"], "Week 43");

    // And the files the bot shares.
    host.call("send", json!({"botId": bot, "text": "share-file"})).await;
    let items = library_until(&host, bot, |items| items.len() == 3).await;
    assert_eq!((items[0]["kind"].as_str(), items[0]["file"]["name"].as_str()), (Some("file"), Some("notes.txt")));

    // A routine's artifact joins the Library without a card, and its answer says so.
    let routine =
        json!({"botId": bot, "name": "Report", "instruction": "make-artifact", "triggers": [{"type": "webhook"}]});
    let routine = host.call("saveRoutine", routine).await;
    host.call("runRoutine", json!({"botId": bot, "id": routine["routine"]["id"]})).await;
    let items = library_until(&host, bot, |items| items.iter().filter(|i| i["kind"] == "artifact").count() == 2).await;
    let made = items.iter().find(|i| i["artifact"]["title"] == "Weekly report").unwrap();
    assert!(made["artifact"]["entryId"].is_null(), "nothing to reply to: {made}");
    let answered = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let made = std::fs::read_to_string(cwd.join("artifact-results.json")).ok();
            let answer = made
                .and_then(|made| serde_json::from_str::<Vec<Value>>(&made).ok())
                .and_then(|made| serde_json::from_str::<Value>(made.first()?["content"][0]["text"].as_str()?).ok());
            if let Some(answer) = answer.filter(|answer| answer["card"].is_string()) {
                return answer;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    assert!(answered["card"].as_str().unwrap().starts_with("not posted"), "{answered}");
    let history = host.call("history", json!({"botId": bot})).await;
    let cards = history["entries"].as_array().unwrap().iter().filter(|e| e["data"]["artifact"].is_object()).count();
    assert_eq!(cards, 2, "the routine's artifact has no card");

    // Deleting the bot deletes its artifacts.
    host.call("deleteBot", json!({"botId": bot})).await;
    let folder = host.home.join("bots").join(bot).join("artifacts");
    tokio::time::timeout(Duration::from_secs(10), async {
        while folder.exists() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the artifacts folder goes");
    let db = rusqlite::Connection::open(host.home.join("codync.db")).unwrap();
    let rows: i64 = db
        .query_row("SELECT count(*) FROM kv WHERE k GLOB ?1", [format!("artifact.{bot}.*")], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 0, "nor do their details");
}

async fn library(host: &Host, bot: &str) -> Vec<Value> {
    host.call("library", json!({"botId": bot})).await["items"].as_array().unwrap().clone()
}
