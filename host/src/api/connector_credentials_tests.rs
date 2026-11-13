use super::*;
use crate::api::dispatch;
use crate::market::{Connector, connectors, save_connectors};

#[tokio::test]
async fn replacing_remote_credentials_invalidates_old_proxies_even_if_verification_fails() {
    let dir = std::env::temp_dir().join(format!("codync-credential-edit-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = crate::store::Store::open(std::path::Path::new(":memory:")).unwrap();
    let hub = Hub::new(
        store,
        "test".into(),
        crate::remote::identity::Identity::load_or_create(&dir).unwrap(),
        "token".into(),
        0,
    );
    let c: Connector = serde_json::from_value(json!({
        "id":"remote", "name":"Remote", "url":"https://example.com/mcp",
        "headers":{"X-Key":"first"}, "oauth":{}
    }))
    .unwrap();
    save_connectors(&hub.store, &[c]).unwrap();
    let before = connectors(&hub.store).unwrap().remove(0);
    let patch = json!({"id":"remote","fields":{"X-Key":"second"}});
    let error = dispatch(&hub, &Caller::Local, "credentialUpdateConnector", patch.clone()).await.unwrap_err();
    assert!(error.to_string().contains("Sign in"));
    let saved = connectors(&hub.store).unwrap().remove(0);
    assert_ne!(saved.auth_revision, before.auth_revision);
    assert_eq!(saved.headers["X-Key"], "second");
    dispatch(&hub, &Caller::Local, "credentialUpdateConnector", patch).await.unwrap_err();
    assert_eq!(
        connectors(&hub.store).unwrap()[0].auth_revision,
        saved.auth_revision,
        "unchanged values keep the revision"
    );
    let invalid = json!({"id":"remote","fields":{"X-Key":"third","unknown":"bad"}});
    dispatch(&hub, &Caller::Local, "credentialUpdateConnector", invalid).await.unwrap_err();
    assert_eq!(connectors(&hub.store).unwrap(), [saved], "invalid fields save neither values nor revision");
    hub.shutdown().await;
    std::fs::remove_dir_all(dir).unwrap();
}
