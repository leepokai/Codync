//! HTTP API: `POST /api/<method>` commands + `GET /events` SSE.
//! Auth: `Authorization: Bearer <token>` (or `?token=` for SSE).

use crate::bot::Cmd;
use crate::hub::Hub;
use crate::store::BotConfig;
use crate::{backends, usage};
use anyhow::{Result, anyhow};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::{Stream, StreamExt, stream};
use serde::Deserialize;
use serde_json::{Value, json};
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;

pub fn router(hub: Arc<Hub>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/events", get(events))
        .route("/api/{method}", post(command))
        .route("/ingest/statusline", post(statusline))
        .with_state(hub)
}

fn authorized(hub: &Hub, headers: &HeaderMap, query_token: Option<&str>) -> bool {
    let header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let given = header.or(query_token).unwrap_or_default();
    // Constant-time compare.
    given.len() == hub.token.len() && given.bytes().zip(hub.token.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}

async fn health(State(hub): State<Arc<Hub>>) -> Json<Value> {
    Json(json!({"ok": true, "hostId": hub.host_id, "version": env!("CARGO_PKG_VERSION")}))
}

async fn statusline(State(hub): State<Arc<Hub>>, headers: HeaderMap, Json(body): Json<Value>) -> StatusCode {
    if !authorized(&hub, &headers, None) {
        return StatusCode::UNAUTHORIZED;
    }
    usage::ingest_statusline(&hub, &body);
    StatusCode::NO_CONTENT
}

async fn command(
    State(hub): State<Arc<Hub>>,
    Path(method): Path<String>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    if !authorized(&hub, &headers, None) {
        return Err(ApiError(StatusCode::UNAUTHORIZED, "invalid token".into()));
    }
    let body = body.map(|b| b.0).unwrap_or(Value::Null);
    match dispatch(&hub, &method, body).await {
        Ok(v) => Ok(Json(v)),
        Err(e) if e.to_string() == "unknown method" => Err(ApiError(StatusCode::NOT_FOUND, e.to_string())),
        Err(e) => Err(ApiError(StatusCode::BAD_REQUEST, e.to_string())),
    }
}

fn str_arg<'a>(b: &'a Value, k: &str) -> Result<&'a str> {
    b[k].as_str().ok_or_else(|| anyhow!("`{k}` is required"))
}

pub async fn dispatch(hub: &Arc<Hub>, method: &str, b: Value) -> Result<Value> {
    Ok(match method {
        "hello" => json!({
            "hostId": hub.host_id,
            "name": crate::service::host_name(),
            "version": env!("CARGO_PKG_VERSION"),
            "os": std::env::consts::OS,
            "home": dirs::home_dir().map(|p| p.to_string_lossy().into_owned()),
            "backends": backends::list(),
            "rev": hub.store.current_rev(),
        }),
        "sync" => {
            let since = b["since"].as_i64().unwrap_or(0);
            json!({
                "hostId": hub.host_id,
                "rev": hub.store.current_rev(),
                "bots": hub.bots_json(since)?,
                "entries": hub.store.entries_since(since, 200)?,
                "usage": hub.usage.lock().unwrap().clone(),
            })
        }
        "history" => {
            let bot = str_arg(&b, "botId")?;
            let before = b["beforeSeq"].as_i64().unwrap_or(i64::MAX);
            let limit = b["limit"].as_i64().unwrap_or(100).clamp(1, 500);
            json!({"entries": hub.store.history(bot, before, limit)?})
        }
        "createBot" => {
            let mut b = b;
            b["id"] = "".into();
            let cfg: BotConfig = serde_json::from_value(b).map_err(|e| anyhow!("invalid bot: {e}"))?;
            json!({"bot": hub.create_bot(cfg)?})
        }
        "updateBot" => json!({"bot": hub.update_bot(b)?}),
        "deleteBot" => {
            hub.delete_bot(str_arg(&b, "botId")?)?;
            json!({})
        }
        "markRead" => {
            hub.mark_read(str_arg(&b, "botId")?)?;
            json!({})
        }
        "send" => {
            let bot = str_arg(&b, "botId")?;
            let text = str_arg(&b, "text")?.trim();
            if text.is_empty() {
                return Err(anyhow!("empty message"));
            }
            let nonce = b["clientNonce"].as_str().unwrap_or_default();
            if !nonce.is_empty()
                && let Some(e) = hub.store.find_by_nonce(bot, nonce)
            {
                return Ok(json!({"entry": e}));
            }
            hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
            let turn = hub.store.max_turn(bot) + 1;
            let e = hub.add_entry(bot, "user", turn, json!({"text": text, "clientNonce": nonce, "status": "queued"}))?;
            hub.send_cmd(bot, Cmd::Send { entry_id: e.id.clone(), text: text.to_owned() })?;
            let _ = hub.mark_read(bot);
            json!({"entry": e})
        }
        "stop" => {
            hub.send_cmd(str_arg(&b, "botId")?, Cmd::Stop)?;
            json!({})
        }
        "newSession" => {
            hub.send_cmd(str_arg(&b, "botId")?, Cmd::NewSession)?;
            json!({})
        }
        "respondPermission" => {
            let entry_id = str_arg(&b, "entryId")?;
            let e = hub.store.entry(entry_id).ok_or_else(|| anyhow!("unknown card"))?;
            let option_id = b["optionId"].as_str().map(str::to_owned);
            hub.send_cmd(&e.bot_id, Cmd::Permission { entry_id: entry_id.to_owned(), option_id })?;
            json!({})
        }
        "registerDevice" => {
            let ticket = str_arg(&b, "ticket")?;
            if let Some(relay) = b["relay"].as_str().filter(|r| r.starts_with("https://")) {
                hub.store.kv_set("relay_url", relay.trim_end_matches('/'))?;
            }
            hub.store.add_device(ticket, b["name"].as_str().unwrap_or("iPhone"))?;
            json!({})
        }
        "unregisterDevice" => {
            hub.store.remove_device(str_arg(&b, "ticket")?)?;
            json!({})
        }
        "registerActivity" => {
            let bot = str_arg(&b, "botId")?.to_owned();
            let ticket = str_arg(&b, "ticket")?.to_owned();
            let mut map = hub.activities.lock().unwrap();
            let list = map.entry(bot).or_default();
            if !list.contains(&ticket) {
                list.push(ticket);
            }
            json!({})
        }
        "usage" => {
            if b["refresh"].as_bool().unwrap_or(false) {
                usage::refresh(hub).await;
            }
            hub.usage.lock().unwrap().clone()
        }
        "listDirs" => list_dirs(b["path"].as_str())?,
        _ => return Err(anyhow!("unknown method")),
    })
}

