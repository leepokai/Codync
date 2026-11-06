use super::*;
use crate::store::{BotConfig, Store};

#[tokio::test]
async fn app_requests_serve_pages_and_data_and_reject_bad_capabilities_and_paths() {
    let dir = std::env::temp_dir().join(format!("codync-app-bridge-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(dir.join("apps/checklist")).unwrap();
    std::fs::write(dir.join("apps/checklist/index.html"), "<!doctype html><title>Checklist</title>").unwrap();
    let store = Store::open(&dir.join("test.db")).unwrap();
    let bot: BotConfig = serde_json::from_value(json!({
        "id":"bot", "name":"Bot", "backend":"claude", "cwd":dir,
    }))
    .unwrap();
    store.save_bot(&bot).unwrap();
    let hub = Hub::new(
        store,
        "test".into(),
        crate::remote::identity::Identity::load_or_create(&dir).unwrap(),
        "token".into(),
        19223,
    );
    crate::chat::apps::call(&hub, "bot", "save_app", &json!({"slug":"checklist", "title":"Checklist"})).unwrap();
    let app = hub.store.get_bot_app("bot", "checklist").unwrap().unwrap();
    let mut body = json!({"botId":"bot","slug":"checklist","token":app.owner_token.unwrap(),"method":"GET","path":""});
    let page = request(&hub, &body).await.unwrap();
    assert_eq!(page["status"], 200);
    assert!(page["contentType"].as_str().unwrap().starts_with("text/html"));
    assert!(String::from_utf8(STANDARD.decode(page["body"].as_str().unwrap()).unwrap()).unwrap().contains("Checklist"));
    let token = body["token"].clone();
    body["token"] = json!("invalid");
    assert_eq!(request(&hub, &body).await.unwrap()["status"], 403);
    body["token"] = token;
    for path in ["../index.html", "a/../../index.html", "a\\index.html"] {
        body["path"] = json!(path);
        assert_eq!(request(&hub, &body).await.unwrap()["status"], 400);
    }
    body["path"] = json!("_data/tasks");
    body["method"] = json!("POST");
    body["body"] = json!(r#"{"id":"one","value":{"done":false}}"#);
    assert_eq!(request(&hub, &body).await.unwrap()["status"], 200);
    body["method"] = json!("GET");
    let data = request(&hub, &body).await.unwrap();
    assert_eq!(data["status"], 200);
    let data: Value = serde_json::from_slice(&STANDARD.decode(data["body"].as_str().unwrap()).unwrap()).unwrap();
    assert_eq!(data["items"][0]["id"], "one");
    assert_eq!(data["items"][0]["value"]["done"], false);
    drop(hub);
    std::fs::remove_dir_all(dir).unwrap();
}
