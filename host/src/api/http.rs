//! HTTP routes: bearer-token checks for this computer's apps, SSE streams, the channel
//! upgrade, OAuth callbacks and local routine webhooks.

use super::devices::Caller;
use super::{dispatch, error_status, events_stream, term_events};
use crate::hub::Hub;
use crate::remote::crypto;
use crate::{market, usage};
use axum::body::Bytes;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::{Stream, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

pub fn router(hub: Arc<Hub>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/hooks/routines/{id}", post(routine_hook))
        .route("/events", get(events))
        .route("/api/{method}", post(command))
        .route("/api/setIcon", post(icon_command).layer(axum::extract::DefaultBodyLimit::max(8 * 1024 * 1024)))
        .route("/term/{id}", get(term_stream))
        .route("/ingest/statusline", post(statusline))
        .route("/channel", get(channel))
        .route("/oauth/callback", get(oauth_callback))
        .with_state(hub)
}

async fn icon_command(
    State(hub): State<Arc<Hub>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !authorized(&hub, &headers, peer) {
        return Err(unauthorized());
    }
    dispatch(&hub, &Caller::Local, "setIcon", body).await.map(Json).map_err(|e| {
        let (status, message) = error_status(&e);
        ApiError(status, message)
    })
}

/// Loopback, also as an IPv4-mapped IPv6 address (`::ffff:127.0.0.1` under `--bind ::`).
fn is_loopback(ip: IpAddr) -> bool {
    ip.to_canonical().is_loopback()
}

/// The bearer token only works from this computer; phones use the E2E channel.
fn authorized(hub: &Hub, headers: &HeaderMap, peer: SocketAddr) -> bool {
    let given = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default();
    is_loopback(peer.ip()) && crypto::ct_eq(given.as_bytes(), hub.token.as_bytes())
}

fn unauthorized() -> ApiError {
    ApiError(StatusCode::UNAUTHORIZED, "invalid token".into())
}

struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}

#[derive(Default, Deserialize)]
struct HealthQuery {
    /// Sent by the desktop app's own poll: it's open to carry out an update request.
    #[serde(default)]
    app: bool,
}

/// `GET /health`, open to the network. Where the binary lives (a path with the user's name) and whether
/// a turn is running are for this computer's updater and desktop app only.
async fn health(
    State(hub): State<Arc<Hub>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Query(q): Query<HealthQuery>,
) -> Json<Value> {
    let mut health = json!({
        "ok": true,
        "hostId": hub.host_id,
        "computerId": hub.identity.computer_id(),
        "version": env!("CARGO_PKG_VERSION"),
        "environment": crate::environment::Environment::current().name(),
    });
    if is_loopback(peer.ip()) {
        if q.app {
            crate::update::note_app_poll();
        }
        let binary = crate::service::binary_identity();
        health["binaryPath"] = json!(binary.map(|(path, _)| path));
        health["binaryHash"] = json!(binary.map(|(_, hash)| hash));
        health["busy"] = hub.busy().into();
        health["updateRequested"] = crate::update::app_requested().into();
    }
    Json(health)
}

async fn statusline(
    State(hub): State<Arc<Hub>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    if !authorized(&hub, &headers, peer) {
        return StatusCode::UNAUTHORIZED;
    }
    let Ok(body) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST;
    };
    usage::ingest_statusline(&hub, &body);
    StatusCode::NO_CONTENT
}

async fn command(
    State(hub): State<Arc<Hub>>,
    Path(method): Path<String>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    // The token is checked before the body is even looked at.
    if !authorized(&hub, &headers, peer) {
        return Err(unauthorized());
    }
    let body = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("invalid JSON: {e}")))?
    };
    dispatch(&hub, &Caller::Local, &method, body).await.map(Json).map_err(|e| {
        let (status, message) = error_status(&e);
        ApiError(status, message)
    })
}

