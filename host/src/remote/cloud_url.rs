//! Which cloud base URL this computer uses, and the address a person saves.
//!
//! `CODYNC_CLOUD=off` wins, then the switch in settings, then `CODYNC_CLOUD_URL`, then the
//! saved `cloud_url`, then the build's cloud. Unit tests point at a fake cloud through kv
//! `test_cloud_url`, so they never reach a real one.

use super::cloud::{self, CloudStatus};
use crate::hub::Hub;
use crate::store::Store;
use anyhow::{Result, bail};

/// The cloud this build belongs to: main only for builds made with `CODYNC_ENV=main` (the
/// release workflows), dev for every local build, debug or release, like the apps built next
/// to it.
fn build_cloud_url() -> &'static str {
    crate::environment::Environment::current().cloud_url()
}

/// The cloud base URL in use, or `None` when the cloud is off.
pub fn url(store: &Store) -> Option<String> {
    let off = std::env::var("CODYNC_CLOUD").is_ok_and(|v| v == "off")
        || store.kv_get("cloud_enabled").as_deref() == Some("false");
    if cfg!(test) {
        return if off { None } else { store.kv_get("test_cloud_url") };
    }
    let env = std::env::var("CODYNC_CLOUD_URL").ok();
    let saved = store.kv_get("cloud_url");
    choose_url(off, [env, saved], build_cloud_url())
}

/// `off` wins. Otherwise the first valid candidate, then `builtin`. An illegal candidate is skipped.
fn choose_url(off: bool, candidates: [Option<String>; 2], builtin: &str) -> Option<String> {
    if off {
        return None;
    }
    for candidate in candidates.into_iter().flatten() {
        if let Some(url) = normalize_url(&candidate) {
            return Some(url);
        }
    }
    Some(builtin.to_owned())
}

/// A stored or typed cloud URL: https, or plain http to this computer, with no path or query.
fn normalize_url(raw: &str) -> Option<String> {
    let stored = raw.trim().trim_end_matches('/').to_owned();
    valid_url(&format!("{stored}/")).then_some(stored)
}

/// A cloud URL this host may be pointed at: https, or plain http to this computer (local dev).
fn valid_url(u: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(u) else { return false };
    let local = matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    parsed.path() == "/"
        && parsed.query().is_none()
        && (parsed.scheme() == "https" || (parsed.scheme() == "http" && local))
}

/// `setCloudUrl`: points this computer at a cloud, or clears the saved URL so the build's cloud
/// is used again. Empty and null clear it. An illegal URL is refused and changes nothing.
/// The relay restarts either way. Local only.
pub fn set_cloud_url(hub: &Hub, raw: Option<&str>) -> Result<CloudStatus> {
    match raw.map(str::trim).filter(|u| !u.is_empty()) {
        None => hub.store.kv_delete("cloud_url")?,
        Some(raw) => {
            let Some(stored) = normalize_url(raw) else {
                bail!("That relay address isn't a usable cloud URL.");
            };
            hub.store.kv_set("cloud_url", &stored)?;
        }
    }
    let now = url(&hub.store);
    cloud::update_status(hub, |s| {
        s.enabled = now.is_some();
        s.url.clone_from(&now);
    });
    hub.cloud.config.send_modify(|g| *g += 1);
    Ok(hub.cloud.status())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::remote::identity::Identity;
    use std::sync::Arc;

    fn temp_hub() -> Arc<Hub> {
        let dir = std::env::temp_dir().join(format!("codync-cloud-url-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("t.db")).unwrap();
        Hub::new(store, "host".into(), Identity::load_or_create(&dir).unwrap(), "token".into(), 0)
    }

    #[test]
    fn only_safe_cloud_urls() {
        assert!(valid_url("https://codync-cloud.example.workers.dev/"));
        assert!(valid_url("http://127.0.0.1:8787/"));
        assert!(!valid_url("http://example.com/"));
        assert!(!valid_url("https://example.com/path/"));
        assert!(!valid_url("ftp://example.com/"));
    }

    #[test]
    fn a_saved_cloud_url_is_used_only_when_nothing_in_front_of_it_is() {
        assert_eq!(
            choose_url(false, [None, Some("https://relay.example.com/".into())], "https://api.codync.dev"),
            Some("https://relay.example.com".into())
        );
        assert_eq!(
            choose_url(
                false,
                [Some("https://from.env".into()), Some("https://saved.example".into())],
                "https://builtin"
            ),
            Some("https://from.env".into())
        );
        assert_eq!(
            choose_url(false, [Some("http://example.com".into()), None], "https://builtin"),
            Some("https://builtin".into()),
            "an illegal candidate is skipped"
        );
        assert_eq!(choose_url(true, [Some("https://relay.example.com".into()), None], "https://builtin"), None);
    }

    #[test]
    fn set_cloud_url_stores_a_normalized_url_or_clears_it() {
        let hub = temp_hub();
        set_cloud_url(&hub, Some("https://relay.example.com/")).unwrap();
        assert_eq!(hub.store.kv_get("cloud_url").as_deref(), Some("https://relay.example.com"));
        let bad = set_cloud_url(&hub, Some("http://example.com")).unwrap_err();
        assert!(bad.to_string().contains("relay address"), "{bad}");
        assert_eq!(hub.store.kv_get("cloud_url").as_deref(), Some("https://relay.example.com"));
        set_cloud_url(&hub, Some("  ")).unwrap();
        assert_eq!(hub.store.kv_get("cloud_url"), None);
        set_cloud_url(&hub, Some("http://127.0.0.1:8788")).unwrap();
        assert_eq!(hub.store.kv_get("cloud_url").as_deref(), Some("http://127.0.0.1:8788"));
    }
}
