//! HTTP API: `POST /api/<method>` commands + `GET /events` SSE.
//! Auth: `Authorization: Bearer <token>` (or `?token=` for SSE).

use crate::LockExt;
use crate::bot::Cmd;
use crate::hub::Hub;
use crate::store::{BotConfig, EntryKind};
use crate::{backends, market, usage};
use anyhow::{Context, Result, anyhow, bail};
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

/// Entries a fresh client receives per bot before paging with `history`.
const CATCH_UP_PER_BOT: i64 = 200;

pub fn router(hub: Arc<Hub>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/events", get(events))
        .route("/api/{method}", post(command))
        .route("/ingest/statusline", post(statusline))
        .with_state(hub)
}

fn authorized(hub: &Hub, headers: &HeaderMap, query_token: Option<&str>) -> bool {
    let header = headers.get("authorization").and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer "));
    let given = header.or(query_token).unwrap_or_default();
    // Constant-time compare.
    given.len() == hub.token.len() && given.bytes().zip(hub.token.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

struct ApiError(StatusCode, String);

/// `POST /api/<method>` with a method this host doesn't know (404, not 400).
#[derive(Debug)]
struct UnknownMethod;

impl std::fmt::Display for UnknownMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unknown method")
    }
}

impl std::error::Error for UnknownMethod {}

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
    let body = body.map_or(Value::Null, |b| b.0);
    match dispatch(&hub, &method, body).await {
        Ok(v) => Ok(Json(v)),
        Err(e) if e.is::<UnknownMethod>() => Err(ApiError(StatusCode::NOT_FOUND, e.to_string())),
        // `{:#}` keeps the context chain ("starting `npx …`: No such file or directory").
        Err(e) => Err(ApiError(StatusCode::BAD_REQUEST, format!("{e:#}"))),
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
                "entries": hub.store.entries_since(since, CATCH_UP_PER_BOT)?,
                "usage": hub.usage.locked().clone(),
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
            let cfg: BotConfig = serde_json::from_value(b).context("invalid bot")?;
            json!({"bot": hub.create_bot(cfg)?})
        }
        "updateBot" => json!({"bot": hub.update_bot(&b)?}),
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
                bail!("empty message");
            }
            let nonce = b["clientNonce"].as_str().unwrap_or_default();
            if !nonce.is_empty()
                && let Some(e) = hub.store.find_by_nonce(bot, nonce)
            {
                return Ok(json!({"entry": e}));
            }
            hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
            let turn = hub.store.max_turn(bot) + 1;
            let e = hub
                .add_entry(bot, EntryKind::User, turn, &json!({"text": text, "clientNonce": nonce, "status": "queued"}))
                .ok_or_else(|| anyhow!("couldn't save the message"))?;
            hub.send_cmd(bot, Cmd::Send { entry_id: e.id.clone(), text: text.to_owned() })?;
            if let Err(error) = hub.mark_read(bot) {
                tracing::warn!(%error, bot, "couldn't mark bot read after send");
            }
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
        "registerActivity" => {
            let bot = str_arg(&b, "botId")?.to_owned();
            let ticket = str_arg(&b, "ticket")?.to_owned();
            let mut map = hub.activities.locked();
            let list = map.entry(bot).or_default();
            if !list.contains(&ticket) {
                list.push(ticket);
            }
            json!({})
        }
        "refreshBackends" => {
            tokio::task::spawn_blocking(backends::hydrate_path).await?;
            json!({"backends": backends::list()})
        }
        "usage" => {
            if b["refresh"].as_bool().unwrap_or(false) {
                usage::refresh(hub).await;
            }
            serde_json::to_value(&*hub.usage.locked())?
        }
        "listDirs" => {
            let path = b["path"].as_str().map(std::path::PathBuf::from);
            tokio::task::spawn_blocking(move || list_dirs(path)).await??
        }
        "pairing" => {
            let port = hub.port;
            // Shells out to `tailscale`: keep it off the async workers.
            let urls = tokio::task::spawn_blocking(move || crate::service::addresses(port)).await?;
            let url = crate::service::pairing_url(&crate::service::host_name(), &hub.token, &urls);
            let svg = qrcode::QrCode::new(url.as_bytes())?
                .render::<qrcode::render::svg::Color>()
                .quiet_zone(false)
                .min_dimensions(200, 200)
                .build();
            json!({"pairingUrl": url, "urls": urls, "svg": svg})
        }
        "marketConnectors" => market::browse_connectors(&hub.store, b["search"].as_str().unwrap_or_default()).await?,
        "marketSkills" => market::browse_skills(&hub.store).await?,
        "connectors" => market::list_connectors(&hub.store),
        "installConnector" => {
            let c = if b["registryName"].is_string() {
                market::install_connector(
                    &hub.store,
                    str_arg(&b, "registryName")?,
                    str_arg(&b, "option")?,
                    &b["inputs"],
                )
                .await?
            } else {
                market::add_custom_connector(&hub.store, &b)?
            };
            json!({"connector": c})
        }
        "removeConnector" => {
            market::remove_connector(&hub.store, str_arg(&b, "id")?)?;
            json!({})
        }
        "skills" => market::list_skills(&hub.store),
        "installSkill" => {
            let s = if b["source"].is_string() {
                market::install_skill(&hub.store, str_arg(&b, "source")?).await?
            } else {
                market::add_custom_skill(&hub.store, &b)?
            };
            json!({"skill": s})
        }
        "removeSkill" => {
            market::remove_skill(&hub.store, str_arg(&b, "id")?)?;
            json!({})
        }
        _ => return Err(UnknownMethod.into()),
    })
}

/// Blocking (`std::fs`): call through `spawn_blocking`.
fn list_dirs(path: Option<std::path::PathBuf>) -> Result<Value> {
    let dir = match path {
        Some(p) => p,
        None => dirs::home_dir().ok_or_else(|| anyhow!("no home directory"))?,
    };
    let mut dirs: Vec<Value> = std::fs::read_dir(&dir)
        .with_context(|| format!("listing {}", dir.display()))?
        .filter_map(Result::ok)
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

/// Counts a connected iOS client (pushes are held while one is connected) for as long as its stream lives.
struct IosClientGuard(Arc<Hub>);

impl IosClientGuard {
    fn new(hub: Arc<Hub>) -> Self {
        hub.ios_clients.fetch_add(1, Ordering::Relaxed);
        Self(hub)
    }
}

impl Drop for IosClientGuard {
    fn drop(&mut self) {
        self.0.ios_clients.fetch_sub(1, Ordering::Relaxed);
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
    let entries = hub
        .store
        .entries_since(since, CATCH_UP_PER_BOT)
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")))?;
    for e in entries {
        catch_up.push(json!({"type": "entry", "rev": e.rev, "entry": e}));
    }
    catch_up.sort_by_key(|v| v["rev"].as_i64().unwrap_or(0));
    let hello = json!({"type": "hello", "hostId": hub.host_id, "rev": hub.store.current_rev(), "usage": hub.usage.locked().clone()});
    catch_up.insert(0, hello);

    let guard = Arc::new((q.client.as_deref() == Some("ios")).then(|| IosClientGuard::new(hub.clone())));
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
