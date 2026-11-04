use super::*;
use axum::{Json, Router, http::HeaderMap, routing};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn old_proxies_never_send_new_credentials_to_cached_endpoints_or_sessions() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let old_url = format!("http://{address}/old");
    let new_url = format!("http://{address}/new");
    let target = Arc::new(Mutex::new(json!({"url":old_url,"headers":{"X-Key":"old-key"},"authRevision":"before"})));
    let read_target = target.clone();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let received = requests.clone();
    let sink = move |headers: HeaderMap| {
        received.lock().unwrap().push(headers);
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
        *target.lock().unwrap() = json!({"url":old_url,"headers":{"X-Key":"old-key"},"authRevision":"before"});
        let remote = Remote {
            port: address.port(),
            token: "fixture".into(),
            connector: "remote".into(),
            endpoint: Mutex::new(None),
        };
        remote.target(false).await.unwrap();
        *target.lock().unwrap() = json!({"url":replacement,"headers":{"X-Key":"new-key"},"authRevision":"after"});
        let message = json!({"jsonrpc":"2.0","id":1,"method":"tools/list"});
        let mut session = Some("old-private-session".into());
        assert!(
            remote
                .forward(&message, &mut session)
                .await
                .unwrap_err()
                .to_string()
                .contains("connection settings changed")
        );
        assert!(
            remote
                .post_legacy(&old_url, &message)
                .await
                .unwrap_err()
                .to_string()
                .contains("connection settings changed")
        );
        assert!(requests.lock().unwrap().is_empty(), "no request to either MCP endpoint");
        // A newly started proxy can use the edited endpoint and its explicit credentials.
        let restarted = Remote { endpoint: Mutex::new(None), ..remote };
        restarted.forward(&message, &mut None).await.unwrap();
        let mut seen = requests.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0]["x-key"], "new-key");
        assert!(!seen[0].contains_key("mcp-session-id"));
        seen.clear();
    }
    server.abort();
    let _ = server.await;
}
