use super::*;

#[test]
fn form_values_are_encoded() {
    assert_eq!(encode("a b&c=d/+"), "a+b%26c%3Dd%2F%2B");
}

/// A sign-in to "notes" at `https://notes.example/mcp`, waiting for its code under `state`.
fn pend(state: &str) {
    let pending = Pending {
        connector: "notes".into(),
        verifier: "v".into(),
        redirect_uri: loopback_uri(1),
        resource: "https://notes.example/mcp".into(),
        started: Instant::now(),
    };
    PENDING.locked().get_or_insert_with(HashMap::new).insert(state.into(), pending);
}

#[tokio::test]
async fn unknown_state_is_refused() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let err = finish(&store, "nope", Some("code"), None).await.unwrap_err();
    assert!(err.to_string().contains("expired"));
}

#[tokio::test]
async fn a_connector_replaced_during_sign_in_gets_no_code() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    // Removed while the sign-in page was open, and another server added under the same id.
    let c: Connector = serde_json::from_value(json!({
        "id": "notes", "name": "Notes", "url": "https://other.example/mcp",
        "oauth": {"clientId": "theirs", "tokenEndpoint": "http://127.0.0.1:1/token"},
    }))
    .unwrap();
    market::save_connectors(&store, &[c]).unwrap();
    pend("replaced");
    let err = finish(&store, "replaced", Some("code"), None).await.unwrap_err();
    assert_eq!(err.to_string(), "unknown connector");
}

/// Connector "notes" signed in with token "old" (refresh token "r1") until `expires_at`,
/// refreshing at `token_endpoint`.
fn notes(token_endpoint: &str, expires_at: u64) -> Store {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let c: Connector = serde_json::from_value(json!({
        "id": "notes", "name": "Notes", "url": "https://notes.example/mcp",
        "oauth": {"clientId": "ours", "tokenEndpoint": token_endpoint,
                  "accessToken": "old", "refreshToken": "r1", "expiresAt": expires_at},
    }))
    .unwrap();
    market::save_connectors(&store, &[c]).unwrap();
    store
}

/// `notes` with an expired token, refreshing at a local token endpoint
/// that answers `answer(form body)` as (status, JSON).
async fn expired_with(answer: fn(&str) -> (u16, Value)) -> Store {
    use axum::{Router, http::StatusCode, routing};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let store = notes(&format!("http://{}/token", listener.local_addr().unwrap()), 1);
    let router = Router::new().route(
        "/token",
        routing::post(move |body: String| async move {
            let (status, v) = answer(&body);
            (StatusCode::from_u16(status).unwrap(), axum::Json(v))
        }),
    );
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    store
}

fn saved(store: &Store) -> OAuth {
    market::connectors(store).unwrap().remove(0).oauth.unwrap()
}

#[tokio::test]
async fn a_temporary_refresh_failure_keeps_the_grant() {
    let store = expired_with(|_| (503, json!({}))).await;
    assert!(target(&store, "notes", false).await.is_err());
    let o = saved(&store);
    assert!(o.signed_in(), "a 503 signed the connector out");
    assert_eq!(o.refresh_token.as_deref(), Some("r1"));
}

#[tokio::test]
async fn a_refresh_names_the_resource() {
    let store = expired_with(|body| {
        if body.contains("resource=https%3A%2F%2Fnotes.example%2Fmcp") {
            (200, json!({"access_token": "new", "expires_in": 3600}))
        } else {
            (400, json!({"error": "invalid_target"}))
        }
    })
    .await;
    let t = target(&store, "notes", false).await.unwrap();
    assert_eq!(t["headers"]["Authorization"], "Bearer new");
}

#[tokio::test]
async fn a_revoked_grant_signs_out() {
    let store = expired_with(|_| (400, json!({"error": "invalid_grant"}))).await;
    assert!(target(&store, "notes", false).await.is_err());
    assert!(!saved(&store).signed_in());
}

#[tokio::test]
async fn an_unknown_client_registers_again() {
    let store = expired_with(|_| (401, json!({"error": "invalid_client"}))).await;
    assert!(target(&store, "notes", false).await.is_err());
    let o = saved(&store);
    assert!(!o.signed_in());
    assert!(o.client_id.is_empty(), "the next sign-in reuses a client the server dropped");
}

#[tokio::test]
async fn an_unreachable_token_endpoint_keeps_the_grant() {
    let store = notes("http://127.0.0.1:1/token", 1);
    assert!(target(&store, "notes", false).await.is_err());
    let o = saved(&store);
    assert!(o.signed_in(), "a dropped connection signed the connector out");
    assert_eq!(o.refresh_token.as_deref(), Some("r1"));
}

