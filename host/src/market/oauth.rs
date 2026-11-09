//! Sign-in for remote MCP connectors (MCP authorization: OAuth 2.1 + PKCE,
//! protected resource metadata, dynamic client registration). Servers where Codync has its own
//! app instead (GitHub) sign in with a code: see `device`.
//!
//! The tokens live on this computer. The sign-in page redirects to one of two places:
//! - `http://127.0.0.1:<port>/oauth/callback`, served by this host, when the user signs
//!   in on this computer (desktop app, TUI);
//! - `<cloud>/v1/oauth/callback`, a stateless page that bounces to `codync://oauth?…`,
//!   which the phone's sign-in sheet catches and hands back with `connectorSignInFinish`.
//!
//! The authorization code alone is useless: the PKCE verifier never leaves this host.

mod device;
mod discovery;

pub use device::offers_sign_in;
pub use discovery::required;
use discovery::{resource_metadata, server_metadata, split};

use crate::LockExt;
use crate::hub::Hub;
use crate::market::{self, Connector};
use crate::store::Store;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const TIMEOUT: Duration = Duration::from_secs(20);
/// How long a started sign-in waits for its callback.
const PENDING_FOR: Duration = Duration::from_mins(15);
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

fn text(v: &Value) -> Option<String> {
    v.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

/// `c` is connector `id` still pointing at `url`: while a request to its server is in flight, it
/// can be removed and another added under its id.
fn is_at(c: &Connector, id: &str, url: Option<&str>) -> bool {
    c.id == id && c.url.as_deref() == url
}

/// Changes connector `id` while it still points at `url`: every change here follows a request to its server.
fn update(store: &Store, id: &str, url: Option<&str>, f: impl FnOnce(&mut Connector)) -> Result<Connector> {
    market::update_connectors(store, |all| {
        let c = all.iter_mut().find(|c| is_at(c, id, url)).ok_or_else(|| anyhow!("unknown connector"))?;
        f(c);
        Ok(c.clone())
    })
}

/// Starts signing in to connector `id`: the page to open, and the code to type there for a device sign-in.
/// `shows_code`: the client can show that code (older apps can't).
pub async fn start(hub: &Arc<Hub>, id: &str, callback: Callback, shows_code: bool) -> Result<Value> {
    let store = &hub.store;
    let c = market::connectors(store)?.into_iter().find(|c| c.id == id).ok_or_else(|| anyhow!("unknown connector"))?;
    let url = c.url.clone().ok_or_else(|| anyhow!("{} runs on this computer; it has nothing to sign in to", c.name))?;

    let resource = resource_metadata(&url).await;
    let issuer = resource["authorization_servers"][0]
        .as_str()
        .map_or_else(|| split(&url).map(|(origin, _)| origin), |s| Ok(s.trim_end_matches('/').to_owned()))?;
    let meta = server_metadata(&issuer).await.with_context(|| format!("signing in to {}", c.name))?;
    let token_endpoint = text(&meta["token_endpoint"]).ok_or_else(|| anyhow!("no token endpoint"))?;
    if let Some(app) = device::app(&issuer, &url) {
        if !shows_code {
            bail!("Update Codync on this device to sign in to {}.", c.name);
        }
        return device::start(hub, &c, &meta, app, token_endpoint).await;
    }

    let app = app_uri(store);
    let redirect_uri = match callback {
        Callback::Host => loopback_uri(hub.port),
        Callback::App => app
            .clone()
            .ok_or_else(|| anyhow!("The Codync cloud is off on this computer, so sign in from the computer itself."))?,
    };
    let authorize = text(&meta["authorization_endpoint"]).ok_or_else(|| anyhow!("no authorization endpoint"))?;

    // Register once with every redirect URI we might use; again if they changed.
    let wanted: Vec<String> = std::iter::once(loopback_uri(hub.port)).chain(app).collect();
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
    update(store, id, Some(&url), |c| c.oauth = Some(oauth))?;

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
        .find(|c| is_at(c, &p.connector, Some(&p.resource)))
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
    update(store, &p.connector, Some(&p.resource), |c| store_tokens(c, &v))
}

async fn token_request(oauth: &OAuth, form: &[(&str, &str)]) -> Result<Value> {
    let (status, v) = token_post(oauth, form).await?;
    if !status.is_success() || !v["access_token"].is_string() {
        bail!("The sign-in server refused the request ({status}). Reopen sign-in and try again.");
    }
    Ok(v)
}

/// The token endpoint's answer as is: a device sign-in reads its `error` while it waits.
async fn token_post(oauth: &OAuth, form: &[(&str, &str)]) -> Result<(reqwest::StatusCode, Value)> {
    let mut form: Vec<(&str, &str)> = form.to_vec();
    form.push(("client_id", &oauth.client_id));
    if let Some(secret) = &oauth.client_secret {
        form.push(("client_secret", secret));
    }
    post_form(&oauth.token_endpoint, &form).await.context("can't reach the sign-in server")
}

/// Posts `form`: the status and the JSON answer (`Null` when it isn't JSON).
async fn post_form(url: &str, form: &[(&str, &str)]) -> Result<(reqwest::StatusCode, Value)> {
    let body = form.iter().map(|(k, v)| format!("{}={}", encode(k), encode(v))).collect::<Vec<_>>().join("&");
    let res = crate::http()
        .post(url)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .body(body)
        .timeout(TIMEOUT)
        .send()
        .await?;
    let status = res.status();
    Ok((status, res.json().await.unwrap_or(Value::Null)))
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
fn sign_out(store: &Store, c: &Connector) -> Result<Connector> {
    update(store, &c.id, c.url.as_deref(), |c| {
        if let Some(o) = &mut c.oauth {
            o.access_token.clear();
            o.refresh_token = None;
            o.expires_at = None;
        }
    })
}

/// Forgets a connector's tokens and its registration: the next sign-in registers again.
fn unregister(store: &Store, c: &Connector) -> Result<Connector> {
    update(store, &c.id, c.url.as_deref(), |c| {
        if let Some(o) = &mut c.oauth {
            *o = OAuth::default();
        }
    })
}

fn signed_out(c: &Connector) -> anyhow::Error {
    anyhow!("Sign in to {} in Extensions first.", c.name)
}

/// What the `remote` MCP proxy sends to connector `id`: its URL and headers, with a fresh token.
/// `stale`: the server just said 401, so refresh even if the token looks valid. A cloud computer's keeper refreshes
/// its own (`connector_keeper`).
pub async fn target(store: &Store, id: &str, stale: bool) -> Result<Value> {
    let c = market::connectors(store)?.into_iter().find(|c| c.id == id).ok_or_else(|| anyhow!("unknown connector"))?;
    let url = c.url.clone().ok_or_else(|| anyhow!("{} isn't a remote connector", c.name))?;
    if crate::remote::managed::is_managed(store) {
        if let Some(o) = &c.oauth {
            if !o.signed_in() {
                return Err(signed_out(&c));
            }
            // The keeper drops a sign-in it can't renew: the connector then shows as signed out, as anywhere else.
            if stale && market::connector_keeper::sign_in_ended(store, &c)? {
                sign_out(store, &c)?;
                return Err(signed_out(&c));
            }
        }
        if let Some(target) = market::connector_keeper::target(store, &c, &url).await? {
            return Ok(target);
        }
    }
    let mut headers = std::collections::BTreeMap::new();
    for (k, v) in &c.headers {
        headers.insert(k.clone(), market::passwords::resolve(store, v).await?);
    }
    if let Some(o) = &c.oauth {
        let o = fresh(store, &c, &url, o, stale).await?;
        headers.insert("Authorization".into(), format!("Bearer {}", o.access_token));
    }
    Ok(json!({"url": url, "headers": headers}))
}

/// `o`, or a new token when it's about to expire (or `stale`). `url`: the connector's.
async fn fresh(store: &Store, c: &Connector, url: &str, o: &OAuth, stale: bool) -> Result<OAuth> {
    if !o.signed_in() {
        return Err(signed_out(c));
    }
    let expiring = o.expires_at.is_some_and(|t| t <= now() + EARLY);
    if !stale && !expiring {
        return Ok(o.clone());
    }
    let _one = REFRESH.lock().await;
    // Someone else may have refreshed while we waited.
    let current = market::connectors(store)?.into_iter().find(|x| is_at(x, &c.id, Some(url))).and_then(|x| x.oauth);
    if let Some(cur) = &current
        && cur.access_token != o.access_token
        && cur.signed_in()
    {
        return Ok(cur.clone());
    }
    renew(store, c, url, o, stale).await
}

/// Trades the refresh token for a new access token. A server that can't answer now leaves the sign-in as is.
async fn renew(store: &Store, c: &Connector, url: &str, o: &OAuth, stale: bool) -> Result<OAuth> {
    let Some(refresh) = o.refresh_token.as_deref() else {
        sign_out(store, c)?;
        return Err(signed_out(c));
    };
    // The old token still works until it expires.
    let retry = || match o.expires_at {
        Some(t) if !stale && t > now() => Ok(o.clone()),
        _ => Err(anyhow!("Couldn't renew the sign-in to {}. Try again in a moment.", c.name)),
    };
    let form = [("grant_type", "refresh_token"), ("refresh_token", refresh), ("resource", url)];
    let (status, v) = match token_post(o, &form).await {
        Ok(answer) => answer,
        Err(error) => {
            tracing::warn!(connector = %c.id, error = format!("{error:#}"), "token refresh failed");
            return retry();
        }
    };
    let renewal = renewal(status, &v);
    if renewal != Renewal::Renewed {
        tracing::warn!(connector = %c.id, %status, error = v["error"].as_str().unwrap_or_default(), "token refresh refused");
    }
    match renewal {
        Renewal::Renewed => Ok(update(store, &c.id, Some(url), |c| store_tokens(c, &v))?.oauth.unwrap_or_default()),
        Renewal::Retry => retry(),
        Renewal::Revoked => {
            sign_out(store, c)?;
            Err(signed_out(c))
        }
        Renewal::Unregistered => {
            unregister(store, c)?;
            Err(signed_out(c))
        }
    }
}

/// What the token endpoint's answer to a refresh means (RFC 6749 §5.2); a cloud computer's keeper reads it the same
/// way (`keeper::proxy`).
#[derive(Debug, PartialEq)]
pub(crate) enum Renewal {
    Renewed,
    /// The server is down or busy, or something in front of it answered: ask again later.
    Retry,
    /// The server ended the sign-in.
    Revoked,
    /// The server no longer knows Codync's client.
    Unregistered,
}

pub(crate) fn renewal(status: reqwest::StatusCode, v: &Value) -> Renewal {
    if status.is_success() && v["access_token"].is_string() {
        return Renewal::Renewed;
    }
    if status.is_server_error() || matches!(status.as_u16(), 408 | 429) {
        return Renewal::Retry;
    }
    match v["error"].as_str() {
        // Not an OAuth error (a proxy's page, say): nothing is known about the grant.
        None | Some("temporarily_unavailable") => Renewal::Retry,
        Some("invalid_client") => Renewal::Unregistered,
        Some(_) => Renewal::Revoked,
    }
}

#[cfg(test)]
mod tests;
