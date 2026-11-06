//! Which cloud this computer uses, the address a person saves, and turning the cloud on or off.
//!
//! `CODYNC_CLOUD=off` wins, then the switch in settings, then `CODYNC_CLOUD_URL`, then the
//! saved `cloud_url`, then the build's cloud. Unit tests point at a fake cloud through kv
//! `test_cloud_url` (and `test_cloud_token` for a private relay), so they never reach a real one.
//!
//! A private relay's registration token is kept apart from its address (kv `cloud_relay_token`,
//! or `CODYNC_CLOUD_TOKEN` beside `CODYNC_CLOUD_URL`), so it never sits in a URL, shell history
//! or the address `hello` publishes.

use super::cloud::{self, CloudStatus};
use crate::environment::Environment;
use crate::hub::Hub;
use crate::store::Store;
use anyhow::{Result, bail};
use serde::Serialize;
use serde_json::Value;

/// kv: `false` while Reach from anywhere is off here.
const ENABLED: &str = "cloud_enabled";
/// kv: the relay address a person saved, and that relay's registration token.
const SAVED_URL: &str = "cloud_url";
const SAVED_TOKEN: &str = "cloud_relay_token";

/// Whose relay this is. Accounts, access requests, public webhooks and the screen's TURN exist
/// only in the Codync cloud, so the host never takes them from a private relay.
#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RelayKind {
    /// A Codync cloud: this build's, a saved official address, or one `CODYNC_CLOUD_URL` names
    /// without a token (a Codync cloud you develop or run yourself).
    #[default]
    Codync,
    /// An owner-run `private-relay/`: a saved address, or `CODYNC_CLOUD_URL` with `CODYNC_CLOUD_TOKEN`.
    Private,
}

/// The cloud in use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address {
    pub url: String,
    /// The private relay's registration token, sent only when enrolling (`Codync-Relay-Token`).
    pub token: Option<String>,
    pub kind: RelayKind,
}

/// Where an address can come from, with the token kept beside it.
#[derive(Clone, Copy, Default)]
struct Candidate<'a> {
    url: Option<&'a str>,
    token: Option<&'a str>,
}

/// The cloud in use: its public address, its kind, and the registration token when it has one.
pub fn current(store: &Store) -> Option<Address> {
    let off =
        std::env::var("CODYNC_CLOUD").is_ok_and(|v| v == "off") || store.kv_get(ENABLED).as_deref() == Some("false");
    if cfg!(test) {
        // No fake cloud, no cloud: a test never reaches a real one.
        let url = store.kv_get("test_cloud_url")?;
        let token = store.kv_get("test_cloud_token");
        return resolve(off, Candidate { url: Some(&url), token: token.as_deref() }, Candidate::default(), &url);
    }
    let (env_url, env_token) = (std::env::var("CODYNC_CLOUD_URL").ok(), std::env::var("CODYNC_CLOUD_TOKEN").ok());
    let (saved_url, saved_token) = (store.kv_get(SAVED_URL), store.kv_get(SAVED_TOKEN));
    resolve(
        off,
        Candidate { url: env_url.as_deref(), token: env_token.as_deref() },
        Candidate { url: saved_url.as_deref(), token: saved_token.as_deref() },
        Environment::current().cloud_url(),
    )
}

/// The cloud base URL in use, or `None` when the cloud is off.
pub fn url(store: &Store) -> Option<String> {
    current(store).map(|address| address.url)
}

/// `off` wins. Otherwise the first valid candidate, then `builtin`. An illegal candidate is skipped.
/// Each address carries only its own token: the saved token is never sent to an env URL.
fn resolve(off: bool, env: Candidate, saved: Candidate, builtin: &str) -> Option<Address> {
    if off {
        return None;
    }
    let token = |c: Candidate| c.token.filter(|token| valid_token(token)).map(str::to_owned);
    if let Some(url) = env.url.and_then(parse_url) {
        // A relay token marks a private relay: the Codync cloud never asks for one.
        let kind = if env.token.is_some_and(|t| !t.trim().is_empty()) { RelayKind::Private } else { RelayKind::Codync };
        return Some(Address { url, token: token(env), kind });
    }
    if let Some(url) = saved.url.and_then(parse_url) {
        let official = [Environment::Main, Environment::Dev].iter().any(|e| e.cloud_url() == url);
        let kind = if official { RelayKind::Codync } else { RelayKind::Private };
        return Some(Address { url, token: token(saved), kind });
    }
    let url = parse_url(builtin).unwrap_or_else(|| builtin.to_owned());
    Some(Address { url, token: None, kind: RelayKind::Codync })
}

