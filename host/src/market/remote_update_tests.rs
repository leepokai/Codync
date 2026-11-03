use super::*;
use crate::market::{save_connectors, vault};
use crate::store::Store;
use serde_json::json;
use std::sync::{Arc, Mutex};

fn fixture() -> (Store, Connector) {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let c: Connector = serde_json::from_value(json!({
        "id":"remote", "name":"Remote", "description":"original", "url":"https://old.example/mcp",
        "headers":{"X-Api-Key":"private-key", "X-Mode":"read"},
        "oauth":{"clientId":"old-client", "accessToken":"private-token", "refreshToken":"private-refresh"}
    }))
    .unwrap();
    save_connectors(&store, std::slice::from_ref(&c)).unwrap();
    (store, c)
}

#[tokio::test]
async fn remote_metadata_and_headers_preserve_identity_and_sign_in() {
    let (store, original) = fixture();
    let public = update_connector(
        &store,
        &json!({"id":" remote ","name":"Renamed",
        "headers":{"x-api-key":"rotated", "X-Mode":null, "X-New":"new"}}),
    )
    .await
    .unwrap();
    let updated = connectors(&store).unwrap().remove(0);
    assert_eq!(updated.id, original.id);
    assert_eq!(updated.url, original.url);
    assert_eq!(updated.oauth, original.oauth);
    assert_eq!(updated.auth_revision, original.auth_revision);
    assert_eq!(updated.description, original.description);
    assert_eq!(
        updated.headers,
        BTreeMap::from([("x-api-key".into(), "rotated".into()), ("X-New".into(), "new".into())])
    );
    assert_eq!(public["name"], "Renamed");
    for secret in ["private-token", "private-refresh", "rotated", "old-client"] {
        assert!(!public.to_string().contains(secret));
        assert!(!store.kv_get("connectors").unwrap().contains(secret));
    }
}

#[tokio::test]
async fn invalid_remote_edits_leave_encrypted_storage_unchanged() {
    let (store, original) = fixture();
    let ciphertext = store.kv_get("connectors");
    for patch in [
        json!({"url":"https://new.example/mcp"}),
        json!({"url":null}),
        json!({"url":"http://localhost.evil.example/mcp","headers":{}}),
        json!({"url":"http://127.0.0.1.evil.example/mcp","headers":{}}),
        json!({"url":"https://user:password@example.com/mcp","headers":{}}),
        json!({"url":"https://example.com/mcp#fragment","headers":{}}),
        json!({"url":"file:///tmp/mcp","headers":{}}),
        json!({"headers":null}),
        json!({"headers":{"X-Test":42}}),
        json!({"headers":{"Bad Header":"x"}}),
        json!({"headers":{"X-Test":"x\r\nInjected: yes"}}),
        json!({"headers":{"X-Test":"one","x-test":"two"}}),
        json!({"headers":{"Authorization":"Bearer replacement"}}),
        json!({"command":"node"}),
        json!({"args":[]}),
        json!({"env":{}}),
        json!({"name":1}),
        json!({"description":false}),
    ] {
        let mut request = patch.clone();
        request["id"] = json!("remote");
        request["description"] = request.get("description").cloned().unwrap_or(json!("must not save"));
        assert!(update_connector(&store, &request).await.is_err(), "accepted {patch}");
        assert_eq!(connectors(&store).unwrap().as_slice(), std::slice::from_ref(&original));
        assert_eq!(store.kv_get("connectors"), ciphertext);
    }
    assert!(vault::read(&store, "connectors").unwrap().is_some());
}

#[test]
fn remote_urls_use_parsed_hosts_and_every_move_invalidates_old_auth() {
    let (_, c) = fixture();
    for url in ["https://example.com/mcp", "http://localhost:123/mcp", "http://127.0.0.2/mcp", "http://[::1]/mcp"] {
        validate_url(url).unwrap();
    }
    let moved = prepare(&c, &json!({"url":"https://new.example/mcp","headers":{}})).unwrap();
    assert!(moved.oauth.is_none());
    assert!(moved.headers.is_empty());
    assert_ne!(moved.auth_revision, c.auth_revision);
    let back = prepare(&moved, &json!({"url":c.url,"headers":{}})).unwrap();
    assert_ne!(back.auth_revision, moved.auth_revision);
    assert_ne!(back.auth_revision, c.auth_revision);
    assert_eq!(prepare(&c, &json!({"url":c.url})).unwrap(), c);
}

