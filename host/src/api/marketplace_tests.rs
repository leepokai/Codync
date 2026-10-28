use crate::{api, hub::Hub, market, remote::identity::Identity, store::Store};
use serde_json::json;

#[tokio::test]
async fn runtime_and_verification_use_the_current_argument_credential() {
    let dir = std::env::temp_dir().join(format!("codync-argument-credentials-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let python = if cfg!(windows) { "python" } else { "python3" };
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/argument_mcp.py");
    let c = serde_json::from_value(json!({
        "id":"fixture", "name":"Fixture", "registryName":"test/fixture", "command":python,
        "args":[fixture, "TOKEN={token}"], "env":{"token":"initial{other}", "other":"literal"},
        "argumentTemplates":{"1":{"token":null}}
    }))
    .unwrap();
    market::save_connectors(&store, &[c]).unwrap();
    let hub = Hub::new(store, "test".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0);
    let caller = api::devices::Caller::Local;
    let runtime = api::dispatch(&hub, &caller, "connectorRuntime", json!({"id":"fixture"})).await.unwrap();
    assert_eq!(runtime["args"][1], "TOKEN=initial{other}");
    assert_eq!(runtime["env"]["token"], "initial{other}");
    assert_eq!(
        api::dispatch(&hub, &caller, "connectorVerify", json!({"id":"fixture"})).await.unwrap()["status"],
        "ready"
    );
    let ready = api::dispatch(
        &hub,
        &caller,
        "credentialUpdateConnector",
        json!({"id":"fixture", "fields":{"token":"replacement{other}"}}),
    )
    .await
    .unwrap();
    assert_eq!(ready["status"], "ready");
    let runtime = api::dispatch(&hub, &caller, "connectorRuntime", json!({"id":"fixture"})).await.unwrap();
    assert_eq!(runtime["args"][1], "TOKEN=replacement{other}");
    let saved = market::connectors(&hub.store).unwrap().pop().unwrap();
    assert_eq!(saved.args[1], "TOKEN={token}");
    assert!(!saved.public().to_string().contains("replacement{other}"));
    assert!(!hub.store.kv_read("connectors").unwrap().unwrap().contains("replacement{other}"));
    std::fs::remove_dir_all(dir).unwrap();
}