#[tokio::test]
async fn a_token_about_to_expire_works_while_the_server_is_away() {
    let store = notes("http://127.0.0.1:1/token", now() + EARLY / 2);
    let t = target(&store, "notes", false).await.unwrap();
    assert_eq!(t["headers"]["Authorization"], "Bearer old");
    assert!(target(&store, "notes", true).await.is_err(), "a token the server refused was sent again");
}

#[test]
fn refresh_answers_are_read_as_rfc_6749_says() {
    let read = |status: u16, v: Value| renewal(reqwest::StatusCode::from_u16(status).unwrap(), &v);
    assert_eq!(read(200, json!({"access_token": "new"})), Renewal::Renewed);
    for (status, v) in [
        (503, json!({})),
        (502, json!({"error": "invalid_grant"})),
        (429, json!({})),
        (408, Value::Null),
        (403, Value::Null),
        (400, json!({"error": "temporarily_unavailable"})),
        (200, Value::Null),
    ] {
        assert_eq!(read(status, v.clone()), Renewal::Retry, "{status} {v}");
    }
    for (status, v) in [
        (400, json!({"error": "invalid_grant"})),
        (400, json!({"error": "invalid_request"})),
        (403, json!({"error": "unauthorized_client"})),
        (401, json!({"error": "invalid_token"})),
        (200, json!({"error": "invalid_scope"})),
    ] {
        assert_eq!(read(status, v.clone()), Renewal::Revoked, "{status} {v}");
    }
    assert_eq!(read(401, json!({"error": "invalid_client"})), Renewal::Unregistered);
}

#[tokio::test]
async fn the_token_is_saved_only_while_the_connector_is_unchanged() {
    use axum::{Json, Router, routing};
    use std::sync::atomic::{AtomicBool, Ordering};
    let store = Arc::new(Store::open(std::path::Path::new(":memory:")).unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let token_endpoint = format!("http://{}/token", listener.local_addr().unwrap());
    let original: Connector = serde_json::from_value(json!({
        "id": "notes", "name": "Notes", "url": "https://notes.example/mcp",
        "oauth": {"clientId": "ours", "tokenEndpoint": token_endpoint},
    }))
    .unwrap();
    market::save_connectors(&store, &[original]).unwrap();
    // The second answer comes only after the connector was removed and another added under its id.
    let shared = store.clone();
    let answered = Arc::new(AtomicBool::new(false));
    let router = Router::new().route(
        "/token",
        routing::post(move || async move {
            if answered.swap(true, Ordering::SeqCst) {
                let other: Connector = serde_json::from_value(json!({
                    "id": "notes", "name": "Notes", "url": "https://other.example/mcp",
                }))
                .unwrap();
                market::save_connectors(&shared, &[other]).unwrap();
            }
            Json(json!({"access_token": "notes-token"}))
        }),
    );
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    pend("unchanged");
    let saved = finish(&store, "unchanged", Some("code"), None).await.unwrap();
    assert_eq!(saved.oauth.unwrap().access_token, "notes-token");
    pend("replaced-mid-request");
    let err = finish(&store, "replaced-mid-request", Some("code"), None).await.unwrap_err();
    assert_eq!(err.to_string(), "unknown connector");
    let left = market::connectors(&store).unwrap();
    assert_eq!(left[0].url.as_deref(), Some("https://other.example/mcp"));
    assert!(left[0].oauth.is_none(), "the replacement got the token");
}

/// A registration server: `/register` accepts only the loopback redirect URI, as servers that
/// follow the native app rule do (issue #440), and repeats its reason for anything else; `/down`
/// always answers 503, `/closed` 401 (it wants an initial access token); `/mcp` asks for sign-in and the authorization server metadata points here.
/// The base URL, and how many registrations it has seen.
async fn registration_server() -> (String, Arc<std::sync::atomic::AtomicUsize>) {
    use axum::{Json, Router, http::StatusCode, routing};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let seen = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (on_register, on_down, on_closed, origin) = (seen.clone(), seen.clone(), seen.clone(), base.clone());
    let router = Router::new()
        .route("/mcp", routing::post(|| async { StatusCode::UNAUTHORIZED }))
        .route(
            "/.well-known/oauth-authorization-server",
            routing::get(move || async move {
                Json(json!({
                    "issuer": origin,
                    "authorization_endpoint": format!("{origin}/authorize"),
                    "token_endpoint": format!("{origin}/token"),
                    "registration_endpoint": format!("{origin}/register"),
                }))
            }),
        )
        .route(
            "/register",
            routing::post(move |body: String| async move {
                on_register.fetch_add(1, Ordering::SeqCst);
                let v: Value = serde_json::from_str(&body).unwrap();
                let uris = v["redirect_uris"].as_array().unwrap();
                if uris.len() == 1 && uris[0].as_str().unwrap().starts_with("http://127.0.0.1:") {
                    (StatusCode::CREATED, Json(json!({"client_id": "loopback-only"})))
                } else {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(json!({
                            "error": "invalid_redirect_uri",
                            "error_description": "One or more redirect URIs are invalid"
                        })),
                    )
                }
            }),
        )
        .route(
            "/down",
            routing::post(move || async move {
                on_down.fetch_add(1, Ordering::SeqCst);
                StatusCode::SERVICE_UNAVAILABLE
            }),
        )
        .route(
            "/closed",
            routing::post(move || async move {
                on_closed.fetch_add(1, Ordering::SeqCst);
                (StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid_token"})))
            }),
        );
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (base, seen)
}

fn wanted(loopback: &str) -> Vec<String> {
    vec![loopback.to_owned(), "https://cloud.example/v1/oauth/callback".to_owned()]
}

#[tokio::test]
async fn a_server_that_only_takes_the_loopback_uri_still_signs_in_here() {
    let (base, seen) = registration_server().await;
    let endpoint = format!("{base}/register");
    let loopback = loopback_uri(19222);

    let mut oauth = OAuth::default();
    register_client(&endpoint, wanted(&loopback), loopback.clone(), Callback::Host, &mut oauth).await.unwrap();
    assert_eq!(oauth.client_id, "loopback-only");
    assert!(oauth.client_secret.is_none());
    assert_eq!(oauth.redirect_uris, vec![loopback], "the client is registered for the URI it uses");
    assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_phone_sign_in_the_server_refuses_registers_nothing_else() {
    let (base, seen) = registration_server().await;
    let loopback = loopback_uri(19222);

    let endpoint = format!("{base}/register");
    let err = register_client(&endpoint, wanted(&loopback), loopback, Callback::App, &mut OAuth::default())
        .await
        .unwrap_err();
    let text = format!("{err:#}");
    assert!(text.contains("sign in there"), "{text}");
    assert!(text.contains("One or more redirect URIs are invalid"), "the server's reason is shown: {text}");
    assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 1, "no loopback-only client left behind");
}

#[tokio::test]
async fn a_server_that_is_down_gets_no_second_registration() {
    let (base, seen) = registration_server().await;
    let loopback = loopback_uri(19222);

    let endpoint = format!("{base}/down");
    let err = register_client(&endpoint, wanted(&loopback), loopback, Callback::Host, &mut OAuth::default())
        .await
        .unwrap_err();
    assert!(format!("{err:#}").contains("503"), "{err:#}");
    assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 1, "only a refusal falls back");
}

