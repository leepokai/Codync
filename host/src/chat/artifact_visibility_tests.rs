use super::*;
use crate::store::{BotConfig, EntryKind, Lane};

fn fixture() -> Arc<Hub> {
    let dir = std::env::temp_dir().join(format!("codync-private-artifact-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(Path::new(":memory:")).unwrap();
    let cfg: BotConfig = serde_json::from_value(json!({"id":"bot", "name":"Bot"})).unwrap();
    store.save_bot(&cfg).unwrap();
    let identity = crate::remote::identity::Identity::load_or_create(&dir).unwrap();
    Hub::new(store, "test".into(), identity, "token".into(), 19223)
}

fn pending() -> Artifact {
    Artifact {
        id: uuid::Uuid::new_v4().to_string(),
        bot_id: "bot".into(),
        title: "Private report".into(),
        size: 12,
        sha256: "hash".into(),
        created_at: 1,
        updated_at: 1,
        entry_id: None,
        rev: 0,
        publication: Publication::Pending,
    }
}

/// A public artifact with its HTML on disk and a card in the chat: what the user already has.
fn published(hub: &Arc<Hub>) -> (Artifact, String) {
    let mut artifact = pending();
    artifact.title = "Quarterly report".into();
    let html = b"Q1: 12 users";
    std::fs::create_dir_all(root("bot")).unwrap();
    std::fs::write(html_path("bot", &artifact.id).unwrap(), html).unwrap();
    artifact.size = u64::try_from(html.len()).unwrap();
    artifact.publication = Publication::Public;
    put(hub, &mut artifact).unwrap();
    let entry = hub.add_entry(&Lane::main("bot"), EntryKind::Agent, 1, &artifact.card_entry()).unwrap();
    artifact.entry_id = Some(entry.id.clone());
    put(hub, &mut artifact).unwrap();
    (artifact, entry.id)
}

/// The replacement a scheduled turn would publish: its HTML and metadata wait beside the published artifact.
fn stage_replacement(hub: &Arc<Hub>, artifact: &Artifact) {
    let html = b"Q2: 40 users";
    std::fs::write(staged_path("bot", &artifact.id).unwrap(), html).unwrap();
    let update = StagedUpdate {
        title: "Quarterly report (Q2)".into(),
        size: u64::try_from(html.len()).unwrap(),
        sha256: "next".into(),
        updated_at: artifact.updated_at + 1,
    };
    save_staged(&hub.store, "bot", &artifact.id, &update).unwrap();
}

async fn read_html(hub: &Arc<Hub>, id: &str) -> String {
    let read = json!({"botId":"bot", "artifactId":id, "offset":0});
    let value = api(hub, "readArtifact", &read).await.unwrap();
    let data = base64::engine::general_purpose::STANDARD.decode(value["data"].as_str().unwrap()).unwrap();
    String::from_utf8(data).unwrap()
}

#[tokio::test]
async fn scheduled_artifact_details_wait_for_promotion_in_live_events_catch_up_library_and_reads() {
    let hub = fixture();
    let mut events = hub.events.subscribe();
    let mut artifact = pending();
    put(&hub, &mut artifact).unwrap();
    assert!(events.try_recv().is_err(), "Pending metadata must not broadcast before a card exists");
    assert_eq!(since(&hub.store, 0).len(), 0);
    assert_eq!(crate::chat::library::list(&hub, "bot").unwrap()["items"].as_array().unwrap().len(), 0);
    let read = json!({"botId":"bot", "artifactId":artifact.id, "offset":0});
    assert!(format!("{:#}", api(&hub, "readArtifact", &read).await.unwrap_err()).contains("not yet published"));

    prepare_card(&hub, "bot", &artifact.card_entry(), true).unwrap();
    let mut card = artifact.card_entry();
    card["internal"] = true.into();
    let entry = hub.add_entry(&Lane::main("bot"), EntryKind::Agent, 1, &card).unwrap();
    artifact = get(&hub.store, "bot", &artifact.id).unwrap();
    artifact.entry_id = Some(entry.id.clone());
    put(&hub, &mut artifact).unwrap();
    while let Ok(event) = events.try_recv() {
        assert_ne!(event["type"], "artifact", "Internal artifact metadata leaked");
    }
    assert_eq!(since(&hub.store, 0).len(), 0);
    assert!(api(&hub, "readArtifact", &read).await.is_err());

    promote_card(&hub, "bot", &artifact.id, &entry.id).unwrap();
    let published = events.try_recv().unwrap();
    assert_eq!(published["type"], "artifact");
    assert_eq!(published["artifact"]["title"], "Private report");
    assert!(published["rev"].as_i64().unwrap() > entry.rev);
    assert!(published["artifact"].get("publication").is_none(), "Public wire metadata stays compatible");
    assert_eq!(since(&hub.store, 0).len(), 1);
    hub.edit_entry(&entry.id, |entry| {
        entry.data["internal"] = false.into();
        Ok(true)
    })
    .unwrap();
    assert_eq!(crate::chat::library::list(&hub, "bot").unwrap()["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_scheduled_replacement_waits_aside_and_lands_only_when_the_turn_promotes_it() {
    let hub = fixture();
    let (artifact, entry) = published(&hub);
    let mut events = hub.events.subscribe();
    stage_replacement(&hub, &artifact);
    prepare_card(&hub, "bot", &artifact.card_entry(), true).unwrap();
    assert!(events.try_recv().is_err(), "a staged replacement must not broadcast");
    assert_eq!(get(&hub.store, "bot", &artifact.id).unwrap().title, "Quarterly report");
    assert_eq!(read_html(&hub, &artifact.id).await, "Q1: 12 users");
    assert_eq!(crate::chat::library::list(&hub, "bot").unwrap()["items"].as_array().unwrap().len(), 1);

    promote_card(&hub, "bot", &artifact.id, &entry).unwrap();
    assert_eq!(events.try_recv().unwrap()["artifact"]["title"], "Quarterly report (Q2)");
    assert_eq!(read_html(&hub, &artifact.id).await, "Q2: 40 users");
    assert!(staged(&hub.store, "bot", &artifact.id).is_none());
    assert!(!staged_path("bot", &artifact.id).unwrap().exists());
}

#[tokio::test]
async fn a_silent_scheduled_turn_leaves_the_published_artifact_and_its_content_untouched() {
    let hub = fixture();
    let (artifact, _entry) = published(&hub);
    let mut events = hub.events.subscribe();
    stage_replacement(&hub, &artifact);
    prepare_card(&hub, "bot", &artifact.card_entry(), true).unwrap();
    // The turn ends with NO_UPDATE: nothing promotes, so nothing the user had may change.
    let visible = get(&hub.store, "bot", &artifact.id).unwrap();
    assert!(visible.is_public());
    assert_eq!(visible.title, "Quarterly report");
    assert_eq!(visible.size, 12);
    assert_eq!(read_html(&hub, &artifact.id).await, "Q1: 12 users");
    assert!(events.try_recv().is_err());
    assert!(crate::chat::library::list(&hub, "bot").is_ok());
}

#[test]
fn ordinary_artifact_metadata_is_announced_before_the_card_and_legacy_metadata_defaults_public() {
    let hub = fixture();
    let mut events = hub.events.subscribe();
    let mut artifact = pending();
    put(&hub, &mut artifact).unwrap();
    prepare_card(&hub, "bot", &artifact.card_entry(), false).unwrap();
    let event = events.try_recv().unwrap();
    assert_eq!(event["type"], "artifact");
    assert!(hub.store.history("bot", i64::MAX, 100).unwrap().is_empty(), "Its card has not been posted yet");
    let decoded: Artifact = serde_json::from_value(event["artifact"].clone()).unwrap();
    assert!(decoded.is_public());
    assert!(since(&hub.store, 0)[0]["rev"].as_i64().unwrap() > 0);
}
