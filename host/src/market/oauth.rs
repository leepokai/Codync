//! Sign-in for remote MCP connectors (MCP authorization: OAuth 2.1 + PKCE,
//! protected resource metadata, dynamic client registration).
//!
//! The tokens live on this computer. The sign-in page redirects to one of two places:
//! - `http://127.0.0.1:<port>/oauth/callback`, served by this host, when the user signs
//!   in on this computer (Mac app, TUI);
//! - `<cloud>/v1/oauth/callback`, a stateless page that bounces to `codync://oauth?…`,
//!   which the phone's sign-in sheet catches and hands back with `connectorSignInFinish`.
//!
//! The authorization code alone is useless: the PKCE verifier never leaves this host.

use crate::LockExt;
use crate::market::{self, Connector};
use crate::store::Store;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const TIMEOUT: Duration = Duration::from_secs(20);
/// How long a started sign-in waits for its callback.
const PENDING_FOR: Duration = Duration::from_secs(15 * 60);
/// Tokens this close to expiring are refreshed before use.
const EARLY: u64 = 60;

/// A connector's OAuth client and tokens. Present (maybe without tokens) when the server asks for sign-in.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OAuth {
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    /// The redirect URIs `client_id` was registered with.
    #[serde(default)]
    pub redirect_uris: Vec<String>,
    #[serde(default)]
    pub token_endpoint: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Unix seconds.
    #[serde(default)]
    pub expires_at: Option<u64>,
}

impl OAuth {
    pub fn signed_in(&self) -> bool {
        !self.access_token.is_empty()
    }
}

/// Where the sign-in page sends the user back to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Callback {
    /// This host's loopback page (the browser runs on this computer).
    Host,
    /// The cloud's bounce page, caught by the phone app.
    App,
}

struct Pending {
    connector: String,
    verifier: String,
    redirect_uri: String,
    resource: String,
    started: Instant,
}

static PENDING: Mutex<Option<HashMap<String, Pending>>> = Mutex::new(None);
/// One refresh at a time: servers that rotate refresh tokens revoke the old one on use.
static REFRESH: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn loopback_uri(port: u16) -> String {
    format!("http://127.0.0.1:{port}/oauth/callback")
}

fn app_uri(store: &Store) -> Option<String> {
    crate::remote::cloud::url(store).map(|base| {
        let suffix = if crate::environment::Environment::current() == crate::environment::Environment::Dev {
            "/dev"
        } else {
            ""
        };
        format!("{base}/v1/oauth/callback{suffix}")
    })
}

async fn get_json(url: &str) -> Result<Value> {
    let res = crate::http().get(url).header("accept", "application/json").timeout(TIMEOUT).send().await?;
    if !res.status().is_success() {
        bail!("{url} answered {}", res.status());
    }
    Ok(res.json().await?)
}

/// `true` when the server answers an unauthenticated `initialize` with 401 (it wants sign-in).
pub async fn required(url: &str, headers: &std::collections::BTreeMap<String, String>) -> bool {
    let mut req = crate::http()
        .post(url)
        .header("accept", "application/json, text/event-stream")
        .json(&json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
            "protocolVersion": crate::mcp::PROTOCOL_VERSION, "capabilities": {},
            "clientInfo": {"name": "codync", "version": env!("CARGO_PKG_VERSION")},
        }}))
        .timeout(TIMEOUT);
    for (k, v) in headers {
        req = req.header(k, v);
    }
    let Ok(res) = req.send().await else { return false };
    if !matches!(res.status().as_u16(), 400 | 404 | 405) {
        return res.status() == reqwest::StatusCode::UNAUTHORIZED;
    }
    // Not streamable HTTP: ask the older SSE endpoint instead (the stream is dropped unread).
    let mut req = crate::http().get(url).header("accept", "text/event-stream").timeout(TIMEOUT);
    for (k, v) in headers {
        req = req.header(k, v);
    }
    req.send().await.is_ok_and(|r| r.status() == reqwest::StatusCode::UNAUTHORIZED)
}

/// `resource_metadata="…"` from a `WWW-Authenticate: Bearer …` header.
fn resource_metadata_hint(header: &str) -> Option<String> {
    let (_, rest) = header.split_once("resource_metadata=")?;
    let rest = rest.trim_start_matches('"');
    let end = rest.find(['"', ',', ' ']).unwrap_or(rest.len());
    Some(rest[..end].to_owned()).filter(|s| s.starts_with("https://") || s.starts_with("http://"))
}

