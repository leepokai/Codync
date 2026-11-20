//! Webhook deliveries, from the local endpoint or the cloud's public one (spec §7.8): who may
//! send one (the routine's bearer key, or a GitHub-style HMAC of the body), and the event the
//! routine's triggers match and its run sees.
//!
//! The cloud checks the same proof before it queues a delivery; the host checks again because
//! it alone decides what runs.

use hmac::{KeyInit as _, Mac as _};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const MAX_BODY: usize = 64 * 1024;
const MAX_DELIVERY_ID: usize = 200;

/// Request headers by lowercase name.
pub type Headers = BTreeMap<String, String>;

/// Why a delivery didn't start a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// Unknown routine or wrong key: told apart nowhere, so ids can't be probed.
    Unauthorized,
    Paused,
    /// The routine's run has started, or its queue is full; the sender (or the cloud queue) tries again later.
    Busy,
    Invalid(String),
}

impl Refused {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::Paused => "paused",
            Self::Busy => "busy",
            Self::Invalid(_) => "invalid",
        }
    }

    pub fn retry(&self) -> bool {
        matches!(self, Self::Busy)
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized => f.write_str("invalid webhook key or signature"),
            Self::Paused => f.write_str("routine is paused"),
            Self::Busy => write!(f, "routine has a run under way or {} queued; try again later", super::MAX_QUEUED),
            Self::Invalid(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for Refused {}

/// `Authorization: Bearer <key>`, or `X-Hub-Signature-256: sha256=<hex HMAC-SHA256(key, body)>`.
pub fn authorized(key: &str, headers: &Headers, body: &[u8]) -> bool {
    if key.is_empty() {
        return false;
    }
    if let Some(given) = headers.get("authorization").and_then(|v| v.strip_prefix("Bearer "))
        && crate::remote::crypto::ct_eq(given.as_bytes(), key.as_bytes())
    {
        return true;
    }
    let Some(sig) = headers.get("x-hub-signature-256").and_then(|v| v.strip_prefix("sha256=")).and_then(hex) else {
        return false;
    };
    let Ok(mut mac) = hmac::Hmac::<sha2::Sha256>::new_from_slice(key.as_bytes()) else { return false };
    mac.update(body);
    mac.verify_slice(&sig).is_ok()
}

fn hex(s: &str) -> Option<Vec<u8>> {
    if s.len() != 64 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// The sender's id for this delivery, so a retry never runs twice.
pub fn delivery_id(headers: &Headers) -> Result<Option<String>, Refused> {
    let id = ["x-delivery-id", "x-github-delivery", "idempotency-key"].iter().find_map(|h| headers.get(*h));
    match id {
        None => Ok(None),
        Some(id) if !id.is_empty() && id.len() <= MAX_DELIVERY_ID && id.bytes().all(|b| b.is_ascii_graphic()) => {
            Ok(Some(id.clone()))
        }
        Some(_) => Err(Refused::Invalid("invalid delivery id".into())),
    }
}

/// What a routine sees and its event triggers match; `None` for a GitHub `ping`, which only
/// checks the connection.
///
/// A JSON object is the event as-is; GitHub deliveries become `{source:"github", event:
/// "<event>.<action>", repo, sender, text, url, payload}`; anything else is `{text}`.
pub fn event(headers: &Headers, body: &[u8]) -> Result<Option<Value>, Refused> {
    if body.len() > MAX_BODY {
        return Err(Refused::Invalid("event too large".into()));
    }
    let parsed: Option<Value> =
        if body.iter().all(u8::is_ascii_whitespace) { Some(json!({})) } else { serde_json::from_slice(body).ok() };
    if let Some(name) = headers.get("x-github-event") {
        if name == "ping" {
            return Ok(None);
        }
        let payload = parsed.ok_or_else(|| Refused::Invalid("GitHub deliveries must use application/json".into()))?;
        return Ok(Some(github(name, &payload)));
    }
    Ok(Some(match parsed {
        Some(v @ Value::Object(_)) => v,
        Some(other) => json!({ "payload": other }),
        None => json!({ "text": String::from_utf8_lossy(body) }),
    }))
}

fn github(name: &str, payload: &Value) -> Value {
    let event = payload["action"].as_str().map_or_else(|| name.to_owned(), |action| format!("{name}.{action}"));
    let first = |paths: &[&[&str]]| {
        paths.iter().find_map(|path| {
            let v = path.iter().fold(payload, |v, k| &v[*k]);
            v.as_str().filter(|s| !s.is_empty()).map(str::to_owned)
        })
    };
    let text = first(&[
        &["pull_request", "title"],
        &["issue", "title"],
        &["discussion", "title"],
        &["release", "name"],
        &["release", "tag_name"],
        &["comment", "body"],
        &["review", "body"],
        &["head_commit", "message"],
        &["workflow_run", "name"],
    ]);
    let url = first(&[
        &["comment", "html_url"],
        &["review", "html_url"],
        &["pull_request", "html_url"],
        &["issue", "html_url"],
        &["discussion", "html_url"],
        &["release", "html_url"],
        &["workflow_run", "html_url"],
        &["compare"],
    ]);
    json!({
        "source": "github",
        "event": event,
        "repo": payload["repository"]["full_name"],
        "sender": payload["sender"]["login"],
        "text": text,
        "url": url,
        "payload": payload,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    const KEY: &str = "0123456789abcdef0123456789abcdef";

    fn headers(pairs: &[(&str, &str)]) -> Headers {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
    }

    fn signature(key: &str, body: &[u8]) -> String {
        let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(key.as_bytes()).unwrap();
        mac.update(body);
        let bytes = mac.finalize().into_bytes();
        bytes.iter().fold(String::from("sha256="), |mut s, b| {
            use std::fmt::Write as _;
            let _ = write!(s, "{b:02x}");
            s
        })
    }

    #[test]
    fn bearer_key_or_signature_of_the_exact_body() {
        let body = br#"{"action":"opened"}"#;
        assert!(authorized(KEY, &headers(&[("authorization", &format!("Bearer {KEY}"))]), body));
        assert!(!authorized(KEY, &headers(&[("authorization", "Bearer nope")]), body));
        assert!(!authorized(KEY, &headers(&[]), body));
        let sig = signature(KEY, body);
        assert!(authorized(KEY, &headers(&[("x-hub-signature-256", &sig)]), body));
        assert!(!authorized(KEY, &headers(&[("x-hub-signature-256", &sig)]), b"{}"));
        assert!(!authorized(KEY, &headers(&[("x-hub-signature-256", &signature("other", body))]), body));
        assert!(!authorized(KEY, &headers(&[("x-hub-signature-256", "sha256=zz")]), body));
        assert!(!authorized("", &headers(&[("authorization", "Bearer ")]), body), "a cleared key never matches");
    }

    #[test]
    fn github_deliveries_become_events_triggers_can_match() {
        let body = br#"{"action":"opened","pull_request":{"title":"Fix login","html_url":"https://github.com/o/r/pull/1"},"repository":{"full_name":"o/r"},"sender":{"login":"kai"}}"#;
        let e = event(&headers(&[("x-github-event", "pull_request")]), body).unwrap().unwrap();
        assert_eq!(e["source"], "github");
        assert_eq!(e["event"], "pull_request.opened");
        assert_eq!(e["repo"], "o/r");
        assert_eq!(e["text"], "Fix login");
        assert_eq!(e["url"], "https://github.com/o/r/pull/1");
        assert_eq!(e["payload"]["sender"]["login"], "kai");
        let push = event(&headers(&[("x-github-event", "push")]), br#"{"head_commit":{"message":"m"},"compare":"c"}"#)
            .unwrap()
            .unwrap();
        assert_eq!(
            (push["event"].as_str(), push["text"].as_str(), push["url"].as_str()),
            (Some("push"), Some("m"), Some("c"))
        );
        assert_eq!(event(&headers(&[("x-github-event", "ping")]), b"{}").unwrap(), None);
        assert!(event(&headers(&[("x-github-event", "push")]), b"payload=%7B%7D").is_err());
    }

    #[test]
    fn other_bodies_keep_their_content() {
        let none = headers(&[]);
        assert_eq!(event(&none, br#"{"a":1}"#).unwrap(), Some(json!({"a":1})));
        assert_eq!(event(&none, b"[1]").unwrap(), Some(json!({"payload":[1]})));
        assert_eq!(event(&none, b"disk full").unwrap(), Some(json!({"text":"disk full"})));
        assert_eq!(event(&none, b"  ").unwrap(), Some(json!({})));
        assert!(event(&none, &vec![b'x'; MAX_BODY + 1]).is_err());
    }

    #[test]
    fn delivery_ids_come_from_the_usual_headers() {
        assert_eq!(delivery_id(&headers(&[])), Ok(None));
        assert_eq!(delivery_id(&headers(&[("x-github-delivery", "g-1")])), Ok(Some("g-1".into())));
        assert_eq!(delivery_id(&headers(&[("x-delivery-id", "a"), ("idempotency-key", "b")])), Ok(Some("a".into())));
        assert!(delivery_id(&headers(&[("x-delivery-id", "has space")])).is_err());
        assert!(delivery_id(&headers(&[("x-delivery-id", &"x".repeat(201))])).is_err());
    }
}
