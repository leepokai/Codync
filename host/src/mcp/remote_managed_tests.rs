#![allow(clippy::unwrap_used)]
//! Managed remotes use a different short grant on every target lookup.
use super::*;
use crate::market::{Connector, connectors, save_connectors, update_connectors};
use crate::store::Store;
use axum::{Json, Router, http::HeaderMap, routing};
use std::sync::{Arc, Mutex};

struct Managed {
    store: Arc<Store>,
    keeper: crate::keeper::tests::TestKeeper,
    port: u16,
    urls: [String; 2],
    seen: Arc<Mutex<Vec<(String, HeaderMap)>>>,
    servers: [tokio::task::JoinHandle<()>; 2],
}

impl Managed {
    async fn new() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let next = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let urls = [format!("http://{address}/old"), format!("http://{}/new", next.local_addr().unwrap())];
        let keeper = crate::keeper::tests::test_keeper();
        let store = Arc::new(Store::open(std::path::Path::new(":memory:")).unwrap());
        keeper.manage(&store);
        let connector: Connector = serde_json::from_value(json!({
            "id":"remote", "name":"Remote", "url":urls[0],
            "headers":{"X-Key":"first", "X-Other":"retained"}, "authRevision":"before"
        }))
        .unwrap();
        save_connectors(&store, &[connector]).unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let received = seen.clone();
        let sink = move |uri: axum::http::Uri, headers: HeaderMap| {
            received.lock().unwrap().push((uri.path().to_owned(), headers));
            async { Json(json!({"jsonrpc":"2.0","id":1,"result":{}})) }
        };
        let read_store = store.clone();
        let app = Router::new()
            .route(
                "/api/connectorTarget",
                routing::post(move || {
                    let store = read_store.clone();
                    async move { Json(crate::market::oauth::target(&store, "remote", false).await.unwrap()) }
                }),
            )
            .route("/old", routing::post(sink.clone()));
        let next_app = Router::new().route("/new", routing::post(sink));
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let next_server = tokio::spawn(async move { axum::serve(next, next_app).await.unwrap() });
        Self { store, keeper, port: address.port(), urls, seen, servers: [server, next_server] }
    }

    fn remote(&self) -> Remote {
        Remote { port: self.port, token: "fixture".into(), connector: "remote".into(), endpoint: Mutex::new(None) }
    }

    async fn send(&self, remote: &Remote) -> Result<()> {
        remote.forward(&json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}), &mut None).await?;
        Ok(())
    }
}

impl Drop for Managed {
    fn drop(&mut self) {
        for server in &self.servers {
            server.abort();
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn managed_grant_rotation_does_not_change_the_connector_identity() {
    let fixture = Managed::new().await;
    let remote = fixture.remote();
    for _ in 0..3 {
        fixture.send(&remote).await.unwrap();
    }
    let seen = fixture.seen.lock().unwrap();
    assert_eq!(seen.len(), 3);
    assert!(seen.iter().all(|(path, headers)| path == "/old" && headers["x-key"] == "first"));
}

#[tokio::test(flavor = "multi_thread")]
async fn managed_header_removal_replaces_the_keeper_record() {
    let fixture = Managed::new().await;
    let old = fixture.remote();
    fixture.send(&old).await.unwrap();
    update_connectors(&fixture.store, |all| {
        all[0].headers.remove("X-Key");
        all[0].auth_revision = "removed-one".into();
        Ok(())
    })
    .unwrap();
    let record = fixture.keeper.keeper.record("connector:remote").unwrap().unwrap();
    assert!(!record.headers.contains_key("x-key"));
    assert_eq!(record.headers["x-other"], "retained");
    assert!(fixture.send(&old).await.is_err(), "a saved credential edit invalidates an old proxy");
    fixture.send(&fixture.remote()).await.unwrap();
    {
        let seen = fixture.seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert!(!seen[1].1.contains_key("x-key"));
        assert_eq!(seen[1].1["x-other"], "retained");
    }
    update_connectors(&fixture.store, |all| {
        all[0].headers.clear();
        all[0].auth_revision = "removed-all".into();
        Ok(())
    })
    .unwrap();
    fixture.send(&fixture.remote()).await.unwrap();
    let seen = fixture.seen.lock().unwrap();
    assert_eq!(seen.len(), 3);
    assert!(!seen[2].1.contains_key("x-key") && !seen[2].1.contains_key("x-other"));
}

#[tokio::test(flavor = "multi_thread")]
async fn managed_url_change_without_credentials_never_uses_the_previous_target() {
    let fixture = Managed::new().await;
    let old = fixture.remote();
    fixture.send(&old).await.unwrap();
    update_connectors(&fixture.store, |all| {
        // Both origin and path change; the old origin must receive no further request.
        all[0].url = Some(fixture.urls[1].clone());
        all[0].headers.clear();
        all[0].auth_revision = "moved".into();
        Ok(())
    })
    .unwrap();
    assert!(fixture.send(&old).await.is_err());
    fixture.send(&fixture.remote()).await.unwrap();
    let seen = fixture.seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[1].0, "/new");
    assert!(!seen[1].1.contains_key("x-key") && !seen[1].1.contains_key("x-other"));
    assert_eq!(connectors(&fixture.store).unwrap()[0].headers, std::collections::BTreeMap::new());
}
