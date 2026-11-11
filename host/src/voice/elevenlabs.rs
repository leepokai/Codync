//! `ElevenLabs` as a speech-only provider: Scribe hears speech and the account's voices read
//! replies. Its model list covers text to speech only, so transcription stays on Scribe v1/v2.

use super::{Model, Provider, TIMEOUT, provider_error, send};
use anyhow::{Result, anyhow};
use serde_json::{Value, json};

const API: &str = "https://api.elevenlabs.io";

/// How many pages of voices to read: 100 each, enough for any account and bounded for one that
/// keeps returning a token.
const VOICE_PAGES: usize = 10;

/// The account's voices, premade, cloned and saved, by id and name. A key that lists none is still
/// valid. `ElevenLabs` pages them, so a saved voice past the first page isn't lost.
pub(super) async fn voices(key: &str) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    let mut token: Option<String> = None;
    for _ in 0..VOICE_PAGES {
        let mut url = reqwest::Url::parse(&format!("{API}/v2/voices"))?;
        url.query_pairs_mut().append_pair("page_size", "100");
        if let Some(token) = &token {
            url.query_pairs_mut().append_pair("next_page_token", token);
        }
        let v = send(Provider::ElevenLabs, crate::http().get(url).header("xi-api-key", key)).await?;
        out.extend(v["voices"].as_array().into_iter().flatten().filter_map(|voice| {
            let id = voice["voice_id"].as_str()?;
            Some((id.to_owned(), voice["name"].as_str().unwrap_or(id).to_owned()))
        }));
        match (v["has_more"].as_bool().unwrap_or(false), v["next_page_token"].as_str()) {
            (true, Some(next)) if !next.is_empty() => token = Some(next.to_owned()),
            _ => break,
        }
    }
    Ok(out)
}

/// One stretch of a reply as MP3, by voice id: 64 kbps keeps each clip inside one channel message.
pub(super) async fn speech(key: &str, model: &str, voice: &str, text: &str) -> Result<Vec<u8>> {
    let res = crate::http()
        .post(format!("{API}/v1/text-to-speech/{voice}?output_format=mp3_44100_64"))
        .header("xi-api-key", key)
        .json(&json!({"model_id": model, "text": text}))
        .timeout(TIMEOUT * 4)
        .send()
        .await
        .map_err(|_| anyhow!("Can't reach ElevenLabs."))?;
    let status = res.status();
    let bytes = res.bytes().await.map_err(|_| anyhow!("ElevenLabs sent no audio."))?;
    if !status.is_success() {
        let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        return Err(provider_error(Provider::ElevenLabs, status, &v));
    }
    Ok(bytes.to_vec())
}

/// Checks both permissions before the key is kept: it lists the voices, and reads one character
/// with the first of them, which costs one character of the account's quota.
pub(super) async fn check(key: &str) -> Result<()> {
    if let Some((voice, _)) = voices(key).await?.first() {
        speech(key, &Provider::ElevenLabs.fallback().speech, voice, ".").await?;
    }
    Ok(())
}

/// The voice models the key can read with; every entry says whether it reads text.
pub(super) async fn models(key: &str) -> Result<Vec<Model>> {
    let v = send(Provider::ElevenLabs, crate::http().get(format!("{API}/v1/models")).header("xi-api-key", key)).await?;
    Ok(v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            Some(Model {
                id: m["model_id"].as_str()?.to_owned(),
                live: false,
                generate: m["can_do_text_to_speech"].as_bool().unwrap_or(false),
                created: None,
            })
        })
        .collect())
}

/// Speech to text for one utterance (a WAV file). The reply is read aloud, so audio events like
/// "(laughter)" have no place in the transcript.
pub(super) async fn transcribe(key: &str, model: &str, audio: Vec<u8>, extension: &str, mime: &str) -> Result<String> {
    let file = reqwest::multipart::Part::bytes(audio).file_name(format!("speech.{extension}")).mime_str(mime)?;
    let form = reqwest::multipart::Form::new()
        .text("model_id", model.to_owned())
        .text("tag_audio_events", "false")
        .part("file", file);
    let req = crate::http().post(format!("{API}/v1/speech-to-text")).header("xi-api-key", key).multipart(form);
    Ok(send(Provider::ElevenLabs, req).await?["text"].as_str().unwrap_or_default().to_owned())
}
