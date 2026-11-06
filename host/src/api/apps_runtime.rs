//! Sandboxed mini-app runtime server (`/apps/{bot_id}/{slug}/{token}/...`).
//!
//! Implements Ideas 1–7 from `bdots`:
//! - Idea 1: Sandboxed static app server (`Content-Security-Policy: sandbox ...` without `allow-same-origin`).
//! - Idea 2: Live WebSocket channel (`_live`) for collection change events and automatic version reloads.
//! - Idea 3: Automatic runtime error capture (`_error`) dispatching self-repair turns to the bot.
//! - Idea 4: Per-app JSON collections (`_data`, `_data/:collection`, `_data/:collection/:id`).
//! - Idea 5: Client helper (`_codync.js` / `_bdots.js`) exposing `window.codync` and `window.bdots`.
//! - Idea 6: App-to-bot intent bridge (`_tell`).
//! - Idea 7: Capability-token links (`AppLinkMode::View` vs `AppLinkMode::Edit`).

use crate::agent::bot::Cmd;
use crate::hub::Hub;
use crate::store::{AppLink, AppLinkMode, BotApp, EntryKind, Lane};
use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{any, get};
use axum::{Json, Router};
use serde_json::{Value, json};
use std::path::{Component, Path as FsPath, PathBuf};
use std::sync::Arc;

const APP_CSP: &str = "sandbox allow-scripts allow-forms allow-popups allow-modals allow-downloads; \
default-src 'self' https: data: blob:; \
img-src * data: blob:; \
 media-src * data: blob:; \
style-src 'self' 'unsafe-inline' https:; \
script-src 'self' 'unsafe-inline' https:; \
font-src * data:; \
connect-src 'self' ws: wss: http: https:;";

pub fn router() -> Router<Arc<Hub>> {
    Router::new()
        .route("/apps/{bot_id}/{slug}/{token}", get(redirect_trailing_slash))
        .route("/apps/{bot_id}/{slug}/{token}/", get(serve_root))
        .route("/apps/{bot_id}/{slug}/{token}/_live", get(handle_live))
        .route("/apps/{bot_id}/{slug}/{token}/{*path}", any(handle_subpath))
}

/// Broadcasts an app live event (`change` or `reload`) to any open `_live` WebSocket and host event listeners.
pub fn emit_app_live(hub: &Hub, bot_id: &str, slug: &str, payload: Value) {
    let mut event = json!({
        "type": "appLive",
        "botId": bot_id,
        "slug": slug,
    });
    event["payload"] = payload;
    let _ = hub.events.send(event);
}

async fn redirect_trailing_slash(Path((bot_id, slug, token)): Path<(String, String, String)>) -> Redirect {
    Redirect::temporary(&format!("/apps/{bot_id}/{slug}/{token}/"))
}

async fn serve_root(
    State(hub): State<Arc<Hub>>,
    Path((bot_id, slug, token)): Path<(String, String, String)>,
) -> Response {
    tokio::task::spawn_blocking(move || handle_request(&hub, &bot_id, &slug, &token, "", &Method::GET, &Bytes::new()))
        .await
        .unwrap_or_else(|error| json_resp(StatusCode::INTERNAL_SERVER_ERROR, json!({"error": error.to_string()})))
}

