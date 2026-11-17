use super::*;
use axum::{Json, Router, http::HeaderMap, routing};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn a_running_proxy_follows_an_edit_without_the_old_session_but_never_to_a_cached_endpoint() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let old_url = format!("http://{address}/old");
    let new_url = format!("http://{address}/new");
    let target = Arc::new(Mutex::new(Value::Null));
    let read_target = target.clone();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let received = requests.clone();
    let sink = move |uri: axum::http::Uri, headers: HeaderMap| {
        received.lock().unwrap().push((uri.path().to_owned(), headers));
        async { Json(json!({"jsonrpc":"2.0","id":1,"result":{}})) }
    };
    let app = Router::new()
        .route(
            "/api/connectorTarget",
            routing::post(move || {
                let value = read_target.lock().unwrap().clone();
                async { Json(value) }
            }),
        )
        .route("/old", routing::post(sink.clone()))
        .route("/new", routing::post(sink));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    for replacement in [&old_url, &new_url] {
        *target.lock().unwrap() =
            json!({"url":old_url,"connectorUrl":old_url,"headers":{"X-Key":"old-key"},"authRevision":"before"});
        let remote = Remote {
            port: address.port(),
            token: "fixture".into(),
            connector: "remote".into(),
            endpoint: Mutex::new(None),
        };
        remote.target(false, Edited::Adopt).await.unwrap();
        *target.lock().unwrap() =
            json!({"url":replacement,"connectorUrl":replacement,"headers":{"X-Key":"new-key"},"authRevision":"after"});
        let message = json!({"jsonrpc":"2.0","id":1,"method":"tools/list"});
        // The older transport's endpoint was named by the old server: refused, nothing sent.
        assert!(
            remote
                .post_legacy(&Endpoint::Url(old_url.clone()), &message)
                .await
                .unwrap_err()
                .to_string()
                .contains("connection settings changed")
        );
        assert!(requests.lock().unwrap().is_empty(), "nothing to the cached endpoint");
        // Streamable HTTP goes where the host says now, with the new key and without the old server's session.
        let mut session = Some("old-private-session".to_owned());
        remote.forward(&message, &mut session).await.unwrap();
        let mut seen = requests.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, reqwest::Url::parse(replacement).unwrap().path());
        assert_eq!(seen[0].1["x-key"], "new-key");
        assert!(!seen[0].1.contains_key("mcp-session-id"));
        assert_eq!(session, None);
        seen.clear();
    }
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn a_server_that_needs_its_session_asks_for_a_restart_after_an_edit() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}/mcp");
    let target = Arc::new(Mutex::new(json!({"url":url,"connectorUrl":url,"headers":{},"authRevision":"before"})));
    let read_target = target.clone();
    // As the SDKs' stateful servers do: a request without the session `initialize` opened is a bad request.
    let stateful = |headers: HeaderMap| async move {
        if headers.contains_key("mcp-session-id") {
            (axum::http::StatusCode::OK, Json(json!({"jsonrpc":"2.0","id":1,"result":{}})))
        } else {
            (axum::http::StatusCode::BAD_REQUEST, Json(json!({"error":"No valid session ID provided"})))
        }
    };
    let app = Router::new()
        .route(
            "/api/connectorTarget",
            routing::post(move || {
                let value = read_target.lock().unwrap().clone();
                async { Json(value) }
            }),
        )
        .route("/mcp", routing::post(stateful));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let remote = Remote {
        port: address.port(),
        token: "fixture".into(),
        connector: "remote".into(),
        endpoint: Mutex::new(None),
    };
    let message = json!({"jsonrpc":"2.0","id":1,"method":"tools/list"});
    let mut session = Some("opened".to_owned());
    remote.forward(&message, &mut session).await.unwrap();
    *target.lock().unwrap() =
        json!({"url":url,"connectorUrl":url,"headers":{"X-Key":"new-key"},"authRevision":"after"});
    // Not taken for the older transport: it says what to do, every time until the tools restart.
    for _ in 0..2 {
        assert_eq!(remote.forward(&message, &mut session).await.unwrap_err().to_string(), SETTINGS_CHANGED);
    }
    server.abort();
    let _ = server.await;
}
