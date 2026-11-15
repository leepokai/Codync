//! Dynamic client registration (RFC 7591) for one sign-in. A server that only allows the loopback
//! URI a native app uses refuses the whole registration when the cloud bounce URI is among the
//! URIs, so signing in on this computer then registers just the loopback URI (issue #440).

use super::{Callback, OAuth, TIMEOUT, text};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

/// The longest server reason a sign-in error repeats.
const REASON_MAX: usize = 300;

/// A server's answer to a dynamic client registration.
enum Registration {
    /// Its client id and secret.
    Client(String, Option<String>),
    /// It refused the redirect URIs: the status and the server's own reason, so a person sees why.
    RedirectRefused(String),
}

/// Registers a client for `redirect_uris`. `Err` is any other failure: unreachable, 5xx, no client
/// id, or a 4xx about something other than the redirect URIs, which another set of URIs can't fix.
async fn register(endpoint: &str, redirect_uris: &[String]) -> Result<Registration> {
    let res = crate::http()
        .post(endpoint)
        .json(&json!({
            "client_name": "Codync",
            "redirect_uris": redirect_uris,
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
        }))
        .timeout(TIMEOUT)
        .send()
        .await
        .context("registering the sign-in")?;
    let status = res.status();
    let v: Value = res.json().await.unwrap_or(Value::Null);
    if let Some(client_id) = text(&v["client_id"]) {
        return Ok(Registration::Client(client_id, text(&v["client_secret"])));
    }
    let error = text(&v["error"]);
    let description = text(&v["error_description"]);
    let about_redirect = match error.as_deref() {
        Some("invalid_redirect_uri") => true,
        Some("invalid_client_metadata") => {
            description.as_deref().is_some_and(|d| d.to_ascii_lowercase().contains("redirect"))
        }
        _ => false,
    };
    let reason = description.or(error).map_or_else(
        || status.to_string(),
        |r| format!("{status}: {}", r.chars().take(REASON_MAX).collect::<String>()),
    );
    if !status.is_client_error() {
        bail!("couldn't register the sign-in ({reason})");
    }
    if !about_redirect {
        bail!("refused the registration ({reason})");
    }
    Ok(Registration::RedirectRefused(reason))
}

/// Registers the client for one sign-in into `oauth`: its id, secret and the redirect URIs it
/// accepted. A phone sign-in can't use the loopback-only fallback, so it stops at the refusal
/// instead of leaving the server a client nobody uses.
pub(super) async fn register_client(
    endpoint: &str,
    wanted: Vec<String>,
    loopback: String,
    callback: Callback,
    oauth: &mut OAuth,
) -> Result<()> {
    let (client_id, client_secret, redirect_uris) = match register(endpoint, &wanted).await? {
        Registration::Client(id, secret) => (id, secret, wanted),
        Registration::RedirectRefused(first) => {
            if callback == Callback::App {
                bail!(
                    "This connector refused a sign-in from the phone ({first}). Open Codync on this computer and sign in there."
                );
            }
            if wanted.len() == 1 {
                bail!("refused the registration ({first})");
            }
            match register(endpoint, std::slice::from_ref(&loopback)).await? {
                Registration::Client(id, secret) => (id, secret, vec![loopback]),
                Registration::RedirectRefused(second) => {
                    bail!("refused the registration ({first}); with only the loopback redirect URI: {second}")
                }
            }
        }
    };
    oauth.client_id = client_id;
    oauth.client_secret = client_secret;
    oauth.redirect_uris = redirect_uris;
    Ok(())
}