async fn handle_live(
    State(hub): State<Arc<Hub>>,
    Path((bot_id, slug, token)): Path<(String, String, String)>,
    ws: WebSocketUpgrade,
) -> Response {
    let Ok(Some(_link)) = hub.store.resolve_app_link(&bot_id, &slug, &token, false) else {
        return text_resp(StatusCode::FORBIDDEN, "text/plain; charset=utf-8", "Link expired or revoked");
    };
    let mut rx = hub.events.subscribe();
    ws.on_upgrade(move |mut socket: WebSocket| async move {
        while let Ok(ev) = rx.recv().await {
            if ev["type"] == "appLive" && ev["botId"] == bot_id && ev["slug"] == slug {
                let text = ev["payload"].to_string();
                if socket.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
        }
    })
}

async fn handle_subpath(
    State(hub): State<Arc<Hub>>,
    Path((bot_id, slug, token, path)): Path<(String, String, String, String)>,
    method: Method,
    body: Bytes,
) -> Response {
    tokio::task::spawn_blocking(move || handle_request(&hub, &bot_id, &slug, &token, &path, &method, &body))
        .await
        .unwrap_or_else(|error| json_resp(StatusCode::INTERNAL_SERVER_ERROR, json!({"error": error.to_string()})))
}

fn security_headers(content_type: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(content_type) {
        h.insert(header::CONTENT_TYPE, v);
    }
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    h.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    h.insert("access-control-allow-origin", HeaderValue::from_static("*"));
    h.insert("access-control-allow-methods", HeaderValue::from_static("GET, POST, PUT, PATCH, DELETE, OPTIONS"));
    h.insert("access-control-allow-headers", HeaderValue::from_static("content-type"));
    h.insert("content-security-policy", HeaderValue::from_static(APP_CSP));
    h
}

fn json_resp(status: StatusCode, body: Value) -> Response {
    (status, security_headers("application/json; charset=utf-8"), Json(body)).into_response()
}

fn text_resp(status: StatusCode, content_type: &str, body: impl Into<Bytes>) -> Response {
    (status, security_headers(content_type), body.into()).into_response()
}

pub(super) fn handle_request(
    hub: &Hub,
    bot_id: &str,
    slug: &str,
    token: &str,
    rel: &str,
    method: &Method,
    body: &Bytes,
) -> Response {
    if *method == Method::OPTIONS {
        return text_resp(StatusCode::NO_CONTENT, "text/plain; charset=utf-8", "");
    }
    let touch = rel.is_empty() || FsPath::new(rel).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("html"));
    let Ok(Some(link)) = hub.store.resolve_app_link(bot_id, slug, token, touch) else {
        return text_resp(StatusCode::FORBIDDEN, "text/plain; charset=utf-8", "Link expired or revoked");
    };
    let Ok(Some(app)) = hub.store.get_bot_app(bot_id, slug) else {
        return text_resp(StatusCode::NOT_FOUND, "text/plain; charset=utf-8", "App not found");
    };

    if (rel == "_bdots.js" || rel == "_codync.js") && *method == Method::GET {
        return text_resp(StatusCode::OK, "text/javascript; charset=utf-8", client_script(&app, &link));
    }
    if rel == "_data" || rel.starts_with("_data/") {
        return handle_data(hub, bot_id, slug, &link, rel, method, body);
    }
    if rel == "_tell" && *method == Method::POST {
        return handle_tell(hub, bot_id, &app, &link, body);
    }
    if rel == "_error" && *method == Method::POST {
        return handle_error(hub, bot_id, &app, body);
    }
    if *method != Method::GET && *method != Method::HEAD {
        return json_resp(StatusCode::METHOD_NOT_ALLOWED, json!({"error": "Method not allowed"}));
    }
    serve_static(hub, bot_id, &app, rel)
}

