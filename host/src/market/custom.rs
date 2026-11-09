//! Custom connectors: a command line or a URL the user describes and names.

use super::{Connector, add_connector, connectors, ensure_name_available, oauth, unique_id};
use anyhow::{Result, anyhow, bail};
use serde_json::Value;
use std::collections::BTreeMap;

/// A connector you describe yourself: a command line or an https URL.
pub async fn add_custom_connector(store: &crate::store::Store, b: &Value) -> Result<Value> {
    let name =
        b["name"].as_str().map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("`name` is required"))?;
    let all = connectors(store)?;
    ensure_name_available(name, &all)?;
    let c = custom(name, b, &all).await?;
    add_connector(store, c)
}

/// A connector from `{command, args?, env?}` or `{url, headers?}`. A command without
/// `args` is split like a shell would (quotes work).
pub(super) async fn custom(name: &str, b: &Value, all: &[Connector]) -> Result<Connector> {
    let map = |v: &Value| -> BTreeMap<String, String> {
        v.as_object().into_iter().flatten().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned()))).collect()
    };
    let mut c = Connector {
        id: unique_id(name, &all.iter().map(|c| c.id.clone()).collect::<Vec<_>>()),
        name: name.to_owned(),
        description: b["description"].as_str().unwrap_or_default().to_owned(),
        registry_name: None,
        command: None,
        args: vec![],
        env: map(&b["env"]),
        url: None,
        headers: map(&b["headers"]),
        oauth: None,
    };
    let url =
        ["url", "serverUrl", "httpUrl"].iter().find_map(|k| b[*k].as_str()).map(str::trim).filter(|s| !s.is_empty());
    if let Some(url) = url {
        let local = ["http://localhost", "http://127.0.0.1", "http://[::1]"].iter().any(|p| url.starts_with(p));
        if !url.starts_with("https://") && !local {
            bail!("remote connectors need an https URL");
        }
        if oauth::required(url, &c.headers).await {
            c.oauth = Some(oauth::OAuth::default());
        }
        c.url = Some(url.to_owned());
        return Ok(c);
    }
    let line = b["command"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("give a command or a URL"))?;
    (c.command, c.args) = super::command_line::split_command(line, b.get("args"))?;
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{devices::Caller, dispatch};
    use crate::hub::Hub;
    use crate::remote::identity::Identity;
    use crate::store::Store;
    use serde_json::json;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
        assert!(error.to_string().contains("already installed"), "{error}");
        assert_eq!(connectors(&hub.store).unwrap(), before);
        assert_eq!(hub.store.kv_read("connectors").unwrap(), sealed, "no vault rewrite");
        assert_eq!(hub.store.bot("uses-server").unwrap().unwrap().config.connectors, [id]);
        assert_eq!(hub.store.bot("unrelated").unwrap().unwrap().config.connectors, Vec::<String>::new());
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
        assert!(add_connector(&store, second).unwrap_err().to_string().contains("already installed"));
        assert_eq!(connectors(&store).unwrap(), [first]);
        assert_eq!(store.kv_read("connectors").unwrap(), sealed);
    }

    #[tokio::test]
    async fn a_duplicate_name_is_rejected_before_contacting_a_remote() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        add_custom_connector(&store, &json!({"name": "Existing", "command": "node server.js"})).await.unwrap();
        let before = connectors(&store).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let contacts = Arc::new(AtomicUsize::new(0));
        let count = contacts.clone();
        let remote = axum::Router::new().fallback(move || {
            count.fetch_add(1, Ordering::SeqCst);
            async { axum::http::StatusCode::UNAUTHORIZED }
        });
        let server = tokio::spawn(async move { axum::serve(listener, remote).await });
        let error = add_custom_connector(&store, &json!({"name": "Existing", "url": format!("http://{address}/mcp")}))
            .await
            .unwrap_err();
        server.abort();
        assert!(error.to_string().contains("already installed"), "{error}");
        assert_eq!(contacts.load(Ordering::SeqCst), 0);
        assert_eq!(connectors(&store).unwrap(), before);
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
            assert!(error.to_string().contains("already installed"));
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