/// `https://a.b/x/y` → (`https://a.b`, `/x/y`), without a trailing `/`.
fn split(url: &str) -> Result<(String, String)> {
    let u = reqwest::Url::parse(url)?;
    let path = u.path().trim_end_matches('/').to_owned();
    Ok((u.origin().ascii_serialization(), path))
}

/// The protected resource metadata (RFC 9728): which authorization server, which scopes.
async fn resource_metadata(url: &str) -> Value {
    let hint = crate::http()
        .post(url)
        .header("accept", "application/json, text/event-stream")
        .json(&json!({"jsonrpc": "2.0", "id": 0, "method": "ping"}))
        .timeout(TIMEOUT)
        .send()
        .await
        .ok()
        .and_then(|r| r.headers().get("www-authenticate")?.to_str().ok().and_then(resource_metadata_hint));
    let mut tries: Vec<String> = hint.into_iter().collect();
    if let Ok((origin, path)) = split(url) {
        tries.push(format!("{origin}/.well-known/oauth-protected-resource{path}"));
        tries.push(format!("{origin}/.well-known/oauth-protected-resource"));
    }
    for t in tries {
        if let Ok(v) = get_json(&t).await {
            return v;
        }
    }
    Value::Null
}

/// The authorization server metadata (RFC 8414, then `OpenID` discovery).
async fn server_metadata(issuer: &str) -> Result<Value> {
    let (origin, path) = split(issuer)?;
    for t in [
        format!("{origin}/.well-known/oauth-authorization-server{path}"),
        format!("{origin}/.well-known/openid-configuration{path}"),
        format!("{issuer}/.well-known/openid-configuration"),
    ] {
        if let Ok(v) = get_json(&t).await
            && v["authorization_endpoint"].is_string()
        {
            return Ok(v);
        }
    }
    bail!("{origin} doesn't describe how to sign in")
}

