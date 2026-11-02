//! Device sign-in (OAuth 2.0 device authorization grant, RFC 8628) for servers that don't let apps
//! register themselves but where Codync has its own app: GitHub. The user types a short code on the
//! server's page, from any device; this host polls for the token, so no redirect has to reach it.

use super::{OAuth, post_form, store_tokens, text, token_post, update};
use crate::hub::Hub;
use crate::market::Connector;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Codync's own OAuth app for one server.
pub(super) struct App {
    issuer: &'static str,
    /// Where the server's MCP endpoints start: only connectors there get this app's tokens.
    resource: &'static str,
    /// Public: the device grant needs no secret.
    client_id: &'static str,
    /// Fixed here, never taken from the server's metadata.
    scopes: &'static [&'static str],
}

/// GitHub: the "Codync" OAuth App, the one Clerk's GitHub sign-in also uses, with the scopes
/// GitHub's MCP server lists.
const APPS: &[App] = &[App {
    issuer: "https://github.com/login/oauth",
    resource: "https://api.githubcopilot.com/mcp",
    client_id: "Ov23li6EcblWW0wYQR4y",
    scopes: &[
        "repo",
        "read:org",
        "read:user",
        "user:email",
        "read:packages",
        "write:packages",
        "read:project",
        "project",
        "gist",
        "notifications",
    ],
}];
const GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// Codync's app for the connector at `url` whose metadata names `issuer`. Both must match: any
/// server can name GitHub as its issuer, and the app's tokens go only to the server it's for.
pub(super) fn app(issuer: &str, url: &str) -> Option<&'static App> {
    APPS.iter().find(|a| a.issuer == issuer && url.starts_with(a.resource))
}

/// A server Codync can sign in to with its own app: a connector set up for it with a token from
/// before that (no `oauth`) can switch to signing in.
pub fn offers_sign_in(url: &str) -> bool {
    APPS.iter().any(|a| url.starts_with(a.resource))
}

/// Asks the server for a code and waits for the token in the background.
/// The plan: the page to type the code on, and the code.
pub(super) async fn start(
    hub: &Arc<Hub>,
    c: &Connector,
    meta: &Value,
    app: &App,
    token_endpoint: String,
) -> Result<Value> {
    let endpoint =
        text(&meta["device_authorization_endpoint"]).ok_or_else(|| anyhow!("{} has no sign-in with a code", c.name))?;
    let (status, v) = post_form(&endpoint, &[("client_id", app.client_id), ("scope", &app.scopes.join(" "))])
        .await
        .with_context(|| format!("can't reach {}", c.name))?;
    let (Some(device_code), Some(code), Some(page)) =
        (text(&v["device_code"]), text(&v["user_code"]), text(&v["verification_uri"]))
    else {
        bail!("{} refused the sign-in ({status})", c.name);
    };
    // Nothing changes on the connector until the token arrives: one that works with a token keeps working.
    let oauth = OAuth { client_id: app.client_id.to_owned(), token_endpoint, ..OAuth::default() };
    let interval = v["interval"].as_u64().unwrap_or(5);
    let lasts = Duration::from_secs(v["expires_in"].as_u64().unwrap_or(900));
    let target = Target { id: c.id.clone(), url: c.url.clone() };
    tokio::spawn(poll(hub.clone(), target, oauth, device_code, interval, lasts));
    Ok(json!({"url": page, "callback": "host", "code": code}))
}

/// The connector a sign-in is for. The URL too: while GitHub waits for the code, that connector
/// can be removed and another one added under its id, and the token goes only where it was asked for.
struct Target {
    id: String,
    url: Option<String>,
}

/// What one answer from the token endpoint means.
#[derive(Debug, PartialEq)]
enum Step {
    Done,
    /// Ask again after this many seconds.
    Wait(u64),
    Stop,
}

