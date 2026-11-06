//! Realtime voice for calls on the user's own provider keys (`OpenAI` Realtime,
//! Gemini Live). The keys stay in this computer's vault; a client starting a
//! call gets a short-lived credential minted here and then streams audio to the
//! provider directly, so audio never crosses the channel or the relay.

use crate::market::vault;
use crate::store::Store;
use anyhow::{Result, anyhow, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

const SLOT: &str = "voice";
const TIMEOUT: Duration = Duration::from_secs(15);
/// Speech to text: keep the speaker's language and script, and coding terms as written.
const TRANSCRIBE_HINT: &str = "Keep the speaker's language and script. Keep code, file and product names as written.";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provider {
    #[serde(rename = "openai")]
    OpenAi,
    #[serde(rename = "gemini")]
    Gemini,
}

impl Provider {
    const ALL: [Self; 2] = [Self::OpenAi, Self::Gemini];

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "openai" => Ok(Self::OpenAi),
            "gemini" => Ok(Self::Gemini),
            _ => bail!("unknown voice provider `{s}`"),
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Gemini => "gemini",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::OpenAi => "OpenAI",
            Self::Gemini => "Gemini",
        }
    }

    /// The defaults until this computer has seen the provider's own list (`refresh_loop`).
    fn fallback(self) -> Defaults {
        let (realtime, transcribe, speech) = match self {
            Self::OpenAi => ("gpt-realtime-2.1", "gpt-transcribe", "gpt-4o-mini-tts"),
            Self::Gemini => ("gemini-3.8-live", "gemini-3.8-flash", "gemini-3.8-flash-tts"),
        };
        Defaults { realtime: realtime.into(), transcribe: transcribe.into(), speech: speech.into() }
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
            if load(&hub.store).is_ok_and(|keys| !keys.get(p).is_empty())
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

#[derive(Default, Serialize, Deserialize)]
struct Keys {
    #[serde(default)]
    openai: String,
    #[serde(default)]
    gemini: String,
}

impl Keys {
    fn get(&self, p: Provider) -> &str {
        match p {
            Provider::OpenAi => &self.openai,
            Provider::Gemini => &self.gemini,
        }
    }

    fn set(&mut self, p: Provider, key: String) {
        match p {
            Provider::OpenAi => self.openai = key,
            Provider::Gemini => self.gemini = key,
        }
    }
}

fn load(store: &Store) -> Result<Keys> {
    Ok(vault::read(store, SLOT)?
        .filter(|s| !s.is_empty())
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or_default())
}

fn key(store: &Store, p: Provider) -> Result<String> {
    let keys = load(store)?;
    let key = keys.get(p);
    if key.is_empty() {
        bail!("No {} key on this computer. Add one in the call settings.", p.name());
    }
    Ok(key.to_owned())
}

/// Model ids and voices go into provider requests: letters, digits, `-`, `.`, `_`.
fn checked(s: &str) -> Result<&str> {
    let ok = !s.is_empty() && s.len() <= 100 && s.chars().all(|c| c.is_ascii_alphanumeric() || "-._".contains(c));
    if ok { Ok(s) } else { bail!("`{s}` isn't a model or voice name") }
}

async fn send(p: Provider, req: reqwest::RequestBuilder) -> Result<Value> {
    let res = req.timeout(TIMEOUT).send().await.map_err(|_| anyhow!("Can't reach {}.", p.name()))?;
    let status = res.status();
    let v: Value = serde_json::from_str(&res.text().await.unwrap_or_default()).unwrap_or(Value::Null);
    if !status.is_success() {
        // The provider's own message ("Incorrect API key provided…") says what to fix.
        let message =
            v["error"]["message"].as_str().map_or_else(|| format!("{} answered {status}.", p.name()), str::to_owned);
        bail!(message);
    }
    Ok(v)
}

/// A credential for one call's audio session: an `OpenAI` client secret (10 minutes)
/// or a one-use Gemini ephemeral token.
async fn mint(p: Provider, key: &str, model: &str, voice: &str) -> Result<String> {
    let v = match p {
        Provider::OpenAi => {
            let body = json!({
                "expires_after": {"anchor": "created_at", "seconds": 600},
                "session": {"type": "realtime", "model": model, "audio": {"output": {"voice": voice}}},
            });
            send(
                p,
                crate::http().post("https://api.openai.com/v1/realtime/client_secrets").bearer_auth(key).json(&body),
            )
            .await?
        }
        Provider::Gemini => {
            let now = chrono::Utc::now();
            let body = json!({
                "uses": 1,
                "expireTime": (now + chrono::Duration::minutes(30)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                "newSessionExpireTime": (now + chrono::Duration::minutes(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            });
            send(
                p,
                crate::http()
                    .post("https://generativelanguage.googleapis.com/v1alpha/auth_tokens")
                    .header("x-goog-api-key", key)
                    .json(&body),
            )
            .await?
        }
    };
    let field = match p {
        Provider::OpenAi => "value",
        Provider::Gemini => "name",
    };
    v[field]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("{} sent no session credential.", p.name()))
}

/// Which providers have a key here, with their default models.
pub fn status(store: &Store) -> Result<Value> {
    let keys = load(store)?;
    let providers: Vec<Value> = Provider::ALL
        .iter()
        .map(|&p| {
            let defaults = defaults(store, p);
            json!({"provider": p.id(), "configured": !keys.get(p).is_empty(), "defaults": defaults})
        })
        .collect();
    Ok(json!({"providers": providers}))
}

/// Checks the key with the provider (one credential mint), then keeps it. An empty key removes it.
pub async fn set_key(store: &Store, p: Provider, key: &str) -> Result<Value> {
    let key = key.trim();
    if !key.is_empty() {
        let voice = match p {
            Provider::OpenAi => "marin",
            Provider::Gemini => "Kore",
        };
        mint(p, key, &defaults(store, p).realtime, voice).await?;
    }
    {
        let _guard = store.connector_lock.lock().map_err(|_| anyhow!("credential storage is busy"))?;
        let mut keys = load(store)?;
        keys.set(p, key.to_owned());
        vault::write(store, SLOT, &serde_json::to_string(&keys)?)?;
    }
    if !key.is_empty() {
        // A new key may see newer models; the daily refresh would only get there tomorrow.
        let _ = models(store, p).await;
    }
    status(store)
}

/// A credential for a call starting now.
pub async fn session(store: &Store, p: Provider, model: &str, voice: &str) -> Result<Value> {
    let key = key(store, p)?;
    Ok(json!({"credential": mint(p, &key, checked(model)?, checked(voice)?).await?}))
}

/// Models the key can use, from the provider's own list: realtime (conversation),
/// transcribe (speech to text) and speech (text to speech).
pub async fn models(store: &Store, p: Provider) -> Result<Value> {
    let key = key(store, p)?;
    let entries: Vec<Model> = match p {
        Provider::OpenAi => {
            let v = send(p, crate::http().get("https://api.openai.com/v1/models").bearer_auth(&key)).await?;
            v["data"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|m| {
                    let id = m["id"].as_str()?;
                    Some(Model {
                        id: id.to_owned(),
                        live: id.contains("realtime"),
                        generate: true,
                        created: m["created"].as_i64(),
                    })
                })
                .collect()
        }
        Provider::Gemini => {
            let req = crate::http()
                .get("https://generativelanguage.googleapis.com/v1beta/models?pageSize=1000")
                .header("x-goog-api-key", &key);
            let v = send(p, req).await?;
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
    };
    let (realtime, transcribe, speech) = classify(p, &entries);
    let fallback = p.fallback();
    let defaults = Defaults {
        realtime: newest(p, &entries, &realtime).unwrap_or(fallback.realtime),
        transcribe: newest(p, &entries, &transcribe).unwrap_or(fallback.transcribe),
        speech: newest(p, &entries, &speech).unwrap_or(fallback.speech),
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

/// Speech to text for one utterance (`audio`: base64 WAV, 16 kHz mono).
pub async fn transcribe(store: &Store, p: Provider, model: &str, audio: &str) -> Result<Value> {
    let key = key(store, p)?;
    let model = checked(model)?;
    let wav = STANDARD.decode(audio).map_err(|_| anyhow!("audio must be base64"))?;
    let names = spoken_names(store);
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
            field("prompt", &format!("{TRANSCRIBE_HINT} {}", names_hint(&names)));
            // Only gpt-transcribe takes keywords; the older models reject the field.
            if model.starts_with("gpt-transcribe") {
                for name in &names {
                    field("keywords[]", name);
                }
            }
            body.extend_from_slice(
                format!(
                    "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"speech.wav\"\r\nContent-Type: audio/wav\r\n\r\n"
                )
                .as_bytes(),
            );
            body.extend_from_slice(&wav);
            body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
            let req = crate::http()
                .post("https://api.openai.com/v1/audio/transcriptions")
                .bearer_auth(&key)
                .header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
                .body(body);
            send(p, req).await?["text"].as_str().unwrap_or_default().to_owned()
        }
        Provider::Gemini => {
            let body = json!({"contents": [{"parts": [
                {"text": format!("Transcribe this audio exactly as spoken. {TRANSCRIBE_HINT} {} Reply with the transcript only; reply with nothing if there is no speech.", names_hint(&names))},
                {"inlineData": {"mimeType": "audio/wav", "data": audio}},
            ]}]});
            let v = send(p, gemini_generate(&key, model).json(&body)).await?;
            v["candidates"][0]["content"]["parts"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|part| part["text"].as_str())
                .collect::<String>()
        }
    };
    Ok(json!({"text": text.trim()}))
}

/// Text to speech for one stretch of a reply: `{audio: base64, format: "aac" | "wav"}`.
pub async fn speak(store: &Store, p: Provider, model: &str, voice: &str, text: &str) -> Result<Value> {
    let key = key(store, p)?;
    let (model, voice) = (checked(model)?, checked(voice)?);
    if text.trim().is_empty() || text.len() > 4000 {
        bail!("text must be 1–4000 bytes");
    }
    match p {
        Provider::OpenAi => {
            let body = json!({"model": model, "voice": voice, "input": text, "response_format": "aac"});
            let res = crate::http()
                .post("https://api.openai.com/v1/audio/speech")
                .bearer_auth(&key)
                .json(&body)
                .timeout(TIMEOUT * 4)
                .send()
                .await
                .map_err(|_| anyhow!("Can't reach {}.", p.name()))?;
            let status = res.status();
            let bytes = res.bytes().await.map_err(|_| anyhow!("{} sent no audio.", p.name()))?;
            if !status.is_success() {
                let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
                bail!(
                    v["error"]["message"]
                        .as_str()
                        .map_or_else(|| format!("{} answered {status}.", p.name()), str::to_owned)
                );
            }
            Ok(json!({"audio": STANDARD.encode(&bytes), "format": "aac"}))
        }
        Provider::Gemini => {
            let body = json!({
                "contents": [{"parts": [{"text": text}]}],
                "generationConfig": {
                    "responseModalities": ["AUDIO"],
                    "speechConfig": {"voiceConfig": {"prebuiltVoiceConfig": {"voiceName": voice}}},
                },
            });
            let v = send(p, gemini_generate(&key, model).json(&body)).await?;
            let pcm = v["candidates"][0]["content"]["parts"]
                .as_array()
                .into_iter()
                .flatten()
                .find_map(|part| part["inlineData"]["data"].as_str())
                .ok_or_else(|| anyhow!("{} sent no audio.", p.name()))?;
            let pcm = STANDARD.decode(pcm).map_err(|_| anyhow!("{} sent unreadable audio.", p.name()))?;
            Ok(json!({"audio": STANDARD.encode(wav(&pcm, 24_000)), "format": "wav"}))
        }
    }
}

fn gemini_generate(key: &str, model: &str) -> reqwest::RequestBuilder {
    crate::http()
        .post(format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"))
        .header("x-goog-api-key", key)
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
        assert!(Provider::parse("anthropic").is_err());
    }
}