fn text(v: &Value) -> Option<String> {
    v.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

fn update(store: &Store, id: &str, f: impl FnOnce(&mut Connector)) -> Result<Connector> {
    market::update_connectors(store, |all| {
        let c = all.iter_mut().find(|c| c.id == id).ok_or_else(|| anyhow!("unknown connector"))?;
        f(c);
        Ok(c.clone())
    })
}

/// Starts signing in to connector `id`: the page to open.
pub async fn start(store: &Store, port: u16, id: &str, callback: Callback) -> Result<Value> {
    let c = market::connectors(store)?.into_iter().find(|c| c.id == id).ok_or_else(|| anyhow!("unknown connector"))?;
    let url = c.url.clone().ok_or_else(|| anyhow!("{} runs on this computer; it has nothing to sign in to", c.name))?;
    let app = app_uri(store);
    let redirect_uri = match callback {
        Callback::Host => loopback_uri(port),
        Callback::App => app
            .clone()
            .ok_or_else(|| anyhow!("The Codync cloud is off on this computer, so sign in from the computer itself."))?,
    };

    let resource = resource_metadata(&url).await;
    let issuer = resource["authorization_servers"][0]
        .as_str()
        .map_or_else(|| split(&url).map(|(origin, _)| origin), |s| Ok(s.trim_end_matches('/').to_owned()))?;
    let meta = server_metadata(&issuer).await.with_context(|| format!("signing in to {}", c.name))?;
    let authorize = text(&meta["authorization_endpoint"]).ok_or_else(|| anyhow!("no authorization endpoint"))?;
    let token_endpoint = text(&meta["token_endpoint"]).ok_or_else(|| anyhow!("no token endpoint"))?;

    // Register once with every redirect URI we might use; again if they changed.
    let wanted: Vec<String> = std::iter::once(loopback_uri(port)).chain(app).collect();
    let mut oauth = c.oauth.clone().unwrap_or_default();
    if oauth.client_id.is_empty() || !wanted.iter().all(|u| oauth.redirect_uris.contains(u)) {
        let register = text(&meta["registration_endpoint"]).ok_or_else(|| {
            anyhow!(
                "{} doesn't let new apps sign in on their own. Add it as a custom connector with a token header.",
                c.name
            )
        })?;
        let res = crate::http()
            .post(&register)
            .json(&json!({
                "client_name": "Codync",
                "redirect_uris": wanted,
                "grant_types": ["authorization_code", "refresh_token"],
                "response_types": ["code"],
                "token_endpoint_auth_method": "none",
            }))
            .timeout(TIMEOUT)
            .send()
            .await
            .with_context(|| format!("registering with {}", c.name))?;
        let status = res.status();
        let v: Value = res.json().await.unwrap_or(Value::Null);
        oauth.client_id =
            text(&v["client_id"]).ok_or_else(|| anyhow!("{} refused the registration ({status})", c.name))?;
        oauth.client_secret = text(&v["client_secret"]);
        oauth.redirect_uris = wanted;
    }
    oauth.token_endpoint = token_endpoint;
    let client_id = oauth.client_id.clone();
    update(store, id, |c| c.oauth = Some(oauth))?;

    let verifier = crate::remote::crypto::b64(&crate::remote::crypto::random::<32>());
    let challenge = crate::remote::crypto::b64(&crate::remote::crypto::sha256(&[verifier.as_bytes()]));
    let state = crate::remote::crypto::b64(&crate::remote::crypto::random::<16>());
    let mut link = reqwest::Url::parse(&authorize)?;
    {
        let mut q = link.query_pairs_mut();
        q.append_pair("response_type", "code")
            .append_pair("client_id", &client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state)
            .append_pair("resource", &url);
        let scopes: Vec<&str> =
            resource["scopes_supported"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        if !scopes.is_empty() {
            q.append_pair("scope", &scopes.join(" "));
        }
    }
    let mut pending = PENDING.locked();
    let map = pending.get_or_insert_with(HashMap::new);
    map.retain(|_, p| p.started.elapsed() < PENDING_FOR);
    map.insert(
        state,
        Pending { connector: id.to_owned(), verifier, redirect_uri, resource: url, started: Instant::now() },
    );
    let callback = match callback {
        Callback::Host => "host",
        Callback::App => "app",
    };
    Ok(json!({"url": link.as_str(), "callback": callback}))
}

/// Trades the code from the sign-in page for tokens.
pub async fn finish(store: &Store, state: &str, code: Option<&str>, error: Option<&str>) -> Result<Connector> {
    let p = PENDING
        .locked()
        .as_mut()
        .and_then(|m| m.remove(state))
        .filter(|p| p.started.elapsed() < PENDING_FOR)
        .ok_or_else(|| anyhow!("This sign-in expired. Start it again."))?;
    if let Some(error) = error {
        bail!("Sign-in didn't finish: {error}");
    }
    let code = code.ok_or_else(|| anyhow!("Sign-in didn't return a code"))?;
    let c = market::connectors(store)?
        .into_iter()
        .find(|c| c.id == p.connector)
        .ok_or_else(|| anyhow!("unknown connector"))?;
    let oauth = c.oauth.ok_or_else(|| anyhow!("unknown connector"))?;
    let v = token_request(
        &oauth,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &p.redirect_uri),
            ("code_verifier", &p.verifier),
            ("resource", &p.resource),
        ],
    )
    .await?;
    update(store, &p.connector, |c| store_tokens(c, &v))
}

async fn token_request(oauth: &OAuth, form: &[(&str, &str)]) -> Result<Value> {
    let mut form: Vec<(&str, &str)> = form.to_vec();
    form.push(("client_id", &oauth.client_id));
    if let Some(secret) = &oauth.client_secret {
        form.push(("client_secret", secret));
    }
    let body = form.iter().map(|(k, v)| format!("{}={}", encode(k), encode(v))).collect::<Vec<_>>().join("&");
    let res = crate::http()
        .post(&oauth.token_endpoint)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .body(body)
        .timeout(TIMEOUT)
        .send()
        .await
        .context("can't reach the sign-in server")?;
    let status = res.status();
    let v: Value = res.json().await.unwrap_or(Value::Null);
    if !status.is_success() || !v["access_token"].is_string() {
        bail!("The sign-in server refused the request ({status}). Reopen sign-in and try again.");
    }
    Ok(v)
}

fn encode(s: &str) -> String {
    reqwest::Url::parse_with_params("x:", [("", s)])
        .ok()
        .and_then(|u| u.query().map(|q| q.trim_start_matches('=').to_owned()))
        .unwrap_or_default()
}

