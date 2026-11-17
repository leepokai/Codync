//! Realtime voice for calls on the user's own provider keys (`OpenAI` Realtime,
//! Gemini Live). The keys stay in this computer's vault (`keys`); a client starting a
//! call gets a short-lived credential minted here and then streams audio to the
//! provider directly, so audio never crosses the channel or the relay.

mod keys;
mod server;

pub use server::Refused as ServerRefused;

use crate::remote::managed::is_managed;
use crate::store::Store;
use anyhow::{Result, anyhow, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use keys::{Access, Upstream};
use reqwest::{Method, StatusCode};
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

/// A request nobody answered, apart from one answered with an error (`refused`).
#[derive(Debug)]
struct Unreachable(&'static str);

impl std::fmt::Display for Unreachable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Can't reach {}.", self.0)
    }
}

impl std::error::Error for Unreachable {}

/// An answer that isn't a success, as an error: the provider's own message ("Incorrect API key provided…")
/// says what to fix.
fn refused(who: &str, status: StatusCode, body: &[u8]) -> anyhow::Error {
    let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    anyhow!(v["error"]["message"].as_str().map_or_else(|| format!("{who} answered {status}."), str::to_owned))
}

/// `who` answers: the provider, or the user's own server (`Access::name`).
async fn send(who: &'static str, req: reqwest::RequestBuilder) -> Result<Value> {
    let res = req.timeout(TIMEOUT).send().await.map_err(|_| Unreachable(who))?;
    let status = res.status();
    let body = res.bytes().await.unwrap_or_default();
    if !status.is_success() {
        return Err(refused(who, status, &body));
    }
    Ok(serde_json::from_slice(&body).unwrap_or(Value::Null))
}

