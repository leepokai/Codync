use super::*;
use crate::remote::identity::Identity;
use crate::store::{BotConfig, Store};
use futures::StreamExt;
use std::path::PathBuf;

struct Fixture {
    hub: Arc<Hub>,
    dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("codync-sync-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let hub = Hub::new(store, "sync-test".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0);
        Self { hub, dir }
    }

    fn add_bot(&self, id: &str) -> i64 {
        let config: BotConfig = serde_json::from_value(json!({
            "id": id, "name": id, "backend": "fixture", "cwd": self.dir,
        }))
        .unwrap();
        self.hub.store.save_bot(&config).unwrap()
    }

    async fn catch_up(&self, since: i64) -> Vec<Value> {
        let stream = events_stream(&self.hub, since, None, &Caller::Local).unwrap();
        tokio::pin!(stream);
        let mut events = Vec::new();
        while let Ok(Some(event)) = tokio::time::timeout(std::time::Duration::from_millis(50), stream.next()).await {
            events.push(event);
        }
        events
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).unwrap();
    }
}

#[tokio::test]
async fn full_sync_removes_a_bot_deleted_while_the_client_was_offline() {
    let f = Fixture::new();
    f.add_bot("deleted");
    f.add_bot("kept");
    let deleted_rev = f.hub.store.delete_bot("deleted").unwrap();

    // An app/host update rewinds the cursor to zero while keeping the cached roster.
    let events = f.catch_up(0).await;
    assert_eq!(events[0]["type"], "hello");
    let mut cached = std::collections::HashSet::from(["deleted".to_owned()]);
    for event in &events[1..] {
        let bot = &event["bot"];
        let id = bot["id"].as_str().unwrap();
        if bot["deleted"] == true {
            cached.remove(id);
            assert_eq!(bot["rev"], deleted_rev);
        } else {
            cached.insert(id.to_owned());
        }
    }
    assert_eq!(cached, std::collections::HashSet::from(["kept".to_owned()]));
    assert!(events[1..].windows(2).all(|pair| pair[0]["rev"].as_i64() < pair[1]["rev"].as_i64()));
}

#[tokio::test]
async fn incremental_sync_replays_deletions_only_after_the_saved_cursor() {
    let f = Fixture::new();
    let before = f.add_bot("deleted");
    let deleted_rev = f.hub.store.delete_bot("deleted").unwrap();
    let events = f.catch_up(before).await;
    assert_eq!(events.len(), 2);
    assert_eq!(events[1]["bot"]["deleted"], true);
    assert_eq!(events[1]["rev"], deleted_rev);
    assert_eq!(f.catch_up(deleted_rev).await.len(), 1);
}

#[tokio::test]
async fn catch_up_brings_an_artifact_renamed_while_the_client_was_offline() {
    let f = Fixture::new();
    f.add_bot("b");
    let id = uuid::Uuid::new_v4().to_string();
    let card = f
        .hub
        .add_entry(
            &crate::store::Lane::main("b"),
            crate::store::EntryKind::Agent,
            0,
            &json!({"text": "Artifact: Old", "final": true, "artifact": {"id": id, "title": "Old", "updatedAt": 1}}),
        )
        .unwrap();
    let saved = json!({"id": id, "botId": "b", "title": "Old", "size": 1, "sha256": "x", "createdAt": 1, "updatedAt": 1,
        "entryId": card.id});
    f.hub.store.kv_set(&format!("artifact.b.{id}"), &saved.to_string()).unwrap();
    let cursor = f.hub.store.current_rev();
    assert_eq!(f.catch_up(cursor).await[1..], [] as [Value; 0], "details without a rev default to 0, never replayed");

    crate::chat::artifacts::api(&f.hub, "renameArtifact", &json!({"botId": "b", "artifactId": id, "title": "New"}))
        .await
        .unwrap();
    let events = f.catch_up(cursor).await;
    let artifact = events.iter().find(|e| e["type"] == "artifact").expect("the rename is in the catch-up");
    assert_eq!(artifact["artifact"]["title"], "New");
    assert!(artifact["rev"].as_i64().unwrap() > cursor);
    let entry = events.iter().find(|e| e["type"] == "entry").expect("its card was retitled");
    assert_eq!(entry["entry"]["data"]["text"], "Artifact: New");
    assert_eq!(entry["entry"]["data"]["artifact"]["title"], "New");
    assert!(events[1..].windows(2).all(|pair| pair[0]["rev"].as_i64() < pair[1]["rev"].as_i64()), "in rev order");
    assert_eq!(f.catch_up(f.hub.store.current_rev()).await[1..], [] as [Value; 0]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn renaming_an_artifact_keeps_the_reactions_its_card_gets_meanwhile() {
    let f = Fixture::new();
    f.add_bot("b");
    let id = uuid::Uuid::new_v4().to_string();
    let card = f
        .hub
        .add_entry(
            &crate::store::Lane::main("b"),
            crate::store::EntryKind::Agent,
            0,
            &json!({"text": "Artifact: Old", "final": true, "artifact": {"id": id, "title": "Old", "updatedAt": 1}}),
        )
        .unwrap();
    let saved = json!({"id": id, "botId": "b", "title": "Old", "size": 1, "sha256": "x", "createdAt": 1, "updatedAt": 1,
        "entryId": card.id});
    f.hub.store.kv_set(&format!("artifact.b.{id}"), &saved.to_string()).unwrap();

    let emoji: [&'static str; 8] = ["👍", "🎉", "❤️", "🔥", "👀", "✅", "🙏", "💯"];
    let reacting: Vec<_> = emoji
        .iter()
        .map(|&e| {
            let (hub, card) = (f.hub.clone(), card.id.clone());
            std::thread::spawn(move || hub.react(&card, e).unwrap())
        })
        .collect();
    for n in 0..40 {
        let title = format!("Title {n}");
        crate::chat::artifacts::api(&f.hub, "renameArtifact", &json!({"botId": "b", "artifactId": id, "title": title}))
            .await
            .unwrap();
    }
    for t in reacting {
        t.join().unwrap();
    }
    let card = f.hub.store.entry(&card.id).unwrap();
    let mut reactions: Vec<String> = serde_json::from_value(card.data["reactions"].clone()).unwrap();
    reactions.sort();
    let mut all: Vec<String> = emoji.iter().map(|e| (*e).to_owned()).collect();
    all.sort();
    assert_eq!(reactions, all, "no rename wrote over a reaction");
    assert_eq!(card.data["artifact"]["title"], "Title 39");
}

#[tokio::test]
async fn what_a_calls_voice_should_say_reaches_only_the_device_holding_it() {
    let f = Fixture::new();
    let phone = Caller::Device { key: "phone".into(), scopes: vec![] };
    let other = Caller::Device { key: "tablet".into(), scopes: vec![] };
    let streams = [&phone, &other, &Caller::Local].map(|c| Box::pin(events_stream(&f.hub, 0, None, c).unwrap()));
    let _ =
        f.hub.events.send(json!({"type": "call", "call": "c1", "owner": "phone", "kind": "reply", "text": "Done."}));
    let mut heard = Vec::new();
    for mut stream in streams {
        let mut got = false;
        while let Ok(Some(event)) = tokio::time::timeout(std::time::Duration::from_millis(50), stream.next()).await {
            got |= event["type"] == "call";
        }
        heard.push(got);
    }
    assert_eq!(heard, [true, false, false]);
}
