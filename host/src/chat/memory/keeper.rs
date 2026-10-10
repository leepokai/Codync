//! The bot's memory keeper: queued exchanges, the one-shot agent runs that remember them,
//! episodes and profile consolidation.

use super::extract::{
    EpisodeTurn, apply, consolidation_system_prompt, consolidation_user_prompt, episode_system_prompt,
    episode_user_prompt, existing_for_extraction, extraction_system_prompt, extraction_user_prompt, parse_extraction,
    pending_episode, set_pending_episode,
};
use super::facts::{Kind, Memory, dedupe_key};
use super::{
    EXCHANGE_CHARS, KEEPER_BATCH, KEEPER_IDLE, KEEPER_QUEUE_LIMIT, KEEPER_TIMEOUT, NONE, PROFILE_PROMPT_LIMIT,
};
use crate::agent::acp::{self, Acp, Incoming};
use crate::hub::Hub;
use crate::store::{BotConfig, Store};
use anyhow::{Result, anyhow, bail};
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// A finished user ↔ agent exchange worth remembering.
pub struct Exchange {
    pub user: String,
    pub agent: String,
    pub at: i64,
    pub session: String,
    pub revision: String,
}

pub enum KeeperEvent {
    Exchange(Exchange),
    Flush,
}

fn unprocessed_key(bot_id: &str) -> String {
    format!("memory.unprocessed.{bot_id}")
}