/// https, or plain http to this computer (local dev). No user name, password, path, query or fragment.
fn parse_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let parsed = reqwest::Url::parse(&format!("{trimmed}/")).ok()?;
    let local = matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    if parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || !(parsed.scheme() == "https" || (parsed.scheme() == "http" && local))
    {
        return None;
    }
    let host = parsed.host_str()?;
    let port = parsed.port().map_or(String::new(), |port| format!(":{port}"));
    Some(format!("{}://{host}{port}", parsed.scheme()))
}

/// What the private relay accepts (`CODYNC_PRIVATE_RELAY_TOKEN`): 16 to 256 visible ASCII characters.
fn valid_token(token: &str) -> bool {
    (16..=256).contains(&token.len()) && token.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}

/// What `setCloudUrl` does with the saved registration token.
#[derive(Debug, PartialEq, Eq)]
pub enum TokenChange {
    /// Keep it while the address stays the same; a new address drops it, since a token belongs to one relay.
    Keep,
    Clear,
    Set(String),
}

impl TokenChange {
    /// `setCloudUrl`'s `token`: absent keeps the saved token for the same address; null or blank clears it.
    pub fn from_json(token: Option<&Value>) -> Result<Self> {
        Ok(match token {
            None => Self::Keep,
            Some(Value::Null) => Self::Clear,
            Some(Value::String(token)) if token.trim().is_empty() => Self::Clear,
            Some(Value::String(token)) => Self::Set(token.trim().to_owned()),
            Some(_) => bail!("`token` must be a string"),
        })
    }
}

/// Writes which cloud is in use into the status. Only the Codync cloud has an account owner.
pub fn show(s: &mut CloudStatus, address: Option<&Address>) {
    s.enabled = address.is_some();
    s.url = address.map(|a| a.url.clone());
    s.token_set = address.is_some_and(|a| a.token.is_some());
    s.relay = address.map(|a| a.kind).unwrap_or_default();
    if s.relay == RelayKind::Private {
        s.owner = None;
    }
}

/// `setCloud`: turns the cloud on or off; the relay restarts.
pub fn set_cloud(hub: &Hub, enabled: bool) -> Result<CloudStatus> {
    hub.store.kv_set(ENABLED, if enabled { "true" } else { "false" })?;
    Ok(restart(hub))
}

/// `setCloudUrl`: points this computer at a cloud, or clears the saved URL (and its token) so the
/// build's cloud is used again. Empty and null clear it. An illegal URL or token is refused and
/// changes nothing. The relay restarts either way. Local only.
pub fn set_cloud_url(hub: &Hub, raw: Option<&str>, token: TokenChange) -> Result<CloudStatus> {
    match raw.map(str::trim).filter(|u| !u.is_empty()) {
        None => {
            hub.store.kv_delete(SAVED_URL)?;
            hub.store.kv_delete(SAVED_TOKEN)?;
        }
        Some(raw) => {
            if raw.contains('@') {
                bail!("Enter the relay token in its own field, not in the address.");
            }
            let Some(url) = parse_url(raw) else {
                bail!("That relay address isn't a usable cloud URL.");
            };
            let same = hub.store.kv_get(SAVED_URL).as_deref() == Some(url.as_str());
            match token {
                TokenChange::Set(token) if !valid_token(&token) => {
                    bail!("A relay token is 16 to 256 visible characters, with no spaces.");
                }
                TokenChange::Set(token) => hub.store.kv_set(SAVED_TOKEN, &token)?,
                TokenChange::Keep if same => {}
                TokenChange::Keep | TokenChange::Clear => hub.store.kv_delete(SAVED_TOKEN)?,
            }
            hub.store.kv_set(SAVED_URL, &url)?;
        }
    }
    Ok(restart(hub))
}

