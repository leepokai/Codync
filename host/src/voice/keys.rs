//! The voice providers' keys, and how a request reaches a provider: straight there with the key, or, on a Codync
//! Cloud computer, through its keeper (`crate::keeper`), whose grant adds the key (proxied slots `voice:<provider>`),
//! so nothing on the computer but the keeper ever holds it again.

use super::Provider;
use crate::keeper::client;
use crate::keeper::record::{GEMINI, Keep, OPENAI, Record};
use crate::market::vault;
use crate::remote::managed::is_managed;
use crate::store::Store;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

const SLOT: &str = "voice";

#[derive(Default, Serialize, Deserialize)]
struct Keys {
    #[serde(default)]
    openai: String,
    #[serde(default)]
    gemini: String,
}

impl Keys {
    fn get(&self, p: Provider) -> &str {
        match p {
            Provider::OpenAi => &self.openai,
            Provider::Gemini => &self.gemini,
        }
    }

    fn set(&mut self, p: Provider, key: String) {
        match p {
            Provider::OpenAi => self.openai = key,
            Provider::Gemini => self.gemini = key,
        }
    }
}

fn load(store: &Store) -> Result<Keys> {
    Ok(vault::read(store, SLOT)?
        .filter(|s| !s.is_empty())
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or_default())
}

fn slot(p: Provider) -> String {
    format!("voice:{}", p.id())
}

impl Provider {
    fn base(self) -> &'static str {
        match self {
            Self::OpenAi => OPENAI,
            Self::Gemini => GEMINI,
        }
    }

    /// The keeper's record for this provider's `key`.
    fn record(self, key: &str) -> Record {
        match self {
            Self::OpenAi => Record::bearer(OPENAI, key),
            Self::Gemini => Record::google_key(GEMINI, key),
        }
    }
}

/// Where requests to a provider go, and the key they carry (none through the keeper's grant, which adds it).
pub(super) struct Access {
    base: String,
    key: Option<String>,
}

impl Access {
    /// Straight to the provider with `key`.
    pub(super) fn direct(p: Provider, key: &str) -> Self {
        Self { base: p.base().to_owned(), key: Some(key.to_owned()) }
    }

    /// A request to `path` on the provider (`/v1/models`).
    pub(super) fn request(&self, p: Provider, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let req = crate::http().request(method, format!("{}{path}", self.base));
        match (&self.key, p) {
            (None, _) => req,
            (Some(key), Provider::OpenAi) => req.bearer_auth(key),
            (Some(key), Provider::Gemini) => req.header("x-goog-api-key", key),
        }
    }
}

fn missing(p: Provider) -> anyhow::Error {
    anyhow!("No {} key on this computer. Add one in the call settings.", p.name())
}

/// Whether `p` has a key here.
pub(super) fn configured(store: &Store, p: Provider) -> Result<bool> {
    if is_managed(store) {
        return client::has(store, &slot(p));
    }
    Ok(!load(store)?.get(p).is_empty())
}

/// How to reach `p` with the key saved here.
pub(super) fn access(store: &Store, p: Provider) -> Result<Access> {
    if is_managed(store) {
        if !client::has(store, &slot(p))? {
            return Err(missing(p));
        }
        return Ok(Access { base: client::short_grant(store, &slot(p))?, key: None });
    }
    let keys = load(store)?;
    match keys.get(p) {
        "" => Err(missing(p)),
        key => Ok(Access::direct(p, key)),
    }
}

/// Keeps `key` for `p`; an empty one removes it.
pub(super) fn save(store: &Store, p: Provider, key: &str) -> Result<()> {
    if is_managed(store) {
        if key.is_empty() {
            return client::delete(store, &slot(p));
        }
        return client::put_proxied(store, &slot(p), &p.record(key), &Keep::default());
    }
    let _guard = store.connector_lock.lock().map_err(|_| anyhow!("credential storage is busy"))?;
    let mut keys = load(store)?;
    keys.set(p, key.to_owned());
    vault::write(store, SLOT, &serde_json::to_string(&keys)?)
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn a_cloud_computers_voice_keys_stay_with_its_keeper() {
        let k = crate::keeper::tests::test_keeper();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        k.manage(&store);
        save(&store, Provider::Gemini, "g-key").unwrap();
        assert!(configured(&store, Provider::Gemini).unwrap());
        assert!(!configured(&store, Provider::OpenAi).unwrap());
        assert!(client::reveal(&store, "voice:gemini").is_err(), "never read back");
        assert!(vault::read(&store, SLOT).unwrap().is_none(), "nothing in the visible slot");
        let record = k.keeper.record("voice:gemini").unwrap().unwrap();
        assert_eq!((record.upstream.as_str(), record.headers["x-goog-api-key"].as_str()), (GEMINI, "g-key"));
        let route = access(&store, Provider::Gemini).unwrap();
        assert!(route.base.starts_with(&format!("http://127.0.0.1:{}/t/", k.keeper.port)), "{}", route.base);
        assert!(route.key.is_none(), "the grant adds the key");
        save(&store, Provider::Gemini, "").unwrap();
        assert!(!configured(&store, Provider::Gemini).unwrap());
    }

    #[test]
    fn a_computer_of_the_users_own_sends_its_voice_key_itself() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        save(&store, Provider::OpenAi, "sk-voice").unwrap();
        assert!(configured(&store, Provider::OpenAi).unwrap());
        let route = access(&store, Provider::OpenAi).unwrap();
        assert_eq!((route.base.as_str(), route.key.as_deref()), (OPENAI, Some("sk-voice")));
    }
}