fn unprocessed(store: &Store, bot_id: &str) -> Vec<EpisodeTurn> {
    store.kv_get(&unprocessed_key(bot_id)).and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

/// Starts the bot's keeper. Exchanges queue up (persisted, so a restart keeps them)
/// and are remembered together once the bot has been quiet for [`KEEPER_IDLE`] or
/// [`KEEPER_BATCH`] have piled up: the frozen prompt only picks facts up at the next
/// compaction or session anyway, and one run over several exchanges sees their
/// context. An unnamed bot is named right away (see `naming`). It stops when the
/// bot actor drops the sender; what's queued is picked up by the next keeper.
pub fn spawn_keeper(hub: Arc<Hub>, bot_id: String) -> mpsc::UnboundedSender<KeeperEvent> {
    let (tx, mut rx) = mpsc::unbounded_channel::<KeeperEvent>();
    tokio::spawn(async move {
        let mut queued = unprocessed(&hub.store, &bot_id);
        let idle = tokio::time::sleep(KEEPER_IDLE);
        tokio::pin!(idle);
        // After a failed run the next one waits, longer each time, instead of retrying on every exchange.
        let mut failures = 0u32;
        let mut retry_at: Option<tokio::time::Instant> = None;
        let quiet_until = |retry_at: Option<tokio::time::Instant>| {
            let after = tokio::time::Instant::now() + KEEPER_IDLE;
            retry_at.map_or(after, |at| at.max(after))
        };
        loop {
            let flush = tokio::select! {
                x = rx.recv() => {
                    let Some(event) = x else { break };
                    let KeeperEvent::Exchange(x) = event else {
                        idle.as_mut().reset(retry_at.unwrap_or_else(tokio::time::Instant::now));
                        continue;
                    };
                    if x.revision != super::maintenance::revision(&hub.store, &bot_id) { continue; }
                    let turn = EpisodeTurn {
                        ts: x.at,
                        user: acp::truncate(&x.user, EXCHANGE_CHARS),
                        agent: acp::truncate(&x.agent, EXCHANGE_CHARS),
                        session: x.session,
                    };
                    let mutation = super::maintenance::lock(&bot_id);
                    let guard = mutation.lock().await;
                    if x.revision != super::maintenance::revision(&hub.store, &bot_id) { continue; }
                    // Re-read: another keeper of this bot (before a restart) may have flushed.
                    queued = unprocessed(&hub.store, &bot_id);
                    queued.push(turn.clone());
                    // ponytail: while the keeper can't run, only the newest exchanges wait for it (the
                    // transcript keeps them all); keep a longer backlog if older ones must still be learned.
                    let over = queued.len().saturating_sub(KEEPER_QUEUE_LIMIT);
                    queued.drain(..over);
                    if let Err(e) = hub.store.kv_set(&unprocessed_key(&bot_id), &serde_json::to_string(&queued).unwrap_or_default()) {
                        tracing::warn!(bot = %bot_id, error = format!("{e:#}"), "couldn't queue an exchange for memory");
                    }
                    drop(guard);
                    if let Err(e) = crate::chat::naming::observe(&hub, &bot_id, turn.clone()).await {
                        tracing::warn!(bot = %bot_id, error = format!("{e:#}"), "naming the bot failed");
                    }
                    idle.as_mut().reset(quiet_until(retry_at));
                    queued.len() >= KEEPER_BATCH && retry_at.is_none()
                }
                () = &mut idle, if !queued.is_empty() => true,
            };
            if !flush {
                continue;
            }
            let processing = super::maintenance::keeper_lock(&bot_id);
            let _processing = processing.lock().await;
            // One batch per run keeps the keeper's prompt bounded; the rest follows right after.
            let batch: Vec<EpisodeTurn> = unprocessed(&hub.store, &bot_id).into_iter().take(KEEPER_BATCH).collect();
            match remember(&hub, &bot_id, batch.clone()).await {
                Ok(()) => {
                    failures = 0;
                    retry_at = None;
                    let mutation = super::maintenance::lock(&bot_id);
                    let _guard = mutation.lock().await;
                    let mut current = unprocessed(&hub.store, &bot_id);
                    acknowledge(&mut current, &batch);
                    if let Err(e) = hub
                        .store
                        .kv_set(&unprocessed_key(&bot_id), &serde_json::to_string(&current).unwrap_or_default())
                    {
                        tracing::warn!(bot = %bot_id, error = format!("{e:#}"), "couldn't acknowledge the memory queue");
                    } else {
                        queued = current;
                    }
                }
                Err(e) => {
                    failures += 1;
                    retry_at = Some(tokio::time::Instant::now() + KEEPER_IDLE * 2u32.pow(failures.min(5)));
                    tracing::warn!(bot = %bot_id, failures, error = format!("{e:#}"), "memory keeper failed; batch retained for retry");
                }
            }
            let next = if retry_at.is_none() && queued.len() >= KEEPER_BATCH {
                tokio::time::Instant::now()
            } else {
                quiet_until(retry_at)
            };
            idle.as_mut().reset(next);
        }
    });
    tx
}

/// Another actor may have appended turns while an old keeper was working.
fn acknowledge(current: &mut Vec<EpisodeTurn>, completed: &[EpisodeTurn]) {
    if current.starts_with(completed) {
        current.drain(..completed.len());
    }
}

async fn remember(hub: &Arc<Hub>, bot_id: &str, turns: Vec<EpisodeTurn>) -> Result<()> {
    let mut groups = std::collections::BTreeMap::<String, Vec<EpisodeTurn>>::new();
    for turn in pending_episode(&hub.store, bot_id).into_iter().chain(turns) {
        groups.entry(turn.session.clone()).or_default().push(turn);
    }
    for (session, turns) in groups {
        remember_session(hub, bot_id, &session, turns).await?;
    }
    set_pending_episode(&hub.store, bot_id, &[])?;
    Ok(())
}

async fn remember_session(hub: &Arc<Hub>, bot_id: &str, session: &str, turns: Vec<EpisodeTurn>) -> Result<()> {
    let Some(cfg) = hub.store.bot(bot_id)?.filter(|b| !b.deleted).map(|b| b.config) else { return Ok(()) };
    let Some(at) = turns.last().map(|t| t.ts) else { return Ok(()) };
    let revision = super::maintenance::revision(&hub.store, bot_id);

    let id = bot_id.to_owned();
    let exchange_text = turns.iter().map(|t| format!("{}\n{}", t.user, t.agent)).collect::<Vec<_>>().join("\n");
    let existing = tokio::task::spawn_blocking(move || {
        Memory::for_bot(&id).and_then(|mem| existing_for_extraction(&mem, &exchange_text))
    })
    .await??;
    let raw = one_shot(hub, &cfg, &extraction_system_prompt(), &extraction_user_prompt(&turns, &existing)).await?;
    let extraction = parse_extraction(&raw, &existing);
    let mutation = super::maintenance::lock(bot_id);
    let guard = mutation.lock().await;
    if super::maintenance::revision(&hub.store, bot_id) != revision {
        return Ok(()); // A user's edit/forget supersedes this extraction from old context.
    }
    let id = bot_id.to_owned();
    let source_session =
        if session.is_empty() { String::new() } else { super::lifecycle::begin(hub, bot_id, session).await? };
    let summary_session = source_session.clone();
    let (added, removed, crowded) = tokio::task::spawn_blocking(move || {
        Memory::for_session(&id, &source_session).and_then(|mem| {
            let (added, removed) = apply(&mem, &extraction, &existing, at)?;
            Ok((added, removed, mem.profile_facts()?.len() > PROFILE_PROMPT_LIMIT))
        })
    })
    .await??;
    tracing::info!(bot = %bot_id, exchanges = turns.len(), added, removed, "memory updated");
    drop(guard);
    if crowded && let Err(e) = consolidate(hub, &cfg, &revision).await {
        tracing::warn!(bot = %bot_id, error = format!("{e:#}"), "consolidating the profile failed");
    }

    let raw = one_shot(hub, &cfg, &episode_system_prompt(&cfg.name), &episode_user_prompt(&cfg.name, &turns)).await?;
    let narrative: String = raw.trim().chars().take(8_000).collect();
    let _guard = mutation.lock().await;
    if super::maintenance::revision(&hub.store, bot_id) != revision {
        return Ok(());
    }
    if !narrative.is_empty() && !narrative.eq_ignore_ascii_case(NONE) {
        let id = bot_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let mem = Memory::for_session(&id, &summary_session)?;
            mem.summary(&narrative)
        })
        .await??;
    }
    Ok(())
}

