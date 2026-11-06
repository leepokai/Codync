//! Starting an agent for a check, and letting it sign itself in.

use super::{check, launch_env, probe_dir};
use crate::agent::acp::{self, Acp, Incoming};
use crate::agent::backends;
use crate::agent::registry::{Candidate, Cmd};
use crate::store::Store;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::ffi::OsStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Starts `backend` and completes `initialize`, trying each way to launch it.
/// The agent's inbox must stay open: the reader stops when nobody listens.
pub(super) type Started = (Arc<Acp>, mpsc::UnboundedReceiver<Incoming>, Value, Cmd);

pub(super) async fn start(store: &Store, backend: &str) -> Result<Started> {
    let mut candidates = backends::launch_candidates(backend)?;
    // The installed CLI alone when there is one: resolving the registry build can mean a long download.
    if matches!(candidates.first(), Some(Candidate::Ready(_))) {
        candidates.truncate(1);
    }
    let env = launch_env(store, backend)?;
    let dir = probe_dir();
    tokio::fs::create_dir_all(&dir).await?;
    let mut last = None;
    let count = candidates.len();
    for (i, candidate) in candidates.into_iter().enumerate() {
        // A first `npx` run downloads the package: give the last candidate time.
        let budget = Duration::from_secs(if i + 1 < count { 20 } else { 180 });
        let cmd = match candidate.resolve(|_| {}).await {
            Ok(cmd) => cmd,
            Err(e) => {
                last = Some(e);
                continue;
            }
        };
        let (acp, mut rx) = match Acp::spawn(&cmd.acp(), &dir.to_string_lossy(), &env) {
            Ok(v) => v,
            Err(e) => {
                last = Some(e);
                continue;
            }
        };
        let init = acp.request(
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientCapabilities": {
                    "fs": {"readTextFile": false, "writeTextFile": false},
                    "terminal": false,
                    // Ask for terminal sign-in methods (spec field and the older `_meta` flag),
                    // and for plain models with options, as bot sessions do (`bot::start_agent`).
                    "auth": {"terminal": true},
                    "_meta": {"terminal-auth": true, "parameterizedModelPicker": true},
                },
                "clientInfo": {"name": "codync", "title": "Codync", "version": env!("CARGO_PKG_VERSION")},
            }),
        );
        match tokio::time::timeout(budget, init).await {
            Ok(Ok(init)) => return Ok((acp, rx, init, cmd)),
            Ok(Err(e)) => {
                let why = std::iter::from_fn(|| rx.try_recv().ok()).find_map(|inc| match inc {
                    Incoming::Closed { stderr_tail } => stderr_reason(&stderr_tail),
                    _ => None,
                });
                last = Some(match why {
                    Some(why) => e.context(why),
                    None => e,
                });
            }
            Err(_) => last = Some(anyhow!("the agent didn't start within {}s", budget.as_secs())),
        }
        acp.kill().await;
    }
    Err(last.unwrap_or_else(|| anyhow!("couldn't start the agent")))
}

/// The stderr line that says why the agent quit: an error line, else the last one that isn't a pretty-printed Node
/// error's closing brace.
fn stderr_reason(stderr_tail: &str) -> Option<String> {
    let lines: Vec<&str> = stderr_tail.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    lines
        .iter()
        .rev()
        .find(|line| line.contains("Error"))
        .or_else(|| lines.iter().rev().find(|line| !matches!(**line, "}" | "{" | "}," | "]" | "[")))
        .map(|line| (*line).to_owned())
}

/// Lets the agent sign itself in (it usually opens a browser on this computer).
pub async fn authenticate(store: &Store, backend: &str, method: &str) -> Result<Value> {
    // A saved session makes WorkBuddy log out first, and that logout cancels the login.
    if backend == "workbuddy" {
        tokio::task::spawn_blocking(|| backends::move_saved_session_aside("workbuddy"))
            .await
            .context("couldn't prepare WorkBuddy sign-in")?;
    }
    let (acp, mut inbox, _, _) = start(store, backend).await?;
    let res = tokio::time::timeout(Duration::from_mins(10), sign_in(&acp, &mut inbox, method)).await;
    acp.kill().await;
    match res {
        Ok(r) => {
            r.context("signing in")?;
        }
        Err(_) => bail!("Sign-in didn't finish within 10 minutes."),
    }
    // The browser flow finished. Ask the account again; a prompt refusal no longer overrides it.
    // Completing `authenticate` is not itself proof: WorkBuddy still answers the next prompt
    // with "Authentication required" when the account isn't usable.
    backends::clear_prompt_refused(backend);
    check(store, backend).await
}

/// The agent's answer to `authenticate`. A headless CLI that only posts its sign-in page (`WorkBuddy`'s) gets it
/// opened here, once; what it asks of us meanwhile is refused.
async fn sign_in(acp: &Acp, inbox: &mut mpsc::UnboundedReceiver<Incoming>, method: &str) -> Result<Value> {
    let request = acp.request("authenticate", json!({"methodId": method}));
    tokio::pin!(request);
    let mut opened = false;
    loop {
        tokio::select! {
            result = &mut request => return result,
            incoming = inbox.recv() => match incoming {
                Some(Incoming::Request { id, .. }) => acp.respond_error(id, -32601, "not supported during sign-in").await?,
                Some(Incoming::Closed { .. }) | None => bail!("the agent closed during sign-in"),
                Some(notification) => {
                    if !opened && let Some(url) = acp::codebuddy::sign_in_url(&notification) {
                        opened = true;
                        if !crate::shell::open(OsStr::new(&url)) {
                            tracing::warn!("couldn't open the sign-in page");
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stderr_reason_skips_a_closing_brace() {
        let tail = "Error: Cannot find module '../dist/codebuddy'\nRequire stack:\n- /cli/bin/codebuddy\n  code: 'MODULE_NOT_FOUND'\n}\n";
        assert_eq!(stderr_reason(tail).as_deref(), Some("Error: Cannot find module '../dist/codebuddy'"));
        assert_eq!(stderr_reason("agent said no\n").as_deref(), Some("agent said no"));
    }
}
