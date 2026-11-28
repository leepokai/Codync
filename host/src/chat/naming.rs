//! Automatic bot names, as in Grok Bot: a bot created without a name is called
//! [`PLACEHOLDER`] until its first few conversations show what it's for. Then
//! a fast optional decision sidecar (or a one-shot agent of the bot's harness,
//! see `oneshot`) is asked for a short name, at the end of a turn. The user
//! renaming it, or asking the bot to rename itself (`profile`), ends this.
//! The description stays the user's: it holds standing instructions.

use crate::hub::Hub;
use crate::store::Store;
use anyhow::Result;
use serde::{Deserialize, Serialize};
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
    let Some(name) = resolve_name(crate::sidecar::decision_call_for_store(&hub.store, SYSTEM_PROMPT, &prompt), || {
        crate::chat::oneshot::one_shot(hub, &cfg, SYSTEM_PROMPT, &prompt)
    })
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
}