#[derive(Deserialize)]
struct OAuthCallback {
    state: Option<String>,
    code: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// `GET /oauth/callback`: where a connector's sign-in page returns when the browser runs on
/// this computer. No bearer token: the one-time `state` (and the host-held PKCE verifier) authenticate.
async fn oauth_callback(
    State(hub): State<Arc<Hub>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Query(q): Query<OAuthCallback>,
) -> Response {
    if !is_loopback(peer.ip()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let error = q.error_description.or(q.error);
    let result =
        market::oauth::finish(&hub.store, q.state.as_deref().unwrap_or_default(), q.code.as_deref(), error.as_deref())
            .await;
    let (title, detail) = match &result {
        Ok(c) => (format!("{} is connected", c.name), "You can close this tab and go back to Codync.".to_owned()),
        Err(e) => ("Sign-in didn't finish".to_owned(), format!("{e:#}")),
    };
    let escape = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    axum::response::Html(format!(
        "<!doctype html><meta charset=utf-8><meta name=viewport content=\"width=device-width\"><title>Codync</title>\
         <body style=\"font:16px system-ui;margin:20vh auto;max-width:28em;padding:0 16px;text-align:center\">\
         <h2>{}</h2><p>{}</p></body>",
        escape(&title),
        escape(&detail)
    ))
    .into_response()
}

#[derive(Deserialize)]
struct ChannelQuery {
    v: Option<u32>,
}

/// `GET /channel?v=1`: the direct E2E channel. No bearer token: the handshake authenticates.
async fn channel(
    State(hub): State<Arc<Hub>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Query(q): Query<ChannelQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    if q.v != Some(1) {
        return (StatusCode::UPGRADE_REQUIRED, Json(json!({"error": {"code": "upgradeRequired"}}))).into_response();
    }
    ws.max_message_size(crate::remote::channel::MAX_WS_MESSAGE)
        .on_upgrade(move |socket| crate::remote::channel::serve_direct(hub, socket, peer.ip()))
}

async fn term_stream(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    if !authorized(&hub, &headers, peer) {
        return Err(unauthorized());
    }
    let events =
        term_events(&hub, &id).ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "that terminal is gone".into()))?;
    let events = events.map(|v| Ok(Event::default().data(v.to_string())));
    Ok(Sse::new(events).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

#[derive(Deserialize)]
struct EventsQuery {
    since: Option<i64>,
    client: Option<String>,
}

async fn events(
    State(hub): State<Arc<Hub>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(q): Query<EventsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    if !authorized(&hub, &headers, peer) {
        return Err(unauthorized());
    }
    let events = events_stream(&hub, q.since.unwrap_or(0), q.client.as_deref(), &Caller::Local)
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")))?;
    let events = events.map(|v| Ok(Event::default().data(v.to_string())));
    Ok(Sse::new(events).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

/// A scoped webhook credential authorizes only firing its own routine.
/// `POST /hooks/routines/:id`: a webhook delivery made on this computer (the public one
/// arrives through the relay, `remote::relay`). Same checks and answers either way.
async fn routine_hook(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    use crate::routines::hooks::{MAX_BODY, Refused};
    if body.len() > MAX_BODY {
        return Err(ApiError(StatusCode::PAYLOAD_TOO_LARGE, "event too large".into()));
    }
    let headers =
        headers.iter().filter_map(|(k, v)| Some((k.as_str().to_owned(), v.to_str().ok()?.to_owned()))).collect();
    hub.routines.receive(&hub, &id, &headers, &body).map(Json).map_err(|e| {
        let status = match e {
            Refused::Unauthorized => StatusCode::UNAUTHORIZED,
            Refused::Paused | Refused::Busy => StatusCode::CONFLICT,
            Refused::Invalid(_) => StatusCode::BAD_REQUEST,
        };
        ApiError(status, e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hub on an in-memory store, with its identity in a temp folder the test removes.
    fn temp_hub() -> (Arc<Hub>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("codync-http-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let hub = Hub::new(
            crate::store::Store::open(std::path::Path::new(":memory:")).expect("store"),
            "test".into(),
            crate::remote::identity::Identity::load_or_create(&dir).expect("identity"),
            "test".into(),
            19222,
        );
        (hub, dir)
    }

    #[test]
    fn phone_streams_hold_pushes_until_the_last_one_closes() {
        let (hub, dir) = temp_hub();
        let desktop = events_stream(&hub, 0, Some("macos"), &Caller::Local).expect("desktop events");
        assert!(!hub.phone_connected());
        let android = events_stream(&hub, 0, Some("android"), &Caller::Local).expect("Android events");
        let ios = events_stream(&hub, 0, Some("ios"), &Caller::Local).expect("iOS events");
        assert!(hub.phone_connected());
        drop(android);
        assert!(hub.phone_connected(), "the iPhone remains connected");
        drop(ios);
        assert!(!hub.phone_connected(), "desktop streams do not hold phone alerts");
        drop(desktop);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn health_keeps_the_binary_and_busy_state_on_this_computer() {
        let (hub, dir) = temp_hub();
        let at = |addr: &str| ConnectInfo(addr.parse::<SocketAddr>().expect("address"));
        let lan = health(State(hub.clone()), at("192.168.1.20:50000"), Query(HealthQuery::default())).await.0;
        assert!(lan["computerId"].is_string());
        for key in ["binaryPath", "binaryHash", "busy", "updateRequested"] {
            assert!(lan.get(key).is_none(), "{key} reached the network");
        }
        let local = health(State(hub), at("[::ffff:127.0.0.1]:50000"), Query(HealthQuery::default())).await.0;
        assert_eq!(local["busy"], false);
        assert!(local["updateRequested"].is_null(), "nobody asked this computer to update");
        assert!(local.get("binaryPath").is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn only_loopback_counts_as_this_computer() {
        let ip = |s: &str| s.parse::<IpAddr>().expect("test addresses are valid");
        assert!(is_loopback(ip("127.0.0.1")));
        assert!(is_loopback(ip("::1")));
        assert!(is_loopback(ip("::ffff:127.0.0.1")), "IPv4 loopback seen through `--bind ::`");
        assert!(!is_loopback(ip("192.168.1.20")));
        assert!(!is_loopback(ip("::ffff:192.168.1.20")));
        assert!(!is_loopback(ip("100.101.102.103")));
    }
}
