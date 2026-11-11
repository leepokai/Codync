//! Source reference: Hermes gateway/platforms/api_server_openai_routes.py (MIT, 10d25f8a).
//! Adaptation: local scoped authentication and completions through Codync's normal bot actor.

use super::openai_keys::{self, Grant};
use crate::hub::Hub;
use anyhow::{Context, Result, ensure};
use axum::body::Bytes;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::{StreamExt, stream};
use serde_json::{Value, json};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

pub(super) fn router() -> Router<Arc<Hub>> {
    Router::new()
        .route("/v1/models", get(models))
        .route("/v1/health", get(health))
        .route("/v1/pricing", get(pricing))
        .route("/v1/chat/completions", post(completions))
        .layer(axum::extract::DefaultBodyLimit::max(256 * 1024))
}

fn error(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(json!({"error":{"message":message,"type":"invalid_request_error","param":null,"code":status.as_u16()}})),
    )
        .into_response()
}

async fn authorized(hub: &Arc<Hub>, peer: SocketAddr, headers: &HeaderMap) -> std::result::Result<Grant, Response> {
    if !peer.ip().to_canonical().is_loopback() {
        return Err(error(StatusCode::UNAUTHORIZED, "Local API keys only work on this computer"));
    }
    match openai_keys::authenticate(hub, headers.get("authorization").and_then(|s| s.to_str().ok())).await {
        Ok(Some(grant)) => Ok(grant),
        Ok(None) => Err(error(StatusCode::UNAUTHORIZED, "Invalid API key")),
        Err(cause) => {
            tracing::warn!(error = format!("{cause:#}"), "local bot API credential storage unavailable");
            Err(error(StatusCode::SERVICE_UNAVAILABLE, "Credential storage is unavailable"))
        }
    }
}

fn model_list(hub: &Hub, grant: &Grant) -> Result<Vec<Value>> {
    let mut models = Vec::new();
    for row in hub.store.bots()? {
        let cfg = row.config;
        if row.deleted || cfg.hidden || cfg.is_group() || !grant.bots.contains(&cfg.id) {
            continue;
        }
        models.push(json!({"id":format!("bot/{}",cfg.id),"object":"model","created":0,"owned_by":"codync",
            "name":cfg.name,"provider":cfg.backend,"model":cfg.model}));
    }
    Ok(models)
}

async fn models(
    State(hub): State<Arc<Hub>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    let grant = match authorized(&hub, peer, &headers).await {
        Ok(g) => g,
        Err(e) => return e,
    };
    match model_list(&hub, &grant) {
        Ok(data) => Json(json!({"object":"list","data":data})).into_response(),
        Err(cause) => {
            tracing::warn!(error = format!("{cause:#}"), "local bot API could not read model catalog");
            error(StatusCode::INTERNAL_SERVER_ERROR, "Could not read models")
        }
    }
}

async fn health(
    State(hub): State<Arc<Hub>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    match authorized(&hub, peer, &headers).await {
        Ok(_) => Json(json!({"status":"ok","version":env!("CARGO_PKG_VERSION")})).into_response(),
        Err(e) => e,
    }
}

async fn pricing(
    State(hub): State<Arc<Hub>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    let grant = match authorized(&hub, peer, &headers).await {
        Ok(g) => g,
        Err(e) => return e,
    };
    match model_list(&hub, &grant) {
        Ok(models) => Json(json!({"object":"list","data":models.into_iter().map(|mut m| {
            // Subscription harnesses do not publish a per-token price. Unknown is never reported as free.
            m["inputPricePerToken"]=Value::Null;m["outputPricePerToken"]=Value::Null;m
        }).collect::<Vec<_>>()}))
        .into_response(),
        Err(cause) => {
            tracing::warn!(error = format!("{cause:#}"), "local bot API could not read pricing catalog");
            error(StatusCode::INTERNAL_SERVER_ERROR, "Could not read models")
        }
    }
}

fn parse(body: &Value, grant: &Grant) -> Result<(String, String, bool)> {
    let fields = body.as_object().context("Request must be an object")?;
    for name in fields.keys() {
        ensure!(
            matches!(name.as_str(), "model" | "messages" | "stream" | "n" | "stream_options"),
            "Unsupported request field {name}; generation settings and tools belong to the bot"
        );
    }
    if let Some(n) = body.get("n") {
        ensure!(n.as_u64() == Some(1), "Only n=1 is supported");
    }
    if let Some(options) = body.get("stream_options").filter(|v| !v.is_null()) {
        let options = options.as_object().context("stream_options must be an object")?;
        ensure!(
            options.is_empty() || (options.len() == 1 && options.get("include_usage") == Some(&Value::Bool(false))),
            "Usage reporting is unavailable for this bot endpoint"
        );
    }
    let model = body["model"].as_str().context("model is required")?;
    let bot = model.strip_prefix("bot/").context("Select a model id from /v1/models")?;
    ensure!(grant.bots.iter().any(|id| id == bot), "This key cannot access that bot");
    ensure!(
        body.get("tools").is_none() && body.get("tool_choice").is_none(),
        "Use the bot's configured tools; client-defined tools are not supported"
    );
    let messages = body["messages"]
        .as_array()
        .filter(|m| !m.is_empty() && m.len() <= 200)
        .context("messages must contain 1 to 200 text messages")?;
    for message in messages {
        ensure!(
            matches!(message["role"].as_str(), Some("system" | "developer" | "user" | "assistant")),
            "Unsupported message role"
        );
        ensure!(message["content"].is_string(), "Only text message content is supported");
    }
    let prompt = serde_json::to_string(messages)?;
    ensure!(prompt.len() <= 128 * 1024, "Conversation is too long");
    let streaming = match body.get("stream") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        _ => anyhow::bail!("stream must be a boolean"),
    };
    Ok((bot.to_owned(), prompt, streaming))
}

