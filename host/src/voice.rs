//! Voice on the user's own provider keys: `OpenAI` Realtime and Gemini Live for
//! a live call, and their speech models (plus `ElevenLabs`) for speech mode, where
//! each utterance is transcribed and each reply is read aloud. The keys stay in
//! this computer's vault; a client starting a live call gets a short-lived
//! credential minted here and then streams audio to the provider directly, so
//! audio never crosses the channel or the relay.

pub(crate) mod decisions;
mod elevenlabs;
mod endpoint;
pub(crate) mod neutts;
pub(crate) mod receipts;
mod streaming;
pub(crate) mod synthesis;
mod transcript;
pub(crate) mod transcription;
pub use transcription::transcribe;
mod keys;

use crate::store::Store;
use anyhow::{Result, anyhow, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use endpoint::{openai_base, realtime_url};
use keys::Access;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(15);
/// Speech to text: keep the speaker's language and script, and coding terms as written.
const TRANSCRIBE_HINT: &str = "Keep the speaker's language and script. Keep code, file and product names as written.";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provider {
    #[serde(rename = "openai")]
    OpenAi,
    #[serde(rename = "gemini")]
    Gemini,
    /// Speech only: `ElevenLabs` holds no live conversation here.
    #[serde(rename = "elevenlabs")]
    ElevenLabs,
}

impl Provider {
    const ALL: [Self; 3] = [Self::OpenAi, Self::Gemini, Self::ElevenLabs];

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "openai" => Ok(Self::OpenAi),
            "gemini" => Ok(Self::Gemini),
            "elevenlabs" => Ok(Self::ElevenLabs),
            _ => bail!("unknown voice provider `{s}`"),
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Gemini => "gemini",
            Self::ElevenLabs => "elevenlabs",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::OpenAi => "OpenAI",
            Self::Gemini => "Gemini",
            Self::ElevenLabs => "ElevenLabs",
        }
    }

    /// The defaults until this computer has seen the provider's own list (`refresh_loop`).
    fn fallback(self) -> Defaults {
        let (realtime, transcribe, speech) = match self {
            Self::OpenAi => ("gpt-realtime-2.1", "gpt-transcribe", "gpt-4o-mini-tts"),
            Self::Gemini => ("gemini-3.8-live", "gemini-3.8-flash", "gemini-3.8-flash-tts"),
            Self::ElevenLabs => ("", "scribe_v1", "eleven_flash_v2_5"),
        };
        Defaults { realtime: realtime.into(), transcribe: transcribe.into(), speech: speech.into() }
    }

    /// `ElevenLabs` reads replies and hears speech, but holds no live call.
    fn live(self) -> bool {
        self != Self::ElevenLabs
    }
}

/// The model each voice mode uses until the user picks one: the provider's newest, worked out
/// from its model list once a day, so a new release becomes the default without an app update.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Defaults {
    realtime: String,
    transcribe: String,
    speech: String,
}

const REFRESH_EVERY: Duration = Duration::from_hours(24);

fn defaults_key(p: Provider) -> String {
    format!("voice-defaults.{}", p.id())
}

fn defaults(store: &Store, p: Provider) -> Defaults {
    store.kv_get(&defaults_key(p)).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| p.fallback())
}

/// Looks at each keyed provider's model list once a day and keeps its newest models as the defaults.
pub async fn refresh_loop(hub: std::sync::Arc<crate::hub::Hub>) {
    loop {
        for p in Provider::ALL {
            if keys::configured(&hub.store, p).unwrap_or(false)
                && let Err(e) = models(&hub.store, p).await
            {
                tracing::info!(
                    provider = p.id(),
                    error = format!("{e:#}"),
                    "voice model refresh failed; keeping the last defaults"
                );
            }
        }
        tokio::time::sleep(REFRESH_EVERY).await;
    }
}

/// Model ids and voices go into provider requests: letters, digits, `-`, `.`, `_`.
fn checked(s: &str) -> Result<&str> {
    let ok = !s.is_empty() && s.len() <= 100 && s.chars().all(|c| c.is_ascii_alphanumeric() || "-._".contains(c));
    if ok { Ok(s) } else { bail!("`{s}` isn't a model or voice name") }
}

