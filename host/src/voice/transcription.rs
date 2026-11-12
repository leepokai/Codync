//! Shared host transcription for calls and messaging attachments.

use super::keys::{self, Access};
use super::{Provider, STANDARD, TRANSCRIBE_HINT, checked, defaults, gemini_generate, names_hint, send, spoken_names};
use crate::store::Store;
use anyhow::{Context, Result, anyhow, ensure};
use base64::Engine as _;
use reqwest::Method;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::collections::VecDeque;
use std::sync::{Arc, LazyLock};

mod container;
use container::Container;

pub(crate) const MAX_NOTE_BYTES: usize = 20 * 1024 * 1024;

static NOTE_REQUESTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);

struct CachedNote {
    key: String,
    text: Arc<tokio::sync::OnceCell<String>>,
}

static NOTES: LazyLock<tokio::sync::Mutex<VecDeque<CachedNote>>> =
    LazyLock::new(|| tokio::sync::Mutex::new(VecDeque::new()));

async fn memo(key: String) -> Arc<tokio::sync::OnceCell<String>> {
    let mut cache = NOTES.lock().await;
    if let Some(index) = cache.iter().position(|entry| entry.key == key)
        && let Some(entry) = cache.remove(index)
    {
        let text = entry.text.clone();
        cache.push_back(entry);
        return text;
    }
    if cache.len() >= 64 {
        cache.pop_front();
    }
    let text = Arc::new(tokio::sync::OnceCell::new());
    cache.push_back(CachedNote { key, text: text.clone() });
    text
}

fn cache_key(context: &[&str], audio: &[u8]) -> String {
    let mut digest = Sha256::new();
    for value in context {
        digest.update(value.len().to_le_bytes());
        digest.update(value.as_bytes());
    }
    digest.update(audio);
    crate::chat::files::digest_hex(&digest.finalize())
}

struct SpeechRequest {
    /// The provider's server (this computer's compatible one, when set) and the key it takes.
    access: Access,
    names: Vec<String>,
}

impl SpeechRequest {
    fn new(store: &Store, provider: Provider) -> Result<Self> {
        Ok(Self { access: keys::access(store, provider)?, names: spoken_names(store) })
    }

    fn cache_key(&self, provider: Provider, model: &str, audio: &[u8]) -> String {
        cache_key(&[provider.id(), model, &self.access.identity(), &names_hint(&self.names)], audio)
    }
}

/// Messaging uses the configured voice vault and provider defaults, never a second credential
/// store. Call only after validating the platform sender and current bot binding.
pub(crate) async fn note(store: &Store, bytes: &[u8]) -> Result<String> {
    ensure!(!bytes.is_empty() && bytes.len() <= MAX_NOTE_BYTES, "Voice notes must contain 1 byte to 20 MiB");
    let container = Container::sniff(bytes).context("The attachment is not a recognized audio container")?;
    let provider = Provider::ALL
        .into_iter()
        .find(|p| keys::configured(store, *p).unwrap_or(false))
        .context("Configure a speech provider in Codync call settings to transcribe voice notes")?;
    let model = defaults(store, provider).transcribe;
    // Equal audio is reusable only within the same provider connection and spelling context.
    // Keep credentials and endpoint details out of the cache itself and out of logs.
    let request = SpeechRequest::new(store, provider)?;
    let cached = memo(request.cache_key(provider, &model, bytes)).await;
    let text = cached
        .get_or_try_init(|| async {
            let _permit = NOTE_REQUESTS.acquire().await.context("Voice transcription is unavailable")?;
            let response = transcribe_bytes(provider, &model, bytes, container, &request).await?;
            let text = response["text"].as_str().context("The speech provider returned no transcript")?.trim();
            ensure!(!text.is_empty(), "No speech was detected in the voice note");
            ensure!(text.len() <= 32 * 1024, "The voice transcript exceeds 32 KiB; use the original attachment");
            anyhow::Ok(text.to_owned())
        })
        .await?;
    Ok(text.clone())
}

/// Speech to text for one utterance (`audio`: base64 WAV, 16 kHz mono).
pub async fn transcribe(store: &Store, p: Provider, model: &str, audio: &str) -> Result<Value> {
    ensure!(audio.len() <= MAX_NOTE_BYTES.div_ceil(3) * 4, "Audio exceeds 20 MiB");
    let wav = STANDARD.decode(audio).map_err(|_| anyhow!("audio must be base64"))?;
    let request = SpeechRequest::new(store, p)?;
    transcribe_bytes(p, model, &wav, Container::Wav, &request).await
}

