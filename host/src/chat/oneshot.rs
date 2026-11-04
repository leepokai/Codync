//! Background jobs on the bot's own harness (dreaming, automatic naming): one prompt to a throwaway agent.

use crate::agent::acp::{self, Acp, Incoming};
use crate::agent::bot::{Adapter, Instructions};
use crate::hub::Hub;
use crate::store::BotConfig;
use anyhow::{Result, anyhow, bail};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

static BACKGROUND: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

const TIMEOUT: Duration = Duration::from_mins(3);

/// Runs one prompt on a throwaway agent of the bot's harness and returns its reply.
/// Claude gets a real system prompt, no tools, no settings and no saved session; an agent with ACP
/// `systemPrompt` gets the instructions there, other harnesses inline. None of them may remember the job.
pub async fn one_shot(hub: &Arc<Hub>, cfg: &BotConfig, system: &str, user: &str) -> Result<String> {
    bounded(&BACKGROUND, TIMEOUT, async |allocation| run(hub, cfg, system, user, allocation).await).await
}

async fn bounded(
    semaphore: &'static tokio::sync::Semaphore,
    timeout: Duration,
    work: impl AsyncFnOnce(&mut Allocation) -> Result<String>,
) -> Result<String> {
    let deadline = tokio::time::Instant::now() + timeout;
    let permit = tokio::time::timeout_at(deadline, semaphore.acquire()).await??;
    let mut allocation = Allocation { acp: None, permit: Some(permit) };
    let result = tokio::time::timeout_at(deadline, work(&mut allocation)).await;
    if let Some(acp) = &allocation.acp {
        acp.stop_and_wait().await?;
    }
    allocation.acp = None;
    result.map_err(|_| anyhow!("the background job timed out"))?
}

/// Dropping a cancelled job must still initiate cleanup. Normal completion awaits it while holding its permit.
struct Allocation {
    acp: Option<Arc<Acp>>,
    permit: Option<tokio::sync::SemaphorePermit<'static>>,
}

impl Drop for Allocation {
    fn drop(&mut self) {
        if let Some(acp) = self.acp.take() {
            let permit = self.permit.take();
            tokio::spawn(async move {
                let stopped = acp.stop_and_wait().await;
                if let Err(error) = stopped {
                    tracing::warn!(%error, "background cleanup failed; its capacity remains reserved until host restart");
                    if let Some(permit) = permit {
                        permit.forget();
                    }
                } else {
                    drop(permit);
                }
            });
        }
    }
}

async fn run(hub: &Arc<Hub>, cfg: &BotConfig, system: &str, user: &str, allocation: &mut Allocation) -> Result<String> {
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
        if let Some(acp) = &allocation.acp {
            acp.stop_and_wait().await?;
        }
        allocation.acp = None;
        let started = async {
            let mut command = [candidate.resolve(|_| {}).await?.acp()];
            let mut env = env.clone();
            crate::agent::own_memory::turn_off(&cfg.backend, &cwd, &mut env, &mut command).await;
            crate::agent::bot::start_agent(&command[0], &cwd, &env, Duration::from_mins(1), |acp| {
                allocation.acp = Some(acp);
            })
            .await
        };
        match started.await {
            Ok(c) => {
                allocation.acp = Some(c.acp.clone());
                conn = Some(c);
                break;
            }
            Err(e) => last_err = Some(e),
        }
    }
    let mut conn = conn.ok_or_else(|| last_err.unwrap_or_else(|| anyhow!("no agent to run a background job")))?;
    let acp = conn.acp.clone();
    async {
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
        let mut out = String::new();
        loop {
            tokio::select! {
                r = &mut prompt => { r.and_then(acp::codebuddy::prompt_result)?; break }
                inc = conn.rx.recv() => {
                    if !collect(&acp, inc, &mut out).await {
                        bail!("the background job's agent exited");
                    }
                }
            }
        }
        while let Ok(inc) = conn.rx.try_recv() {
            collect(&acp, Some(inc), &mut out).await;
        }
        Ok(out)
    }
    .await
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

    #[tokio::test]
    async fn queued_background_jobs_share_capacity_and_a_deadline_that_includes_setup() {
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut jobs = Vec::new();
        for _ in 0..6 {
            let (active, peak) = (active.clone(), peak.clone());
            jobs.push(tokio::spawn(async move {
                bounded(&SLOTS, Duration::from_millis(100), async move |_| {
                    struct Running(Arc<AtomicUsize>);
                    impl Drop for Running {
                        fn drop(&mut self) {
                            self.0.fetch_sub(1, Ordering::SeqCst);
                        }
                    }
                    let _running = Running(active.clone());
                    let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    std::future::pending::<Result<String>>().await
                })
                .await
            }));
        }
        for job in jobs {
            let finished = job.await;
            assert!(finished.expect("job task").is_err());
        }
        assert_eq!(peak.load(Ordering::SeqCst), 2);
        assert_eq!(SLOTS.available_permits(), 2);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_deadline_during_initialize_retains_ownership_through_cleanup() {
        static SLOT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
        let pid = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let started = pid.clone();
        let result = bounded(&SLOT, Duration::from_millis(300), async move |allocation| {
            let dir = std::env::temp_dir();
            let command = "python3 -u -c 'import time; time.sleep(60)'";
            let _conn =
                crate::agent::bot::start_agent(command, &dir.to_string_lossy(), &[], Duration::from_secs(60), |acp| {
                    started.store(acp.pid().expect("pid"), Ordering::SeqCst);
                    allocation.acp = Some(acp);
                })
                .await?;
            Ok(String::new())
        })
        .await;
        assert!(result.is_err());
        assert_eq!(SLOT.available_permits(), 1);
        let pid = pid.load(Ordering::SeqCst);
        assert_ne!(pid, 0, "the test reached the handshake");
        // SAFETY: signal zero only tests existence.
        assert_eq!(unsafe { libc::kill(i32::try_from(pid).expect("pid fits"), 0) }, -1);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancellation_holds_the_slot_until_the_agent_has_exited() {
        static SLOT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
        let dir = std::env::temp_dir();
        let command = "python3 -u -c 'import time; time.sleep(60)'";
        let (acp, _) = Acp::spawn(command, &dir.to_string_lossy(), &[]).expect("start fixture");
        let pid = acp.pid().expect("fixture pid");
        let acquired = SLOT.acquire().await;
        let permit = acquired.expect("permit");
        let allocation = Allocation { acp: Some(acp.clone()), permit: Some(permit) };
        drop(allocation);
        assert_eq!(SLOT.available_permits(), 0);
        tokio::time::timeout(Duration::from_secs(20), async {
            let acquired = SLOT.acquire().await;
            let _permit = acquired.expect("permit after cleanup");
            // SAFETY: signal zero tests existence without sending a signal.
            assert_eq!(unsafe { libc::kill(i32::try_from(pid).expect("pid fits"), 0) }, -1);
        })
        .await
        .expect("cleanup deadline");
    }
}