fn handle_data(
    hub: &Hub,
    bot_id: &str,
    slug: &str,
    link: &AppLink,
    rel: &str,
    method: &Method,
    body: &Bytes,
) -> Response {
    let rest = rel.strip_prefix("_data").unwrap_or_default().trim_start_matches('/');
    let parts: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() && *method == Method::GET {
        return match hub.store.app_collections(bot_id, slug) {
            Ok(collections) => json_resp(StatusCode::OK, json!({"collections": collections, "mode": link.mode})),
            Err(e) => json_resp(StatusCode::BAD_REQUEST, json!({"error": e.to_string()})),
        };
    }
    let Some(&collection) = parts.first() else {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "collection required"}));
    };
    let item_id = parts.get(1).copied();

    if *method == Method::GET {
        if let Some(id) = item_id {
            return match hub.store.app_data_get(bot_id, slug, collection, id) {
                Ok(Some(item)) => json_resp(StatusCode::OK, json!({"item": item})),
                Ok(None) => json_resp(StatusCode::NOT_FOUND, json!({"error": "Not found"})),
                Err(e) => json_resp(StatusCode::BAD_REQUEST, json!({"error": e.to_string()})),
            };
        }
        return match hub.store.app_data_list(bot_id, slug, collection, 500) {
            Ok(items) => json_resp(StatusCode::OK, json!({"items": items, "mode": link.mode})),
            Err(e) => json_resp(StatusCode::BAD_REQUEST, json!({"error": e.to_string()})),
        };
    }

    if link.mode != AppLinkMode::Edit {
        return json_resp(StatusCode::FORBIDDEN, json!({"error": "Read-only share link"}));
    }

    if *method == Method::DELETE {
        return match hub.store.app_data_delete(bot_id, slug, collection, item_id) {
            Ok(removed) => {
                emit_app_live(hub, bot_id, slug, json!({"type": "change", "collection": collection, "id": item_id}));
                json_resp(StatusCode::OK, json!({"removed": removed}))
            }
            Err(e) => json_resp(StatusCode::BAD_REQUEST, json!({"error": e.to_string()})),
        };
    }

    if matches!(*method, Method::POST | Method::PUT | Method::PATCH) {
        let Ok(payload) = serde_json::from_slice::<Value>(body) else {
            return json_resp(StatusCode::BAD_REQUEST, json!({"error": "Invalid JSON"}));
        };
        let mut next_val = payload.get("value").cloned().unwrap_or_else(|| payload.clone());
        let target_id = item_id.or_else(|| payload["id"].as_str());
        if *method == Method::PATCH {
            let Some(id) = target_id else {
                return json_resp(StatusCode::BAD_REQUEST, json!({"error": "PATCH requires item id"}));
            };
            if let Ok(Some(existing)) = hub.store.app_data_get(bot_id, slug, collection, id)
                && let (Some(mut base), Some(patch)) = (existing.value.as_object().cloned(), next_val.as_object())
            {
                for (k, v) in patch {
                    base.insert(k.clone(), v.clone());
                }
                next_val = Value::Object(base);
            }
        }
        return match hub.store.app_data_put(bot_id, slug, collection, target_id, &next_val, Some(&link.label)) {
            Ok(item) => {
                emit_app_live(hub, bot_id, slug, json!({"type": "change", "collection": collection, "id": item.id}));
                json_resp(StatusCode::OK, json!({"item": item}))
            }
            Err(e) => json_resp(StatusCode::BAD_REQUEST, json!({"error": e.to_string()})),
        };
    }
    json_resp(StatusCode::METHOD_NOT_ALLOWED, json!({"error": "Method not allowed"}))
}

pub(crate) fn dispatch_bot_turn(hub: &Hub, bot_id: &str, text: &str, from: &str) -> anyhow::Result<()> {
    let row = hub.store.bot(bot_id)?.filter(|r| !r.deleted).ok_or_else(|| anyhow::anyhow!("unknown bot"))?;
    let lane = Lane::main(bot_id);
    let turn = hub.store.max_turn(bot_id) + 1;
    let mut data = json!({"text": text, "status": "queued", "from": from});
    if from == "proactive" {
        data["internal"] = true.into();
    }
    let entry =
        hub.add_entry(&lane, EntryKind::User, turn, &data).ok_or_else(|| anyhow::anyhow!("couldn't save message"))?;
    if !row.config.is_group() && !crate::external::is_external(&row.config) {
        let prompt = crate::chat::prompt::of_entry(hub, &row.config, &entry.data);
        hub.send_cmd(bot_id, Cmd::Send { lane, entry_id: entry.id, text: prompt })?;
    }
    Ok(())
}

fn handle_tell(hub: &Hub, bot_id: &str, app: &BotApp, link: &AppLink, body: &Bytes) -> Response {
    if link.mode != AppLinkMode::Edit {
        return json_resp(StatusCode::FORBIDDEN, json!({"error": "Read-only share link"}));
    }
    let Ok(payload) = serde_json::from_slice::<Value>(body) else {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "Invalid JSON"}));
    };
    let text: String = payload["text"].as_str().unwrap_or_default().trim().chars().take(2000).collect();
    if text.is_empty() {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "text required"}));
    }
    let msg = format!("[From mini-app \"{}\" ({})]: {text}", app.title, link.label);
    match dispatch_bot_turn(hub, bot_id, &msg, "app") {
        Ok(()) => json_resp(StatusCode::OK, json!({"ok": true})),
        Err(e) => json_resp(StatusCode::BAD_REQUEST, json!({"error": e.to_string()})),
    }
}