/// A credential for one call's audio session: an `OpenAI` client secret (10 minutes)
/// or a one-use Gemini ephemeral token.
async fn mint(p: Provider, access: &Access, model: &str, voice: &str) -> Result<String> {
    let v = match p {
        Provider::OpenAi if access.upstream == Upstream::Compatible => {
            bail!("Realtime talks to OpenAI itself. With your own server, use Speech mode.")
        }
        Provider::OpenAi => {
            let body = json!({
                "expires_after": {"anchor": "created_at", "seconds": 600},
                "session": {"type": "realtime", "model": model, "audio": {"output": {"voice": voice}}},
            });
            send(access.name(p), access.request(p, Method::POST, "/v1/realtime/client_secrets").json(&body)).await?
        }
        Provider::Gemini => {
            let now = chrono::Utc::now();
            let body = json!({
                "uses": 1,
                "expireTime": (now + chrono::Duration::minutes(30)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                "newSessionExpireTime": (now + chrono::Duration::minutes(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            });
            send(access.name(p), access.request(p, Method::POST, "/v1alpha/auth_tokens").json(&body)).await?
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

/// Which providers have a key here, with their default models, and that Gemini realtime calls can use Google Search.
/// `OpenAI` on a computer of the user's own also says its `server`: the saved compatible one, or `""` for `OpenAI`
/// itself. A cloud computer's keeper sends its key to `OpenAI` only, so it has no `server` to choose.
pub fn status(store: &Store) -> Result<Value> {
    let providers = Provider::ALL
        .iter()
        .map(|&p| {
            let defaults = defaults(store, p);
            let mut entry =
                json!({"provider": p.id(), "configured": keys::configured(store, p)?, "defaults": defaults});
            if p == Provider::OpenAi && !is_managed(store) {
                entry["server"] = json!(server::saved(store).unwrap_or_default());
            }
            Ok(entry)
        })
        .collect::<Result<Vec<Value>>>()?;
    Ok(json!({"providers": providers, "googleSearch": true}))
}

/// Checks `key` where it is about to be used: one credential mint at the provider, or the compatible server's
/// model list (it mints no Realtime credentials).
async fn check(store: &Store, p: Provider, access: &Access) -> Result<()> {
    if access.upstream == Upstream::Compatible {
        send(access.name(p), access.request(p, Method::GET, "/v1/models")).await?;
        return Ok(());
    }
    let voice = match p {
        Provider::OpenAi => "marin",
        Provider::Gemini => "Kore",
    };
    mint(p, access, &defaults(store, p).realtime, voice).await.map(drop)
}

/// Checks the key where it will be used (`check`), then keeps it. An empty key removes it.
pub async fn set_key(store: &Store, p: Provider, key: &str) -> Result<Value> {
    let key = key.trim();
    if !key.is_empty() {
        let server = server::saved(store).filter(|_| !is_managed(store));
        check(store, p, &Access::direct(p, key, server)).await?;
    }
    keys::save(store, p, key)?;
    if !key.is_empty() {
        // A new key may see newer models; the daily refresh would only get there tomorrow.
        let _ = models(store, p).await;
    }
    status(store)
}

/// Points the `OpenAI` engine's Speech mode at an OpenAI-compatible server (`url`), or back at `OpenAI` (blank).
/// A saved key is checked with the new server first, so a wrong address fails here and not in a call. Going back
/// to `OpenAI` checks nothing and deletes the key saved with the server: it may be one only the server took, so it
/// must not hold the address and never goes to `OpenAI` (the user adds an `OpenAI` key again).
pub async fn set_server(store: &Store, p: Provider, url: &str) -> Result<Value> {
    if p != Provider::OpenAi {
        return Err(ServerRefused::error("unsupported", "Only the OpenAI engine can use another server."));
    }
    if is_managed(store) {
        return Err(ServerRefused::error("unsupported", "A cloud computer's OpenAI key works with OpenAI only."));
    }
    let url = server::parse(url)?;
    if url.is_some()
        && let Some(key) = keys::own(store, p)?
    {
        check(store, p, &Access::direct(p, &key, url.clone())).await.map_err(|e| {
            if e.is::<Unreachable>() {
                return ServerRefused::error("unreachable", e.to_string());
            }
            let why = e.to_string();
            ServerRefused::error(
                "refused",
                format!(
                    "The saved key doesn't work with this server ({}). Remove the key, save the server, then add the server's key.",
                    why.trim_end_matches('.')
                ),
            )
        })?;
    }
    if url.is_none() && server::saved(store).is_some() {
        // Only a key that is there: deleting none would open the vault for nothing.
        if keys::own(store, p)?.is_some() {
            keys::save(store, p, "")?;
        }
        store.kv_delete(&defaults_key(p))?;
    }
    server::save(store, url.as_deref())?;
    if url.is_some() {
        // The other server has its own models; the defaults follow it now instead of tomorrow.
        let _ = models(store, p).await;
    }
    status(store)
}

/// A credential for a call starting now.
pub async fn session(store: &Store, p: Provider, model: &str, voice: &str) -> Result<Value> {
    let access = keys::access(store, p)?;
    Ok(json!({"credential": mint(p, &access, checked(model)?, checked(voice)?).await?}))
}

/// Models the key can use, from the provider's own list: realtime (conversation),
/// transcribe (speech to text) and speech (text to speech).
pub async fn models(store: &Store, p: Provider) -> Result<Value> {
    let access = keys::access(store, p)?;
    let entries: Vec<Model> = match p {
        Provider::OpenAi => {
            let v = send(access.name(p), access.request(p, Method::GET, "/v1/models")).await?;
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
            let v = send(access.name(p), access.request(p, Method::GET, "/v1beta/models?pageSize=1000")).await?;
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
    let mut defaults = Defaults {
        realtime: newest(p, &entries, &realtime).unwrap_or(fallback.realtime),
        transcribe: newest(p, &entries, &transcribe).unwrap_or(fallback.transcribe),
        speech: newest(p, &entries, &speech).unwrap_or(fallback.speech),
    };
    let (realtime, transcribe, speech) = if access.upstream == Upstream::Compatible {
        // A server of the user's own names its models as it likes (`whisper-large-v3`, `kokoro`): any of them can
        // do either Speech job, the first by default where the names don't say. It has no Realtime.
        let all: Vec<String> = entries.into_iter().map(|m| m.id).collect();
        if let Some(first) = all.first() {
            if transcribe.is_empty() {
                defaults.transcribe.clone_from(first);
            }
            if speech.is_empty() {
                defaults.speech.clone_from(first);
            }
        }
        (Vec::new(), all.clone(), all)
    } else {
        (realtime, transcribe, speech)
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
    let access = keys::access(store, p)?;
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
            let req = access
                .request(p, Method::POST, "/v1/audio/transcriptions")
                .header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
                .body(body);
            send(access.name(p), req).await?["text"].as_str().unwrap_or_default().to_owned()
        }
        Provider::Gemini => {
            let body = json!({"contents": [{"parts": [
                {"text": format!("Transcribe this audio exactly as spoken. {TRANSCRIBE_HINT} {} Reply with the transcript only; reply with nothing if there is no speech.", names_hint(&names))},
                {"inlineData": {"mimeType": "audio/wav", "data": audio}},
            ]}]});
            let v = send(access.name(p), gemini_generate(&access, model).json(&body)).await?;
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
    let access = keys::access(store, p)?;
    let (model, voice) = (checked(model)?, checked(voice)?);
    if text.trim().is_empty() || text.len() > 4000 {
        bail!("text must be 1–4000 bytes");
    }
    match p {
        Provider::OpenAi => {
            // A compatible server answers WAV, so it needs no AAC encoder; every client plays both.
            let format = if access.upstream == Upstream::Compatible { "wav" } else { "aac" };
            let body = json!({"model": model, "voice": voice, "input": text, "response_format": format});
            let who = access.name(p);
            let res = access
                .request(p, Method::POST, "/v1/audio/speech")
                .json(&body)
                .timeout(TIMEOUT * 4)
                .send()
                .await
                .map_err(|_| Unreachable(who))?;
            let status = res.status();
            let bytes = res.bytes().await.map_err(|_| anyhow!("{who} sent no audio."))?;
            if !status.is_success() {
                return Err(refused(who, status, &bytes));
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
            let v = send(access.name(p), gemini_generate(&access, model).json(&body)).await?;
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

    #[test]
    fn status_says_calls_can_use_google_search() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        assert_eq!(status(&store).unwrap()["googleSearch"], true);
    }

    /// An OpenAI-compatible server on this computer that lists `models`, and the address to save for it.
    async fn compatible_server(models: &'static [&'static str]) -> String {
        use axum::{Json, Router, routing};
        let list =
            move || async move { Json(json!({"data": models.iter().map(|id| json!({"id": id})).collect::<Vec<_>>()})) };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/v1/models", routing::get(list))).await.unwrap();
        });
        url
    }

    #[tokio::test]
    async fn the_openai_engine_moves_to_a_saved_server_and_back() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let openai = |v: &Value| v["providers"][0].clone();
        assert_eq!(openai(&status(&store).unwrap())["server"], "", "OpenAI itself until one is saved");
        assert!(status(&store).unwrap()["providers"][1].get("server").is_none(), "Gemini has no other server");

        let url = compatible_server(&["whisper-1", "tts-1"]).await;
        let saved = set_server(&store, Provider::OpenAi, &format!("{url}/v1/")).await.unwrap();
        assert_eq!(openai(&saved)["server"], url);
        // The key is checked with that server (OpenAI would refuse it), and its models become the defaults.
        let keyed = set_key(&store, Provider::OpenAi, "local-key").await.unwrap();
        assert_eq!(openai(&keyed)["configured"], true);
        assert_eq!(openai(&keyed)["defaults"]["transcribe"], "whisper-1");
        let realtime = session(&store, Provider::OpenAi, "gpt-realtime-2.1", "marin").await.unwrap_err();
        assert!(realtime.to_string().contains("use Speech mode"), "{realtime}");

        // A server that doesn't answer is refused while a key would go to it, and the saved one stays.
        let dead = set_server(&store, Provider::OpenAi, "http://127.0.0.1:9").await.unwrap_err();
        assert_eq!((dead.to_string().as_str(), category(&dead)), ("Can't reach your server.", "unreachable"));
        // So is one that answers but doesn't take the saved key.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let elsewhere = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, axum::Router::new()).await.unwrap() });
        let keyless = set_server(&store, Provider::OpenAi, &elsewhere).await.unwrap_err();
        assert_eq!(category(&keyless), "refused");
        assert!(keyless.to_string().starts_with("The saved key doesn't work with this server ("), "{keyless}");
        assert_eq!(openai(&status(&store).unwrap())["server"], url);
        let cleartext = set_server(&store, Provider::OpenAi, "http://192.168.1.20:8890").await.unwrap_err();
        assert_eq!(category(&cleartext), "bad_address", "cleartext off this computer");
        assert_eq!(category(&set_server(&store, Provider::Gemini, &url).await.unwrap_err()), "unsupported");

        // Back to OpenAI without asking it about a key only the server took: that key goes with the server.
        let cleared = set_server(&store, Provider::OpenAi, "").await.unwrap();
        assert_eq!((&openai(&cleared)["server"], &openai(&cleared)["configured"]), (&json!(""), &json!(false)));
        assert_eq!(openai(&cleared)["defaults"]["transcribe"], "gpt-transcribe", "OpenAI's own until its list is read");
    }

    #[tokio::test]
    async fn every_model_a_server_lists_can_be_picked() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let url = compatible_server(&["whisper-large-v3", "kokoro"]).await;
        set_server(&store, Provider::OpenAi, &url).await.unwrap();
        set_key(&store, Provider::OpenAi, "local-key").await.unwrap();
        let listed = models(&store, Provider::OpenAi).await.unwrap();
        let all = json!(["whisper-large-v3", "kokoro"]);
        assert_eq!((&listed["transcribe"], &listed["speech"], &listed["models"]), (&all, &all, &json!([])));
        let defaults = &listed["defaults"];
        assert_eq!(
            (&defaults["transcribe"], &defaults["speech"]),
            (&json!("whisper-large-v3"), &json!("whisper-large-v3"))
        );
    }

    fn category(error: &anyhow::Error) -> &'static str {
        error.downcast_ref::<ServerRefused>().expect("a categorized refusal").category
    }

    /// A cloud computer's keeper sends the key to `OpenAI` only: no server is taken or shown, even one left saved.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_cloud_computer_has_no_server_to_choose() {
        let k = crate::keeper::tests::test_keeper();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        k.manage(&store);
        server::save(&store, Some("http://127.0.0.1:8890")).unwrap();
        let refused = set_server(&store, Provider::OpenAi, "http://127.0.0.1:8890").await.unwrap_err();
        assert_eq!(category(&refused), "unsupported");
        assert!(status(&store).unwrap()["providers"][0].get("server").is_none());
    }

    #[tokio::test]
    async fn a_server_is_saved_before_any_key() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let saved = set_server(&store, Provider::OpenAi, "https://speech.example.com/v1").await.unwrap();
        assert_eq!(saved["providers"][0]["server"], "https://speech.example.com");
        assert_eq!(set_server(&store, Provider::OpenAi, "").await.unwrap()["providers"][0]["server"], "");
    }

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
