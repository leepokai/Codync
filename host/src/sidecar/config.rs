//! Purely explicit opt-in configuration for decision sidecar endpoints.
//! Never probes or falls back to any implicit local default port.

use crate::store::Store;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;

pub const ENV_TARGETS: &str = "CODYNC_DECISION_SIDECAR_TARGETS";
pub const ENV_URL: &str = "CODYNC_DECISION_SIDECAR_URL";
pub const ENV_MODEL: &str = "CODYNC_DECISION_SIDECAR_MODEL";
pub const ENV_API_KEY: &str = "CODYNC_DECISION_SIDECAR_API_KEY";
pub const STORE_TARGETS_KEY: &str = "settings.decision_sidecar.targets";

/// An ordered candidate endpoint speaking `OpenAI` Chat Completions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarTarget {
    pub url: String,
    pub model: String,
    #[serde(default, alias = "apiKey", skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

impl SidecarTarget {
    /// Full `POST` endpoint URL for Chat Completions (`.../v1/chat/completions` or
    /// `.../chat/completions`).
    pub fn endpoint_url(&self) -> String {
        let base = self.url.trim().trim_end_matches('/');
        if base.ends_with("/chat/completions") {
            return base.to_owned();
        }
        let has_base_path = reqwest::Url::parse(base).is_ok_and(|u| u.path() != "/" && !u.path().is_empty());
        if has_base_path { format!("{base}/chat/completions") } else { format!("{base}/v1/chat/completions") }
    }

    /// Stable in-memory key for circuit breaking, distinguishing API-key variants via a
    /// non-reversible SHA-256 fingerprint without ever exposing raw credentials.
    pub fn breaker_key(&self) -> String {
        let key_fp = match self.api_key.as_deref().filter(|k| !k.is_empty()) {
            Some(key) => {
                let hash = Sha256::digest(key.as_bytes());
                hash[..8].iter().fold(String::with_capacity(16), |mut s, b| {
                    let _ = write!(s, "{b:02x}");
                    s
                })
            }
            None => "none".to_owned(),
        };
        format!("{}#{}#{key_fp}", self.endpoint_url(), self.model)
    }

    /// Safe target identifier for logs; never includes `api_key`.
    pub fn redacted_label(&self) -> String {
        format!("{} (model={})", self.endpoint_url(), self.model)
    }
}

fn normalize_target(raw: SidecarTarget) -> Option<SidecarTarget> {
    let url = raw.url.trim().trim_end_matches('/').to_owned();
    let model = raw.model.trim().to_owned();
    if url.is_empty() || model.is_empty() {
        return None;
    }
    let parsed = reqwest::Url::parse(&url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return None;
    }
    let api_key = raw.api_key.map(|k| k.trim().to_owned()).filter(|k| !k.is_empty());
    Some(SidecarTarget { url, model, api_key })
}

fn parse_targets_json(raw: &str, source: &'static str) -> Vec<SidecarTarget> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let parsed: Vec<SidecarTarget> = if trimmed.starts_with('{') {
        match serde_json::from_str::<SidecarTarget>(trimmed) {
            Ok(one) => vec![one],
            Err(error) => {
                tracing::warn!(source, %error, "invalid decision sidecar target JSON");
                return Vec::new();
            }
        }
    } else {
        match serde_json::from_str::<Vec<SidecarTarget>>(trimmed) {
            Ok(list) => list,
            Err(error) => {
                tracing::warn!(source, %error, "invalid decision sidecar targets JSON");
                return Vec::new();
            }
        }
    };
    parsed.into_iter().filter_map(normalize_target).collect()
}

/// Loads explicitly configured targets from process environment variables only.
pub fn load_from_env() -> Vec<SidecarTarget> {
    load_targets_with(None, |name| std::env::var(name).ok())
}

/// Loads explicitly configured targets from environment variables or `store`.
pub fn load_targets(store: Option<&Store>) -> Vec<SidecarTarget> {
    load_targets_with(store, |name| std::env::var(name).ok())
}

