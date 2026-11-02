//! Background jobs on the bot's own harness (dreaming, automatic naming): one prompt to a throwaway agent.

use crate::agent::acp::{self, Acp, Incoming};
use crate::agent::bot::{Adapter, Instructions};
use crate::hub::Hub;
use crate::store::BotConfig;
use anyhow::{Result, anyhow, bail};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_mins(3);

/// Runs one prompt on a throwaway agent of the bot's harness and returns its reply.
/// Claude gets a real system prompt, no tools, no settings and no saved session; an agent with ACP
/// `systemPrompt` gets the instructions there, other harnesses inline. None of them may remember the job.
pub async fn one_shot(hub: &Arc<Hub>, cfg: &BotConfig, system: &str, user: &str) -> Result<String> {
    let cwd = crate::service::data_dir().join("background");
    tokio::fs::create_dir_all(&cwd).await?;
    let cwd = cwd.to_string_lossy().into_owned();
    if hub.store.kv_read(&format!("agent-env:{}", cfg.backend))?.is_some() {
        crate::market::vault::unlock(hub.clone()).await?;
    }
    let env = crate::agent::auth::launch_env(&hub.store, &cfg.backend)?;
    let mut conn = None;
    let mut last_err = None;
    for candidate in crate::agent::bot::launch_commands(cfg)? {
        let started = async {
            let mut command = [candidate.resolve(|_| {}).await?.acp()];
            let mut env = env.clone();
            crate::agent::own_memory::turn_off(&cfg.backend, &cwd, &mut env, &mut command).await;
            crate::agent::bot::start_agent(&command[0], &cwd, &env, Duration::from_mins(1)).await
        };
        match started.await {
            Ok(c) => {
                conn = Some(c);
                break;
            }
            Err(e) => last_err = Some(e),
        }
    }
    let mut conn = conn.ok_or_else(|| last_err.unwrap_or_else(|| anyhow!("no agent to run a background job")))?;
    let acp = conn.acp.clone();
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
        // Same model as the bot: the harness default may be another (local) model that
        // would load next to the bot's, or a cloud one the user kept these chats away from.
        if conn.adapter != Adapter::Claude {
            crate::agent::bot::select_model(&acp, &res, &sid, cfg).await?;
        }
        let prompt = acp.request("session/prompt", json!({"sessionId": sid, "prompt": [{"type": "text", "text": text}]}));
        tokio::pin!(prompt);
        let deadline = tokio::time::sleep(TIMEOUT);
        tokio::pin!(deadline);
        let mut out = String::new();
        loop {
            tokio::select! {
                r = &mut prompt => { r.and_then(acp::codebuddy::prompt_result)?; break }
                inc = conn.rx.recv() => {
                    if !collect(&acp, inc, &mut out).await {
                        bail!("the background job's agent exited");
                    }
                }
                () = &mut deadline => bail!("the background job timed out"),
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
                acp.respond(id, acp::permission_answer(None)).await
            } else {
                acp.respond_error(id, -32601, "not supported").await
            };
            true
        }
    }
}
