//! Artifacts through real ACP/MCP and the apps' API: publish, card, update in a reply to the card, read, rename, the live
//! `artifact` events, a routine's artifact and its card, a group's and another bot's with none, the chat's Library, and
//! what deleting the bot removes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use base64::Engine as _;
use common::host::Host;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;

/// A bot on the artifact fixture, working in `<home>/workspace`: that folder and the bot's id.
async fn artifact_bot(host: &Host) -> (PathBuf, String) {
    let cwd = host.home.join("workspace");
    std::fs::create_dir_all(&cwd).unwrap();
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/artifact_agent.py");
    let created = host.call("createBot", json!({"name":"Artifacts", "backend":"custom", "cwd":cwd, "command":common::python_agent(&agent), "connectors":[], "notify":false})).await;
    (cwd, created["bot"]["id"].as_str().unwrap().to_owned())
}

/// The bot's chat cards once `turns` turns have ended with `Done.` and it holds `cards` artifact cards.
async fn cards(host: &Host, bot: &str, cards: usize, turns: usize) -> Vec<Value> {
    let waited = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let entries = host.call("history", json!({"botId": bot})).await["entries"].as_array().unwrap().clone();
            let artifacts: Vec<Value> = entries.iter().filter(|e| e["data"]["artifact"].is_object()).cloned().collect();
            let done = entries.iter().filter(|e| e["data"]["text"] == "Done." && e["data"]["final"] == true).count();
            if artifacts.len() == cards && done == turns {
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
    let (cwd, bot) = artifact_bot(&host).await;
    let bot = bot.as_str();
    let mut events = host.events().await;

    host.call("send", json!({"botId": bot, "text": "make-artifact"})).await;
    let card = cards(&host, bot, 1, 1).await.remove(0);
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
    let update = cards(&host, bot, 2, 2).await.remove(1);
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

    // A routine's artifact joins the Library and puts its card in the chat too, so the user can reply to it.
    let routine =
        json!({"botId": bot, "name": "Report", "instruction": "make-artifact", "triggers": [{"type": "webhook"}]});
    let routine = host.call("saveRoutine", routine).await;
    host.call("runRoutine", json!({"botId": bot, "id": routine["routine"]["id"]})).await;
    // The first artifact is "Week 43" now, and `entryId` is set only once the card is in the chat.
    let items = library_until(&host, bot, |items| {
        items.iter().any(|i| i["artifact"]["title"] == "Weekly report" && i["artifact"]["entryId"].is_string())
    })
    .await;
    let made = items.iter().find(|i| i["artifact"]["title"] == "Weekly report").unwrap();
    let answer = answer_until(&cwd, |answer| answer["published"] == true).await;
    assert!(answer["card"].is_null(), "the routine's publish says nothing about a missing card: {answer}");
    let history = host.call("history", json!({"botId": bot})).await["entries"].as_array().unwrap().clone();
    assert_eq!(history.iter().filter(|e| e["data"]["artifact"].is_object()).count(), 3);
    let routine_card = history.iter().find(|e| e["id"] == made["artifact"]["entryId"]).expect("the routine's card");
    assert_eq!(routine_card["data"]["artifact"]["id"], made["artifact"]["id"]);

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

/// A group chat's or another bot's publish keeps the artifact in the bot's Library, with no card in either chat.
#[tokio::test]
async fn groups_and_other_bots_requests_post_no_artifact_card() {
    let host = Host::start().await;
    let (cwd, bot) = artifact_bot(&host).await;
    let bot = bot.as_str();
    let group = host.call("createBot", json!({"name":"Group", "kind":"group", "members":[bot]})).await;
    let group = group["bot"]["id"].as_str().unwrap();
    host.call("send", json!({"botId": group, "text": "make-artifact"})).await;
    let caller_dir = host.home.join("caller");
    std::fs::create_dir_all(&caller_dir).unwrap();
    let caller_agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
    let caller = host.call("createBot", json!({"name":"Caller", "backend":"custom", "cwd":caller_dir, "command":common::python_agent(&caller_agent), "connectors":[], "notify":false})).await;
    let caller = caller["bot"]["id"].as_str().unwrap();
    host.call("send", json!({"botId": caller, "text": "warm up"})).await;

    for (from, published) in [(group, 1), (caller, 2)] {
        let answer = answer_until(&cwd, |answer| answer["published"] == true).await;
        assert!(answer["card"].as_str().is_some_and(|card| card.starts_with("not posted")), "{answer}");
        let items = library(&host, bot).await;
        assert_eq!(items.len(), published, "{items:?}");
        assert!(items.iter().all(|i| i["artifact"]["entryId"].is_null()), "{items:?}");
        for chat in [bot, from] {
            let entries = host.call("history", json!({"botId": chat})).await["entries"].as_array().unwrap().clone();
            assert!(entries.iter().all(|e| e["data"]["artifact"].is_null()), "no card in {chat}");
        }
        std::fs::remove_file(cwd.join("artifact-results.json")).unwrap();
        if from == group {
            host.call("send", json!({"botId": caller, "text": format!("DELEGATE {bot} make-artifact")})).await;
        }
    }
}

/// The fixture's answer to its first call once `ready` holds for it.
async fn answer_until(cwd: &Path, ready: impl Fn(&Value) -> bool) -> Value {
    let waited = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let answer = std::fs::read_to_string(cwd.join("artifact-results.json"))
                .ok()
                .and_then(|made| serde_json::from_str::<Vec<Value>>(&made).ok())
                .and_then(|made| serde_json::from_str::<Value>(made.first()?["content"][0]["text"].as_str()?).ok());
            if let Some(answer) = answer.filter(&ready) {
                return answer;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    waited.unwrap()
}

async fn library(host: &Host, bot: &str) -> Vec<Value> {
    host.call("library", json!({"botId": bot})).await["items"].as_array().unwrap().clone()
}

/// An update with `notify: false` (#387) replaces the HTML and refreshes the artifact's existing card and Library entry,
/// without posting a new card in the chat; a new artifact with `notify: false` still gets its card.
#[tokio::test]
async fn a_quiet_update_posts_no_new_card() {
    let host = Host::start().await;
    let (cwd, bot) = artifact_bot(&host).await;
    let bot = bot.as_str();
    host.call("send", json!({"botId": bot, "text": "quiet-new"})).await;
    let card = cards(&host, bot, 1, 1).await.remove(0);
    assert_eq!(library(&host, bot).await[0]["artifact"]["entryId"], card["id"], "a new artifact gets its card");
    let id = card["data"]["artifact"]["id"].as_str().unwrap().to_owned();
    std::fs::remove_file(cwd.join("artifact-results.json")).unwrap();

    host.call("send", json!({"botId": bot, "text": "quiet-update"})).await;
    let answer = answer_until(&cwd, |answer| answer["published"] == true).await;
    assert!(answer["card"].is_null() && answer["quiet"] == true, "{answer}");
    let quiet = cards(&host, bot, 1, 2).await.remove(0);
    assert_eq!(html(&host, bot, &id).await, "<h1>Weekly report</h1><p>Quiet fix</p>");
    assert_eq!(quiet["id"], card["id"], "no new card in the chat");
    let items = library(&host, bot).await;
    assert_eq!(items[0]["artifact"]["entryId"], card["id"], "the Library keeps pointing at the existing card");
    assert_ne!(
        quiet["data"]["artifact"]["updatedAt"], card["data"]["artifact"]["updatedAt"],
        "the existing card shows the new version"
    );
}