/// The profile outgrew what the prompt shows: have the keeper merge it down to
/// [`PROFILE_TARGET`] facts. Nothing is lost: facts it drops move to the log.
async fn consolidate(hub: &Arc<Hub>, cfg: &BotConfig, revision: &str) -> Result<()> {
    let id = cfg.id.clone();
    let facts = tokio::task::spawn_blocking(move || Memory::for_bot(&id).and_then(|m| m.profile_facts())).await??;
    let raw = one_shot(hub, cfg, &consolidation_system_prompt(), &consolidation_user_prompt(&facts)).await?;
    let plan = parse_extraction(&raw, &[]);
    let keep: Vec<&String> = plan.additions.iter().filter(|(_, k)| *k == Kind::Profile).map(|(c, _)| c).collect();
    if keep.is_empty() || keep.len() > PROFILE_PROMPT_LIMIT {
        bail!("the consolidated profile had {} facts", keep.len());
    }
    let now = crate::store::now_ms();
    let dated = |c: &str| facts.iter().find(|f| dedupe_key(&f.content) == dedupe_key(c)).map_or(now, |f| f.created_at);
    let kept: Vec<(String, i64)> = keep.iter().map(|c| ((*c).clone(), dated(c))).collect();
    let kept_keys: HashSet<String> = kept.iter().map(|(c, _)| dedupe_key(c)).collect();
    let mut demoted: Vec<(String, i64)> = facts
        .iter()
        .filter(|f| !kept_keys.contains(&dedupe_key(&f.content)))
        .map(|f| (f.content.clone(), f.created_at))
        .collect();
    demoted.extend(plan.additions.iter().filter(|(_, k)| *k == Kind::Log).map(|(c, _)| (c.clone(), now)));
    let mutation = super::maintenance::lock(&cfg.id);
    let _guard = mutation.lock().await;
    if super::maintenance::revision(&hub.store, &cfg.id) != revision {
        return Ok(());
    }
    let id = cfg.id.clone();
    let (before, after) = (facts.len(), kept.len());
    tokio::task::spawn_blocking(move || Memory::for_bot(&id).and_then(|m| m.rewrite_profile(&kept, &demoted)))
        .await??;
    tracing::info!(bot = %cfg.id, before, after, "profile consolidated");
    Ok(())
}