fn step(v: &Value, interval: u64) -> Step {
    if v["access_token"].is_string() {
        return Step::Done;
    }
    match v["error"].as_str() {
        // No answer we can read (a 5xx page) is worth another try too.
        Some("authorization_pending") | None => Step::Wait(interval),
        // RFC 8628 §3.5: five seconds more (GitHub sends the new interval).
        Some("slow_down") => Step::Wait(v["interval"].as_u64().unwrap_or(interval + 5)),
        _ => Step::Stop,
    }
}

async fn poll(hub: Arc<Hub>, target: Target, oauth: OAuth, device_code: String, mut interval: u64, lasts: Duration) {
    let id = &target.id;
    let until = Instant::now() + lasts;
    while Instant::now() < until {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        let v = match token_post(&oauth, &[("grant_type", GRANT), ("device_code", &device_code)]).await {
            Ok((_, v)) => v,
            // A dropped connection doesn't end the sign-in.
            Err(error) => {
                tracing::debug!(connector = %id, error = format!("{error:#}"), "device sign-in poll failed");
                continue;
            }
        };
        match step(&v, interval) {
            Step::Done => {
                // Fails, dropping the token, when the connector no longer points where the sign-in started.
                if let Err(error) = update(&hub.store, id, target.url.as_deref(), |c| signed_in(c, &oauth, &v)) {
                    tracing::warn!(connector = %id, error = format!("{error:#}"), "couldn't save the sign-in");
                }
                return;
            }
            Step::Wait(next) => interval = next,
            Step::Stop => {
                tracing::info!(connector = %id, error = v["error"].as_str().unwrap_or("unknown"), "device sign-in ended");
                return;
            }
        }
    }
}

/// Keeps the token, and drops the token header the connector was set up with before it could sign in.
fn signed_in(c: &mut Connector, app: &OAuth, v: &Value) {
    c.oauth = Some(app.clone());
    store_tokens(c, v);
    c.headers.retain(|k, _| !k.eq_ignore_ascii_case("authorization"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_in_replaces_the_token_header() {
        let mut c: Connector = serde_json::from_value(json!({
            "id": "github", "name": "GitHub", "url": "https://api.githubcopilot.com/mcp/",
            "headers": {"Authorization": "Bearer ghp_old", "X-MCP-Toolsets": "repos"},
        }))
        .unwrap();
        assert!(offers_sign_in(c.url.as_deref().unwrap()));
        let app = OAuth { client_id: "id".into(), token_endpoint: "https://github.com/t".into(), ..OAuth::default() };
        signed_in(&mut c, &app, &json!({"access_token": "gho_new"}));
        let o = c.oauth.unwrap();
        assert_eq!((o.client_id.as_str(), o.access_token.as_str()), ("id", "gho_new"));
        assert_eq!(c.headers.keys().collect::<Vec<_>>(), ["X-MCP-Toolsets"]);
    }

    #[test]
    fn only_github_s_own_server_gets_codync_s_github_app() {
        let github = "https://github.com/login/oauth";
        assert!(app(github, "https://api.githubcopilot.com/mcp/").is_some());
        assert!(app(github, "https://api.githubcopilot.com/mcp/readonly").is_some());
        // Another server naming GitHub as its issuer would get a GitHub token sent to it.
        assert!(app(github, "https://evil.example/mcp").is_none());
        assert!(app(github, "https://api.githubcopilot.com.evil.example/mcp").is_none());
        assert!(app("https://mcp.linear.app", "https://mcp.linear.app/mcp").is_none());
    }

    #[test]
    fn polls_until_the_token_arrives() {
        assert_eq!(step(&json!({"error": "authorization_pending"}), 5), Step::Wait(5));
        assert_eq!(step(&json!({"error": "slow_down", "interval": 10}), 5), Step::Wait(10));
        assert_eq!(step(&json!({"error": "slow_down"}), 5), Step::Wait(10));
        assert_eq!(step(&json!({"access_token": "gho_x", "token_type": "bearer"}), 5), Step::Done);
        assert_eq!(step(&json!({"error": "access_denied"}), 5), Step::Stop);
        assert_eq!(step(&json!({"error": "expired_token"}), 5), Step::Stop);
        assert_eq!(step(&Value::Null, 5), Step::Wait(5));
    }
}
