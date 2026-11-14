//! The keeper's loopback proxy: `http://127.0.0.1:<port>/t/<token><rest>?<query>` goes to the granted record's
//! `upstream` + `<rest>` + the query, with the record's headers in place of whatever credentials the client sent.
//! Where a request goes is the record's alone: the client picks only the path below it and the query.
//!
//! Answers say nothing about why a request was refused (401 for an unknown or expired token), and no secret goes
//! into a log line, an error or a response header. Only the methods an API takes go through: TRACE would echo the
//! request back, the secret in it too.

use super::daemon::Keeper;
use super::record::{self, Record};
use crate::market::oauth::{Renewal, renewal};
use anyhow::{Context, Result, anyhow, bail, ensure};
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, request::Parts};
use axum::response::{IntoResponse, Response};
use serde_json::Value;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

const MAX_BODY: usize = 64 * 1024 * 1024;
/// Refresh a sign-in this long before its token expires.
const EARLY_SECS: u64 = 60;
/// Passed on in neither direction: hop-by-hop headers (the body is streamed again here).
const HOP_BY_HOP: [&str; 8] =
    ["connection", "keep-alive", "proxy-connection", "te", "trailer", "transfer-encoding", "upgrade", "content-length"];
/// Never forwarded from the client either: where it goes, and any credential it may carry.
const FROM_CLIENT: [&str; 6] =
    ["host", "authorization", "proxy-authorization", "x-api-key", "x-goog-api-key", "cookie"];
const METHODS: [Method; 7] =
    [Method::GET, Method::HEAD, Method::POST, Method::PUT, Method::PATCH, Method::DELETE, Method::OPTIONS];

/// The proxy's client follows no redirect: one to another host would carry the record's headers there.
fn client() -> &'static reqwest::Client {
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(30))
            .build()
            .expect("TLS setup failed: install the ca-certificates package")
    });
    &CLIENT
}

pub async fn serve(keeper: Arc<Keeper>, listener: tokio::net::TcpListener) -> Result<()> {
    let app = axum::Router::new().fallback(handle).with_state(keeper);
    axum::serve(listener, app).await.context("the keeper's proxy stopped")
}

async fn handle(State(keeper): State<Arc<Keeper>>, request: Request) -> Response {
    forward(&keeper, request).await.unwrap_or_else(IntoResponse::into_response)
}

