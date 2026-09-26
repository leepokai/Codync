//! HTTP API: `POST /api/<method>` commands + `GET /events` SSE for this computer's own
//! apps and helpers (loopback + `Authorization: Bearer <token>`), and `GET /channel`, the
//! direct end-to-end encrypted channel phones use (see `channel`).

use crate::LockExt;
use crate::bot::Cmd;
use crate::devices::{Caller, Forbidden};
use crate::hub::Hub;
use crate::store::{BotConfig, DeviceSource, EntryKind, Lane};
use crate::{backends, crypto, market, usage};
use anyhow::{Context, Result, anyhow, bail};
use axum::body::Bytes;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::{Stream, StreamExt, stream};
use serde::Deserialize;
use serde_json::{Value, json};
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
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
        .route("/term/{id}", get(term_stream))
        .route("/ingest/statusline", post(statusline))
        .route("/channel", get(channel))
        .with_state(hub)
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

/// `POST /api/<method>` with a method this host doesn't know (404, not 400).
#[derive(Debug)]
struct UnknownMethod;

impl std::fmt::Display for UnknownMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unknown method")
    }
}

impl std::error::Error for UnknownMethod {}

/// HTTP status and message for a failed call; channels send the same pair in `err`.
pub fn error_status(e: &anyhow::Error) -> (StatusCode, String) {
    if e.is::<UnknownMethod>() {
        (StatusCode::NOT_FOUND, e.to_string())
    } else if e.is::<Forbidden>() {
        (StatusCode::FORBIDDEN, e.to_string())
    } else if e.is::<crate::cloud::Conflict>() {
        (StatusCode::CONFLICT, e.to_string())
    } else {
        // `{:#}` keeps the context chain ("starting `npx …`: No such file or directory").
        (StatusCode::BAD_REQUEST, format!("{e:#}"))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}

async fn health(State(hub): State<Arc<Hub>>) -> Json<Value> {
    Json(json!({
        "ok": true,
        "hostId": hub.host_id,
        "computerId": hub.identity.computer_id(),
        "version": env!("CARGO_PKG_VERSION"),
    }))
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
    ws.max_message_size(crate::channel::MAX_WS_MESSAGE)
        .on_upgrade(move |socket| crate::channel::serve_direct(hub, socket, peer.ip()))
}

fn str_arg<'a>(b: &'a Value, k: &str) -> Result<&'a str> {
    b[k].as_str().ok_or_else(|| anyhow!("`{k}` is required"))
}

