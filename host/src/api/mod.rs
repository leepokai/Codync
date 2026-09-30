//! HTTP API: `POST /api/<method>` commands + `GET /events` SSE for this computer's own
//! apps and helpers (loopback + `Authorization: Bearer <token>`), and `GET /channel`, the
//! direct end-to-end encrypted channel phones use (see `channel`).

pub mod devices;

use crate::LockExt;
use crate::agent::backends;
use crate::agent::bot::Cmd;
use crate::api::devices::{Caller, Forbidden};
use crate::hub::{BotStatus, Hub};
use crate::remote::crypto;
use crate::store::{BotConfig, DeviceSource, EntryKind, Lane, ReadScope};
use crate::{market, usage};
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
        .route("/hooks/routines/{id}", post(routine_hook))
        .route("/events", get(events))
        .route("/api/{method}", post(command))
        .route("/term/{id}", get(term_stream))
        .route("/ingest/statusline", post(statusline))
        .route("/channel", get(channel))
        .route("/oauth/callback", get(oauth_callback))
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
    } else if e.is::<crate::remote::cloud::Conflict>() {
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
        "binaryPath": crate::service::binary_identity().map(|(path, _)| path),
        "binaryHash": crate::service::binary_identity().map(|(_, hash)| hash),
        "busy": hub.busy(),
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

fn str_arg<'a>(b: &'a Value, k: &str) -> Result<&'a str> {
    b[k].as_str().ok_or_else(|| anyhow!("`{k}` is required"))
}

