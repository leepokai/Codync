#![allow(clippy::unwrap_used)]
use super::*;
use crate::keeper::{client, record::Keep, tests::test_keeper};
use crate::store::Store;
use axum::{Json, Router, routing};
use std::collections::BTreeMap;
use std::sync::Mutex;
use tokio::sync::Notify;

#[tokio::test(flavor = "multi_thread")]
async fn a_remote_edit_during_refresh_never_sends_replacement_credentials_to_the_old_url() {
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let old_seen = Arc::new(Mutex::new(Vec::new()));
    let capture = old_seen.clone();
    let token_started = started.clone();
    let token_release = release.clone();
    let app = Router::new()
        .route(
            "/mcp",
            routing::post(move |headers: HeaderMap| {
                capture.lock().unwrap().push(headers);
                async { Json(serde_json::json!({"ok":true})) }
            }),
        )
        .route(
            "/token",
            routing::post(move || {
                let started = token_started.clone();
                let release = token_release.clone();
                async move {
                    started.notify_one();
                    release.notified().await;
                    Json(serde_json::json!({"access_token":"rotated-old-token", "expires_in":3600}))
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let old_origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let new_seen = Arc::new(Mutex::new(Vec::new()));
    let capture = new_seen.clone();
    let next_app = Router::new().route(
        "/mcp",
        routing::post(move |headers: HeaderMap| {
            capture.lock().unwrap().push(headers);
            async { Json(serde_json::json!({"ok":true})) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let new_origin = format!("http://{}", listener.local_addr().unwrap());
    let next_server = tokio::spawn(async move { axum::serve(listener, next_app).await.unwrap() });
    let keeper = test_keeper();
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    keeper.manage(&store);
    let mut original = Record::bearer(&old_origin, "old-token");
    original.oauth = Some(record::OAuth {
        token_url: format!("{old_origin}/token"),
        client_id: "fixture".into(),
        client_secret: None,
        refresh_token: Some("old-refresh".into()),
        expires_at: 1,
        header: "authorization".into(),
        resource: Some(format!("{old_origin}/mcp")),
    });
    client::put_proxied(&store, "connector:race", &original, &Keep::default()).unwrap();
    let grant = client::short_grant(&store, "connector:race").unwrap();
    let pending_url = format!("{grant}/mcp");
    let pending =
        tokio::spawn(async move { reqwest::Client::new().post(pending_url).body("{}").send().await.unwrap().status() });
    tokio::time::timeout(Duration::from_secs(5), started.notified()).await.unwrap();
    let replacement = Record::header(&new_origin, "x-new", "replacement-secret".into());
    client::put_proxied(&store, "connector:race", &replacement, &Keep::default()).unwrap();
    release.notify_one();
    let status = tokio::time::timeout(Duration::from_secs(5), pending).await.unwrap().unwrap();
    assert_eq!(status, StatusCode::UNAUTHORIZED, "the in-flight request must stop after its connection changes");
    assert_eq!(old_seen.lock().unwrap().len(), 0, "no replacement credential reaches the previous origin");
    assert_eq!(new_seen.lock().unwrap().len(), 0, "no old request is retargeted to the replacement");
    assert!(keeper.keeper.record("connector:race").unwrap().unwrap() == replacement);
    assert_eq!(
        reqwest::Client::new().post(format!("{grant}/mcp")).body("{}").send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let fresh = client::short_grant(&store, "connector:race").unwrap();
    assert_eq!(
        reqwest::Client::new().post(format!("{fresh}/mcp")).body("{}").send().await.unwrap().status(),
        StatusCode::OK
    );
    assert_eq!(new_seen.lock().unwrap()[0]["x-new"], "replacement-secret");
    server.abort();
    next_server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn replacing_a_record_while_reading_the_body_revokes_the_request() {
    let keeper = test_keeper();
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    keeper.manage(&store);
    let original = Record::new("https://old.example", BTreeMap::new());
    client::put_proxied(&store, "connector:body", &original, &Keep::default()).unwrap();
    let grant = keeper.keeper.grant("connector:body", None).unwrap();
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let polled = started.clone();
    let wait = release.clone();
    let body = Body::from_stream(futures::stream::once(async move {
        polled.notify_one();
        wait.notified().await;
        Ok::<_, std::io::Error>("{}")
    }));
    let request =
        axum::http::Request::builder().method("POST").uri(format!("/t/{}/mcp", grant.token)).body(body).unwrap();
    let proxy = keeper.keeper.clone();
    let pending = tokio::spawn(async move { forward(&proxy, request).await });
    tokio::time::timeout(Duration::from_secs(5), started.notified()).await.unwrap();
    client::put_proxied(
        &store,
        "connector:body",
        &Record::new("https://new.example", BTreeMap::new()),
        &Keep::default(),
    )
    .unwrap();
    release.notify_one();
    let result = tokio::time::timeout(Duration::from_secs(5), pending).await.unwrap().unwrap();
    assert_eq!(result.unwrap_err(), StatusCode::UNAUTHORIZED, "no request should leave the keeper");
}

#[test]
fn unchanged_connector_records_and_provider_updates_preserve_their_grants() {
    let keeper = test_keeper();
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    keeper.manage(&store);
    let original = Record::header("https://example.com", "x-key", "one".into());
    client::put_proxied(&store, "connector:same", &original, &Keep::default()).unwrap();
    let grant = keeper.keeper.grant("connector:same", None).unwrap();
    client::put_proxied(
        &store,
        "connector:same",
        &Record::new("https://example.com", BTreeMap::new()),
        &Keep { headers: vec!["x-key".into()], oauth: false },
    )
    .unwrap();
    assert_eq!(keeper.keeper.granted(&grant.token).as_deref(), Some("connector:same"));

    client::put_proxied(&store, "agent:provider", &original, &Keep::default()).unwrap();
    let provider = keeper.keeper.grant("agent:provider", None).unwrap();
    client::put_proxied(
        &store,
        "agent:provider",
        &Record::header("https://example.com", "x-key", "two".into()),
        &Keep::default(),
    )
    .unwrap();
    assert_eq!(
        keeper.keeper.granted(&provider.token).as_deref(),
        Some("agent:provider"),
        "connector edits do not change long-lived provider grant behavior"
    );
}
