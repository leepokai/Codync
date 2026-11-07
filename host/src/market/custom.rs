//! Custom connector installation without duplicating an existing name.

use super::install::custom;
use super::{Connector, add_connector, connectors};
use anyhow::{Result, anyhow, bail};
use serde_json::Value;

/// A connector you describe yourself: a command line or an https URL.
pub async fn add_custom_connector(store: &crate::store::Store, b: &Value) -> Result<Value> {
    let name =
        b["name"].as_str().map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("`name` is required"))?;
    let all = connectors(store)?;
    ensure_name_available(name, &all)?;
    let c = custom(name, b, &all).await?;
    add_connector(store, c)
}

/// Names are display names, not slugs: distinct names may legitimately share an ID prefix.
pub(super) fn ensure_name_available(name: &str, all: &[Connector]) -> Result<()> {
    if all.iter().any(|c| c.name == name) {
        bail!("A connector with this name already exists. Choose a different name for a new connector.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{devices::Caller, dispatch};
    use crate::hub::Hub;
    use crate::market::oauth;
    use crate::remote::identity::Identity;
    use crate::store::Store;
    use serde_json::json;
    use std::path::Path;

    #[tokio::test]
    async fn duplicate_install_preserves_credentials_and_bot_assignments() {
        let dir = std::env::temp_dir().join(format!("codync-duplicate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(Path::new(":memory:")).unwrap();
        let hub = Hub::new(store, "test".into(), Identity::load_or_create(&dir).unwrap(), "token".into(), 0);
        let first = dispatch(
            &hub,
            &Caller::Local,
            "installConnector",
            json!({
                "name": "Local server", "command": "node", "args": ["server.js"],
                "env": {"TOKEN": "original-secret"}
            }),
        )
        .await
        .unwrap();
        let id = first["connector"]["id"].as_str().unwrap();
        for (bot, on) in [("uses-server", vec![id]), ("unrelated", vec![])] {
            hub.store
                .save_bot(
                    &serde_json::from_value(json!({
                        "id": bot, "name": bot, "backend": "claude", "cwd": dir,
                        "connectors": on
                    }))
                    .unwrap(),
                )
                .unwrap();
        }
        let before = connectors(&hub.store).unwrap();
        let sealed = hub.store.kv_read("connectors").unwrap();
        let error = dispatch(
            &hub,
            &Caller::Local,
            "installConnector",
            json!({
                "name": "  Local server  ", "command": "/opt/node/bin/node", "args": [],
                "env": {"TOKEN": "replacement-secret"}
            }),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("already exists"), "{error}");
        assert_eq!(connectors(&hub.store).unwrap(), before);
        assert_eq!(hub.store.kv_read("connectors").unwrap(), sealed, "no vault rewrite");
        assert_eq!(hub.store.bot("uses-server").unwrap().unwrap().config.connectors, [id]);
        assert!(hub.store.bot("unrelated").unwrap().unwrap().config.connectors.is_empty());
        assert!(!error.to_string().contains("secret"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn a_prepared_duplicate_is_rejected_when_it_reaches_the_write_lock() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        // Both requests saw the empty list before either finished preparing its connector.
        let first = custom("Server", &json!({"command": "node first.js"}), &[]).await.unwrap();
        let second = custom("Server", &json!({"command": "node second.js"}), &[]).await.unwrap();
        add_connector(&store, first.clone()).unwrap();
        let sealed = store.kv_read("connectors").unwrap();
        assert!(add_connector(&store, second).unwrap_err().to_string().contains("already exists"));
        assert_eq!(connectors(&store).unwrap(), [first]);
        assert_eq!(store.kv_read("connectors").unwrap(), sealed);
    }

    #[tokio::test]
    async fn duplicate_install_returns_an_http_error_before_contacting_a_remote() {
        let dir = std::env::temp_dir().join(format!("codync-duplicate-http-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(Path::new(":memory:")).unwrap();
        let hub = Hub::new(store, "test".into(), Identity::load_or_create(&dir).unwrap(), "token".into(), 0);
        add_custom_connector(&hub.store, &json!({"name": "Existing", "command": "node server.js"})).await.unwrap();
        let before = connectors(&hub.store).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let contacts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = contacts.clone();
        let router = crate::api::router(hub.clone()).route(
            "/probe",
            axum::routing::post(move || {
                let count = count.clone();
                async move {
                    count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    axum::http::StatusCode::UNAUTHORIZED
                }
            }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, router.into_make_service_with_connect_info::<std::net::SocketAddr>()).await.unwrap();
        });
        let response = reqwest::Client::new()
            .post(format!("http://{address}/api/installConnector"))
            .bearer_auth("token")
            .json(&json!({"name": "Existing", "url": format!("http://{address}/probe")}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
        assert!(response.text().await.unwrap().contains("already exists"));
        assert_eq!(contacts.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(connectors(&hub.store).unwrap(), before);
        server.abort();
        assert!(server.await.unwrap_err().is_cancelled());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn an_existing_remote_or_registry_name_cannot_be_reused_by_a_custom_install() {
        for registry_name in [None, Some("io.example/server".to_owned())] {
            let store = Store::open(Path::new(":memory:")).unwrap();
            let mut existing = custom("Hosted", &json!({"command": "unused"}), &[]).await.unwrap();
            existing.command = None;
            existing.url = Some("https://example.invalid/mcp".into());
            existing.headers.insert("Authorization".into(), "Bearer saved-secret".into());
            existing.oauth = Some(oauth::OAuth::default());
            existing.registry_name = registry_name;
            add_connector(&store, existing.clone()).unwrap();
            let error =
                add_custom_connector(&store, &json!({"name": "Hosted", "command": "node new.js"})).await.unwrap_err();
            assert!(error.to_string().contains("already exists"));
            assert_eq!(connectors(&store).unwrap(), [existing]);
        }
    }

    #[tokio::test]
    async fn distinct_names_can_share_a_slug_and_imports_keep_their_existing_behavior() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        let first = add_custom_connector(&store, &json!({"name": "A B", "command": "node a.js"})).await.unwrap();
        let second = add_custom_connector(&store, &json!({"name": "A-B", "command": "node b.js"})).await.unwrap();
        assert_ne!(first["id"], second["id"]);
        super::super::import_connectors(&store, r#"{"mcpServers":{"A B":{"command":"node c.js"}}}"#).await.unwrap();
        assert_eq!(connectors(&store).unwrap().len(), 3);
    }
}