fn handle_error(hub: &Hub, bot_id: &str, app: &BotApp, body: &Bytes) -> Response {
    let Ok(payload) = serde_json::from_slice::<Value>(body) else {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "Invalid JSON"}));
    };
    let msg: String = payload["message"].as_str().unwrap_or_default().trim().chars().take(600).collect();
    if msg.is_empty() {
        return json_resp(StatusCode::OK, json!({"ok": false}));
    }
    let reported_version = payload["version"].as_i64().unwrap_or(0);
    if reported_version > 0 && reported_version < app.version {
        return json_resp(StatusCode::OK, json!({"ok": false, "stale": true}));
    }
    let stack: String = payload["stack"].as_str().unwrap_or_default().trim().chars().take(1200).collect();
    let repair_prompt = format!(
        "[App runtime error in \"{}\" (apps/{}/{}, v{})]: {msg}{}\n\
Fix the bug in apps/{}/ and call save_app so the open app reloads cleanly.",
        app.title,
        app.slug,
        app.entry,
        app.version,
        if stack.is_empty() { String::new() } else { format!("\n{stack}") },
        app.slug
    );
    let _ = dispatch_bot_turn(hub, bot_id, &repair_prompt, "app");
    json_resp(StatusCode::OK, json!({"ok": true}))
}

