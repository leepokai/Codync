//! Composio's HTTP API. Composio takes its key only as the `x-api-key` header on one base URL, so on a Codync Cloud
//! computer the key is its keeper's (proxied slot `composio:key`, `crate::keeper`): calls go through a grant, which
//! adds it, and the saved config holds a marker in its place.

use super::Config;
use crate::keeper::client;
use crate::keeper::record::{Keep, Record};
use crate::remote::managed::is_managed;
use crate::store::Store;
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::time::Duration;

const API: &str = "https://backend.composio.dev/api/v3.1";
const TIMEOUT: Duration = Duration::from_secs(30);
const KEEPER_SLOT: &str = "composio:key";
/// A cloud computer's saved `key`: the keeper has the key. A new marker per key, so a refresh still tells one key
/// from the next (`save_refresh`).
const KEPT: &str = "keeper:";

/// Where Composio calls go, and the key they carry (none through the keeper's grant).
pub(super) struct Api {
    base: String,
    key: Option<String>,
}

impl Api {
    /// Straight to Composio with `key`.
    pub(super) fn direct(key: &str) -> Self {
        Self { base: API.to_owned(), key: Some(key.to_owned()) }
    }

    /// Composio with the key `c` was saved with.
    pub(super) fn of(store: &Store, c: &Config) -> Result<Self> {
        if c.key.starts_with(KEPT) {
            return Ok(Self { base: client::short_grant(store, KEEPER_SLOT)?, key: None });
        }
        Ok(Self::direct(&c.key))
    }
}

/// Keeps `key` for good: what the config saves as its key (on a cloud computer, a marker; the keeper has the key).
pub(super) fn keep(store: &Store, key: &str) -> Result<String> {
    if !is_managed(store) {
        return Ok(key.to_owned());
    }
    client::put_proxied(store, KEEPER_SLOT, &Record::header(API, "x-api-key", key.to_owned()), &Keep::default())?;
    Ok(format!("{KEPT}{}", uuid::Uuid::new_v4()))
}

/// Forgets the key the keeper has for Composio.
pub(super) fn forget(store: &Store) -> Result<()> {
    if is_managed(store) { client::delete(store, KEEPER_SLOT) } else { Ok(()) }
}

pub(super) async fn request(
    api: &Api,
    method: reqwest::Method,
    path: &str,
    query: &[(&str, &str)],
    body: Option<&Value>,
) -> Result<Value> {
    let url = reqwest::Url::parse_with_params(&format!("{}{path}", api.base), query)?;
    let mut req = crate::http().request(method.clone(), url).timeout(TIMEOUT);
    if let Some(key) = &api.key {
        req = req.header("x-api-key", key);
    }
    if let Some(body) = body {
        req = req.json(body);
    }
    let res = req.send().await.with_context(|| format!("can't reach Composio ({method} {path})"))?;
    let status = res.status();
    let text = res.text().await.unwrap_or_default();
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if !status.is_success() {
        if status.as_u16() == 401 {
            bail!("Composio did not accept the API key. Reconnect in Extensions.");
        }
        bail!("Composio request failed ({status}). Check the connection and try again.");
    }
    Ok(v)
}

pub(super) async fn get(api: &Api, path: &str, query: &[(&str, &str)]) -> Result<Value> {
    request(api, reqwest::Method::GET, path, query, None).await
}

pub(super) async fn post(api: &Api, path: &str, body: &Value) -> Result<Value> {
    request(api, reqwest::Method::POST, path, &[], Some(body)).await
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn config(key: String) -> Config {
        Config { key, user_id: "u".into(), connections: Vec::new() }
    }

    #[test]
    fn a_cloud_computers_composio_key_stays_with_its_keeper() {
        let k = crate::keeper::tests::test_keeper();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        k.manage(&store);
        let saved = keep(&store, "ak-secret").unwrap();
        assert!(saved.starts_with(KEPT) && !saved.contains("ak-secret"), "{saved}");
        assert!(client::reveal(&store, KEEPER_SLOT).is_err(), "never read back");
        let record = k.keeper.record(KEEPER_SLOT).unwrap().unwrap();
        assert_eq!((record.upstream.as_str(), record.headers["x-api-key"].as_str()), (API, "ak-secret"));
        let api = Api::of(&store, &config(saved)).unwrap();
        assert!(api.base.starts_with(&format!("http://127.0.0.1:{}/t/", k.keeper.port)), "{}", api.base);
        assert!(api.key.is_none(), "the grant adds the key");
        forget(&store).unwrap();
        assert!(!client::has(&store, KEEPER_SLOT).unwrap());
    }

    #[test]
    fn a_computer_of_the_users_own_sends_its_composio_key_itself() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let saved = keep(&store, "ak-secret").unwrap();
        assert_eq!(saved, "ak-secret");
        let api = Api::of(&store, &config(saved)).unwrap();
        assert_eq!((api.base.as_str(), api.key.as_deref()), (API, Some("ak-secret")));
        forget(&store).unwrap();
    }
}
