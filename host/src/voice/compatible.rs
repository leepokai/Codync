//! `CODYNC_OPENAI_BASE_URL`: points the `OpenAI` engine's Speech mode at an OpenAI-compatible server
//! (a local speech to text and text to speech server) instead of `api.openai.com`.
//!
//! Like `CODYNC_CLOUD_URL` it must be https, or plain http to this computer, since the saved key goes
//! out as a bearer token. A trailing `/v1` is accepted (servers document their base URL either way).
//! A value that isn't usable is an error, never a quiet fallback to `OpenAI`: the user meant their
//! audio to stay on their own server.

use anyhow::{Result, bail};

const VAR: &str = "CODYNC_OPENAI_BASE_URL";

/// The compatible server's base URL, or `None` when the variable is unset or blank.
pub(super) fn base() -> Result<Option<String>> {
    std::env::var(VAR).ok().as_deref().map(parse).transpose().map(Option::flatten)
}

fn parse(raw: &str) -> Result<Option<String>> {
    let raw = raw.trim().trim_end_matches('/');
    if raw.is_empty() {
        return Ok(None);
    }
    let without_v1 = raw.strip_suffix("/v1").unwrap_or(raw);
    match crate::remote::parse_url(without_v1) {
        Some(url) => Ok(Some(url)),
        None => bail!("{VAR} must be an https URL, or http to this computer (http://127.0.0.1:8890)."),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::parse;

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
}