/// Shows the new choice and restarts the relay on it.
fn restart(hub: &Hub) -> CloudStatus {
    let address = current(&hub.store);
    cloud::update_status(hub, |s| show(s, address.as_ref()));
    hub.cloud.config.send_modify(|g| *g += 1);
    hub.cloud.status()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::remote::identity::Identity;
    use serde_json::json;
    use std::sync::Arc;

    fn temp_hub() -> Arc<Hub> {
        let dir = std::env::temp_dir().join(format!("codync-cloud-url-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("t.db")).unwrap();
        Hub::new(store, "host".into(), Identity::load_or_create(&dir).unwrap(), "token".into(), 0)
    }

    fn at(url: &'static str) -> Candidate<'static> {
        Candidate { url: Some(url), token: None }
    }

    fn with_token(url: &'static str, token: &'static str) -> Candidate<'static> {
        Candidate { url: Some(url), token: Some(token) }
    }

    #[test]
    fn only_safe_cloud_urls() {
        assert_eq!(
            parse_url("https://codync-cloud.example.workers.dev/").as_deref(),
            Some("https://codync-cloud.example.workers.dev")
        );
        assert_eq!(parse_url("http://127.0.0.1:8787/").as_deref(), Some("http://127.0.0.1:8787"));
        assert_eq!(parse_url("https://[2001:db8::1]:8443").as_deref(), Some("https://[2001:db8::1]:8443"));
        assert!(parse_url("https://tok@relay.example.com/").is_none(), "a token never rides in the address");
        assert!(parse_url("https://user:secret@relay.example.com").is_none());
        assert!(parse_url("http://example.com/").is_none());
        assert!(parse_url("https://example.com/path/").is_none());
        assert!(parse_url("ftp://example.com/").is_none());
        assert!(parse_url("https://relay.example.com/#setup").is_none());
        assert!(parse_url("https://relay.example.com/?a=1").is_none());
    }

    #[test]
    fn tokens_are_header_safe() {
        assert!(valid_token("a-long-random-secret"));
        assert!(valid_token("abcdefghijklmnop@"));
        assert!(!valid_token("too-short"));
        assert!(!valid_token("has a space in the middle"));
        assert!(!valid_token("line\nbreak-0123456789"));
        assert!(!valid_token(&"x".repeat(257)));
    }

    #[test]
    fn a_saved_cloud_url_is_used_only_when_nothing_in_front_of_it_is() {
        let none = Candidate::default();
        let url = |a: Option<Address>| a.map(|a| a.url);
        assert_eq!(
            url(resolve(false, none, at("https://relay.example.com/"), "https://builtin")).as_deref(),
            Some("https://relay.example.com")
        );
        assert_eq!(
            url(resolve(false, at("https://from.env"), at("https://saved.example"), "https://builtin")).as_deref(),
            Some("https://from.env")
        );
        assert_eq!(
            url(resolve(false, at("http://example.com"), none, "https://builtin")).as_deref(),
            Some("https://builtin"),
            "an illegal candidate is skipped"
        );
        assert_eq!(resolve(true, at("https://relay.example.com"), none, "https://builtin"), None);

        let saved =
            resolve(false, none, with_token("https://relay.example.com", "s3cret-token-value"), "https://builtin")
                .unwrap();
        assert_eq!((saved.token.as_deref(), saved.kind), (Some("s3cret-token-value"), RelayKind::Private));
        let from_env = resolve(
            false,
            at("https://from.env"),
            with_token("https://saved.example", "saved-token-value"),
            "https://builtin",
        )
        .unwrap();
        assert_eq!(from_env.token, None, "the env address does not inherit the saved token");
        let env_token =
            resolve(false, with_token("https://from.env", "env-token-value-123"), none, "https://builtin").unwrap();
        assert_eq!(env_token.token.as_deref(), Some("env-token-value-123"));
    }

    #[test]
    fn only_a_codync_cloud_has_accounts() {
        let none = Candidate::default();
        let kind = |env, saved| resolve(false, env, saved, "https://dev-api.codync.dev").unwrap().kind;
        assert_eq!(kind(none, none), RelayKind::Codync, "the build's cloud");
        assert_eq!(kind(at("http://127.0.0.1:8787"), none), RelayKind::Codync, "a Codync cloud you run yourself");
        assert_eq!(kind(with_token("https://relay.example.com", "env-token-value-123"), none), RelayKind::Private);
        assert_eq!(kind(none, at("https://relay.example.com")), RelayKind::Private, "even before its token is typed");
        assert_eq!(kind(none, at("https://api.codync.dev/")), RelayKind::Codync, "a saved official address");
    }

    #[test]
    fn token_changes_follow_the_json() {
        assert_eq!(TokenChange::from_json(None).unwrap(), TokenChange::Keep);
        assert_eq!(TokenChange::from_json(Some(&Value::Null)).unwrap(), TokenChange::Clear);
        assert_eq!(TokenChange::from_json(Some(&json!("  "))).unwrap(), TokenChange::Clear);
        assert_eq!(TokenChange::from_json(Some(&json!(" t "))).unwrap(), TokenChange::Set("t".into()));
        assert!(TokenChange::from_json(Some(&json!(1))).is_err());
    }

    #[test]
    fn set_cloud_url_stores_the_address_and_its_token_apart() {
        let hub = temp_hub();
        set_cloud_url(&hub, Some("https://relay.example.com/"), TokenChange::Keep).unwrap();
        assert_eq!(hub.store.kv_get(SAVED_URL).as_deref(), Some("https://relay.example.com"));
        let bad = set_cloud_url(&hub, Some("http://example.com"), TokenChange::Keep).unwrap_err();
        assert!(bad.to_string().contains("relay address"), "{bad}");
        assert_eq!(hub.store.kv_get(SAVED_URL).as_deref(), Some("https://relay.example.com"));

        set_cloud_url(&hub, Some("https://relay.example.com"), TokenChange::Set("s3cret-token-value".into())).unwrap();
        assert_eq!(hub.store.kv_get(SAVED_TOKEN).as_deref(), Some("s3cret-token-value"));
        let short =
            set_cloud_url(&hub, Some("https://relay.example.com"), TokenChange::Set("short".into())).unwrap_err();
        assert!(short.to_string().contains("relay token"), "{short}");
        assert_eq!(
            hub.store.kv_get(SAVED_TOKEN).as_deref(),
            Some("s3cret-token-value"),
            "a refused token changes nothing"
        );
        let inline =
            set_cloud_url(&hub, Some("https://other-token-value@relay.example.com"), TokenChange::Keep).unwrap_err();
        assert!(inline.to_string().contains("its own field"), "{inline}");
        let fragment = set_cloud_url(&hub, Some("https://relay.example.com/#setup"), TokenChange::Keep).unwrap_err();
        assert!(fragment.to_string().contains("relay address"), "{fragment}");

        set_cloud_url(&hub, Some("https://relay.example.com/"), TokenChange::Keep).unwrap();
        assert_eq!(hub.store.kv_get(SAVED_TOKEN).as_deref(), Some("s3cret-token-value"), "same address keeps it");
        set_cloud_url(&hub, Some("https://another.example.com"), TokenChange::Keep).unwrap();
        assert_eq!(hub.store.kv_get(SAVED_TOKEN), None, "a new address drops the old relay's token");

        set_cloud_url(&hub, Some("https://another.example.com"), TokenChange::Set("another-token-value".into()))
            .unwrap();
        set_cloud_url(&hub, Some("https://another.example.com"), TokenChange::Clear).unwrap();
        assert_eq!(hub.store.kv_get(SAVED_TOKEN), None);
        set_cloud_url(&hub, Some("https://another.example.com"), TokenChange::Set("another-token-value".into()))
            .unwrap();
        set_cloud_url(&hub, None, TokenChange::Keep).unwrap();
        assert_eq!(hub.store.kv_get(SAVED_URL), None);
        assert_eq!(hub.store.kv_get(SAVED_TOKEN), None);
    }

    #[test]
    fn the_status_shows_the_token_of_the_address_in_use() {
        let hub = temp_hub();
        hub.store.kv_set("test_cloud_url", "https://relay.example.com").unwrap();
        hub.store.kv_set("test_cloud_token", "s3cret-token-value").unwrap();
        cloud::update_status(&hub, |s| s.owner = Some(cloud::Owner { user_id: "user_1".into(), email: None }));
        let status = set_cloud(&hub, true).unwrap();
        assert_eq!((status.token_set, status.relay), (true, RelayKind::Private));
        assert_eq!(status.owner, None, "a private relay has no account owner");
        hub.store.kv_delete("test_cloud_token").unwrap();
        let status = set_cloud(&hub, true).unwrap();
        assert_eq!((status.token_set, status.relay), (false, RelayKind::Codync));
        assert!(!set_cloud(&hub, false).unwrap().enabled);
    }
}