/// Runs one prompt on a throwaway agent of the bot's harness and returns its reply.
/// Claude gets a real system prompt, no tools, no settings and no saved session;
/// other harnesses get the instructions inline.
pub(crate) async fn one_shot(hub: &Arc<Hub>, cfg: &BotConfig, system: &str, user: &str) -> Result<String> {
    let cwd = crate::service::data_dir().join("memory-keeper");
    tokio::fs::create_dir_all(&cwd).await?;
    let cwd = cwd.to_string_lossy().into_owned();
    if hub.store.kv_read(&format!("agent-env:{}", cfg.backend))?.is_some() {
        crate::market::vault::unlock(hub.clone()).await?;
    }
    let env = crate::agent::auth::env(&hub.store, &cfg.backend)?;
    let mut conn = None;
    let mut last_err = None;
    for command in crate::agent::bot::launch_commands(cfg, |_| {}).await? {
        match crate::agent::bot::start_agent(&command, &cwd, &env, Duration::from_secs(60)).await {
            Ok(c) => {
                conn = Some(c);
                break;
            }
            Err(e) => last_err = Some(e),
        }
    }
    let mut conn = conn.ok_or_else(|| last_err.unwrap_or_else(|| anyhow!("no agent to run the memory keeper")))?;
    let acp = conn.acp.clone();
    let result = async {
        let mut params = json!({"cwd": cwd, "mcpServers": []});
        let text = if conn.claude {
            params["_meta"] = json!({
                "systemPrompt": system,
                "claudeCode": {"options": {"tools": [], "persistSession": false, "settingSources": [], "model": "haiku"}},
            });
            user.to_owned()
        } else {
            format!("{system}\n\n---\n\n{user}")
        };
        let res = acp.request("session/new", params).await?;
        let sid = res["sessionId"].as_str().ok_or_else(|| anyhow!("agent returned no sessionId"))?.to_owned();
        // Same model as the bot: the harness default may be another (local) model that
        // would load next to the bot's, or a cloud one the user kept these chats away from.
        if !conn.claude {
            crate::agent::bot::select_model(&acp, &res, &sid, cfg).await?;
        }
        let prompt = acp.request("session/prompt", json!({"sessionId": sid, "prompt": [{"type": "text", "text": text}]}));
        tokio::pin!(prompt);
        let deadline = tokio::time::sleep(KEEPER_TIMEOUT);
        tokio::pin!(deadline);
        let mut out = String::new();
        loop {
            tokio::select! {
                r = &mut prompt => { r?; break }
                inc = conn.rx.recv() => {
                    if !collect(&acp, inc, &mut out).await {
                        bail!("the memory keeper's agent exited");
                    }
                }
                () = &mut deadline => bail!("the memory keeper timed out"),
            }
        }
        while let Ok(inc) = conn.rx.try_recv() {
            collect(&acp, Some(inc), &mut out).await;
        }
        Ok(out)
    }
    .await;
    acp.kill().await;
    result
}

/// Keeps reply text; refuses anything the agent asks of us. False once the agent is gone.
async fn collect(acp: &Acp, inc: Option<Incoming>, out: &mut String) -> bool {
    match inc {
        None | Some(Incoming::Closed { .. }) => false,
        Some(Incoming::Notification { method, params }) => {
            let u = &params["update"];
            if method == "session/update" && u["sessionUpdate"] == "agent_message_chunk" {
                out.push_str(&acp::content_text(&u["content"]));
            }
            true
        }
        Some(Incoming::Request { id, method, .. }) => {
            let _ = if method == "session/request_permission" {
                acp.respond(id, json!({"outcome": {"outcome": "cancelled"}})).await
            } else {
                acp.respond_error(id, -32601, "not supported").await
            };
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acknowledging_a_batch_preserves_newer_or_replaced_work() {
        let turn = |ts| EpisodeTurn { ts, user: "preference".into(), agent: "noted".into(), session: "session".into() };
        let mut current = vec![turn(1), turn(2), turn(3)];
        acknowledge(&mut current, &[turn(1), turn(2)]);
        assert_eq!(current, vec![turn(3)]);
        acknowledge(&mut current, &[turn(1)]);
        assert_eq!(current, vec![turn(3)]);
    }
}