/// Everything a bot can turn on: installed connectors and connected apps.
fn connector_ids(store: &crate::store::Store) -> Result<Vec<String>> {
    Ok(market::list_connectors(store)?["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["id"].as_str().map(str::to_owned))
        .collect())
}

/// A connector installed on the computer is on for every bot; each bot can turn it off.
fn enable_new_connectors(hub: &Hub, before: &[String]) -> Result<()> {
    let new: Vec<String> = connector_ids(&hub.store)?.into_iter().filter(|id| !before.contains(id)).collect();
    if new.is_empty() {
        return Ok(());
    }
    for row in hub.store.bots()? {
        if row.deleted || row.config.is_group() {
            continue;
        }
        let mut on = row.config.connectors;
        for id in &new {
            if !on.contains(id) {
                on.push(id.clone());
            }
        }
        hub.update_bot(&json!({"id": row.config.id, "connectors": on}))?;
    }
    Ok(())
}

/// Runs one API method for `caller` (permissions per spec §6.6).
pub async fn dispatch(hub: &Arc<Hub>, caller: &Caller, method: &str, b: Value) -> Result<Value> {
    devices::permit(caller, method)?;
    if method.contains("onnector")
        || method.starts_with("composio")
        || method == "setComposioKey"
        || method.starts_with("credential")
        || method == "computerCall"
        || matches!(method, "agentAuth" | "agentAuthenticate" | "setAgentEnv")
    {
        market::vault::unlock(hub.clone()).await?;
    }
    Ok(match method {
        "hostUpdateStatus" => crate::update::status()?,
        "checkHostUpdate" => crate::update::check().await?,
        "setHostAutomaticUpdates" => {
            crate::update::set_automatic(b["enabled"].as_bool().context("enabled must be a boolean")?)?
        }
        "installHostUpdate" => {
            let port = hub.port;
            let force = b["force"].as_bool().unwrap_or(false);
            if !force && hub.busy() {
                bail!("host is busy; retry when idle or explicitly allow interruption");
            }
            tokio::task::spawn_blocking(move || crate::update::spawn_worker(port, force)).await??;
            json!({"scheduled": true})
        }
        "credentialUpdateConnector" => {
            let id = str_arg(&b, "id")?;
            let fields = b["fields"].as_object().ok_or_else(|| anyhow!("Credential fields are required"))?;
            market::update_connectors(&hub.store, |all| {
                let c = all.iter_mut().find(|c| c.id == id).ok_or_else(|| anyhow!("Unknown connector"))?;
                let values = if c.command.is_some() { &mut c.env } else { &mut c.headers };
                for (key, value) in fields {
                    if !values.contains_key(key) {
                        bail!("Unknown credential field");
                    }
                    let value = value
                        .as_str()
                        .filter(|s| !s.is_empty() && s.len() <= 64 * 1024)
                        .ok_or_else(|| anyhow!("Invalid credential value"))?;
                    values.insert(key.clone(), value.to_owned());
                }
                Ok(())
            })?;
            let ready = market::verify::verify(&hub.store, id, hub.port).await?;
            for row in hub.store.bots()? {
                if !row.deleted && !row.config.is_group() && row.config.connectors.iter().any(|c| c == id) {
                    hub.send_cmd(&row.config.id, Cmd::RefreshTools)?;
                }
            }
            ready
        }
        "credentialStatus" => market::passwords::status(&hub.store)?,
        "credentialLogins" => market::logins::list(&hub.store)?,
        "credentialSaveLogin" => {
            let login = market::logins::save(
                &hub.store,
                str_arg(&b, "site")?,
                b["username"].as_str().unwrap_or_default(),
                str_arg(&b, "password")?,
            )?;
            json!({"login": login.public()})
        }
        "credentialRemoveLogin" => {
            market::logins::remove(&hub.store, str_arg(&b, "id")?)?;
            json!({})
        }
        "credentialSetOnePassword" => {
            market::passwords::set_token(&hub.store, b["token"].as_str().unwrap_or_default()).await?
        }
        "connectorRuntime" => {
            let c = market::connectors(&hub.store)?
                .into_iter()
                .find(|c| Some(c.id.as_str()) == b["id"].as_str())
                .ok_or_else(|| anyhow!("Unknown connector"))?;
            let mut env = serde_json::Map::new();
            for (k, v) in &c.env {
                env.insert(k.clone(), market::passwords::resolve(&hub.store, v).await?.into());
            }
            json!({"command":c.command,"args":c.args,"env":env})
        }
        "connectorCall" => {
            market::requests::call(hub, str_arg(&b, "botId")?, str_arg(&b, "name")?, &b["arguments"]).await?
        }
        "connectorInfo" => market::connector_info(&hub.store, str_arg(&b, "registryName")?).await?,
        "connectorRequestFinish" => market::requests::finish(hub, &b).await?,
        "connectorVerify" => market::verify::verify(&hub.store, str_arg(&b, "id")?, hub.port).await?,
        "composioCall" => {
            json!({"result": market::composio::call(&hub.store, str_arg(&b, "botId")?, str_arg(&b, "name")?, &b["arguments"]).await?})
        }
        "composioStatus" => market::composio::status(&hub.store)?,
        "setComposioKey" => market::composio::set_key(&hub.store, b["key"].as_str().unwrap_or_default()).await?,
        "composioToolkits" => {
            market::composio::toolkits(
                &hub.store,
                b["search"].as_str().unwrap_or_default(),
                b["cursor"].as_str().unwrap_or_default(),
            )
            .await?
        }
        "composioConnect" => market::composio::connect(&hub.store, str_arg(&b, "toolkit")?).await?,
        "composioConnectFields" => {
            let fields = b["fields"].as_object().ok_or_else(|| anyhow!("`fields` is required"))?;
            let before = connector_ids(&hub.store)?;
            let r = market::composio::connect_with_fields(
                &hub.store,
                str_arg(&b, "toolkit")?,
                str_arg(&b, "mode")?,
                fields,
            )
            .await?;
            enable_new_connectors(hub, &before)?;
            r
        }
        "composioConnection" => {
            let before = connector_ids(&hub.store)?;
            let r = market::composio::connection(&hub.store, str_arg(&b, "id")?).await?;
            enable_new_connectors(hub, &before)?;
            r
        }
        "routineSchedule" => crate::routines::schedule_preview(&b, crate::store::now_ms())?,
        "routines" => hub.routines.list(str_arg(&b, "botId")?),
        "saveRoutine" => hub.routines.save(hub, str_arg(&b, "botId")?, &b)?,
        "setRoutineEnabled" => hub.routines.set_enabled(
            &hub.store,
            str_arg(&b, "botId")?,
            str_arg(&b, "id")?,
            b["enabled"].as_bool().context("enabled is required")?,
        )?,
        "deleteRoutine" => hub.routines.remove(&hub.store, str_arg(&b, "botId")?, str_arg(&b, "id")?)?,
        "runRoutine" => hub.routines.enqueue(
            &hub.store,
            str_arg(&b, "botId")?,
            str_arg(&b, "id")?,
            json!({"source":"test"}),
            None,
            true,
        )?,
        "routineWebhook" => hub.routines.credentials(hub, str_arg(&b, "botId")?, str_arg(&b, "id")?)?,
        "routineCall" => crate::routines::call(hub, str_arg(&b, "botId")?, str_arg(&b, "name")?, &b["arguments"])?,
        "memoryCall" => {
            crate::chat::memory::call(hub, str_arg(&b, "botId")?, str_arg(&b, "name")?, &b["arguments"]).await?
        }
        "teamCall" => {
            crate::chat::team::call(hub, str_arg(&b, "botId")?, str_arg(&b, "name")?, &b["arguments"]).await?
        }
        "hello" => {
            let port = hub.port;
            json!({
                "hostId": hub.host_id,
                "computerId": hub.identity.computer_id(),
                "signKey": hub.identity.sign_pub_b64(),
                "boxKey": hub.identity.box_pub_b64(),
                "protocol": 1,
                "cloud": crate::remote::cloud::url(&hub.store),
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
            // Connectors are on for a new bot unless the client picked them.
            // Without a credential store (headless Linux) no connector can run, so the bot starts with none.
            if b["connectors"].is_null() && b["kind"] != "group" {
                b["connectors"] = match market::vault::unlock(hub.clone()).await {
                    Ok(()) => json!(connector_ids(&hub.store)?),
                    Err(e) => {
                        tracing::warn!(
                            error = format!("{e:#}"),
                            "credential store unavailable; new bot starts without connectors"
                        );
                        json!([])
                    }
                };
            }
            let cfg: BotConfig = serde_json::from_value(b).context("invalid bot")?;
            let hub = hub.clone();
            json!({"bot": tokio::task::spawn_blocking(move || hub.create_bot(cfg)).await??})
        }
        "updateBot" => {
            let hub = hub.clone();
            json!({"bot": tokio::task::spawn_blocking(move || hub.update_bot(&b)).await??})
        }
        "deleteBot" => {
            hub.delete_bot(str_arg(&b, "botId")?)?;
            json!({})
        }
        // The main chat, one thread (`threadId`), or everything (`all`, the roster's "Mark as read").
        "markRead" => {
            let scope = match b["threadId"].as_str().filter(|t| !t.is_empty()) {
                Some(root) => ReadScope::Thread(root),
                None if b["all"] == true => ReadScope::All,
                None => ReadScope::Chat,
            };
            hub.mark_read(str_arg(&b, "botId")?, scope)?;
            json!({})
        }
        // One chunk of a file for a later `send` (`attachments`); base64 `data` at `offset`.
        "upload" => {
            use base64::Engine as _;
            let row = hub.store.bot(str_arg(&b, "botId")?)?.filter(|r| !r.deleted && !r.config.is_group());
            let root = crate::chat::uploads::root(&row.ok_or_else(|| anyhow!("unknown bot"))?.config);
            let (id, name) = (str_arg(&b, "uploadId")?.to_owned(), str_arg(&b, "name")?.to_owned());
            let data =
                base64::engine::general_purpose::STANDARD.decode(str_arg(&b, "data")?).context("invalid data")?;
            let (offset, done) = (b["offset"].as_u64().unwrap_or(0), b["done"] == true);
            let attachment = tokio::task::spawn_blocking(move || {
                crate::chat::uploads::append(&root, &id, &name, offset, &data, done)
            })
            .await??;
            json!({"attachment": attachment})
        }
        // A sent file, back in chunks (chat previews on other devices).
        "readUpload" => {
            use base64::Engine as _;
            let row = hub.store.bot(str_arg(&b, "botId")?)?.filter(|r| !r.deleted && !r.config.is_group());
            let root = crate::chat::uploads::root(&row.ok_or_else(|| anyhow!("unknown bot"))?.config);
            let id = str_arg(&b, "uploadId")?.to_owned();
            let offset = b["offset"].as_u64().unwrap_or(0);
            let (data, size) =
                tokio::task::spawn_blocking(move || crate::chat::uploads::read(&root, &id, offset, 384 * 1024))
                    .await??;
            json!({"data": base64::engine::general_purpose::STANDARD.encode(data), "size": size})
        }
        "send" => {
            let bot = str_arg(&b, "botId")?;
            let text = b["text"].as_str().unwrap_or_default().trim();
            let uploads: Vec<&str> =
                b["attachments"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            if text.is_empty() && uploads.is_empty() {
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
            let (mut paths, mut attachments) = (Vec::new(), Vec::new());
            if !uploads.is_empty() {
                if row.config.is_group() {
                    bail!("files can't be sent to a group");
                }
                let root = crate::chat::uploads::root(&row.config);
                for id in uploads {
                    let (path, meta) = crate::chat::uploads::resolve(&root, id)?;
                    paths.push(path);
                    attachments.push(meta);
                }
            }
            let prompt = if paths.is_empty() {
                text.to_owned()
            } else {
                format!("{text}{}", crate::chat::uploads::prompt_suffix(&paths))
            };
            let turn = hub.store.max_turn(bot) + 1;
            // A group has no agent of its own to queue behind: its message is simply sent.
            let status = if row.config.is_group() { "sent" } else { "queued" };
            let e = hub
                .add_entry(
                    &lane,
                    EntryKind::User,
                    turn,
                    &json!({"text": text, "clientNonce": nonce, "status": status, "attachments": attachments}),
                )
                .ok_or_else(|| anyhow!("couldn't save the message"))?;
            // Before the turn starts, so its reply lands unread.
            let scope = lane.thread.as_deref().map_or(ReadScope::Chat, ReadScope::Thread);
            if let Err(error) = hub.mark_read(bot, scope) {
                tracing::warn!(%error, bot, "couldn't mark bot read after send");
            }
            if row.config.is_group() {
                crate::chat::group::start(hub, &row.config, lane);
            } else {
                hub.send_cmd(bot, Cmd::Send { lane, entry_id: e.id.clone(), text: prompt })?;
            }
            json!({"entry": e})
        }
        // A finished voice call, as a line in the chat ("Voice chat · 0:16").
        "logCall" => {
            let bot = str_arg(&b, "botId")?;
            hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
            let seconds = b["seconds"].as_u64().unwrap_or(0);
            let data = json!({"text": "Voice chat", "callSeconds": seconds});
            let e = hub.add_entry(&Lane::main(bot), EntryKind::Notice, hub.store.max_turn(bot), &data);
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
            tokio::task::spawn_blocking(move || crate::chat::memory::describe(&bot)).await??
        }
        "forgetMemory" => {
            let bot = str_arg(&b, "botId")?.to_owned();
            let id = str_arg(&b, "id")?.to_owned();
            let removed =
                tokio::task::spawn_blocking(move || crate::chat::memory::Memory::for_bot(&bot)?.remove(&id)).await??;
            json!({"removed": removed})
        }
        "clearMemory" => {
            let bot = str_arg(&b, "botId")?.to_owned();
            tokio::task::spawn_blocking(move || crate::chat::memory::Memory::for_bot(&bot)?.clear()).await??;
            json!({})
        }
        "react" => {
            let e = hub.react(str_arg(&b, "entryId")?, str_arg(&b, "emoji")?)?;
            json!({"entry": e})
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
        "unregisterDevice" => {
            hub.store.remove_push_tickets(caller.device_key())?;
            json!({})
        }
        "registerActivity" => {
            let bot_id = str_arg(&b, "botId")?;
            let row = hub.store.bot(bot_id)?.filter(|row| !row.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
            hub.store.add_activity_ticket(str_arg(&b, "ticket")?, bot_id, caller.device_key())?;
            // Registration can finish after the task does. Send the current state immediately.
            let state = hub.bot_json(&row);
            let status = match state["status"].as_str() {
                Some("working") => BotStatus::Working,
                Some("needsInput") => BotStatus::NeedsInput,
                Some("error") => BotStatus::Error,
                _ => BotStatus::Idle,
            };
            // A send is acknowledged before its actor necessarily starts. An unanswered user
            // message must not end the activity as "done" during that window.
            let pending = status == BotStatus::Idle
                && hub.store.last_message(bot_id).is_some_and(|message| message.author.is_none());
            if !pending {
                crate::remote::push::live_activity_update(
                    hub,
                    bot_id,
                    &crate::hub::Runtime {
                        status,
                        started_at: state["startedAt"].as_i64(),
                        ..crate::hub::Runtime::default()
                    },
                );
            }
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
                tokio::spawn(crate::remote::cloud::revoke_grant(hub.clone(), grant));
            }
            json!({})
        }
        "accessRequests" => json!({"requests": hub.cloud.requests_json()}),
        "decideAccessRequest" => {
            let approve = b["approve"].as_bool().ok_or_else(|| anyhow!("`approve` is required"))?;
            crate::remote::cloud::decide(hub, str_arg(&b, "requestId")?, approve).await?;
            json!({})
        }
        "unclaim" => {
            crate::remote::cloud::unclaim(hub).await?;
            json!({})
        }
        "cloudStatus" => serde_json::to_value(hub.cloud.status())?,
        "setCloud" => {
            let enabled = b["enabled"].as_bool().ok_or_else(|| anyhow!("`enabled` is required"))?;
            serde_json::to_value(crate::remote::cloud::set_cloud(hub, enabled, b["url"].as_str())?)?
        }
        "setApproval" => {
            let approval = serde_json::from_value(b["approval"].clone()).context("`approval` is code or auto")?;
            serde_json::to_value(crate::remote::cloud::set_approval(hub, approval)?)?
        }
        "claimSign" => claim_sign(hub, &b).await?,
        "marketConnectors" => {
            market::browse_connectors(
                &hub.store,
                b["search"].as_str().unwrap_or_default(),
                b["cursor"].as_str().unwrap_or_default(),
            )
            .await?
        }
        "marketSkills" => market::browse_skills(&hub.store).await?,
        "connectors" => market::list_connectors(&hub.store)?,
        "importConnectors" => {
            let before = connector_ids(&hub.store)?;
            let added = market::import_connectors(&hub.store, str_arg(&b, "config")?).await?;
            enable_new_connectors(hub, &before)?;
            added
        }
        "installConnector" => {
            let before = connector_ids(&hub.store)?;
            let c = if b["registryName"].is_string() {
                market::install_connector(
                    &hub.store,
                    str_arg(&b, "registryName")?,
                    str_arg(&b, "option")?,
                    &b["inputs"],
                )
                .await?
            } else {
                market::add_custom_connector(&hub.store, &b).await?
            };
            enable_new_connectors(hub, &before)?;
            json!({"connector": c})
        }
        "connectorSignIn" => {
            // The browser can only reach this host's loopback page when it runs on this computer.
            let callback = if matches!(caller, Caller::Local) {
                market::oauth::Callback::Host
            } else {
                market::oauth::Callback::App
            };
            market::oauth::start(&hub.store, hub.port, str_arg(&b, "id")?, callback).await?
        }
        "connectorSignInFinish" => {
            let c = market::oauth::finish(&hub.store, str_arg(&b, "state")?, b["code"].as_str(), b["error"].as_str())
                .await?;
            json!({"connector": c.public()})
        }
        "connectorSignOut" => json!({"connector": market::oauth::sign_out(&hub.store, str_arg(&b, "id")?)?.public()}),
        "connectorTarget" => {
            market::oauth::target(&hub.store, str_arg(&b, "id")?, b["stale"].as_bool().unwrap_or(false)).await?
        }
        "removeConnector" => {
            let id = str_arg(&b, "id")?;
            match id.strip_prefix(market::composio::PREFIX) {
                Some(toolkit) => market::composio::disconnect(&hub.store, toolkit).await?,
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
            let step: crate::agent::term::Step =
                serde_json::from_value(b["step"].clone()).context("`step` is install or login")?;
            let (cols, rows) = term_size(&b);
            json!({"term": hub.terms.start(str_arg(&b, "backend")?, step, b["method"].as_str(), cols, rows).await?})
        }
        "agentModels" => crate::agent::auth::models(&hub.store, str_arg(&b, "backend")?).await?,
        "agentAuth" => crate::agent::auth::check(&hub.store, str_arg(&b, "backend")?).await?,
        "agentAuthenticate" => {
            crate::agent::auth::authenticate(&hub.store, str_arg(&b, "backend")?, str_arg(&b, "method")?).await?
        }
        "setAgentEnv" => {
            let vars = b["vars"].as_object().ok_or_else(|| anyhow!("`vars` is required"))?;
            crate::agent::auth::set_env(&hub.store, str_arg(&b, "backend")?, vars).await?
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
        "screenPrepare" => {
            if !hub.screen.enabled() {
                bail!("Remote screen is turned off on this computer.");
            }
            let ice = if b["relay"].as_bool().unwrap_or(false) {
                crate::remote::cloud::screen_ice(hub, caller.device_key()).await?
            } else {
                crate::screen::IceConfig::default()
            };
            let result = hub.screen.prepare(caller.device_key(), ice)?;
            let session = result["session"].as_str().context("missing screen session")?.to_owned();
            tokio::spawn(crate::screen::watch_viewer(hub.clone(), session, caller.device_key().to_owned()));
            result
        }
        "screenOffer" => {
            let display = b["display"].as_u64().and_then(|d| u32::try_from(d).ok());
            let result =
                hub.screen.offer(caller.device_key(), str_arg(&b, "sdp")?, b["session"].as_str(), display).await?;
            if b["session"].as_str().is_none() {
                let session = result["session"].as_str().context("missing screen session")?.to_owned();
                tokio::spawn(crate::screen::watch_viewer(hub.clone(), session, caller.device_key().to_owned()));
            }
            result
        }
        "screenClose" => {
            let session = str_arg(&b, "session")?;
            if hub.screen.owns_session(session, caller.device_key()) {
                hub.screen.close(session).await?;
            }
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
    let cloud = crate::remote::cloud::url(&hub.store);
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

/// A scoped webhook credential authorizes only firing its own routine.
async fn routine_hook(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    if body.len() > 64_000 {
        return Err(ApiError(StatusCode::PAYLOAD_TOO_LARGE, "event too large".into()));
    }
    let key = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default();
    let delivery = headers.get("x-delivery-id").and_then(|v| v.to_str().ok()).map(str::to_owned);
    if delivery.as_ref().is_some_and(|v| v.len() > 200) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "delivery id too long".into()));
    }
    let event =
        serde_json::from_slice(&body).map_err(|_| ApiError(StatusCode::BAD_REQUEST, "invalid event JSON".into()))?;
    hub.routines
        .webhook(&hub, &id, key, event, delivery)
        .map(Json)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))
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

    #[tokio::test]
    async fn new_connectors_turn_on_for_every_bot() {
        let dir = std::env::temp_dir().join(format!("codync-connectors-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("memory store");
        for (id, on) in [("a", json!([])), ("b", json!(["old"]))] {
            let cfg: BotConfig = serde_json::from_value(
                json!({"id": id, "name": id, "backend": "fixture", "command": "true", "cwd": dir, "connectors": on, "notify": false}),
            )
            .expect("bot config");
            store.save_bot(&cfg).expect("save bot");
        }
        let connector = |id: &str| -> market::Connector {
            serde_json::from_value(json!({"id": id, "name": id, "url": "https://example.com/mcp"})).expect("connector")
        };
        market::save_connectors(&store, &[connector("old")]).expect("save");
        let hub = Hub::new(
            store,
            "test".into(),
            crate::remote::identity::Identity::load_or_create(&dir).expect("identity"),
            "test".into(),
            19222,
        );
        hub.start().expect("start");
        let before = connector_ids(&hub.store).expect("ids");
        market::save_connectors(&hub.store, &[connector("old"), connector("linear")]).expect("save");
        enable_new_connectors(&hub, &before).expect("enable");
        let on = |id: &str| hub.store.bot(id).expect("read").expect("bot").config.connectors;
        assert_eq!(on("a"), ["linear"], "the new connector is on; one turned off stays off");
        assert_eq!(on("b"), ["old", "linear"]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