/// The provider's own message from an error body ("Incorrect API key provided…"), whatever shape
/// it uses: `error.message` for `OpenAI` and Gemini, `detail` for `ElevenLabs`.
pub(super) fn provider_error(p: Provider, status: reqwest::StatusCode, v: &Value) -> anyhow::Error {
    let message = v["error"]["message"]
        .as_str()
        .or_else(|| v["detail"]["message"].as_str())
        .or_else(|| v["detail"].as_str())
        .map_or_else(|| format!("{} answered {status}.", p.name()), str::to_owned);
    anyhow!(message)
}

async fn send(p: Provider, req: reqwest::RequestBuilder) -> Result<Value> {
    let res = req.timeout(TIMEOUT).send().await.map_err(|_| anyhow!("Can't reach {}.", p.name()))?;
    let status = res.status();
    let v: Value = serde_json::from_str(&res.text().await.unwrap_or_default()).unwrap_or(Value::Null);
    if !status.is_success() {
        return Err(provider_error(p, status, &v));
    }
    Ok(v)
}

/// A credential for one call's audio session: an `OpenAI` client secret (10 minutes)
/// or a one-use Gemini ephemeral token.
async fn mint(p: Provider, access: &Access, model: &str, voice: &str) -> Result<String> {
    let v = match p {
        Provider::OpenAi => {
            let body = json!({
                "expires_after": {"anchor": "created_at", "seconds": 600},
                "session": {"type": "realtime", "model": model, "audio": {"output": {"voice": voice}}},
            });
            // A compatible server on this computer has no Realtime credential minting.
            realtime_url(openai_base()?.as_ref())?;
            send(p, access.request(p, Method::POST, "/v1/realtime/client_secrets").json(&body)).await?
        }
        Provider::Gemini => {
            let now = chrono::Utc::now();
            let body = json!({
                "uses": 1,
                "expireTime": (now + chrono::Duration::minutes(30)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                "newSessionExpireTime": (now + chrono::Duration::minutes(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            });
            send(p, access.request(p, Method::POST, "/v1alpha/auth_tokens").json(&body)).await?
        }
        Provider::ElevenLabs => bail!("{} doesn't hold live calls. Choose Speech mode.", p.name()),
    };
    let field = match p {
        Provider::OpenAi => "value",
        Provider::Gemini => "name",
        Provider::ElevenLabs => unreachable!(),
    };
    v[field]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("{} sent no session credential.", p.name()))
}

/// Which providers have a key here, with their default models.
pub fn status(store: &Store) -> Result<Value> {
    let providers = Provider::ALL
        .iter()
        .map(|&p| {
            let defaults = defaults(store, p);
            Ok(json!({"provider": p.id(), "configured": keys::configured(store, p)?, "defaults": defaults}))
        })
        .collect::<Result<Vec<Value>>>()?;
    Ok(json!({"providers": providers}))
}

/// The voices the key can read with, for the picker. Other providers have a fixed list.
pub async fn voices(store: &Store, p: Provider) -> Result<Value> {
    match p {
        Provider::ElevenLabs => {
            let voices: Vec<Value> = elevenlabs::voices(keys::access(store, p)?.secret()?)
                .await?
                .into_iter()
                .map(|(id, name)| json!({"id": id, "name": name}))
                .collect();
            Ok(json!({"voices": voices}))
        }
        _ => bail!("{} has a fixed voice list.", p.name()),
    }
}

/// Checks the key with the provider (a credential mint, or for `ElevenLabs` its voices and one
/// character read), then keeps it. An empty key removes it.
pub async fn set_key(store: &Store, p: Provider, key: &str) -> Result<Value> {
    let key = key.trim();
    if !key.is_empty() {
        if p == Provider::ElevenLabs {
            elevenlabs::check(key).await?;
        } else {
            let access = Access::direct(p, key)?;
            if p == Provider::OpenAi && openai_base()?.is_some() {
                // Compatible speech servers may not implement Realtime credential minting.
                send(p, access.request(p, Method::GET, "/v1/models")).await?;
            } else {
                let voice = match p {
                    Provider::OpenAi => "marin",
                    _ => "Kore",
                };
                mint(p, &access, &defaults(store, p).realtime, voice).await?;
            }
        }
    }
    keys::save(store, p, key)?;
    if !key.is_empty() {
        // A new key may see newer models; the daily refresh would only get there tomorrow.
        let _ = models(store, p).await;
    }
    status(store)
}

/// A credential for a call starting now.
pub async fn session(store: &Store, p: Provider, model: &str, voice: &str) -> Result<Value> {
    if !p.live() {
        bail!("{} doesn't hold live calls. Choose Speech mode.", p.name());
    }
    let access = keys::access(store, p)?;
    Ok(json!({"credential": mint(p, &access, checked(model)?, checked(voice)?).await?}))
}

/// Models the key can use, from the provider's own list: realtime (conversation),
/// transcribe (speech to text) and speech (text to speech).
pub async fn models(store: &Store, p: Provider) -> Result<Value> {
    let access = keys::access(store, p)?;
    let entries: Vec<Model> = match p {
        Provider::OpenAi => {
            let base = openai_base()?;
            let v = send(p, access.request(p, Method::GET, "/v1/models")).await?;
            v["data"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|m| {
                    let id = m["id"].as_str()?;
                    Some(Model {
                        id: id.to_owned(),
                        live: base.is_none() && id.contains("realtime"),
                        generate: true,
                        created: m["created"].as_i64(),
                    })
                })
                .collect()
        }
        Provider::Gemini => {
            let v = send(p, access.request(p, Method::GET, "/v1beta/models?pageSize=1000")).await?;
            v["models"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|m| {
                    let methods = m["supportedGenerationMethods"].as_array()?;
                    Some(Model {
                        id: m["name"].as_str()?.trim_start_matches("models/").to_owned(),
                        live: methods.iter().any(|x| x == "bidiGenerateContent"),
                        generate: methods.iter().any(|x| x == "generateContent"),
                        created: None,
                    })
                })
                .collect()
        }
        Provider::ElevenLabs => elevenlabs::models(access.secret()?).await?,
    };
    let (realtime, transcribe, speech) = classify(p, &entries);
    let fallback = p.fallback();
    let defaults = if p == Provider::ElevenLabs {
        // Flash 2.5 is the low-latency model a call needs; Scribe v1 is on every plan.
        Defaults {
            realtime: String::new(),
            transcribe: fallback.transcribe,
            speech: newest(p, &entries, &speech).unwrap_or(fallback.speech),
        }
    } else {
        Defaults {
            realtime: newest(p, &entries, &realtime).unwrap_or(fallback.realtime),
            transcribe: newest(p, &entries, &transcribe).unwrap_or(fallback.transcribe),
            speech: newest(p, &entries, &speech).unwrap_or(fallback.speech),
        }
    };
    store.kv_set(&defaults_key(p), &serde_json::to_string(&defaults)?)?;
    Ok(json!({"models": realtime, "transcribe": transcribe, "speech": speech, "defaults": defaults}))
}

/// The provider's current model among `ids`, skipping dated snapshots and previews.
/// `OpenAI` dates every model (`created`): its latest generation is whatever came out within a
/// few months of the newest release, and in it the full model beats `mini` (`gpt-realtime-2.1`
/// over `gpt-realtime-2.1-mini`, yet `gpt-4o-mini-tts` over the older `tts-1`). Gemini's names
/// carry the version (`gemini-3.8-flash`); within one version the full, plain name wins
/// (`-flash` over `-flash-lite`, `-live` over `-live-extended-thinking`).
fn newest(p: Provider, models: &[Model], ids: &[String]) -> Option<String> {
    const GENERATION: i64 = 120 * 24 * 60 * 60;
    let lesser = |id: &str| ["mini", "lite"].iter().any(|w| id.contains(w));
    let pinned = |id: &str| {
        ["preview", "exp", "latest"].iter().any(|w| id.contains(w))
            || id
                .split('-')
                .any(|part| part.len() == 4 && part.starts_with("20") && part.bytes().all(|c| c.is_ascii_digit()))
    };
    let version = |id: &str| -> Vec<u32> {
        id.split('-')
            .find(|part| {
                part.starts_with(|c: char| c.is_ascii_digit()) && part.split('.').all(|n| n.parse::<u32>().is_ok())
            })
            .map(|v| v.split('.').filter_map(|n| n.parse().ok()).collect())
            .unwrap_or_default()
    };
    let created = |id: &str| models.iter().find(|m| m.id == id).and_then(|m| m.created).unwrap_or(0);
    let stable: Vec<&String> = ids.iter().filter(|id| !pinned(id)).collect();
    let pool = if stable.is_empty() { ids.iter().collect() } else { stable };
    match p {
        Provider::OpenAi => {
            let latest = pool.iter().map(|id| created(id)).max()?;
            pool.into_iter()
                .filter(|id| created(id) >= latest - GENERATION)
                .max_by_key(|id| (!lesser(id), created(id)))
                .cloned()
        }
        Provider::Gemini => pool
            .into_iter()
            .max_by(|a, b| (version(a), !lesser(a)).cmp(&(version(b), !lesser(b))).then_with(|| b.len().cmp(&a.len())))
            .cloned(),
        Provider::ElevenLabs => ["eleven_flash_v2_5", "eleven_turbo_v2_5", "eleven_flash_v2", "eleven_turbo_v2"]
            .iter()
            .find(|want| pool.iter().any(|id| id == *want))
            .map(|want| (*want).to_owned())
            .or_else(|| pool.first().map(|id| (*id).clone())),
    }
}

struct Model {
    id: String,
    /// Holds a live conversation (`OpenAI` `realtime`, Gemini `bidiGenerateContent`).
    live: bool,
    generate: bool,
    /// When `OpenAI` released it (Unix seconds); Gemini's list has no date.
    created: Option<i64>,
}

/// Sorts a provider's model list into what each voice mode can use. Translation,
/// diarization and live-transcription models can't serve either mode.
fn classify(p: Provider, models: &[Model]) -> (Vec<String>, Vec<String>, Vec<String>) {
    let has = |id: &str, words: &[&str]| words.iter().any(|w| id.contains(w));
    let pick = |f: &dyn Fn(&Model) -> bool| {
        let mut ids: Vec<String> = models.iter().filter(|m| f(m)).map(|m| m.id.clone()).collect();
        ids.sort();
        ids.dedup();
        ids
    };
    let realtime = pick(&|m| m.live && !has(&m.id, &["translate", "whisper", "transcribe", "tts"]));
    let (transcribe, speech) = match p {
        Provider::OpenAi => (
            pick(&|m| {
                (m.id.contains("transcribe") || m.id == "whisper-1") && !has(&m.id, &["diarize", "live", "realtime"])
            }),
            pick(&|m| m.id.contains("tts")),
        ),
        Provider::Gemini => (
            pick(&|m| {
                m.generate
                    && m.id.starts_with("gemini")
                    && m.id.contains("flash")
                    && !has(&m.id, &["tts", "live", "image", "embedding", "native-audio", "translate"])
            }),
            pick(&|m| m.generate && m.id.contains("tts")),
        ),
        Provider::ElevenLabs => (
            // Batch transcription is Scribe; the model list only covers text to speech.
            vec!["scribe_v2".to_owned(), "scribe_v1".to_owned()],
            pick(&|m| m.generate && !has(&m.id, &["music"])),
        ),
    };
    (realtime, transcribe, speech)
}

/// Names a speaker is likely to say that speech-to-text would otherwise misspell: the app and
/// the bots on this computer.
fn spoken_names(store: &Store) -> Vec<String> {
    let mut names = vec!["Codync".to_owned()];
    for bot in store.bots().unwrap_or_default() {
        if !bot.deleted && !names.contains(&bot.config.name) {
            names.push(bot.config.name);
        }
    }
    names.truncate(50);
    names
}

fn names_hint(names: &[String]) -> String {
    format!("Names that may come up: {}.", names.join(", "))
}

/// Text to speech for one stretch of a reply: `{audio: base64, format: "aac" | "wav" | "mp3"}`.
pub async fn speak(store: &Store, p: Provider, model: &str, voice: &str, text: &str) -> Result<Value> {
    let access = keys::access(store, p)?;
    let (model, voice) = (checked(model)?, checked(voice)?);
    if text.trim().is_empty() || text.len() > 4000 {
        bail!("text must be 1–4000 bytes");
    }
    match p {
        Provider::OpenAi => {
            // A local server answers WAV (no AAC encoder needed); the phone plays both.
            let format = if openai_base()?.is_some() { "wav" } else { "aac" };
            let body = json!({"model": model, "voice": voice, "input": text, "response_format": format});
            let res = access
                .request(p, Method::POST, "/v1/audio/speech")
                .json(&body)
                .timeout(TIMEOUT * 4)
                .send()
                .await
                .map_err(|_| anyhow!("Can't reach {}.", p.name()))?;
            let status = res.status();
            let bytes = res.bytes().await.map_err(|_| anyhow!("{} sent no audio.", p.name()))?;
            if !status.is_success() {
                let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
                return Err(provider_error(p, status, &v));
            }
            Ok(json!({"audio": STANDARD.encode(&bytes), "format": format}))
        }
        Provider::Gemini => {
            let body = json!({
                "contents": [{"parts": [{"text": text}]}],
                "generationConfig": {
                    "responseModalities": ["AUDIO"],
                    "speechConfig": {"voiceConfig": {"prebuiltVoiceConfig": {"voiceName": voice}}},
                },
            });
            let v = send(p, gemini_generate(&access, model).json(&body)).await?;
            let pcm = v["candidates"][0]["content"]["parts"]
                .as_array()
                .into_iter()
                .flatten()
                .find_map(|part| part["inlineData"]["data"].as_str())
                .ok_or_else(|| anyhow!("{} sent no audio.", p.name()))?;
            let pcm = STANDARD.decode(pcm).map_err(|_| anyhow!("{} sent unreadable audio.", p.name()))?;
            Ok(json!({"audio": STANDARD.encode(wav(&pcm, 24_000)), "format": "wav"}))
        }
        Provider::ElevenLabs => {
            let bytes = elevenlabs::speech(access.secret()?, model, voice, text).await?;
            Ok(json!({"audio": STANDARD.encode(bytes), "format": "mp3"}))
        }
    }
}

fn gemini_generate(access: &Access, model: &str) -> reqwest::RequestBuilder {
    access
        .request(Provider::Gemini, Method::POST, &format!("/v1beta/models/{model}:generateContent"))
        .timeout(TIMEOUT * 4)
}

/// 16-bit mono PCM wrapped in a WAV header (Gemini speech is raw PCM).
fn wav(pcm: &[u8], rate: u32) -> Vec<u8> {
    let len = u32::try_from(pcm.len()).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(pcm.len() + 44);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(pcm);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(p: Provider, ids: &[&str]) -> (Vec<String>, Vec<String>, Vec<String>) {
        let models: Vec<Model> = ids
            .iter()
            .map(|id| Model { id: (*id).to_owned(), live: id.contains("realtime"), generate: true, created: None })
            .collect();
        classify(p, &models)
    }

    #[test]
    fn openai_models_sort_by_mode() {
        let (realtime, transcribe, speech) = ids(
            Provider::OpenAi,
            &[
                "gpt-realtime-2.1",
                "gpt-realtime-translate",
                "gpt-realtime-whisper",
                "gpt-realtime",
                "gpt-4o-transcribe",
                "gpt-4o-transcribe-diarize",
                "gpt-live-transcribe",
                "whisper-1",
                "gpt-4o-mini-tts",
                "gpt-5",
            ],
        );
        assert_eq!(realtime, ["gpt-realtime", "gpt-realtime-2.1"]);
        assert_eq!(transcribe, ["gpt-4o-transcribe", "whisper-1"]);
        assert_eq!(speech, ["gpt-4o-mini-tts"]);
    }

    fn dated(id: &str, created: i64) -> Model {
        Model { id: id.to_owned(), live: id.contains("realtime"), generate: true, created: Some(created) }
    }

    #[test]
    fn newest_openai_models_are_the_defaults() {
        let day = 24 * 60 * 60;
        let models = [
            dated("gpt-realtime", 1_756_000_000),
            dated("gpt-realtime-2", 1_778_000_000),
            dated("gpt-realtime-2.1", 1_786_000_000),
            dated("gpt-realtime-2.1-mini", 1_786_000_000 + 5 * day),
            dated("gpt-realtime-mini-2025-12-15", 1_765_000_000),
            dated("gpt-4o-transcribe", 1_742_000_000),
            dated("gpt-4o-mini-transcribe", 1_742_000_000),
            dated("gpt-transcribe", 1_785_000_000),
            dated("whisper-1", 1_677_000_000),
            dated("tts-1-hd", 1_699_000_000),
            dated("gpt-4o-mini-tts", 1_742_000_000),
            dated("gpt-4o-mini-tts-2025-12-15", 1_765_000_000),
        ];
        let (realtime, transcribe, speech) = classify(Provider::OpenAi, &models);
        let pick = |ids: &[String]| newest(Provider::OpenAi, &models, ids);
        assert_eq!(pick(&realtime).as_deref(), Some("gpt-realtime-2.1"));
        assert_eq!(pick(&transcribe).as_deref(), Some("gpt-transcribe"));
        assert_eq!(pick(&speech).as_deref(), Some("gpt-4o-mini-tts"));
    }

    #[test]
    fn newest_gemini_models_are_the_defaults() {
        let pick = |ids: &[&str]| {
            let ids: Vec<String> = ids.iter().map(|s| (*s).to_owned()).collect();
            newest(Provider::Gemini, &[], &ids)
        };
        assert_eq!(
            pick(&[
                "gemini-3.1-flash-live-preview",
                "gemini-3.8-live",
                "gemini-3.8-live-extended-thinking",
                "gemini-2.5-flash-live"
            ])
            .as_deref(),
            Some("gemini-3.8-live")
        );
        assert_eq!(
            pick(&["gemini-3.5-flash-lite", "gemini-3.8-flash", "gemini-3.9-flash-preview", "gemini-flash-latest"])
                .as_deref(),
            Some("gemini-3.8-flash")
        );
        assert_eq!(
            pick(&["gemini-3.8-flash-lite-tts", "gemini-3.8-flash-tts", "gemini-3.1-flash-tts-preview"]).as_deref(),
            Some("gemini-3.8-flash-tts")
        );
    }

    #[test]
    fn elevenlabs_models_and_defaults() {
        let models = [
            Model { id: "eleven_flash_v2_5".into(), live: false, generate: true, created: None },
            Model { id: "eleven_multilingual_v2".into(), live: false, generate: true, created: None },
            Model { id: "eleven_v3".into(), live: false, generate: true, created: None },
            Model { id: "eleven_music".into(), live: false, generate: true, created: None },
            Model { id: "not_a_voice_model".into(), live: false, generate: false, created: None },
        ];
        let (realtime, transcribe, speech) = classify(Provider::ElevenLabs, &models);
        assert_eq!(realtime, Vec::<String>::new());
        assert_eq!(transcribe, ["scribe_v2", "scribe_v1"]);
        assert_eq!(speech, ["eleven_flash_v2_5", "eleven_multilingual_v2", "eleven_v3"]);
        assert_eq!(newest(Provider::ElevenLabs, &models, &speech).as_deref(), Some("eleven_flash_v2_5"));
        assert!(!Provider::ElevenLabs.live());
        assert!(Provider::OpenAi.live());
    }

    #[test]
    fn wav_header_matches_pcm() {
        let out = wav(&[0, 0, 1, 0], 24_000);
        assert_eq!(out.len(), 48);
        assert_eq!(&out[..4], b"RIFF");
        assert_eq!(u32::from_le_bytes([out[40], out[41], out[42], out[43]]), 4);
    }

    #[test]
    fn names_are_checked() {
        assert!(checked("gpt-realtime-2.1").is_ok());
        assert!(checked("Kore").is_ok());
        assert!(checked("x\"}").is_err());
        assert!(checked("").is_err());
        assert!(Provider::parse("openai").is_ok());
        assert!(Provider::parse("elevenlabs").is_ok());
        assert!(Provider::parse("anthropic").is_err());
    }
}
