//! A rename must obey the same name rule as a custom install.
use super::*;
use crate::api::{devices::Caller, dispatch};
use crate::market::{add_custom_connector, requests, save_connectors};
use crate::store::Store;
use std::path::Path;

fn local(id: &str, name: &str) -> Connector {
    serde_json::from_value(json!({
        "id":id, "name":name, "command":"python3",
        "args":[Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/credential_mcp.py")],
        "env":{"API_KEY":"test-private-key"}
    }))
    .unwrap()
}

#[tokio::test]
async fn a_taken_name_refuses_the_edit_without_changing_credentials_or_bot_choices() {
    for kind in ["local", "registry", "remote"] {
        let (hub, dir) = requests::tests::fixture();
        let mut first = local("fixture", "First");
        first.description = "saved description".into();
        let mut taken = local("taken", "Taken");
        if kind == "registry" {
            taken.registry_name = Some("example/server".into());
        } else if kind == "remote" {
            taken.command = None;
            taken.url = Some("https://example.com/mcp".into());
            taken.headers.insert("X-Key".into(), "kept-remote-secret".into());
        }
        save_connectors(&hub.store, &[first.clone(), taken.clone()]).unwrap();
        let mut bot = hub.store.bot("bot").unwrap().unwrap().config;
        bot.connectors = vec!["fixture".into()];
        hub.store.save_bot(&bot).unwrap();
        let ciphertext = hub.store.kv_get("connectors");
        let patch = json!({"id":"fixture", "name":" Taken ", "description":"must not save",
                          "command":"/nonexistent/codync-rename", "env":{"API_KEY":"replacement"}});
        let error = dispatch(&hub, &Caller::Local, "updateConnector", patch).await.unwrap_err();
        assert!(error.to_string().contains("already installed"), "{kind}: {error}");
        assert!(error.to_string().contains("Taken"), "{error}");
        assert_eq!(connectors(&hub.store).unwrap(), [first, taken]);
        assert_eq!(hub.store.kv_get("connectors"), ciphertext);
        assert_eq!(hub.store.bot("bot").unwrap().unwrap().config.connectors, ["fixture"]);
        hub.shutdown().await;
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[tokio::test]
async fn keeping_an_imported_duplicate_name_is_allowed_but_new_names_must_be_free() {
    let store = Store::open(Path::new(":memory:")).unwrap();
    save_connectors(&store, &[local("first", "Shared"), local("second", "Shared")]).unwrap();
    for edit in [json!({"description":"new"}), json!({"name":" Shared "}), json!({"name":"Distinct"})] {
        let result = update_custom_connector(&store, "first", &edit, 0).await.unwrap();
        assert_eq!(result["connector"]["id"], "first");
        assert!(result["verify"].is_null());
    }
    let saved = connectors(&store).unwrap();
    assert_eq!(saved[0].name, "Distinct");
    assert_eq!(saved[1].name, "Shared");
}

#[tokio::test]
async fn an_install_during_verification_cannot_take_the_renames_name() {
    let store = Store::open(Path::new(":memory:")).unwrap();
    let saved = local("first", "First");
    save_connectors(&store, std::slice::from_ref(&saved)).unwrap();
    let args = json!({"name":"Taken", "args":["-u", &saved.args[0]]});
    let install = json!({"name":"Taken","command":"python3"});
    let (rename, added) =
        tokio::join!(update_custom_connector(&store, "first", &args, 0), add_custom_connector(&store, &install));
    added.unwrap();
    assert!(rename.unwrap_err().to_string().contains("already installed"));
    let all = connectors(&store).unwrap();
    assert_eq!(all[0], saved);
    assert_eq!(all.iter().filter(|c| c.name == "Taken").count(), 1);
}