pub(crate) fn load_targets_with(store: Option<&Store>, env_fn: impl Fn(&str) -> Option<String>) -> Vec<SidecarTarget> {
    if let Some(raw) = env_fn(ENV_TARGETS).filter(|v| !v.trim().is_empty()) {
        let targets = parse_targets_json(&raw, ENV_TARGETS);
        if !targets.is_empty() {
            return targets;
        }
    }

    if let Some(url) = env_fn(ENV_URL).map(|v| v.trim().to_owned()).filter(|v| !v.is_empty()) {
        let model = env_fn(ENV_MODEL).map(|v| v.trim().to_owned()).unwrap_or_default();
        let api_key = env_fn(ENV_API_KEY);
        if let Some(target) = normalize_target(SidecarTarget { url, model, api_key }) {
            return vec![target];
        }
        tracing::warn!("ignoring incomplete or invalid {ENV_URL}/{ENV_MODEL} sidecar configuration");
    }

    if let Some(store) = store
        && let Some(raw) = store.kv_get(STORE_TARGETS_KEY).filter(|v| !v.trim().is_empty())
    {
        return parse_targets_json(&raw, STORE_TARGETS_KEY);
    }

    Vec::new()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;

    #[test]
    fn unconfigured_returns_empty_without_default_port_probing() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        let targets = load_targets_with(Some(&store), |_| None);
        assert!(targets.is_empty(), "must never fall back to any implicit local default port");
    }

    #[test]
    fn parses_multi_target_and_single_shorthand_and_store_settings() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        store
            .kv_set(
                STORE_TARGETS_KEY,
                r#"[{"url":"http://127.0.0.1:8000/v1/","model":"qwen-fast","apiKey":"secret-store"}]"#,
            )
            .unwrap();

        let from_store = load_targets_with(Some(&store), |_| None);
        assert_eq!(from_store.len(), 1);
        assert_eq!(from_store[0].endpoint_url(), "http://127.0.0.1:8000/v1/chat/completions");
        assert_eq!(from_store[0].api_key.as_deref(), Some("secret-store"));
        assert!(!from_store[0].breaker_key().contains("secret-store"));
        assert!(!from_store[0].redacted_label().contains("secret-store"));

        let single_env: HashMap<&str, &str> =
            [(ENV_URL, "https://api.example.com"), (ENV_MODEL, "gpt-4o-mini"), (ENV_API_KEY, "sk-single")]
                .into_iter()
                .collect();
        let from_single = load_targets_with(Some(&store), |k| single_env.get(k).map(|v| (*v).to_owned()));
        assert_eq!(from_single.len(), 1);
        assert_eq!(from_single[0].endpoint_url(), "https://api.example.com/v1/chat/completions");

        let multi_env: HashMap<&str, &str> = [(
            ENV_TARGETS,
            r#"[
                {"url":"https://primary.example.com/v1","model":"m1","api_key":"k1"},
                {"url":"http://localhost:11434","model":"llama3.2"}
            ]"#,
        )]
        .into_iter()
        .collect();
        let from_multi = load_targets_with(Some(&store), |k| multi_env.get(k).map(|v| (*v).to_owned()));
        assert_eq!(from_multi.len(), 2);
        assert_eq!(from_multi[0].endpoint_url(), "https://primary.example.com/v1/chat/completions");
        assert_eq!(from_multi[1].endpoint_url(), "http://localhost:11434/v1/chat/completions");
        assert_eq!(from_multi[1].api_key, None);
    }

    #[test]
    fn rejects_malformed_or_userinfo_urls() {
        let env: HashMap<&str, &str> = [(
            ENV_TARGETS,
            r#"[
                {"url":"https://user:pass@example.com/v1","model":"m1"},
                {"url":"ftp://127.0.0.1:3425","model":"m2"},
                {"url":"http://127.0.0.1:8080/v1?token=leak","model":"m3"},
                {"url":"https://ok.example.com/api/v1","model":"m4"}
            ]"#,
        )]
        .into_iter()
        .collect();
        let targets = load_targets_with(None, |k| env.get(k).map(|v| (*v).to_owned()));
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].endpoint_url(), "https://ok.example.com/api/v1/chat/completions");
    }

    #[test]
    fn breaker_key_distinguishes_api_key_variants_without_leaking_secrets() {
        let primary = SidecarTarget {
            url: "https://api.example.com/v1".into(),
            model: "fast".into(),
            api_key: Some("secret-key-primary".into()),
        };
        let backup = SidecarTarget {
            url: "https://api.example.com/v1".into(),
            model: "fast".into(),
            api_key: Some("secret-key-backup".into()),
        };
        let keyless = SidecarTarget { url: "https://api.example.com/v1".into(), model: "fast".into(), api_key: None };
        assert_ne!(primary.breaker_key(), backup.breaker_key());
        assert_ne!(primary.breaker_key(), keyless.breaker_key());
        assert!(!primary.breaker_key().contains("secret-key"));
        assert!(!backup.breaker_key().contains("secret-key"));
    }
}
