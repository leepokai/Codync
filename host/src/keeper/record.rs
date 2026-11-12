//! What a proxied slot holds: where its secret may go, and how it is sent there. The host writes these
//! (`putProxied`) and never reads them back; only the keeper's proxy opens them.

use anyhow::{Result, anyhow, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The providers' APIs, as the host and its agents reach them.
pub const ANTHROPIC: &str = "https://api.anthropic.com";
pub const OPENAI: &str = "https://api.openai.com";
pub const GEMINI: &str = "https://generativelanguage.googleapis.com";

/// A secret the keeper sends for its user: only to `upstream`, as `headers`, never readable.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Record {
    /// Every request through a grant goes here (its path, then the client's): `https://`, no query or user.
    pub upstream: String,
    /// Added to every request, replacing whatever the client sent under these names (lowercase).
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// Query parameters dropped from the client's request (where an SDK puts the key, `?key=`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub strip_query: Vec<String>,
    /// A sign-in the keeper keeps fresh itself: its access token is `headers[oauth.header]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth: Option<OAuth>,
}

/// An OAuth 2 sign-in, refreshed with the `refresh_token` grant; one without a refresh token ends with its access
/// token.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OAuth {
    pub token_url: String,
    pub client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Unix seconds when the access token expires; 0: unknown (refreshed only when the upstream says 401).
    #[serde(default)]
    pub expires_at: u64,
    /// The header the access token goes in, as `Bearer …`.
    #[serde(default = "authorization")]
    pub header: String,
    /// RFC 8707 `resource`, sent with each refresh (MCP servers ask for it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
}

fn authorization() -> String {
    "authorization".into()
}

/// What a `putProxied` keeps from the slot's record before it: only while `upstream` stays the same, so nothing
/// kept can be sent anywhere new.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Keep {
    /// Headers the new record doesn't set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<String>,
    /// The sign-in (and its access token header), when the new record has none.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub oauth: bool,
}

impl Record {
    /// `headers` sent to `upstream`.
    pub fn new(upstream: &str, headers: BTreeMap<String, String>) -> Self {
        Self { upstream: upstream.to_owned(), headers, strip_query: Vec::new(), oauth: None }
    }

    /// A key sent to `upstream` as one header.
    pub fn header(upstream: &str, name: &str, value: String) -> Self {
        Self::new(upstream, BTreeMap::from([(name.to_owned(), value)]))
    }

    /// A key sent as `authorization: Bearer <key>` (an `OpenAI` key, an Anthropic auth token).
    pub fn bearer(upstream: &str, key: &str) -> Self {
        Self::header(upstream, "authorization", format!("Bearer {key}"))
    }

    /// A Google API key: sent as `x-goog-api-key`, and dropped from the query, where Google's SDKs may also put it.
    pub fn google_key(upstream: &str, key: &str) -> Self {
        let mut record = Self::header(upstream, "x-goog-api-key", key.to_owned());
        record.strip_query = vec!["key".into()];
        record
    }

    /// Lowercase header names; refuses an upstream or header that couldn't be sent as given.
    #[cfg(unix)]
    pub fn checked(mut self) -> Result<Self> {
        upstream(&self.upstream)?;
        self.headers = self.headers.into_iter().map(|(k, v)| (k.to_ascii_lowercase(), v)).collect();
        for (k, v) in &self.headers {
            reqwest::header::HeaderName::from_bytes(k.as_bytes()).map_err(|_| anyhow!("invalid header name"))?;
            reqwest::header::HeaderValue::from_str(v).map_err(|_| anyhow!("invalid value for header {k}"))?;
        }
        if let Some(o) = &mut self.oauth {
            o.header = o.header.to_ascii_lowercase();
            reqwest::header::HeaderName::from_bytes(o.header.as_bytes())
                .map_err(|_| anyhow!("invalid OAuth header name"))?;
            upstream(&o.token_url).map_err(|_| anyhow!("the OAuth token URL must be https"))?;
        }
        Ok(self)
    }

    /// `self` with what `keep` names taken from `old`, when both go to the same upstream.
    #[cfg(unix)]
    pub fn keeping(mut self, old: Option<Self>, keep: &Keep) -> Self {
        let Some(old) = old.filter(|old| old.upstream == self.upstream) else { return self };
        for name in &keep.headers {
            let name = name.to_ascii_lowercase();
            if let (false, Some(v)) = (self.headers.contains_key(&name), old.headers.get(&name)) {
                self.headers.insert(name, v.clone());
            }
        }
        if keep.oauth
            && self.oauth.is_none()
            && let Some(o) = old.oauth
        {
            if let (false, Some(v)) = (self.headers.contains_key(&o.header), old.headers.get(&o.header)) {
                self.headers.insert(o.header.clone(), v.clone());
            }
            self.oauth = Some(o);
        }
        self
    }
}

/// An upstream the keeper may send a secret to: `https://` (in unit tests, also a local test server).
pub fn upstream(raw: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(raw).map_err(|_| anyhow!("invalid upstream URL"))?;
    let local_test = cfg!(test) && url.scheme() == "http" && url.host_str() == Some("127.0.0.1");
    ensure!(url.scheme() == "https" || local_test, "the upstream must be https");
    if url.host_str().is_none_or(str::is_empty)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("the upstream must be a plain https address");
    }
    Ok(url)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn upstreams_are_plain_https() {
        assert!(upstream("https://api.openai.com/v1").is_ok());
        for bad in ["http://api.openai.com", "https://u:p@x.com", "https://x.com/?key=1", "file:///etc", "nope"] {
            assert!(upstream(bad).is_err(), "{bad}");
        }
        let record = Record::header("https://x.com", "X-Api-Key", "k".into()).checked().unwrap();
        assert_eq!(record.headers.keys().collect::<Vec<_>>(), ["x-api-key"]);
        assert!(Record::header("https://x.com", "bad name", "k".into()).checked().is_err());
        assert!(Record::header("https://x.com", "x", "a\nb".into()).checked().is_err());
    }

    #[test]
    fn keeps_only_for_the_same_upstream() {
        let oauth = OAuth {
            token_url: "https://x.com/token".into(),
            client_id: "c".into(),
            client_secret: None,
            refresh_token: Some("r".into()),
            expires_at: 0,
            header: "authorization".into(),
            resource: None,
        };
        let mut old = Record::header("https://x.com", "x-a", "a".into());
        old.headers.insert("authorization".into(), "Bearer t".into());
        old.oauth = Some(oauth);
        let keep = Keep { headers: vec!["X-A".into()], oauth: true };
        let kept = Record::header("https://x.com", "x-b", "b".into()).keeping(Some(old.clone()), &keep);
        assert_eq!(kept.headers.len(), 3);
        assert!(kept.oauth.is_some());
        let elsewhere = Record::header("https://evil.example", "x-b", "b".into()).keeping(Some(old), &keep);
        assert_eq!(elsewhere.headers.len(), 1, "nothing follows a record to a new upstream");
        assert!(elsewhere.oauth.is_none());
    }
}
