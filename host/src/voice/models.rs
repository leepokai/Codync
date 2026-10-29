//! Provider model discovery and stable defaults, shared by all clients.

use super::{Defaults, Provider, defaults_key, key, live, send};
use crate::store::Store;
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::time::Duration;

const KEY_VALIDATION_TIMEOUT: Duration = Duration::from_secs(20);

/// Models the key can use, from the provider's own list: realtime (conversation),
/// transcribe (speech to text) and speech (text to speech).
pub async fn models(store: &Store, p: Provider) -> Result<Value> {
    models_for_client(store, p, false).await
}

/// Live is opt-in: older clients never receive a model they cannot connect to.
pub async fn models_for_client(store: &Store, p: Provider, supports_live: bool) -> Result<Value> {
    let key = key(store, p)?;
    let entries: Vec<Model> = match p {
        Provider::OpenAi => {
            let v = send(p, crate::http().get("https://api.openai.com/v1/models").bearer_auth(&key)).await?;
            openai_entries(&v)
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
    let defaults = model_defaults(p, &entries);
    store.kv_set(&defaults_key(p), &serde_json::to_string(&defaults)?)?;
    let offered = offered_models(p, realtime, &entries, supports_live);
    Ok(json!({"models": offered, "transcribe": transcribe, "speech": speech, "defaults": defaults}))
}

fn model_defaults(p: Provider, entries: &[Model]) -> Defaults {
    let (realtime, transcribe, speech) = classify(p, entries);
    let fallback = p.fallback();
    Defaults {
        realtime: newest(p, entries, &realtime).unwrap_or(fallback.realtime),
        transcribe: newest(p, entries, &transcribe).unwrap_or(fallback.transcribe),
        speech: newest(p, entries, &speech).unwrap_or(fallback.speech),
    }
}

fn offered_models(p: Provider, mut realtime: Vec<String>, entries: &[Model], supports_live: bool) -> Vec<String> {
    if p == Provider::OpenAi && supports_live {
        realtime.extend(entries.iter().filter(|m| live::is_model(&m.id)).map(|m| m.id.clone()));
        realtime.sort();
        realtime.dedup();
    }
    realtime
}

fn openai_entries(value: &Value) -> Vec<Model> {
    value["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            Some(Model {
                id: m["id"].as_str()?.to_owned(),
                live: m["id"].as_str()?.contains("realtime"),
                generate: true,
                created: m["created"].as_i64(),
            })
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
enum KeyValidation {
    Realtime(String),
    Live(String),
}

fn validation_model(entries: &[Model]) -> Result<KeyValidation> {
    let (realtime, _, _) = classify(Provider::OpenAi, entries);
    if let Some(model) = newest(Provider::OpenAi, entries, &realtime) {
        return Ok(KeyValidation::Realtime(model));
    }
    let live: Vec<String> = entries.iter().filter(|m| live::is_model(&m.id)).map(|m| m.id.clone()).collect();
    newest(Provider::OpenAi, entries, &live)
        .map(KeyValidation::Live)
        .ok_or_else(|| anyhow!("This OpenAI key has no available Realtime or GPT-Live conversation model."))
}

pub(super) async fn validate_openai_key(key: &str, fallback: &str) -> Result<Option<Defaults>> {
    validate_openai_with(
        send(Provider::OpenAi, crate::http().get("https://api.openai.com/v1/models").bearer_auth(key)),
        fallback,
        KEY_VALIDATION_TIMEOUT,
        |validation| async move {
            match validation {
                KeyValidation::Realtime(model) => {
                    super::mint(Provider::OpenAi, key, &model, "marin").await?;
                }
                KeyValidation::Live(model) => live::validate_access(key, &model).await?,
            }
            Ok(())
        },
    )
    .await
}

async fn validate_openai_with<F: Future<Output = Result<()>>>(
    models: impl Future<Output = Result<Value>>,
    fallback: &str,
    budget: Duration,
    validate: impl FnOnce(KeyValidation) -> F,
) -> Result<Option<Defaults>> {
    tokio::time::timeout(budget, async {
        let (validation, defaults) = match models.await {
            Ok(models) => {
                let entries = openai_entries(&models);
                (validation_model(&entries)?, Some(model_defaults(Provider::OpenAi, &entries)))
            }
            // Restricted keys may create Realtime sessions without permission to enumerate models.
            // A real credential mint must still succeed before the key can be saved.
            Err(_) => (KeyValidation::Realtime(fallback.to_owned()), None),
        };
        validate(validation).await?;
        Ok(defaults)
    })
    .await
    .map_err(|_| anyhow!("OpenAI key validation timed out. The key was not saved; try again."))?
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn restricted_key_can_validate_without_model_listing_permission() {
        let mint_called = &std::cell::Cell::new(false);
        let defaults = validate_openai_with(
            async { Err(anyhow!("OpenAI answered 403: missing api.model.read")) },
            "saved-realtime-model",
            KEY_VALIDATION_TIMEOUT,
            |validation| async move {
                assert_eq!(validation, KeyValidation::Realtime("saved-realtime-model".into()));
                mint_called.set(true);
                Ok(())
            },
        )
        .await
        .unwrap();
        assert!(mint_called.get());
        assert!(defaults.is_none(), "failed discovery must not trigger another model request after saving");
    }

    #[tokio::test]
    async fn failed_discovery_does_not_bypass_credential_validation() {
        let result = validate_openai_with(
            async { Err(anyhow!("Model listing forbidden")) },
            "gpt-realtime-2.1",
            KEY_VALIDATION_TIMEOUT,
            async |_| Err(anyhow!("Invalid API key")),
        )
        .await;
        assert_eq!(result.unwrap_err().to_string(), "Invalid API key");
    }

    #[tokio::test]
    async fn live_only_key_still_uses_model_access_validation() {
        validate_openai_with(
            async { Ok(json!({"data": [{"id": "gpt-live-1"}]})) },
            "gpt-realtime-2.1",
            KEY_VALIDATION_TIMEOUT,
            |validation| async move {
                assert_eq!(validation, KeyValidation::Live("gpt-live-1".into()));
                Ok(())
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn successful_validation_reuses_discovered_defaults() {
        let defaults = validate_openai_with(
            async { Ok(json!({"data": [{"id": "gpt-realtime-2.2"}]})) },
            "gpt-realtime-2.1",
            KEY_VALIDATION_TIMEOUT,
            |validation| async move {
                assert_eq!(validation, KeyValidation::Realtime("gpt-realtime-2.2".into()));
                Ok(())
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(defaults.realtime, "gpt-realtime-2.2");
    }

    #[tokio::test]
    async fn discovery_and_fallback_validation_share_a_deadline_before_saving() {
        let budget = Duration::from_millis(10);
        let pending_discovery = validate_openai_with(std::future::pending(), "gpt-realtime-2.1", budget, |_| async {
            panic!("must not validate a key before discovery finishes")
        })
        .await;
        assert!(pending_discovery.unwrap_err().to_string().contains("not saved"));

        let pending_mint =
            validate_openai_with(async { Err(anyhow!("Model listing forbidden")) }, "gpt-realtime-2.1", budget, |_| {
                std::future::pending()
            })
            .await;
        assert!(pending_mint.unwrap_err().to_string().contains("not saved"));
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
    fn live_models_are_opt_in_and_cannot_replace_realtime_defaults() {
        let entries =
            [dated("gpt-realtime-2.1", 100), dated("gpt-live-1", 100_000), dated("gpt-live-transcribe", 200_000)];
        let (realtime, _, _) = classify(Provider::OpenAi, &entries);
        assert_eq!(newest(Provider::OpenAi, &entries, &realtime).as_deref(), Some("gpt-realtime-2.1"));
        assert_eq!(offered_models(Provider::OpenAi, realtime.clone(), &entries, false), ["gpt-realtime-2.1"]);
        assert_eq!(offered_models(Provider::OpenAi, realtime, &entries, true), ["gpt-live-1", "gpt-realtime-2.1"]);
    }
    #[test]
    fn saving_an_openai_key_accepts_live_only_and_realtime_access_but_not_transcription_only() {
        let entries =
            |ids: &[&str]| openai_entries(&json!({"data":ids.iter().map(|id| json!({"id":id})).collect::<Vec<_>>()}));
        assert_eq!(validation_model(&entries(&["gpt-live-1"])).unwrap(), KeyValidation::Live("gpt-live-1".into()));
        assert_eq!(
            validation_model(&entries(&["gpt-realtime-2.1", "gpt-live-1"])).unwrap(),
            KeyValidation::Realtime("gpt-realtime-2.1".into())
        );
        assert!(validation_model(&entries(&["gpt-live-transcribe", "gpt-realtime-translate", "gpt-6-luna"])).is_err());
    }
}