fn store_tokens(c: &mut Connector, v: &Value) {
    let o = c.oauth.get_or_insert_with(OAuth::default);
    o.access_token = text(&v["access_token"]).unwrap_or_default();
    if let Some(r) = text(&v["refresh_token"]) {
        o.refresh_token = Some(r);
    }
    o.expires_at = v["expires_in"].as_u64().map(|s| now() + s);
}

/// Forgets a connector's tokens (keeps its registration).
pub fn sign_out(store: &Store, id: &str) -> Result<Connector> {
    update(store, id, |c| {
        if let Some(o) = &mut c.oauth {
            o.access_token.clear();
            o.refresh_token = None;
            o.expires_at = None;
        }
    })
}

/// What the `remote` MCP proxy sends to connector `id`: its URL and headers, with a fresh token.
/// `stale`: the server just said 401, so refresh even if the token looks valid.
pub async fn target(store: &Store, id: &str, stale: bool) -> Result<Value> {
    let c = market::connectors(store)?.into_iter().find(|c| c.id == id).ok_or_else(|| anyhow!("unknown connector"))?;
    let url = c.url.clone().ok_or_else(|| anyhow!("{} isn't a remote connector", c.name))?;
    let mut headers = std::collections::BTreeMap::new();
    for (k, v) in &c.headers {
        headers.insert(k.clone(), market::passwords::resolve(store, v).await?);
    }
    if let Some(o) = &c.oauth {
        let o = fresh(store, &c, o, stale).await?;
        headers.insert("Authorization".into(), format!("Bearer {}", o.access_token));
    }
    Ok(json!({"url": url, "headers": headers}))
}

async fn fresh(store: &Store, c: &Connector, o: &OAuth, stale: bool) -> Result<OAuth> {
    let signed_out = || anyhow!("Sign in to {} in Marketplace first.", c.name);
    if !o.signed_in() {
        return Err(signed_out());
    }
    let expiring = o.expires_at.is_some_and(|t| t <= now() + EARLY);
    if !stale && !expiring {
        return Ok(o.clone());
    }
    let _one = REFRESH.lock().await;
    // Someone else may have refreshed while we waited.
    let current = market::connectors(store)?.into_iter().find(|x| x.id == c.id).and_then(|x| x.oauth);
    if let Some(cur) = &current
        && cur.access_token != o.access_token
        && cur.signed_in()
    {
        return Ok(cur.clone());
    }
    let Some(refresh) = o.refresh_token.clone() else {
        sign_out(store, &c.id)?;
        return Err(signed_out());
    };
    match token_request(o, &[("grant_type", "refresh_token"), ("refresh_token", &refresh)]).await {
        Ok(v) => Ok(update(store, &c.id, |c| store_tokens(c, &v))?.oauth.unwrap_or_default()),
        Err(error) => {
            tracing::warn!(connector = %c.id, error = format!("{error:#}"), "token refresh failed");
            sign_out(store, &c.id)?;
            Err(signed_out())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_resource_metadata_hint() {
        let h = r#"Bearer error="invalid_token", resource_metadata="https://mcp.example.com/.well-known/oauth-protected-resource""#;
        assert_eq!(
            resource_metadata_hint(h).as_deref(),
            Some("https://mcp.example.com/.well-known/oauth-protected-resource")
        );
        assert_eq!(resource_metadata_hint("Bearer realm=\"x\""), None);
        assert_eq!(resource_metadata_hint("Bearer resource_metadata=\"javascript:x\""), None);
    }

    #[test]
    fn splits_origin_and_path() {
        assert_eq!(split("https://mcp.linear.app/mcp/").unwrap(), ("https://mcp.linear.app".into(), "/mcp".into()));
        assert_eq!(split("https://a.b").unwrap(), ("https://a.b".into(), String::new()));
    }

    #[test]
    fn form_values_are_encoded() {
        assert_eq!(encode("a b&c=d/+"), "a+b%26c%3Dd%2F%2B");
    }

    #[test]
    fn unknown_state_is_refused() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let err = rt.block_on(finish(&store, "nope", Some("code"), None)).unwrap_err();
        assert!(err.to_string().contains("expired"));
    }
}
