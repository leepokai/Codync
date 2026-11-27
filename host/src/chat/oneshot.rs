//! Background jobs on the bot's own harness (dreaming, automatic naming, call titles): one prompt to a throwaway
//! agent.

use crate::agent::bot::{Adapter, Instructions};
use crate::hub::Hub;
use crate::store::BotConfig;
use anyhow::{Result, anyhow, bail};
use codync_agent::acp::{self, Acp, Incoming};
use codync_agent::events::{Event, Update};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_mins(3);
const DELETE_TIMEOUT: Duration = Duration::from_secs(10);

/// Runs one prompt on a throwaway agent of the bot's harness and returns its reply.
/// Claude gets a real system prompt, no tools, no settings and no saved session; an agent with ACP
/// `systemPrompt` gets the instructions there, other harnesses inline. None of them may remember the job: an agent
/// that saves sessions and offers `session/delete` gets the job's deleted (Codex archives it), or it would show in
/// the user's own history (#346).
pub async fn one_shot(hub: &Arc<Hub>, cfg: &BotConfig, system: &str, user: &str) -> Result<String> {
    let cwd = crate::service::data_dir().join("background");
    tokio::fs::create_dir_all(&cwd).await?;
    let cwd = cwd.to_string_lossy().into_owned();
    let profile = cfg.login_profile.as_deref();
    if crate::agent::auth::launch_reads_vault(&hub.store, &cfg.backend, profile)? {
        crate::market::vault::unlock(hub.clone()).await?;
    }
    let (env, grants) = crate::agent::auth::launch_env(&hub.store, &cfg.backend, profile)?;
    let candidates = crate::agent::bot::launch_commands(cfg)?;
    let workspace = cwd.as_str();
    let mut conn = crate::agent::bot::start_first(candidates, &env, |candidate, mut env, _| async move {
        let mut command = [candidate.resolve(|_| {}).await?.acp()];
        crate::agent::own_memory::turn_off(&cfg.backend, workspace, &mut env, &mut command).await;
        crate::agent::bot::start_agent(
            &cfg.backend,
            &command[0],
            workspace,
            &env,
            Duration::from_mins(1),
            crate::agent::bot::Background::Unreported,
        )
        .await
    })
    .await?;
    grants.held_by(&conn.acp);
    let acp = conn.acp.clone();
    let mut opened = None;
    let result = async {
        let mut params = json!({"cwd": cwd, "mcpServers": []});
        let text = match conn.instructions() {
            Instructions::ClaudeMeta => {
                params["_meta"] = json!({
                    "systemPrompt": system,
                    "claudeCode": {"options": {"tools": [], "persistSession": false, "settingSources": [], "model": "haiku"}},
                });
                user.to_owned()
            }
            Instructions::SystemPrompt => {
                params["systemPrompt"] = crate::agent::bot::system_prompt(system);
                user.to_owned()
            }
            Instructions::FirstMessage => format!("{system}\n\n---\n\n{user}"),
        };
        let res = acp.request("session/new", params).await?;
        let sid = res["sessionId"].as_str().ok_or_else(|| anyhow!("agent returned no sessionId"))?.to_owned();
        opened = Some(sid.clone());
        // Same model as the bot: the harness default may be another (local) model that
        // would load next to the bot's, or a cloud one the user kept these chats away from.
        if conn.adapter() != Adapter::Claude {
            crate::agent::bot::select_model(&acp, &res, &sid, cfg).await?;
        }
        let prompt = async { acp.session(&sid).prompt(json!([{"type": "text", "text": text}])).await?.response().await };
        tokio::pin!(prompt);
        let deadline = tokio::time::sleep(TIMEOUT);
        tokio::pin!(deadline);
        let mut out = String::new();
        loop {
            tokio::select! {
                r = &mut prompt => { r.and_then(acp::codebuddy::prompt_result)?; break }
                inc = conn.rx.recv() => {
                    if !collect(inc, &mut out) {
                        bail!("the background job's agent exited");
                    }
                }
                () = &mut deadline => bail!("the background job timed out"),
            }
        }
        while let Ok(inc) = conn.rx.try_recv() {
            collect(Some(inc), &mut out);
        }
        Ok(out)
    }
    .await;
    if let Some(sid) = opened.filter(|_| conn.deletes_sessions() && conn.adapter() != Adapter::Claude) {
        forget(&acp, &cfg.id, &sid).await;
    }
    acp.kill().await;
    result
}

/// Deletes the job's session from the agent's saved history; the job's result stands either way.
async fn forget(acp: &Acp, bot: &str, sid: &str) {
    match tokio::time::timeout(DELETE_TIMEOUT, acp.request("session/delete", json!({"sessionId": sid}))).await {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => {
            tracing::warn!(
                bot,
                session = sid,
                error = format!("{error:#}"),
                "couldn't delete a background job's session"
            );
        }
        Err(_) => tracing::warn!(bot, session = sid, "deleting a background job's session timed out"),
    }
}

/// Keeps reply text; refuses anything the agent asks of us (a request dropped unanswered refuses itself). False once
/// the agent is gone.
fn collect(inc: Option<Incoming>, out: &mut String) -> bool {
    match inc {
        None | Some(Incoming::Closed { .. }) => false,
        Some(Incoming::Update(Update { event: Event::Text { text, .. }, .. })) => {
            out.push_str(&text);
            true
        }
        Some(_) => true,
    }
}
