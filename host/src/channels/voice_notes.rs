//! Authorized channel attachments reuse the host's speech connection and container detector.

use crate::{egress, store::Store, voice::transcription};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Audio {
    url: String,
    size: u64,
}

#[derive(Clone, Copy)]
pub(crate) enum Platform {
    Slack,
    Discord,
}

impl Platform {
    fn fields(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Slack => ("files", "mimetype", "url_private_download"),
            Self::Discord => ("attachments", "content_type", "url"),
        }
    }

    pub(crate) fn has_audio(self, message: &Value) -> bool {
        let (files, mime, _) = self.fields();
        message[files].as_array().is_some_and(|files| {
            files.iter().take(10).any(|file| file[mime].as_str().is_some_and(|mime| mime.starts_with("audio/")))
        })
    }

    fn hosts(self) -> &'static [&'static str] {
        match self {
            Self::Slack => &["files.slack.com"],
            Self::Discord => &["cdn.discordapp.com", "media.discordapp.net"],
        }
    }

    fn url(self, value: &str) -> Result<reqwest::Url> {
        ensure!(value.len() <= 4096, "Voice attachment URL is too long");
        let url = reqwest::Url::parse(value).context("Invalid voice attachment URL")?;
        ensure!(
            url.scheme() == "https"
                && url.username().is_empty()
                && url.password().is_none()
                && url.fragment().is_none()
                && url.port_or_known_default() == Some(443)
                && url.host_str().is_some_and(|host| self.hosts().contains(&host)),
            "Untrusted voice attachment location"
        );
        let path = match self {
            Self::Slack => "/files-pri/",
            Self::Discord => "/attachments/",
        };
        ensure!(url.path().starts_with(path), "Unexpected voice attachment path");
        Ok(url)
    }

    /// Keep only the first audio attachment, with bounded metadata. Bytes are never in the inbox.
    pub(crate) fn attachment(self, message: &Value) -> Option<Audio> {
        let (files, mime, url) = self.fields();
        message[files].as_array()?.iter().take(10).find_map(|file| {
            if !file[mime].as_str()?.starts_with("audio/") {
                return None;
            }
            let size =
                file["size"].as_u64().filter(|size| (1..=transcription::MAX_NOTE_BYTES as u64).contains(size))?;
            let raw = file[url]
                .as_str()
                .or_else(|| matches!(self, Self::Slack).then(|| file["url_private"].as_str()).flatten())?;
            self.url(raw).ok()?;
            Some(Audio { url: raw.to_owned(), size })
        })
    }
}

pub(crate) fn unavailable(text: &str) -> String {
    format!("{text}\n\n[Voice note could not be transcribed. Ask the sender to clarify; keep the original attachment.]")
}

async fn transcript(store: &Store, platform: Platform, audio: &Audio, token: Option<&str>) -> Result<String> {
    let url = platform.url(&audio.url)?;
    ensure!((1..=transcription::MAX_NOTE_BYTES as u64).contains(&audio.size), "Voice attachment exceeds 20 MiB");
    let client = egress::client(egress::Policy::Only(platform.hosts()), url.as_str()).await?;
    let mut request = client.get(url).timeout(Duration::from_secs(30));
    if matches!(platform, Platform::Slack) {
        request = request.bearer_auth(token.context("Slack connection is unavailable")?);
    }
    let mut response = request.send().await.map_err(|_| anyhow::anyhow!("Voice attachment download failed"))?;
    ensure!(response.status().is_success(), "Voice attachment download was refused");
    ensure!(
        response.content_length().is_none_or(|size| size <= transcription::MAX_NOTE_BYTES as u64),
        "Voice attachment exceeds 20 MiB"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| anyhow::anyhow!("Voice attachment download failed"))? {
        ensure!(
            bytes.len().saturating_add(chunk.len()) <= transcription::MAX_NOTE_BYTES,
            "Voice attachment exceeds 20 MiB"
        );
        bytes.extend_from_slice(&chunk);
    }
    transcription::note(store, &bytes).await
}