#[tokio::test]
async fn a_refusal_about_something_else_gets_no_fallback_or_phone_advice() {
    let (base, seen) = registration_server().await;
    let endpoint = format!("{base}/closed");
    let loopback = loopback_uri(19222);

    for callback in [Callback::Host, Callback::App] {
        let err = register_client(&endpoint, wanted(&loopback), loopback.clone(), callback, &mut OAuth::default())
            .await
            .unwrap_err();
        let text = format!("{err:#}");
        assert!(text.contains("refused the registration (401 Unauthorized: invalid_token)"), "{text}");
    }
    assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 2, "one registration per sign-in");
}

#[tokio::test]
async fn signing_in_on_the_computer_with_the_cloud_on_falls_back_to_the_loopback_uri() {
    let (base, seen) = registration_server().await;
    let dir = std::env::temp_dir().join(format!("codync-oauth-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let c: Connector =
        serde_json::from_value(json!({"id": "notes", "name": "Notes", "url": format!("{base}/mcp")})).unwrap();
    market::save_connectors(&store, &[c]).unwrap();
    store.kv_set("test_cloud_url", "https://cloud.example").unwrap();
    assert!(app_uri(&store).is_some(), "the cloud is on, so both redirect URIs are wanted");
    let identity = crate::remote::identity::Identity::load_or_create(&dir).unwrap();
    let hub = Hub::new(store, "h".into(), identity, "t".into(), 19222);

    let v = start(&hub, "notes", Callback::Host, true).await.unwrap();
    let url = v["url"].as_str().unwrap();
    assert!(url.contains("client_id=loopback-only"), "{url}");
    assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A19222%2Foauth%2Fcallback"), "{url}");
    let saved = market::connectors(&hub.store).unwrap().remove(0).oauth.unwrap();
    assert_eq!(saved.redirect_uris, vec![loopback_uri(19222)]);
    assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 2);

    start(&hub, "notes", Callback::Host, true).await.unwrap();
    assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 2, "the loopback-only client is reused");
    let err = start(&hub, "notes", Callback::App, true).await.unwrap_err();
    assert!(format!("{err:#}").contains("sign in there"), "{err:#}");
    assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 3, "a phone sign-in can't reuse it");
    std::fs::remove_dir_all(&dir).ok();
}
