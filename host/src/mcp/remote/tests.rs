use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn tool_transport_never_follows_a_redirect_with_the_approved_arguments() {
    use axum::{Router, routing::post};
    let count = Arc::new(AtomicUsize::new(0));
    let target = count.clone();
    let router = Router::new()
        .route(
            "/redirect",
            post(|| async {
                (axum::http::StatusCode::TEMPORARY_REDIRECT, [(axum::http::header::LOCATION, "/target")])
            }),
        )
        .route(
            "/target",
            post(move || {
                let target = target.clone();
                async move {
                    target.fetch_add(1, Ordering::SeqCst);
                    "unexpected"
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let reply = client()
        .post(format!("http://{address}/redirect"))
        .json(&json!({"secret":"approved arguments"}))
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status().as_u16(), 307);
    assert_eq!(count.load(Ordering::SeqCst), 0);
    server.abort();
}

#[test]
fn legacy_endpoints_follow_each_grant() {
    let at = |base: Option<&str>, path: &str| {
        Endpoint::named("http://127.0.0.1:1/t/old/sse", base, path)
            .unwrap()
            .url(Some("http://127.0.0.1:1/t/new"))
            .unwrap()
    };
    let grant = Some("http://127.0.0.1:1/t/old");
    assert_eq!(at(grant, "/messages?sessionId=1"), "http://127.0.0.1:1/t/new/messages?sessionId=1");
    assert_eq!(at(grant, "messages?sessionId=1"), "http://127.0.0.1:1/t/new/messages?sessionId=1");
    assert_eq!(at(None, "/messages"), "http://127.0.0.1:1/messages", "any other computer: as the server named it");
}

#[test]
fn a_legacy_endpoint_stays_on_the_connectors_origin() {
    assert!(Endpoint::named("https://example.com/sse", None, "https://evil.example/messages").is_err());
    assert!(Endpoint::named("https://example.com/sse", None, "/messages").is_ok());
}
