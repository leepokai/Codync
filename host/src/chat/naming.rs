//! Automatic bot names, as in Grok Bot: a bot created without a name is called
//! [`PLACEHOLDER`] until its first few conversations show what it's for. Then
//! a fast optional decision sidecar (or a one-shot agent of the bot's harness,
//! see `oneshot`) is asked for a short name, at the end of a turn. The user
//! renaming it, or asking the bot to rename itself (`profile`), ends this.
//! The description stays the user's: it holds standing instructions.

use crate::analytics::{self, Event};
use crate::hub::Hub;
use crate::sidecar::{DecisionCallOutcome, DecisionCallReport};
use crate::store::Store;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Map;
use serde_json::json;
use std::future::Future;
use std::sync::Arc;

/// A finished user ↔ bot exchange.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Turn {
    pub user: String,
    pub agent: String,
}

pub const PLACEHOLDER: &str = "New Bot";
/// Conversations before the first naming attempt.
const NAME_AFTER: usize = 3;
/// The most recent conversations shown to the namer (it retries after each one).
const WINDOW: usize = 6;
pub const MAX_NAME_CHARS: usize = 40;

fn key(bot_id: &str) -> String {
    format!("naming.turns.{bot_id}")
}

fn pending(store: &Store, bot_id: &str) -> Vec<Turn> {
    store.kv_get(&key(bot_id)).and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

/// Records a finished conversation and, once there are enough, names the bot.
pub async fn observe(hub: &Arc<Hub>, bot_id: &str, turn: Turn) -> Result<()> {
    let Some(cfg) = hub.store.bot(bot_id)?.filter(|b| !b.deleted && b.config.auto_name).map(|b| b.config) else {
        return Ok(());
    };
    let mut turns = pending(&hub.store, bot_id);
    turns.push(turn);
    let skip = turns.len().saturating_sub(WINDOW);
    turns.drain(..skip);
    hub.store.kv_set(&key(bot_id), &serde_json::to_string(&turns)?)?;
    if turns.len() < NAME_AFTER {
        return Ok(());
    }
    let prompt = user_prompt(&turns);
    let Some(name) = resolve_name_with_telemetry(
        Some(hub.as_ref()),
        crate::sidecar::decision_call_for_store_report(&hub.store, SYSTEM_PROMPT, &prompt),
        || crate::chat::oneshot::one_shot(hub, &cfg, SYSTEM_PROMPT, &prompt),
    )
    .await?
    else {
        tracing::info!(bot = %bot_id, "purpose not clear yet; naming later");
        return Ok(());
    };
    // The user may have renamed it while the namer ran.
    if !hub.store.bot(bot_id)?.is_some_and(|b| !b.deleted && b.config.auto_name) {
        return Ok(());
    }
    let patch = json!({"id": bot_id, "name": name, "autoName": false});
    let hub2 = hub.clone();
    tokio::task::spawn_blocking(move || hub2.update_bot(&patch)).await??;
    hub.store.kv_set(&key(bot_id), "[]")?;
    tracing::info!(bot = %bot_id, "named from its conversations");
    Ok(())
}

#[allow(dead_code)]
async fn resolve_name<S, F, Fut>(sidecar: S, oneshot_fallback: F) -> Result<Option<String>>
where
    S: Future<Output = Option<String>>,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<String>>,
{
    if let Some(raw) = sidecar.await
        && let Some(name) = parse(&raw)
    {
        return Ok(Some(name));
    }
    let raw = oneshot_fallback().await?;
    Ok(parse(&raw))
}

async fn resolve_name_with_telemetry<S, F, Fut>(
    hub: Option<&Hub>,
    sidecar: S,
    oneshot_fallback: F,
) -> Result<Option<String>>
where
    S: Future<Output = DecisionCallReport>,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<String>>,
{
    let report = sidecar.await;
    let sidecar_outcome = if let Some(raw) = report.content.as_deref() {
        if let Some(name) = parse(raw) {
            if let Some(hub) = hub
                && report.targets > 0
            {
                capture_sidecar_called(
                    hub,
                    DecisionCallOutcome::Hit.as_str(),
                    report.ms,
                    report.targets,
                    report.attempted,
                );
            }
            return Ok(Some(name));
        }
        "unparseable"
    } else {
        report.outcome.as_str()
    };

    if let Some(hub) = hub
        && report.targets > 0
    {
        capture_sidecar_called(hub, sidecar_outcome, report.ms, report.targets, report.attempted);
    }

    let fallback_res = oneshot_fallback().await;
    if let Some(hub) = hub
        && report.targets > 0
    {
        let (oneshot_outcome, parsed) = match &fallback_res {
            Ok(raw) => {
                let name = parse(raw);
                (if name.is_some() { "named" } else { "none" }, name)
            }
            Err(_) => ("error", None),
        };
        capture_sidecar_fallback(hub, sidecar_outcome, report.ms, oneshot_outcome);
        return match fallback_res {
            Ok(_) => Ok(parsed),
            Err(e) => Err(e),
        };
    }

    let raw = fallback_res?;
    Ok(parse(&raw))
}

fn capture_sidecar_called(hub: &Hub, outcome: &'static str, ms: u64, targets: usize, attempted: usize) {
    let props = Map::from_iter([
        ("outcome".into(), json!(outcome)),
        ("ms".into(), json!(ms)),
        ("targets".into(), json!(targets)),
        ("attempted".into(), json!(attempted)),
    ]);
    analytics::capture(hub, Event::SidecarNamingCalled, props);
}

fn capture_sidecar_fallback(hub: &Hub, reason: &'static str, ms: u64, oneshot_outcome: &'static str) {
    let props = Map::from_iter([
        ("reason".into(), json!(reason)),
        ("ms".into(), json!(ms)),
        ("oneshot_outcome".into(), json!(oneshot_outcome)),
    ]);
    analytics::capture(hub, Event::SidecarNamingFallback, props);
}

const SYSTEM_PROMPT: &str = "\
You name a personal assistant bot after its first conversations with the user.
Read the conversations and work out what the user relies on this bot for.
Output exactly one line:
name: <a short name for the bot, 1-4 words, saying what it does, like a job title or a team nickname>
Write it in the language and script the user writes in (Traditional and Simplified Chinese are different). No quotes, no emoji, no trailing punctuation.
Output exactly NONE if the conversations don't show a clear purpose yet (only greetings, tests or small talk).";

fn user_prompt(turns: &[Turn]) -> String {
    let body = turns
        .iter()
        .map(|t| format!("User: {}\nBot: {}", t.user.trim(), t.agent.trim()))
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("Conversations so far, oldest first:\n\n{body}")
}

fn parse(raw: &str) -> Option<String> {
    let name = raw.lines().find_map(|line| {
        let (tag, rest) = line.trim().trim_start_matches(['-', '*', ' ']).split_once(':')?;
        tag.trim().eq_ignore_ascii_case("name").then(|| rest.trim().trim_matches(['"', '“', '”', '「', '」']))
    })?;
    let name = name.trim_end_matches(['.', '。']).trim();
    (!name.is_empty() && !name.eq_ignore_ascii_case("none") && name.chars().count() <= MAX_NAME_CHARS)
        .then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::Value;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::net::TcpListener;

    #[test]
    fn names_are_parsed() {
        assert_eq!(parse("name: 「記帳助手」").as_deref(), Some("記帳助手"));
        assert_eq!(parse("Sure!\n- Name: Release Captain.").as_deref(), Some("Release Captain"));
    }

    #[test]
    fn unclear_or_bad_names_are_rejected() {
        assert_eq!(parse("NONE"), None);
        assert_eq!(parse("name: NONE"), None);
        assert_eq!(parse("name: "), None);
        assert_eq!(parse(&format!("name: {}", "x".repeat(41))), None);
    }

    #[tokio::test]
    async fn unconfigured_sidecar_falls_back_to_oneshot_with_zero_network_calls() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        let targets = crate::sidecar::config::load_targets_with(Some(&store), |_| None);
        assert_eq!(targets, Vec::new());

        let called = Arc::new(AtomicBool::new(false));
        let called_flag = called.clone();
        let out = resolve_name(async { None }, || async move {
            called_flag.store(true, Ordering::SeqCst);
            Ok("name: Fallback Bot".to_owned())
        })
        .await
        .unwrap();
        assert!(called.load(Ordering::SeqCst));
        assert_eq!(out.as_deref(), Some("Fallback Bot"));
    }

    #[tokio::test]
    async fn sidecar_success_names_bot_without_invoking_oneshot() {
        let app = Router::new().route(
            "/v1/chat/completions",
            post(|| async { Json(json!({"choices": [{"message": {"content": "name: 「記帳助手」"}}]})) }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let store = Store::open(Path::new(":memory:")).unwrap();
        store
            .kv_set(
                crate::sidecar::config::STORE_TARGETS_KEY,
                &json!([{"url": format!("http://{addr}/v1"), "model": "fast"}]).to_string(),
            )
            .unwrap();

        let called = Arc::new(AtomicBool::new(false));
        let called_flag = called.clone();
        let out = resolve_name(
            crate::sidecar::decision_call_for_store(&store, SYSTEM_PROMPT, "User: log lunch"),
            || async move {
                called_flag.store(true, Ordering::SeqCst);
                Ok("name: Slow Fallback".to_owned())
            },
        )
        .await
        .unwrap();
        assert!(!called.load(Ordering::SeqCst), "oneshot must be skipped when sidecar succeeds");
        assert_eq!(out.as_deref(), Some("記帳助手"));
    }

    #[tokio::test]
    async fn sidecar_failure_or_unparseable_output_falls_back_to_oneshot() {
        let called = Arc::new(AtomicBool::new(false));
        let called_flag = called.clone();
        let out = resolve_name(async { Some("NONE".to_owned()) }, || async move {
            called_flag.store(true, Ordering::SeqCst);
            Ok("name: Reliable Bot".to_owned())
        })
        .await
        .unwrap();
        assert!(called.load(Ordering::SeqCst));
        assert_eq!(out.as_deref(), Some("Reliable Bot"));
    }

    #[tokio::test]
    async fn sidecar_telemetry_emits_non_sensitive_called_and_fallback_events() {
        let hub = crate::chat::calls::context::tests::hub();
        let hit = resolve_name_with_telemetry(
            Some(&hub),
            async {
                DecisionCallReport {
                    content: Some("name: Fast Bot".to_owned()),
                    targets: 1,
                    attempted: 1,
                    outcome: DecisionCallOutcome::Hit,
                    ms: 12,
                }
            },
            || async { Ok("name: Unused".to_owned()) },
        )
        .await
        .unwrap();
        assert_eq!(hit.as_deref(), Some("Fast Bot"));

        let fallback = resolve_name_with_telemetry(
            Some(&hub),
            async {
                DecisionCallReport {
                    content: None,
                    targets: 2,
                    attempted: 2,
                    outcome: DecisionCallOutcome::Http402,
                    ms: 25,
                }
            },
            || async { Ok("name: Backup Bot".to_owned()) },
        )
        .await
        .unwrap();
        assert_eq!(fallback.as_deref(), Some("Backup Bot"));

        let events: Vec<(String, Value, Value)> = hub
            .analytics
            .drain_test_events()
            .into_iter()
            .map(|ev| {
                (
                    ev["event"].as_str().unwrap().to_owned(),
                    ev["properties"]["outcome"].clone(),
                    ev["properties"]["reason"].clone(),
                )
            })
            .collect();
        assert_eq!(
            events,
            vec![
                ("sidecar_naming_called".to_owned(), json!("hit"), Value::Null),
                ("sidecar_naming_called".to_owned(), json!("http_402"), Value::Null),
                ("sidecar_naming_fallback".to_owned(), Value::Null, json!("http_402")),
            ]
        );
    }
}