async fn transcribe_bytes(
    p: Provider,
    model: &str,
    wav: &[u8],
    container: Container,
    request: &SpeechRequest,
) -> Result<Value> {
    ensure!(!wav.is_empty() && wav.len() <= MAX_NOTE_BYTES, "Audio must contain 1 byte to 20 MiB");
    let model = checked(model)?;
    let extension = container.extension();
    let mime = container.mime();
    let names = &request.names;
    let text = match p {
        Provider::OpenAi => {
            let boundary = format!("codync-{}", uuid::Uuid::new_v4().simple());
            let mut body = Vec::with_capacity(wav.len() + 512);
            body.extend_from_slice(
                format!("--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n{model}\r\n").as_bytes(),
            );
            // Keeps Traditional Chinese from coming back Simplified, and code terms intact.
            let mut field = |name: &str, value: &str| {
                body.extend_from_slice(
                    format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
                        .as_bytes(),
                );
            };
            field("prompt", &format!("{TRANSCRIBE_HINT} {}", names_hint(names)));
            // Only gpt-transcribe takes keywords; the older models reject the field.
            if model.starts_with("gpt-transcribe") {
                for name in names {
                    field("keywords[]", name);
                }
            }
            body.extend_from_slice(
                format!(
                    "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"speech.{extension}\"\r\nContent-Type: {mime}\r\n\r\n"
                )
                .as_bytes(),
            );
            body.extend_from_slice(wav);
            body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
            let req = request
                .access
                .request(p, Method::POST, "/v1/audio/transcriptions")
                .header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
                .body(body);
            send(p, req).await?["text"].as_str().unwrap_or_default().to_owned()
        }
        Provider::Gemini => {
            let audio = STANDARD.encode(wav);
            let body = json!({"contents": [{"parts": [
                {"text": format!("Transcribe this audio exactly as spoken. {TRANSCRIBE_HINT} {} Reply with the transcript only; reply with nothing if there is no speech.", names_hint(names))},
                {"inlineData": {"mimeType": mime, "data": audio}},
            ]}]});
            let v = send(p, gemini_generate(&request.access, model).json(&body)).await?;
            v["candidates"][0]["content"]["parts"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|part| part["text"].as_str())
                .collect::<String>()
        }
        Provider::ElevenLabs => {
            super::elevenlabs::transcribe(request.access.secret()?, model, wav.to_vec(), extension, mime).await?
        }
    };
    Ok(json!({"text": text.trim()}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn repeated_audio_coalesces_only_within_the_same_speech_connection_and_context() {
        let first = cache_key(&["openai", "model", "credential-a", "endpoint-a", "bot-a"], b"audio");
        let same = cache_key(&["openai", "model", "credential-a", "endpoint-a", "bot-a"], b"audio");
        assert!(Arc::ptr_eq(&memo(first.clone()).await, &memo(same).await));
        for context in [
            ["openai", "model", "credential-b", "endpoint-a", "bot-a"],
            ["openai", "model", "credential-a", "endpoint-b", "bot-a"],
            ["openai", "model", "credential-a", "endpoint-a", "bot-b"],
            ["openai", "other-model", "credential-a", "endpoint-a", "bot-a"],
        ] {
            let other = cache_key(&context, b"audio");
            assert_ne!(first, other);
            assert!(!Arc::ptr_eq(&memo(first.clone()).await, &memo(other).await));
        }
        assert!(!first.contains("credential-a"));
    }

    #[tokio::test]
    async fn messaging_audio_uses_the_captured_endpoint_container_and_spelling_context() {
        use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = reqwest::Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut stream = BufReader::new(stream);
            let mut size = None;
            let mut total = 0;
            loop {
                let mut line = String::new();
                let count = stream.read_line(&mut line).await.unwrap();
                assert!(count > 0, "request ended before its headers");
                total += count;
                assert!(total < 16 * 1024, "unexpectedly large headers");
                if line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    size = Some(value.trim().parse::<usize>().unwrap());
                }
            }
            let size = size.expect("bounded multipart body must advertise its length");
            assert!(size < 64 * 1024);
            let mut body = vec![0; size];
            stream.read_exact(&mut body).await.unwrap();
            let body = String::from_utf8_lossy(&body);
            assert!(body.contains("filename=\"speech.ogg\""));
            assert!(body.contains("Content-Type: audio/ogg"));
            assert!(body.contains("Captured Bot"));
            assert!(body.contains("OggSfixture"));
            let answer = r#"{"text":" hello world "}"#;
            stream.get_mut().write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}", answer.len()).as_bytes()).await.unwrap();
        });
        let request = SpeechRequest {
            access: Access::direct_at(endpoint.as_str().trim_end_matches('/'), "fixture-only-key"),
            names: vec!["Captured Bot".into()],
        };
        let operation = async {
            let result = transcribe_bytes(Provider::OpenAi, "fixture-model", b"OggSfixture", Container::Ogg, &request)
                .await
                .unwrap();
            assert_eq!(result["text"], "hello world");
            server.await.unwrap();
        };
        tokio::time::timeout(std::time::Duration::from_secs(5), operation).await.unwrap();
    }
}