async fn forward(keeper: &Arc<Keeper>, request: Request) -> Result<Response, StatusCode> {
    let (token, rest) = split(request.uri().path()).ok_or(StatusCode::UNAUTHORIZED)?;
    let slot = keeper.granted(token).ok_or(StatusCode::UNAUTHORIZED)?;
    if !METHODS.contains(request.method()) {
        return Err(StatusCode::METHOD_NOT_ALLOWED);
    }
    let mut record = match keeper.record(&slot) {
        Ok(Some(record)) => record,
        Ok(None) => return Err(StatusCode::UNAUTHORIZED),
        Err(_) if !keeper.unlocked() => return Err(StatusCode::SERVICE_UNAVAILABLE),
        Err(e) => {
            tracing::warn!(slot, error = format!("{e:#}"), "couldn't open a proxied record");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let url = target(&record, rest, request.uri().query()).map_err(|_| StatusCode::BAD_REQUEST)?;
    let (parts, body) = request.into_parts();
    let body = axum::body::to_bytes(body, MAX_BODY).await.map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?;
    if record.oauth.as_ref().is_some_and(|o| o.expires_at != 0 && o.expires_at <= now() + EARLY_SECS) {
        record = refreshed(keeper, &slot, record).await;
    }
    let mut answer = send(&slot, &record, &parts, &url, body.clone()).await?;
    // Once, on a 401: the sign-in may have ended early.
    if answer.status() == StatusCode::UNAUTHORIZED && record.oauth.is_some() {
        let fresh = refreshed(keeper, &slot, record.clone()).await;
        if fresh.oauth.is_some() && fresh != record {
            answer = send(&slot, &fresh, &parts, &url, body).await?;
        }
    }
    Ok(response(answer))
}

/// `/t/<token><rest>` → the token and the rest of the path (`""` or starting with `/`).
fn split(path: &str) -> Option<(&str, &str)> {
    let tail = path.strip_prefix("/t/")?;
    let (token, rest) = tail.find('/').map_or((tail, ""), |i| tail.split_at(i));
    (!token.is_empty()).then_some((token, rest))
}

/// The upstream URL for `rest` and the client's `query`, without the parameters the record strips.
fn target(record: &Record, rest: &str, query: Option<&str>) -> Result<reqwest::Url> {
    let upstream = record::upstream(&record.upstream)?;
    let mut url = upstream.clone();
    if !rest.is_empty() {
        let path = format!("{}{rest}", upstream.path().trim_end_matches('/'));
        url.set_path(&path);
    }
    let query = query.map(|q| {
        if record.strip_query.is_empty() {
            return q.to_owned();
        }
        let kept =
            url::form_urlencoded::parse(q.as_bytes()).filter(|(k, _)| !record.strip_query.iter().any(|s| s == k));
        url::form_urlencoded::Serializer::new(String::new()).extend_pairs(kept).finish()
    });
    url.set_query(query.as_deref().filter(|q| !q.is_empty()));
    ensure!(url.origin() == upstream.origin(), "a request may not leave its upstream");
    Ok(url)
}

async fn send(
    slot: &str,
    record: &Record,
    parts: &Parts,
    url: &reqwest::Url,
    body: axum::body::Bytes,
) -> Result<reqwest::Response, StatusCode> {
    let mut headers = HeaderMap::new();
    for (name, value) in &parts.headers {
        let n = name.as_str();
        if !HOP_BY_HOP.contains(&n) && !FROM_CLIENT.contains(&n) && !record.headers.contains_key(n) {
            headers.append(name.clone(), value.clone());
        }
    }
    for (name, value) in &record.headers {
        let (Ok(name), Ok(mut value)) = (HeaderName::from_bytes(name.as_bytes()), HeaderValue::from_str(value)) else {
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        };
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    client().request(parts.method.clone(), url.clone()).headers(headers).body(body).send().await.map_err(|e| {
        tracing::warn!(slot, error = %e.without_url(), "the upstream couldn't be reached");
        StatusCode::BAD_GATEWAY
    })
}

/// The upstream's answer as it arrives (an SSE stream flows event by event).
fn response(answer: reqwest::Response) -> Response {
    let mut builder = Response::builder().status(answer.status());
    for (name, value) in answer.headers() {
        if !HOP_BY_HOP.contains(&name.as_str()) {
            builder = builder.header(name, value);
        }
    }
    builder.body(Body::from_stream(answer.bytes_stream())).unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// `record` with a fresh access token; as it was while the token endpoint can't answer (the upstream then says why);
/// without its sign-in once that ended (no refresh token, or the endpoint refused it), which the host then shows as
/// signed out (`signedIn`).
async fn refreshed(keeper: &Arc<Keeper>, slot: &str, record: Record) -> Record {
    match refresh(keeper, slot, &record).await {
        Ok(fresh) => fresh,
        Err(e) => {
            tracing::warn!(slot, error = format!("{e:#}"), "couldn't refresh a sign-in");
            record
        }
    }
}

/// Trades `slot`'s refresh token for a new access token and saves both, or drops a sign-in that ended. When another
/// request refreshed it while this one waited, or the host saved a new sign-in meanwhile, that one is used instead.
async fn refresh(keeper: &Arc<Keeper>, slot: &str, seen: &Record) -> Result<Record> {
    let _one = keeper.refresh.lock().await;
    let current = keeper.record(slot)?.ok_or_else(|| anyhow!("the record is gone"))?;
    if current != *seen {
        return Ok(current);
    }
    let mut oauth = current.oauth.clone().ok_or_else(|| anyhow!("the record has no sign-in"))?;
    let Some(refresh_token) = oauth.refresh_token.clone() else {
        tracing::info!(slot, "a sign-in without a refresh token ended");
        return save(keeper, slot, current.clone(), signed_out(current)).await;
    };
    // Built before the request: the serializer can't be held across an await.
    let form = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("grant_type", "refresh_token")
            .append_pair("refresh_token", &refresh_token)
            .append_pair("client_id", &oauth.client_id);
        if let Some(secret) = &oauth.client_secret {
            form.append_pair("client_secret", secret);
        }
        if let Some(resource) = &oauth.resource {
            form.append_pair("resource", resource);
        }
        form.finish()
    };
    let answer = client()
        .post(&oauth.token_url)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .body(form)
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| anyhow!("can't reach the token endpoint: {}", e.without_url()))?;
    let status = answer.status();
    let v: Value = answer.json().await.unwrap_or(Value::Null);
    let fresh = match renewal(status, &v) {
        Renewal::Renewed => {
            let text = |k: &str| v[k].as_str().filter(|s| !s.is_empty()).map(str::to_owned);
            let access = text("access_token").ok_or_else(|| anyhow!("the token endpoint sent no access token"))?;
            let mut fresh = current.clone();
            fresh.headers.insert(oauth.header.clone(), format!("Bearer {access}"));
            if let Some(rotated) = text("refresh_token") {
                oauth.refresh_token = Some(rotated);
            }
            oauth.expires_at = v["expires_in"].as_u64().map_or(0, |s| now() + s);
            fresh.oauth = Some(oauth);
            fresh.checked().map_err(|_| anyhow!("the token endpoint sent an unusable token"))?
        }
        Renewal::Retry => bail!("the token endpoint can't refresh it now ({status})"),
        Renewal::Revoked | Renewal::Unregistered => {
            tracing::info!(slot, %status, "the token endpoint ended a sign-in");
            signed_out(current.clone())
        }
    };
    save(keeper, slot, current, fresh).await
}

/// `record` without its sign-in: the grant to refresh it, and the access token it sent.
fn signed_out(mut record: Record) -> Record {
    if let Some(o) = record.oauth.take() {
        record.headers.remove(&o.header);
    }
    record
}

/// Saves `fresh` in `slot` in place of `current`; when the record changed meanwhile, that one stays and is used.
async fn save(keeper: &Arc<Keeper>, slot: &str, current: Record, fresh: Record) -> Result<Record> {
    let keeper = keeper.clone();
    let slot = slot.to_owned();
    tokio::task::spawn_blocking(move || {
        let now = keeper.record(&slot)?;
        if now.as_ref() != Some(&current) {
            return now.ok_or_else(|| anyhow!("the record is gone"));
        }
        keeper.put_record(&slot, &fresh)?;
        Ok(fresh)
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_stay_under_their_upstream() {
        assert_eq!(split("/t/abc/v1/models"), Some(("abc", "/v1/models")));
        assert_eq!(split("/t/abc"), Some(("abc", "")));
        assert_eq!(split("/t//x"), None);
        assert_eq!(split("/x/abc"), None);
        let mut record = Record::header("https://api.example.com/v1", "x-api-key", "k".into());
        let url = |r: &Record, rest, q| target(r, rest, q).unwrap().to_string();
        assert_eq!(url(&record, "/models", None), "https://api.example.com/v1/models");
        assert_eq!(url(&record, "", Some("a=1")), "https://api.example.com/v1?a=1");
        assert_eq!(url(&record, "//evil.example/x", None), "https://api.example.com/v1//evil.example/x");
        assert!(url(&record, "/../../x", None).starts_with("https://api.example.com/"));
        assert!(url(&record, "@evil.example/x", None).starts_with("https://api.example.com/"));
        record.strip_query = vec!["key".into()];
        assert_eq!(url(&record, "/m", Some("key=codync-keeper&alt=sse")), "https://api.example.com/v1/m?alt=sse");
        assert_eq!(url(&record, "/m", Some("key=x")), "https://api.example.com/v1/m");
    }
}