#[tokio::test]
async fn replacement_endpoint_receives_only_explicit_headers_and_requires_new_sign_in() {
    use axum::{
        Router,
        http::{HeaderMap, StatusCode},
        routing,
    };
    let seen = Arc::new(Mutex::new(Vec::new()));
    let captured = seen.clone();
    let app = Router::new().route(
        "/mcp",
        routing::post(move |headers: HeaderMap| {
            captured.lock().unwrap().push(headers);
            async { StatusCode::UNAUTHORIZED }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let (store, old) = fixture();
    let public =
        update_connector(&store, &json!({"id":old.id,"url":url,"headers":{"X-New":"explicit"}})).await.unwrap();
    let c = connectors(&store).unwrap().remove(0);
    assert_eq!(c.oauth, Some(oauth::OAuth::default()));
    assert_eq!(public["auth"], "signedOut");
    assert_eq!(c.id, old.id);
    assert_eq!(c.headers, BTreeMap::from([("X-New".into(), "explicit".into())]));
    {
        let requests = seen.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["x-new"], "explicit");
        assert!(!requests[0].contains_key("authorization"));
        assert!(!requests[0].contains_key("x-api-key"));
    }
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn discovery_does_not_overwrite_a_concurrent_edit() {
    use axum::{Router, http::StatusCode, routing};
    let (store, old) = fixture();
    let store = Arc::new(store);
    let concurrent = store.clone();
    let app = Router::new().route(
        "/mcp",
        routing::post(move || {
            update_connectors(&concurrent, |all| {
                all[0].description = "concurrent".into();
                Ok(())
            })
            .unwrap();
            async { StatusCode::UNAUTHORIZED }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let error = update_connector(&store, &json!({"id":old.id,"url":url,"headers":{}})).await.unwrap_err();
    assert!(error.to_string().contains("changed during"));
    let current = connectors(&store).unwrap().remove(0);
    assert_eq!(current.description, "concurrent");
    assert_eq!(current.url, old.url);
    assert_eq!(current.oauth, old.oauth);
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn http_edit_stays_saved_when_sign_in_is_needed_and_preserves_bot_selections() {
    use crate::{hub::Hub, remote::identity::Identity};
    let dir = std::env::temp_dir().join(format!("codync-remote-edit-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let (store, mut old) = fixture();
    old.oauth = Some(oauth::OAuth::default());
    save_connectors(&store, &[old]).unwrap();
    let hub = Hub::new(store, "test".into(), Identity::load_or_create(&dir).unwrap(), "token".into(), 0);
    for (id, connectors) in [("uses-remote", vec!["remote"]), ("unrelated", vec![])] {
        hub.store
            .save_bot(
                &serde_json::from_value(
                    json!({"id":id,"name":id,"backend":"claude","cwd":dir,"connectors":connectors}),
                )
                .unwrap(),
            )
            .unwrap();
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = crate::api::router(hub.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, router.into_make_service_with_connect_info::<std::net::SocketAddr>()).await.unwrap();
    });
    let response = reqwest::Client::new()
        .post(format!("http://{address}/api/updateConnector"))
        .bearer_auth("token")
        .json(&json!({"id":" remote ","description":"saved despite verification","headers":{"X-New":"private"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let error = response.text().await.unwrap();
    assert!(error.contains("Sign in"));
    assert!(!error.contains("private"));
    let updated = connectors(&hub.store).unwrap().remove(0);
    assert_eq!(updated.description, "saved despite verification");
    assert_eq!(updated.headers["X-New"], "private");
    assert_eq!(hub.store.bot("uses-remote").unwrap().unwrap().config.connectors, ["remote"]);
    assert!(hub.store.bot("unrelated").unwrap().unwrap().config.connectors.is_empty());
    server.abort();
    let _ = server.await;
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn transport_fields_and_registry_definitions_cannot_be_changed_through_the_wrong_path() {
    let (store, mut c) = fixture();
    c.registry_name = Some("test/remote".into());
    save_connectors(&store, std::slice::from_ref(&c)).unwrap();
    assert!(
        update_connector(&store, &json!({"id":c.id,"headers":{"X-New":"value"}}))
            .await
            .unwrap_err()
            .to_string()
            .contains("MCP registry")
    );
    c.registry_name = None;
    c.command = Some("node".into());
    c.url = None;
    c.oauth = None;
    save_connectors(&store, std::slice::from_ref(&c)).unwrap();
    for patch in [json!({"id":c.id,"url":"https://new.example/mcp"}), json!({"id":c.id,"headers":{}})] {
        assert!(update_connector(&store, &patch).await.unwrap_err().to_string().contains("local connector"));
        assert_eq!(connectors(&store).unwrap().as_slice(), std::slice::from_ref(&c));
    }
}