/// Runs one API method for `caller` (permissions per spec §6.6).
pub async fn dispatch(hub: &Arc<Hub>, caller: &Caller, method: &str, b: Value) -> Result<Value> {
    crate::devices::permit(caller, method)?;
    Ok(match method {
        "composioCall" => {
            json!({"result": crate::composio::call(&hub.store, str_arg(&b, "botId")?, str_arg(&b, "name")?, &b["arguments"]).await?})
        }
        "composioStatus" => crate::composio::status(&hub.store),
        "setComposioKey" => crate::composio::set_key(&hub.store, b["key"].as_str().unwrap_or_default()).await?,
        "composioToolkits" => {
            crate::composio::toolkits(
                &hub.store,
                b["search"].as_str().unwrap_or_default(),
                b["cursor"].as_str().unwrap_or_default(),
            )
            .await?
        }
        "composioConnect" => crate::composio::connect(&hub.store, str_arg(&b, "toolkit")?).await?,
        "composioConnectFields" => {
            let fields = b["fields"].as_object().ok_or_else(|| anyhow!("`fields` is required"))?;
            crate::composio::connect_with_fields(&hub.store, str_arg(&b, "toolkit")?, str_arg(&b, "mode")?, fields)
                .await?
        }
        "composioConnection" => crate::composio::connection(&hub.store, str_arg(&b, "id")?).await?,
        "teamCall" => crate::team::call(hub, str_arg(&b, "botId")?, str_arg(&b, "name")?, &b["arguments"]).await?,
        "hello" => {
            let port = hub.port;
            json!({
                "hostId": hub.host_id,
                "computerId": hub.identity.computer_id(),
                "signKey": hub.identity.sign_pub_b64(),
                "boxKey": hub.identity.box_pub_b64(),
                "protocol": 1,
                "cloud": crate::cloud::url(&hub.store),
                "name": crate::service::host_name(),
                "version": env!("CARGO_PKG_VERSION"),
                "os": std::env::consts::OS,
                "device": tokio::task::spawn_blocking(crate::service::device).await?,
                "home": dirs::home_dir().map(|p| p.to_string_lossy().into_owned()),
                "backends": backends::list(),
                "rev": hub.store.current_rev(),
                "screen": hub.screen.state(),
                // Shells out to `tailscale`: keep it off the async workers.
                "urls": tokio::task::spawn_blocking(move || crate::service::addresses(port)).await?,
            })
        }
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
        // A thread's replies (its newest 500); the root is in the main chat.
        "thread" => json!({"entries": hub.store.thread(str_arg(&b, "botId")?, str_arg(&b, "rootId")?, 500)?}),
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
            let row = hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
            // `threadId`: reply in the thread on that message of the main chat (threads are flat).
            let thread = match b["threadId"].as_str().filter(|t| !t.is_empty()) {
                Some(root) => {
                    let r = hub.store.entry(root).ok_or_else(|| anyhow!("unknown message"))?;
                    if r.bot_id != bot || r.thread_id.is_some() {
                        bail!("threads start from a message in the main chat");
                    }
                    Some(root.to_owned())
                }
                None => None,
            };
            let lane = Lane { chat: bot.to_owned(), thread };
            let turn = hub.store.max_turn(bot) + 1;
            // A group has no agent of its own to queue behind: its message is simply sent.
            let status = if row.config.is_group() { "sent" } else { "queued" };
            let e = hub
                .add_entry(&lane, EntryKind::User, turn, &json!({"text": text, "clientNonce": nonce, "status": status}))
                .ok_or_else(|| anyhow!("couldn't save the message"))?;
            if row.config.is_group() {
                crate::group::start(hub, &row.config, lane);
            } else {
                hub.send_cmd(bot, Cmd::Send { lane, entry_id: e.id.clone(), text: text.to_owned() })?;
            }
            if let Err(error) = hub.mark_read(bot) {
                tracing::warn!(%error, bot, "couldn't mark bot read after send");
            }
            json!({"entry": e})
        }
        "stop" => {
            let bot = str_arg(&b, "botId")?;
            match hub.store.bot(bot)? {
                Some(row) if row.config.is_group() => hub.groups.stop(hub, bot),
                _ => hub.send_cmd(bot, Cmd::Stop)?,
            }
            json!({})
        }
        "newSession" => {
            hub.send_cmd(str_arg(&b, "botId")?, Cmd::NewSession)?;
            json!({})
        }
        "memory" => {
            let bot = str_arg(&b, "botId")?.to_owned();
            tokio::task::spawn_blocking(move || crate::memory::describe(&bot)).await??
        }
        "forgetMemory" => {
            let bot = str_arg(&b, "botId")?.to_owned();
            let id = str_arg(&b, "id")?.to_owned();
            let removed =
                tokio::task::spawn_blocking(move || crate::memory::Memory::for_bot(&bot)?.remove(&id)).await??;
            json!({"removed": removed})
        }
        "clearMemory" => {
            let bot = str_arg(&b, "botId")?.to_owned();
            tokio::task::spawn_blocking(move || crate::memory::Memory::for_bot(&bot)?.clear()).await??;
            json!({})
        }
        "respondPermission" => {
            let entry_id = str_arg(&b, "entryId")?;
            let e = hub.store.entry(entry_id).ok_or_else(|| anyhow!("unknown card"))?;
            let option_id = b["optionId"].as_str().map(str::to_owned);
            // A card in a group belongs to the bot that asked.
            let bot = e.data["author"].as_str().unwrap_or(&e.bot_id);
            hub.send_cmd(bot, Cmd::Permission { entry_id: entry_id.to_owned(), option_id })?;
            json!({})
        }
        "registerDevice" => {
            let ticket = str_arg(&b, "ticket")?;
            if let Some(relay) = b["relay"].as_str().filter(|r| r.starts_with("https://")) {
                hub.store.kv_set("relay_url", relay.trim_end_matches('/'))?;
            }
            let push_key = b["pushKey"].as_str();
            if let Some(k) = push_key {
                crypto::unb64_n::<32>(k).context("`pushKey` must be a 32-byte X25519 key")?;
            }
            let name = b["name"].as_str().unwrap_or("iPhone");
            hub.store.add_push_ticket(ticket, caller.device_key(), push_key, b["ctx"].as_str(), name)?;
            json!({})
        }
        "registerActivity" => {
            hub.store.add_activity_ticket(str_arg(&b, "ticket")?, str_arg(&b, "botId")?, caller.device_key())?;
            json!({})
        }
        "refreshBackends" => {
            tokio::task::spawn_blocking(backends::hydrate_path).await?;
            backends::refresh_sign_in().await;
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
        "pairing" => pairing(hub).await?,
        "devices" => {
            let connected = hub.connected.locked().clone();
            let devices: Vec<Value> = hub
                .store
                .devices()?
                .into_iter()
                .map(|d| {
                    let mut v = serde_json::to_value(&d).expect("Device is plain data and always serializes");
                    v["connected"] = connected.contains_key(&d.key).into();
                    v
                })
                .collect();
            json!({"devices": devices})
        }
        "revokeDevice" => {
            let key = str_arg(&b, "key")?;
            let device = hub.store.device(key).ok_or_else(|| anyhow!("unknown device"))?;
            hub.revoke_device(key)?;
            if let Some(grant) = device.grant_id.filter(|_| device.source == DeviceSource::Account) {
                tokio::spawn(crate::cloud::revoke_grant(hub.clone(), grant));
            }
            json!({})
        }
        "accessRequests" => json!({"requests": hub.cloud.requests_json()}),
        "decideAccessRequest" => {
            let approve = b["approve"].as_bool().ok_or_else(|| anyhow!("`approve` is required"))?;
            crate::cloud::decide(hub, str_arg(&b, "requestId")?, approve).await?;
            json!({})
        }
        "unclaim" => {
            crate::cloud::unclaim(hub).await?;
            json!({})
        }
        "cloudStatus" => serde_json::to_value(hub.cloud.status())?,
        "setCloud" => {
            let enabled = b["enabled"].as_bool().ok_or_else(|| anyhow!("`enabled` is required"))?;
            serde_json::to_value(crate::cloud::set_cloud(hub, enabled, b["url"].as_str())?)?
        }
        "claimSign" => claim_sign(hub, &b).await?,
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
            let id = str_arg(&b, "id")?;
            match id.strip_prefix(crate::composio::PREFIX) {
                Some(toolkit) => crate::composio::disconnect(&hub.store, toolkit).await?,
                None => market::remove_connector(&hub.store, id)?,
            }
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
        "agentSetup" => {
            let step: crate::term::Step =
                serde_json::from_value(b["step"].clone()).context("`step` is install or login")?;
            let (cols, rows) = term_size(&b);
            json!({"term": hub.terms.start(str_arg(&b, "backend")?, step, b["method"].as_str(), cols, rows).await?})
        }
        "agentAuth" => crate::auth::check(&hub.store, str_arg(&b, "backend")?).await?,
        "agentAuthenticate" => {
            crate::auth::authenticate(&hub.store, str_arg(&b, "backend")?, str_arg(&b, "method")?).await?
        }
        "setAgentEnv" => {
            let vars = b["vars"].as_object().ok_or_else(|| anyhow!("`vars` is required"))?;
            crate::auth::set_env(&hub.store, str_arg(&b, "backend")?, vars).await?
        }
        "termInput" => {
            hub.terms.write(str_arg(&b, "term")?, str_arg(&b, "data")?)?;
            json!({})
        }
        "termResize" => {
            let (cols, rows) = term_size(&b);
            hub.terms.resize(str_arg(&b, "term")?, cols, rows)?;
            json!({})
        }
        "termClose" => {
            hub.terms.close(str_arg(&b, "term")?);
            json!({})
        }
        "screenStatus" => hub.screen.state(),
        "screenOffer" => {
            let display = b["display"].as_u64().and_then(|d| u32::try_from(d).ok());
            hub.screen.offer(str_arg(&b, "sdp")?, b["session"].as_str(), display).await?
        }
        "screenClose" => {
            hub.screen.close(str_arg(&b, "session")?).await?;
            json!({})
        }
        "screenTakeover" => {
            hub.screen.takeover(b["on"].as_bool().unwrap_or(false));
            hub.screen.state()
        }
        "setScreenEnabled" => {
            hub.screen.set_enabled(&hub.store, b["enabled"].as_bool().unwrap_or(false)).await?;
            hub.screen.state()
        }
        "computerCall" => {
            let tool: crate::screen::ComputerTool =
                serde_json::from_value(b.clone()).context("invalid computer tool call")?;
            json!({"content": crate::screen::computer(hub, str_arg(&b, "botId")?, tool).await?})
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

fn term_size(b: &Value) -> (u16, u16) {
    let dim = |k: &str, default: u16| b[k].as_u64().and_then(|v| u16::try_from(v).ok()).unwrap_or(default);
    (dim("cols", 80), dim("rows", 24))
}

/// A setup terminal's output: everything so far, then live, ending after `exit`.
pub fn term_events(hub: &Hub, id: &str) -> Option<impl Stream<Item = Value> + Send + use<>> {
    let term = hub.terms.get(id)?;
    let (head, rx) = term.attach();
    let done = head.iter().any(|v| v["type"] == "exit");
    let live = BroadcastStream::new(rx).filter_map(|msg| async move { msg.ok() }).scan(done, |done, v| {
        let out = (!*done).then(|| {
            *done = v["type"] == "exit";
            v
        });
        async move { out }
    });
    Some(stream::iter(head).chain(live))
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

/// Event types only this computer's own apps receive.
const LOCAL_EVENTS: &[&str] = &["accessRequests", "cloud"];

/// Subscribes first, then yields a catch-up (everything after `since`, in rev order),
/// then live events. Duplicates are fine: clients upsert by id/rev. Shared by SSE and channels.
pub fn events_stream(
    hub: &Arc<Hub>,
    since: i64,
    client: Option<&str>,
    caller: &Caller,
) -> Result<impl Stream<Item = Value> + Send + use<>> {
    let live = BroadcastStream::new(hub.events.subscribe());
    let mut catch_up: Vec<Value> = vec![];
    for bot in hub.bots_json(since)? {
        catch_up.push(json!({"type": "bot", "rev": bot["rev"], "bot": bot}));
    }
    for e in hub.store.entries_since(since, CATCH_UP_PER_BOT)? {
        catch_up.push(json!({"type": "entry", "rev": e.rev, "entry": e}));
    }
    catch_up.sort_by_key(|v| v["rev"].as_i64().unwrap_or(0));
    let hello = json!({
        "type": "hello",
        "hostId": hub.host_id,
        "computerId": hub.identity.computer_id(),
        "rev": hub.store.current_rev(),
        "usage": hub.usage.locked().clone(),
        "screen": hub.screen.state(),
    });
    catch_up.insert(0, hello);

    let local = matches!(caller, Caller::Local);
    let guard = Arc::new((client == Some("ios")).then(|| IosClientGuard::new(hub.clone())));
    let tail = live.filter_map(move |msg| {
        let _keep = guard.clone();
        async move {
            match msg {
                Ok(v) if !local && v["type"].as_str().is_some_and(|t| LOCAL_EVENTS.contains(&t)) => None,
                Ok(v) => Some(v),
                // Lagged: tell the client to resync from its last rev.
                Err(_) => Some(json!({"type": "resync"})),
            }
        }
    });
    Ok(stream::iter(catch_up).chain(tail))
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

/// A new one-time pairing code and the QR that carries it (§4.1).
async fn pairing(hub: &Arc<Hub>) -> Result<Value> {
    let port = hub.port;
    // Shells out to `tailscale`: keep it off the async workers.
    let urls = tokio::task::spawn_blocking(move || crate::service::addresses(port)).await?;
    let cloud = crate::cloud::url(&hub.store);
    if urls.is_empty() && cloud.is_none() {
        bail!("No network address a phone could reach, and the Codync cloud is off.");
    }
    let issued = hub.pairing.locked().issue();
    hub.auth_changed();
    let url = crate::service::pairing_url(&crate::service::PairingQr {
        name: &crate::service::host_name(),
        computer_id: &hub.identity.computer_id(),
        sign_key: &hub.identity.sign_pub_b64(),
        box_key: &hub.identity.box_pub_b64(),
        code: &issued.code,
        urls: &urls,
        cloud: cloud.as_deref(),
    });
    let svg = qrcode::QrCode::new(url.as_bytes())?
        .render::<qrcode::render::svg::Color>()
        .quiet_zone(false)
        .min_dimensions(200, 200)
        .build();
    Ok(json!({"pairingUrl": url, "urls": urls, "svg": svg, "expiresAt": issued.expires_at}))
}

/// Signs this computer into an account claim (§4.2 A) for the signed-in Mac app to complete.
async fn claim_sign(hub: &Arc<Hub>, b: &Value) -> Result<Value> {
    let field = |k: &str| -> Result<&str> {
        let v = str_arg(b, k)?;
        // Fields are newline-separated in the signed string: a newline would forge another field.
        if v.is_empty() || v.contains('\n') {
            bail!("`{k}` is invalid");
        }
        Ok(v)
    };
    let (claim_id, nonce, user_id) = (field("claimId")?, field("nonce")?, field("userId")?);
    let computer_id = hub.identity.computer_id();
    let box_key = hub.identity.box_pub_b64();
    let input = crypto::claim_input(claim_id, nonce, user_id, &computer_id, &box_key);
    Ok(json!({
        "computerId": computer_id,
        "signKey": hub.identity.sign_pub_b64(),
        "boxKey": box_key,
        "name": crate::service::host_name(),
        "platform": std::env::consts::OS,
        "device": tokio::task::spawn_blocking(crate::service::device).await?,
        "version": env!("CARGO_PKG_VERSION"),
        "sig": crypto::b64(&hub.identity.sign(input.as_bytes())),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

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