fn list_dirs(path: Option<&str>) -> Result<Value> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("no home"))?;
    let dir = path.map(std::path::PathBuf::from).unwrap_or(home);
    let mut dirs: Vec<Value> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            let p = e.path();
            Some(json!({"name": name, "path": p.to_string_lossy(), "isGit": p.join(".git").exists()}))
        })
        .collect();
    dirs.sort_by_key(|d| d["name"].as_str().unwrap_or_default().to_lowercase());
    Ok(json!({
        "path": dir.to_string_lossy(),
        "parent": dir.parent().map(|p| p.to_string_lossy().into_owned()),
        "isGit": dir.join(".git").exists(),
        "dirs": dirs,
    }))
}

#[derive(Deserialize)]
struct EventsQuery {
    since: Option<i64>,
    token: Option<String>,
    client: Option<String>,
}

struct ClientGuard(Arc<Hub>, bool);

impl Drop for ClientGuard {
    fn drop(&mut self) {
        if self.1 {
            self.0.ios_clients.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

/// Subscribes first, then sends a catch-up (everything after `since`, in rev
/// order), then live events. Duplicates are fine: clients upsert by id/rev.
async fn events(
    State(hub): State<Arc<Hub>>,
    headers: HeaderMap,
    Query(q): Query<EventsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    if !authorized(&hub, &headers, q.token.as_deref()) {
        return Err(ApiError(StatusCode::UNAUTHORIZED, "invalid token".into()));
    }
    let live = BroadcastStream::new(hub.events.subscribe());
    let since = q.since.unwrap_or(0);
    let mut catch_up: Vec<Value> = vec![];
    for bot in hub.bots_json(since).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))? {
        catch_up.push(json!({"type": "bot", "rev": bot["rev"], "bot": bot}));
    }
    for e in hub.store.entries_since(since, 200).unwrap_or_default() {
        catch_up.push(json!({"type": "entry", "rev": e.rev, "entry": e}));
    }
    catch_up.sort_by_key(|v| v["rev"].as_i64().unwrap_or(0));
    let hello = json!({"type": "hello", "hostId": hub.host_id, "rev": hub.store.current_rev(), "usage": hub.usage.lock().unwrap().clone()});
    catch_up.insert(0, hello);

    let is_ios = q.client.as_deref() == Some("ios");
    if is_ios {
        hub.ios_clients.fetch_add(1, Ordering::Relaxed);
    }
    let guard = Arc::new(ClientGuard(hub.clone(), is_ios));
    let head = stream::iter(catch_up.into_iter().map(|v| Ok(Event::default().data(v.to_string()))));
    let tail = live.filter_map(move |msg| {
        let _keep = guard.clone();
        async move {
            match msg {
                Ok(v) => Some(Ok(Event::default().data(v.to_string()))),
                // Lagged: tell the client to resync from its last rev.
                Err(_) => Some(Ok(Event::default().data(json!({"type": "resync"}).to_string()))),
            }
        }
    });
    Ok(Sse::new(head.chain(tail)).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}
