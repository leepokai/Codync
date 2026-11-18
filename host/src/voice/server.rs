//! The server the `OpenAI` engine's Speech mode talks to: `api.openai.com`, or an OpenAI-compatible
//! one a person saves in the call settings (a local speech to text and text to speech server).
//!
//! Like the cloud address it must be https, or plain http to this computer, since the saved key
//! goes out as a bearer token. A trailing `/v1` is accepted (servers document their base URL
//! either way). An address that isn't usable is an error, never a quiet fallback to `OpenAI`: the
//! user meant their audio to stay on their own server.

use crate::store::Store;
use anyhow::Result;

/// kv: the compatible server's base URL; absent while the engine talks to `OpenAI`.
const SAVED: &str = "voice-server.openai";

/// A server that wasn't saved, with why for analytics (`category`): the message can hold what the server said.
#[derive(Debug)]
pub struct Refused {
    pub category: &'static str,
    message: String,
}

impl Refused {
    pub(crate) fn error(category: &'static str, message: impl Into<String>) -> anyhow::Error {
        Self { category, message: message.into() }.into()
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Refused {}

/// The saved compatible server, or `None` while the engine talks to `OpenAI`.
pub(super) fn saved(store: &Store) -> Option<String> {
    store.kv_get(SAVED).filter(|url| !url.is_empty())
}

/// Keeps `url` (as `parse` returned it); `None` goes back to `OpenAI`.
pub(super) fn save(store: &Store, url: Option<&str>) -> Result<()> {
    match url {
        Some(url) => store.kv_set(SAVED, url),
        None => store.kv_delete(SAVED),
    }
}

/// The base URL a person typed, or `None` for a blank one (`OpenAI` itself).
pub(super) fn parse(raw: &str) -> Result<Option<String>> {
    let raw = raw.trim().trim_end_matches('/');
    if raw.is_empty() {
        return Ok(None);
    }
    let without_v1 = raw.strip_suffix("/v1").unwrap_or(raw);
    match crate::remote::parse_url(without_v1) {
        Some(url) => Ok(Some(url)),
        None => Err(Refused::error(
            "bad_address",
            "The server must be an https URL, or http to this computer (http://127.0.0.1:8890).",
        )),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn a_local_server_with_or_without_v1() {
        for raw in [
            "http://127.0.0.1:8890",
            "http://127.0.0.1:8890/",
            "http://127.0.0.1:8890/v1",
            " http://localhost:8890/v1/ ",
        ] {
            let url = parse(raw).unwrap().unwrap();
            assert!(url == "http://127.0.0.1:8890" || url == "http://localhost:8890", "{raw} → {url}");
        }
        assert_eq!(parse("https://speech.example.com/v1").unwrap().unwrap(), "https://speech.example.com");
        assert_eq!(parse("  ").unwrap(), None);
    }

    #[test]
    fn the_key_never_crosses_the_network_in_cleartext() {
        for raw in [
            "http://192.168.1.20:8890",
            "http://speech.lan/v1",
            "ftp://127.0.0.1",
            "https://u:p@example.com",
            "nonsense",
        ] {
            assert!(parse(raw).is_err(), "{raw} accepted");
        }
    }

    #[test]
    fn a_saved_server_stays_until_cleared() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        assert_eq!(saved(&store), None);
        save(&store, Some("http://127.0.0.1:8890")).unwrap();
        assert_eq!(saved(&store).as_deref(), Some("http://127.0.0.1:8890"));
        save(&store, None).unwrap();
        assert_eq!(saved(&store), None);
    }
}