async fn completions(
    State(hub): State<Arc<Hub>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let grant = match authorized(&hub, peer, &headers).await {
        Ok(g) => g,
        Err(e) => return e,
    };
    let request: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return error(StatusCode::BAD_REQUEST, "Invalid JSON"),
    };
    let (bot, prompt, streaming) = match parse(&request, &grant) {
        Ok(v) => v,
        Err(e) => return error(StatusCode::BAD_REQUEST, &e.to_string()),
    };
    let id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
    let model = format!("bot/{bot}");
    let created = crate::store::now_ms() / 1000;
    let caller = format!("openai:{}", grant.id);
    let complete = async move { crate::chat::team::openai::complete(&hub, &caller, &bot, &prompt).await };
    if !streaming {
        return match complete.await {
            Ok(text) => Json(json!({"id":id,"object":"chat.completion","created":created,"model":model,
                "choices":[{"index":0,"message":{"role":"assistant","content":text},"finish_reason":"stop"}]}))
            .into_response(),
            Err(e) => error(StatusCode::BAD_GATEWAY, &e.to_string()),
        };
    }
    // Keep-alives hold the HTTP connection while tools or approvals are pending. The final answer
    // is a single content delta; dropping the stream drops the future and cancels only its request.
    let answer = stream::once(async move {
        let payload = match complete.await {
            Ok(text) => json!({"id":id,"object":"chat.completion.chunk","created":created,"model":model,
                "choices":[{"index":0,"delta":{"role":"assistant","content":text},"finish_reason":"stop"}]}),
            Err(e) => json!({"error":{"message":e.to_string(),"type":"server_error"}}),
        };
        Ok::<_, Infallible>(Event::default().data(payload.to_string()))
    });
    let tail = stream::once(async { Ok::<_, Infallible>(Event::default().data("[DONE]")) });
    Sse::new(answer.chain(tail)).keep_alive(KeepAlive::default()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::identity::Identity;
    use crate::store::{BotConfig, Store};

    fn grant() -> Grant {
        serde_json::from_value(json!({"id":"test","label":"Test","hash":"unused","bots":["a"],"createdAt":0})).unwrap()
    }

    #[test]
    fn hermes_openai_completion_validation_enforces_key_scope_and_text_roles() {
        let body = json!({"model":"bot/a","messages":[{"role":"user","content":"hello"}],"stream":true});
        let (bot, prompt, streaming) = parse(&body, &grant()).unwrap();
        assert_eq!(bot, "a");
        assert!(streaming);
        assert!(prompt.contains("hello"));
        for patch in [
            json!({"model":"bot/b"}),
            json!({"messages":[]}),
            json!({"messages":[{"role":"tool","content":"unsafe"}]}),
            json!({"tools":[]}),
            json!({"stream":"yes"}),
            json!({"max_tokens":1}),
            json!({"response_format":{"type":"json_object"}}),
            json!({"n":2}),
            json!({"stream_options":{"include_usage":true}}),
        ] {
            let mut request = body.clone();
            for (key, value) in patch.as_object().unwrap() {
                request[key] = value.clone();
            }
            assert!(parse(&request, &grant()).is_err(), "{patch}");
        }
    }

    #[tokio::test]
    async fn hermes_openai_http_routes_authenticate_before_parsing_and_list_only_granted_bots() {
        let dir = std::env::temp_dir().join(format!("codync-openai-http-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        for id in ["a", "b"] {
            let bot: BotConfig =
                serde_json::from_value(json!({"id":id,"name":id,"backend":"hermes","cwd":dir})).unwrap();
            store.save_bot(&bot).unwrap();
        }
        let hub = Hub::new(store, "openai-http-test".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0);
        let saved = openai_keys::call(&hub, "openaiCreateKey", &json!({"label":"Test","bots":["a"]})).await.unwrap();
        let token = saved["key"].as_str().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = super::super::http::router(hub);
        let server = tokio::spawn(async move {
            axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap();
        });
        let client = reqwest::Client::new();
        let base = format!("http://{address}/v1");
        let refused = client.post(format!("{base}/chat/completions")).body("not JSON").send().await.unwrap();
        assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
        let models: Value =
            client.get(format!("{base}/models")).bearer_auth(token).send().await.unwrap().json().await.unwrap();
        assert_eq!(models["data"].as_array().unwrap().len(), 1);
        assert_eq!(models["data"][0]["id"], "bot/a");
        let denied = client
            .post(format!("{base}/chat/completions"))
            .bearer_auth(token)
            .json(&json!({"model":"bot/b","messages":[{"role":"user","content":"hello"}]}))
            .send()
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::BAD_REQUEST);
        let pricing: Value =
            client.get(format!("{base}/pricing")).bearer_auth(token).send().await.unwrap().json().await.unwrap();
        assert!(pricing["data"][0]["inputPricePerToken"].is_null());
        server.abort();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
