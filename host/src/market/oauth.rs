//! Sign-in for remote MCP connectors (MCP authorization: OAuth 2.1 + PKCE,
//! protected resource metadata, dynamic client registration).
//!
//! The tokens live on this computer. The sign-in page redirects to one of two places:
//! - `http://127.0.0.1:<port>/oauth/callback`, served by this host, when the user signs
//!   in on this computer (desktop app, TUI);
//! - `<cloud>/v1/oauth/callback`, a stateless page that bounces to `codync://oauth?…`,
//!   which the phone's sign-in sheet catches and hands back with `connectorSignInFinish`.
//!
//! The authorization code alone is useless: the PKCE verifier never leaves this host.

use super::discovery::{MAX_METADATA, TIMEOUT, resource_metadata, server_metadata, split};
use crate::LockExt;
use crate::egress::Policy;
use crate::market::{self, Connector};
use crate::store::Store;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

    let resource = resource_metadata(c.egress(), &url).await;
    let issuer = resource["authorization_servers"][0]
        .as_str()
        .map_or_else(|| split(&url).map(|(origin, _)| origin), |s| Ok(s.trim_end_matches('/').to_owned()))?;
    let policy = crate::egress::derived(c.egress(), &url).await;
    let meta = server_metadata(policy, &issuer).await.with_context(|| format!("signing in to {}", c.name))?;
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
        let res = crate::egress::client(policy, &register)
            .await?
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
    let policy = sign_in_policy(&c).await;
    let oauth = c.oauth.ok_or_else(|| anyhow!("unknown connector"))?;
    let v = token_request(
        policy,
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

/// The policy for the endpoints a connector's sign-in server names (see `egress::derived`).
async fn sign_in_policy(c: &Connector) -> Policy {
    match c.url.as_deref() {
        Some(url) => crate::egress::derived(c.egress(), url).await,
        None => Policy::Public,
    }
}

async fn token_request(policy: Policy, oauth: &OAuth, form: &[(&str, &str)]) -> Result<Value> {
    let mut form: Vec<(&str, &str)> = form.to_vec();
    form.push(("client_id", &oauth.client_id));
    if let Some(secret) = &oauth.client_secret {
        form.push(("client_secret", secret));
    }
    let body = form.iter().map(|(k, v)| format!("{}={}", encode(k), encode(v))).collect::<Vec<_>>().join("&");
    let res = crate::egress::client(policy, &oauth.token_endpoint)
        .await?
        .post(&oauth.token_endpoint)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .body(body)
        .timeout(TIMEOUT)
        .send()
        .await
        .context("can't reach the sign-in server")?;
    let status = res.status();
    let v = crate::egress::json(res, MAX_METADATA).await.unwrap_or(Value::Null);
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
fn sign_out(store: &Store, id: &str) -> Result<Connector> {
    update(store, id, |c| {
        if let Some(o) = &mut c.oauth {
            o.access_token.clear();
            o.refresh_token = None;
            o.expires_at = None;
        }
    })
}

/// What the `remote` MCP proxy sends to connector `id`: its URL and headers, with a fresh token, and its egress policy.
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
    // `public`: the URL came from the MCP Registry, so the proxy may not reach the local network.
    Ok(json!({"url": url, "headers": headers, "public": c.egress() == Policy::Public}))
}

async fn fresh(store: &Store, c: &Connector, o: &OAuth, stale: bool) -> Result<OAuth> {
    let signed_out = || anyhow!("Sign in to {} in Extensions first.", c.name);
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
    match token_request(sign_in_policy(c).await, o, &[("grant_type", "refresh_token"), ("refresh_token", &refresh)])
        .await
    {
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
