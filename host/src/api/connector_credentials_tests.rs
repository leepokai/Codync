use super::*;
use crate::api::dispatch;
use crate::market::{Connector, connectors, save_connectors};
use tokio::sync::mpsc::UnboundedReceiver;

/// A hub with connector `remote` waiting for sign-in, so every check fails, and bot `b` using it.
fn fixture(dir: &std::path::Path) -> (Arc<Hub>, UnboundedReceiver<Cmd>) {
    std::fs::create_dir_all(dir).unwrap();
    let store = crate::store::Store::open(std::path::Path::new(":memory:")).unwrap();
    let bot = json!({"id":"b", "name":"B", "backend":"claude", "cwd":"/tmp", "connectors":["remote"]});
    store.save_bot(&serde_json::from_value(bot).unwrap()).unwrap();
    let hub = Hub::new(
        store,
        "test".into(),
        crate::remote::identity::Identity::load_or_create(dir).unwrap(),
        "token".into(),
        0,
    );
    let c: Connector = serde_json::from_value(json!({
        "id":"remote", "name":"Remote", "url":"https://example.com/mcp",
        "headers":{"X-Key":"first"}, "oauth":{}
    }))
    .unwrap();
    save_connectors(&hub.store, &[c]).unwrap();
    let commands = hub.attach_bot_for_test("b");
    (hub, commands)
}

fn refreshed(commands: &mut UnboundedReceiver<Cmd>) -> bool {
    let mut any = false;
    while let Ok(cmd) = commands.try_recv() {
        any |= matches!(cmd, Cmd::RefreshTools);
    }
    any
}

#[tokio::test]
async fn replacing_remote_credentials_invalidates_old_proxies_even_if_verification_fails() {
    let dir = std::env::temp_dir().join(format!("codync-credential-edit-{}", uuid::Uuid::new_v4()));
    let (hub, mut commands) = fixture(&dir);
    let before = connectors(&hub.store).unwrap().remove(0);
    let patch = json!({"id":"remote","fields":{"X-Key":"second"}});
    let error = dispatch(&hub, &Caller::Local, "credentialUpdateConnector", patch.clone()).await.unwrap_err();
    assert!(error.to_string().contains("Sign in"));
    let saved = connectors(&hub.store).unwrap().remove(0);
    assert_ne!(saved.auth_revision, before.auth_revision);
    assert_eq!(saved.headers["X-Key"], "second");
    assert!(refreshed(&mut commands), "the new revision stopped the bot's proxy: it restarts its tools");
    dispatch(&hub, &Caller::Local, "credentialUpdateConnector", patch).await.unwrap_err();
    assert_eq!(
        connectors(&hub.store).unwrap()[0].auth_revision,
        saved.auth_revision,
        "unchanged values keep the revision"
    );
    assert!(!refreshed(&mut commands), "a failed check of the same connection leaves the bot alone");
    let invalid = json!({"id":"remote","fields":{"X-Key":"third","unknown":"bad"}});
    dispatch(&hub, &Caller::Local, "credentialUpdateConnector", invalid).await.unwrap_err();
    assert_eq!(connectors(&hub.store).unwrap(), [saved], "invalid fields save neither values nor revision");
    hub.shutdown().await;
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn remote_edits_refresh_the_bots_whenever_the_revision_moves() {
    let dir = std::env::temp_dir().join(format!("codync-remote-edit-{}", uuid::Uuid::new_v4()));
    let (hub, mut commands) = fixture(&dir);
    let rename = json!({"id":"remote","name":"Renamed"});
    dispatch(&hub, &Caller::Local, "updateConnector", rename).await.unwrap();
    assert!(!refreshed(&mut commands), "a metadata edit runs nothing new");
    let invalid = json!({"id":"remote","url":"https://example.com/other"});
    dispatch(&hub, &Caller::Local, "updateConnector", invalid).await.unwrap_err();
    assert!(!refreshed(&mut commands), "a refused edit saves nothing");
    let headers = json!({"id":"remote","headers":{"X-Key":"second"}});
    let error = dispatch(&hub, &Caller::Local, "updateConnector", headers).await.unwrap_err();
    assert!(error.to_string().contains("Sign in"), "{error}");
    assert!(refreshed(&mut commands), "the saved edit retired the bot's proxy even though its check failed");
    hub.shutdown().await;
    std::fs::remove_dir_all(dir).unwrap();
}
