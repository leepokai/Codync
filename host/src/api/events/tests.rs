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