/// Recheck the same generation, binding and sender while downloading/transcribing. Revocation
/// drops in-flight work and must never become a fallback prompt to a newly authorized bot.
pub(crate) async fn append(
    store: &Store,
    platform: Platform,
    audio: &Audio,
    token: Option<&str>,
    text: &str,
    authorized: impl Fn() -> Result<bool>,
) -> Result<Option<String>> {
    if !authorized()? {
        return Ok(None);
    }
    let revoked = async {
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            if !authorized()? {
                return anyhow::Ok(());
            }
        }
    };
    let result = tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(90), transcript(store, platform, audio, token)) => result,
        result = revoked => { result?; return Ok(None); }
    };
    if !authorized()? {
        return Ok(None);
    }
    Ok(Some(if let Ok(Ok(transcript)) = result {
        format!("{text}\n\n[Voice note transcript]\n{transcript}")
    } else {
        tracing::warn!("Channel voice note could not be transcribed");
        unavailable(text)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn voice_downloads_reject_offsite_credentials_ports_and_unexpected_paths() {
        for url in [
            "http://files.slack.com/files-pri/voice.ogg",
            "https://files.slack.com.evil.example/files-pri/voice.ogg",
            "https://user@files.slack.com/files-pri/voice.ogg",
            "https://files.slack.com:444/files-pri/voice.ogg",
            "https://files.slack.com/files-pri/voice.ogg#fragment",
            "https://files.slack.com/api/voice.ogg",
            "https://127.0.0.1/files-pri/voice.ogg",
            "https://cdn.discordapp.com/attachments/1/2/voice.ogg",
        ] {
            assert!(Platform::Slack.url(url).is_err(), "{url}");
        }
        assert!(Platform::Slack.url("https://files.slack.com/files-pri/T-F/download/voice.ogg").is_ok());
        assert!(Platform::Discord.url("https://cdn.discordapp.com/attachments/1/2/voice.ogg?ex=123&hm=abc").is_ok());
        assert!(Platform::Discord.url("https://media.discordapp.net/attachments/1/2/voice.ogg").is_ok());
        assert!(Platform::Discord.url("https://files.slack.com/files-pri/voice.ogg").is_err());
    }

    #[test]
    fn only_bounded_audio_metadata_is_kept_and_slack_private_url_is_supported() {
        let file =
            json!({"mimetype":"audio/ogg","size":42,"url_private":"https://files.slack.com/files-pri/T-F/a.ogg"});
        let note = Platform::Slack.attachment(&json!({"files":[file.clone()]})).unwrap();
        assert_eq!(note.size, 42);
        let serialized = serde_json::to_string(&note).unwrap();
        assert_eq!(serde_json::from_str::<Audio>(&serialized).unwrap().url, note.url);
        for replacement in [json!(0), json!(-1), json!(transcription::MAX_NOTE_BYTES + 1), json!("42")] {
            let mut invalid = file.clone();
            invalid["size"] = replacement;
            assert!(Platform::Slack.attachment(&json!({"files":[invalid]})).is_none());
        }
        assert!(
            Platform::Discord
                .attachment(&json!({"attachments":[{
                    "content_type":"image/png","size":42,"url":"https://cdn.discordapp.com/attachments/1/2/a.png"
                }]}))
                .is_none()
        );
    }

    #[tokio::test]
    async fn a_revoked_note_never_downloads_or_becomes_a_fallback_prompt() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        // Deliberately invalid: reaching the downloader would produce a fallback instead of None.
        let audio = Audio { url: "http://127.0.0.1/never-request".into(), size: 42 };
        assert!(append(&store, Platform::Discord, &audio, None, "voice", || Ok(false)).await.unwrap().is_none());
        assert!(append(&store, Platform::Slack, &audio, None, "voice", || anyhow::bail!("read failed")).await.is_err());
    }
}
