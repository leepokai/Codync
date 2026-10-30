use super::*;
use crate::remote::identity::Identity;

fn hub() -> (Arc<Hub>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("codync-icons-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("t.db")).unwrap();
    let identity = Identity::load_or_create(&dir).unwrap();
    (Hub::new(store, "test".into(), identity, "token".into(), 0), dir)
}

#[tokio::test]
async fn cached_icons_and_color_changes_do_not_need_cloud_downloads() {
    let (hub, dir) = hub();
    let png = image::normalize(br##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><rect width="32" height="32" fill="#f00"/></svg>"##).unwrap();
    let icon = Icon { slot: 0, data_url: format!("data:image/png;base64,{}", STANDARD.encode(png)), tinted: false };
    commit(&hub, BTreeMap::from([("agent:claude".to_owned(), icon)])).unwrap();
    let before = list(&hub.store).unwrap()["agent:claude"].data_url.clone();
    let before_revision = snapshot(&hub.store).unwrap().icons_revision;
    let result = set(&hub, &json!({"target": "agent:claude", "tinted": true})).await.unwrap();
    assert_eq!(result["icons"]["agent:claude"]["dataUrl"], before);
    assert_eq!(result["icons"]["agent:claude"]["tinted"], true);
    assert!(result["iconsRevision"].as_i64().unwrap() > before_revision);
    // The cache survives reopening SQLite; there is no HTTP request on either read path.
    let reopened = Store::open(&dir.join("t.db")).unwrap();
    assert_eq!(list(&reopened).unwrap()["agent:claude"].data_url, before);
    assert_eq!(snapshot(&reopened).unwrap().icons_revision, result["iconsRevision"].as_i64().unwrap());
    let error = set(&hub, &json!({"target": "agent:claude", "data": STANDARD.encode(b"bad image")})).await;
    assert!(error.is_err());
    assert_eq!(list(&hub.store).unwrap()["agent:claude"].data_url, before);
    drop(reopened);
    drop(hub);
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn concurrent_changes_and_unknown_targets_are_rejected_before_uploading() {
    let (hub, dir) = hub();
    assert!(set(&hub, &json!({"target": "bot:missing", "data": ""})).await.is_err());
    let guard = hub.store.icon_lock.lock().await;
    let error = set(&hub, &json!({"target": "agent:claude", "data": ""})).await.unwrap_err();
    assert!(error.to_string().contains("Another icon"));
    drop(guard);
    drop(hub);
    std::fs::remove_dir_all(dir).unwrap();
}