fn safe_subpath(root: &FsPath, rel: &str) -> Option<PathBuf> {
    let mut out = root.to_path_buf();
    for comp in FsPath::new(rel).components() {
        match comp {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

fn mime_for(path: &FsPath) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "txt" | "md" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn inject_runtime_tags(html: &str, version: i64) -> String {
    let meta = format!("<meta name=\"bdots-version\" content=\"{version}\">");
    let has_script = html.contains("_bdots.js") || html.contains("_codync.js");
    let tag = if has_script { meta } else { format!("{meta}\n<script src=\"_codync.js\"></script>") };
    if let Some(idx) = html.to_ascii_lowercase().find("<head>") {
        let split = idx + "<head>".len();
        format!("{}\n{tag}{}", &html[..split], &html[split..])
    } else {
        format!("{tag}\n{html}")
    }
}

fn serve_static(hub: &Hub, bot_id: &str, app: &BotApp, rel: &str) -> Response {
    let Ok(Some(row)) = hub.store.bot(bot_id) else {
        return text_resp(StatusCode::NOT_FOUND, "text/plain; charset=utf-8", "Bot not found");
    };
    let app_root = PathBuf::from(&row.config.cwd).join("apps").join(&app.slug);
    let target_rel = if rel.is_empty() { app.entry.as_str() } else { rel };
    let Some(file_path) = safe_subpath(&app_root, target_rel) else {
        return text_resp(StatusCode::FORBIDDEN, "text/plain; charset=utf-8", "Forbidden path");
    };
    let Ok(bytes) = std::fs::read(&file_path) else {
        return text_resp(StatusCode::NOT_FOUND, "text/plain; charset=utf-8", "File not found");
    };
    let mime = mime_for(&file_path);
    if mime.starts_with("text/html") {
        let html = inject_runtime_tags(&String::from_utf8_lossy(&bytes), app.version);
        return text_resp(StatusCode::OK, mime, html);
    }
    text_resp(StatusCode::OK, mime, bytes)
}

fn client_script(app: &BotApp, link: &AppLink) -> String {
    let info = json!({
        "slug": app.slug,
        "title": app.title,
        "version": app.version,
        "mode": link.mode,
        "label": link.label,
    });
    format!(
        r#"// Codync / bdots mini-app runtime helper
(() => {{
  const INFO = {info};
  const base = location.pathname.replace(/\/[^/]*$/, "/");
  const subs = new Map();
  let wsRetries = 0;

  async function req(path, init) {{
    const r = await fetch(base + path, {{
      ...init,
      headers: {{ "content-type": "application/json", ...(init && init.headers) }},
    }});
    const data = await r.json().catch(() => ({{}}));
    if (!r.ok) throw new Error(data.error || r.statusText);
    return data;
  }}

  const api = {{
    info: () => INFO,
    collections: () => req("_data").then(d => d.collections),
    list: (col) => req(`_data/${{encodeURIComponent(col)}}`).then(d => d.items),
    get: (col, id) => req(`_data/${{encodeURIComponent(col)}}/${{encodeURIComponent(id)}}`).then(d => d.item),
    put: (col, value, id) => req(
      id ? `_data/${{encodeURIComponent(col)}}/${{encodeURIComponent(id)}}` : `_data/${{encodeURIComponent(col)}}`,
      {{ method: id ? "PUT" : "POST", body: JSON.stringify({{ id, value }}) }}
    ).then(d => d.item),
    update: (col, id, patch) => req(
      `_data/${{encodeURIComponent(col)}}/${{encodeURIComponent(id)}}`,
      {{ method: "PATCH", body: JSON.stringify({{ value: patch }}) }}
    ).then(d => d.item),
    remove: (col, id) => req(
      id ? `_data/${{encodeURIComponent(col)}}/${{encodeURIComponent(id)}}` : `_data/${{encodeURIComponent(col)}}`,
      {{ method: "DELETE" }}
    ),
    tell: (text) => req("_tell", {{ method: "POST", body: JSON.stringify({{ text }}) }}),
    subscribe: (col, fn) => {{
      if (!subs.has(col)) subs.set(col, new Set());
      subs.get(col).add(fn);
      api.list(col).then(fn).catch(() => {{}});
      return () => subs.get(col)?.delete(fn);
    }},
  }};

  let reported = 0;
  function reportError(message, stack) {{
    if (!message || reported >= 3) return;
    reported++;
    fetch(base + "_error", {{
      method: "POST",
      headers: {{ "content-type": "application/json" }},
      body: JSON.stringify({{ message: String(message), stack: stack ? String(stack) : undefined, version: INFO.version }}),
    }}).catch(() => {{}});
    try {{
      parent.postMessage({{ type: "bdots-app-error", slug: INFO.slug, message: String(message) }}, "*");
    }} catch {{}}
  }}
  window.addEventListener("error", (e) => reportError(e.error?.message || e.message, e.error?.stack));
  window.addEventListener("unhandledrejection", (e) => reportError(e.reason?.message || String(e.reason), e.reason?.stack));

  function connectLive() {{
    try {{
      const proto = location.protocol === "https:" ? "wss:" : "ws:";
      const ws = new WebSocket(`${{proto}}//${{location.host}}${{base}}_live`);
      ws.onopen = () => {{ wsRetries = 0; }};
      ws.onmessage = (ev) => {{
        try {{
          const msg = JSON.parse(ev.data);
          if (msg.type === "reload" && (!msg.version || msg.version > INFO.version)) {{
            location.reload();
          }} else if (msg.type === "change" && msg.collection && subs.has(msg.collection)) {{
            api.list(msg.collection).then((items) => {{
              for (const fn of subs.get(msg.collection) || []) fn(items, msg);
            }}).catch(() => {{}});
          }}
        }} catch {{}}
      }};
      ws.onclose = () => {{
        const delay = Math.min(1000 * Math.pow(1.6, wsRetries++), 15000);
        setTimeout(connectLive, delay);
      }};
    }} catch {{}}
  }}
  connectLive();

  window.codync = api;
  window.bdots = api;
}})();
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_injection_adds_version_meta_and_client_script() {
        let html = "<html><head><title>App</title></head><body>Hello</body></html>";
        let injected = inject_runtime_tags(html, 3);
        assert!(injected.contains("<meta name=\"bdots-version\" content=\"3\">"));
        assert!(injected.contains("<script src=\"_codync.js\"></script>"));
    }

    #[test]
    fn path_traversal_is_rejected() {
        let root = PathBuf::from("/tmp/apps/demo");
        assert!(safe_subpath(&root, "../secret.txt").is_none());
        assert!(safe_subpath(&root, "assets/app.js").is_some());
    }
}
