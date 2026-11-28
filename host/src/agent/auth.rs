//! Signing agents in the ACP way, so every agent that speaks ACP can be set up
//! from Codync, not just the ones we know by name.
//!
//! A check starts the agent and reads the `authMethods` it advertises. An agent that
//! reports its login (`_auth/status_update`) is believed; when its report doesn't
//! come, the harness's own CLI check decides, else it can't tell (a session proves
//! nothing for it). Other agents get `session/new`: `-32000` means it needs signing
//! in (`WorkBuddy` opens one signed out, so its account tells). Methods come in three kinds:
//! - terminal: a command (the agent's `_meta.terminal-auth`, or its own binary
//!   with `args`), run in a setup terminal (`term`);
//! - agent: the agent signs itself in on `authenticate` (usually a browser on
//!   this computer);
//! - env var: keys the user types, kept in the store and handed to the agent's
//!   environment on every launch (on a Codync Cloud computer, provider keys through its keeper: `proxied`).

use crate::LockExt;
use crate::keeper::client::Grants;
use crate::remote::managed::is_managed;
use crate::store::Store;
use anyhow::{Result, bail};
use codync_agent::acp::{self, AUTH_REQUIRED, Incoming, RpcError};
use codync_agent::backends;
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::mpsc;

mod agent;
mod model_cache;
mod models;
mod proxied;
pub use agent::authenticate;
use agent::start;
pub(crate) use agent::{Started, authenticate_account, start_with};
pub use codync_agent::acp::auth::{Method, MethodKind};
use codync_agent::acp::auth::{methods, reported_login};
pub use model_cache::{
    Wanted as ModelListWanted, forget as forget_model_list, shared_models, signed_in as model_list_signed_in,
    warm as warm_models,
};
pub use models::{InFlight, models};

/// What the last check found for one backend.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub signed_in: Option<bool>,
    /// The agent's own words when it wants signing in (can hold a pairing code).
    pub detail: Option<String>,
    pub methods: Vec<Method>,
}

static CHECKED: Mutex<BTreeMap<String, Status>> = Mutex::new(BTreeMap::new());

/// Where probe sessions live, away from the user's projects.
pub(crate) fn probe_dir() -> PathBuf {
    crate::service::data_dir().join("probe")
}

/// Where `backend`'s saved keys are. A login profile's are in a slot of its own (`login_profiles::keys`).
fn env_slot(backend: &str) -> String {
    format!("agent-env:{backend}")
}

/// The variables saved in `slot` that agents may read: all of them, except a cloud computer's proxied ones.
fn visible(store: &Store, slot: &str) -> Result<BTreeMap<String, String>> {
    let Some(raw) = crate::market::vault::read(store, slot)? else {
        return Ok(BTreeMap::new());
    };
    Ok(serde_json::from_str(&raw)?)
}

/// What the user typed (name → value) as changes: an empty value removes the variable.
pub(crate) fn env_changes(vars: &Map<String, Value>) -> Result<Vec<(String, Option<String>)>> {
    let mut changes = Vec::new();
    for (k, v) in vars {
        if !codync_agent::shell::is_env_name(k) {
            bail!("`{k}` isn't an environment variable name");
        }
        changes.push((k.clone(), v.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)));
    }
    Ok(changes)
}

/// Applies `changes` (a value, or `None` to remove it) to `saved`: the names removed.
fn apply(saved: &mut BTreeMap<String, String>, changes: &[(String, Option<String>)]) -> BTreeSet<String> {
    let mut removed = BTreeSet::new();
    for (name, value) in changes {
        if let Some(value) = value {
            saved.insert(name.clone(), value.clone());
        } else {
            saved.remove(name);
            removed.insert(name.clone());
        }
    }
    removed
}

/// Applies `changes` to the variables saved in `slot`. On a Codync Cloud computer, provider keys go to its keeper
/// (`proxied`).
pub(crate) fn save_env(store: &Store, slot: &str, changes: &[(String, Option<String>)]) -> Result<()> {
    if is_managed(store) {
        return proxied::save(store, slot, changes);
    }
    let mut saved = visible(store, slot)?;
    apply(&mut saved, changes);
    crate::market::vault::write(store, slot, &serde_json::to_string(&saved)?)
}

/// Deletes every variable saved in `slot`, proxied ones too. Needs no open vault.
pub(crate) fn forget_env(store: &Store, slot: &str) -> Result<()> {
    if is_managed(store) {
        proxied::forget(store, slot)?;
    }
    crate::market::vault::delete(store, slot)
}

/// The variables saved in `slot` but those `skip` names (each with the address saved for it), as environment for
/// a process, and the grants a cloud computer's proxied ones are on: the process holds those for as long as it runs
/// (`Acp::hold`); dropping them revokes them.
pub(crate) fn env_without(store: &Store, slot: &str, skip: &[&str]) -> Result<(Vec<(String, String)>, Grants)> {
    let mut saved = visible(store, slot)?;
    for key in skip {
        if saved.remove(*key).is_some()
            && let Some(address) = proxied::address_of(key)
        {
            saved.remove(address);
        }
    }
    let mut env: Vec<(String, String)> = saved.into_iter().collect();
    let mut grants = Grants::default();
    if is_managed(store) {
        proxied::add(store, slot, skip, &mut env, &mut grants)?;
    }
    Ok((env, grants))
}

/// One saved value its agent reads as is (a folder, such as `CODEX_HOME`); never a proxied key.
pub fn saved_value(store: &Store, backend: &str, name: &str) -> Result<Option<String>> {
    Ok(visible(store, &env_slot(backend))?.remove(name))
}

/// The environment `backend`'s process starts with on `login_profile` (none: the computer's own login): the saved
/// keys, what the harness needs to start, and the login profile's own (and [`env_without`]'s grants).
pub fn launch_env(
    store: &Store,
    backend: &str,
    login_profile: Option<&str>,
) -> Result<(Vec<(String, String)>, Grants)> {
    let (own, own_grants) = crate::agent::login_profiles::env(store, backend, login_profile)?;
    // A login profile wins over a key saved for the agent, which the agent would run on instead.
    let login_keys = if own.is_empty() { &[] } else { crate::agent::login_profiles::login_keys(backend) };
    let (mut env, mut grants) = env_without(store, &env_slot(backend), login_keys)?;
    grants.absorb(own_grants);
    backends::apply_process_env(backend, &mut env);
    // And its variables over the ones the user saved for the agent.
    env.retain(|(name, _)| !own.iter().any(|(set, _)| set == name));
    env.extend(own);
    Ok((env, grants))
}

/// Whether [`launch_env`] reads the vault, which has to be open then: keys are saved for the agent, or it runs on
/// an API key login profile.
pub fn launch_reads_vault(store: &Store, backend: &str, login_profile: Option<&str>) -> Result<bool> {
    Ok(crate::market::vault::saved(store, &env_slot(backend))?
        || crate::agent::login_profiles::reads_vault(store, backend, login_profile))
}

/// Whether [`launch_env`] sets variable `name`, answered from the saved names alone (no vault, no grant).
pub fn launch_sets(store: &Store, backend: &str, login_profile: Option<&str>, name: &str) -> bool {
    let saved = || saved_env(store, backend).unwrap_or_default().iter().any(|k| k == name);
    match crate::agent::login_profiles::vars(store, backend, login_profile) {
        Some(own) => {
            own.iter().any(|k| k == name)
                || (!crate::agent::login_profiles::login_keys(backend).contains(&name) && saved())
        }
        None => saved(),
    }
}

/// Names of the variables saved in `slot` (values never leave the host), proxied ones too.
pub(crate) fn saved_names(store: &Store, slot: &str) -> Result<Vec<String>> {
    let mut names: Vec<String> = visible(store, slot)?.into_keys().collect();
    if is_managed(store) {
        names.extend(proxied::names(store, slot)?);
        names.sort();
    }
    Ok(names)
}

/// Names of the keys saved for `backend`.
pub(crate) fn saved_env(store: &Store, backend: &str) -> Result<Vec<String>> {
    saved_names(store, &env_slot(backend))
}

/// Runs a check and returns `{signedIn, detail, methods, savedEnv}`.
pub async fn check(store: &Store, backend: &str) -> Result<Value> {
    let mut status = probe(store, backend).await?;
    // Where a session proves nothing, a check that can't tell keeps what the last sign-in showed.
    if status.signed_in.is_none() && !backends::session_proves_sign_in(backend) {
        status.signed_in = backends::signed_in(backend);
    }
    status.signed_in = backends::apply_prompt_refusal(backend, status.signed_in);
    backends::set_signed_in(backend, status.signed_in);
    CHECKED.locked().insert(backend.to_owned(), status.clone());
    let mut v = serde_json::to_value(&status)?;
    v["savedEnv"] = json!(saved_env(store, backend)?);
    // Codync's own sign-in command (phone-friendly device flows where the CLI has one).
    v["login"] = json!(backends::login_available(backend));
    Ok(v)
}

async fn probe(store: &Store, backend: &str) -> Result<Status> {
    if !backends::agent_checks_on(backend) {
        return Ok(Status::default());
    }
    let Started { acp, mut inbox, init, cmd } = start(store, backend).await?;
    let methods = methods(backend, &init, &cmd.program);
    // Held until the check below is done.
    let (vars, _grants) = launch_env(store, backend, None)?;
    if let ControlFlow::Break(signed_in) = login_without_session(&init, &mut inbox, backend, &vars).await {
        acp.kill().await;
        return Ok(Status { signed_in, detail: None, methods });
    }
    let dir = probe_dir();
    tokio::fs::create_dir_all(&dir).await?;
    let res = tokio::time::timeout(Duration::from_mins(1), async {
        acp.request("session/new", json!({"cwd": dir, "mcpServers": []})).await?;
        // WorkBuddy opens a session before it has a token: its account says whether it's signed in.
        Ok::<_, anyhow::Error>(if backends::session_proves_sign_in(backend) {
            Some(true)
        } else {
            acp::codebuddy::signed_in(&acp).await
        })
    })
    .await;
    acp.kill().await;
    let (signed_in, detail) = match res {
        Ok(Ok(signed_in)) => (signed_in, None),
        Ok(Err(e)) => match e.downcast_ref::<RpcError>() {
            Some(r) if r.code == AUTH_REQUIRED => (Some(false), Some(r.message.clone())),
            _ => (None, Some(format!("{e:#}"))),
        },
        Err(_) => (None, None),
    };
    Ok(Status { signed_in, detail, methods })
}

/// `Break`: the login of an agent that reports it, never judged by a session: Claude's adapter
/// opens one without checking the login, which then fails at the first prompt. When its report
/// doesn't come (the adapter gives its own `claude auth status` 5 s, too few on many Windows
/// computers), the harness's CLI check decides, with more time and the agent's `env`, else it
/// can't tell. `Continue`: the agent doesn't report its login, so a session tells.
async fn login_without_session(
    init: &Value,
    inbox: &mut mpsc::UnboundedReceiver<Incoming>,
    backend: &str,
    env: &[(String, String)],
) -> ControlFlow<Option<bool>> {
    if !init["agentCapabilities"]["_meta"]["authStatus"].is_object() {
        return ControlFlow::Continue(());
    }
    if let Some(reported) = reported_login(inbox).await {
        return ControlFlow::Break(Some(reported));
    }
    ControlFlow::Break(match backends::harness(backend) {
        Some(h) => backends::cli_signed_in(h, env).await,
        None => None,
    })
}

/// The terminal command for a method found by the last check.
pub fn terminal_command(backend: &str, method: &str) -> Option<String> {
    CHECKED.locked().get(backend)?.methods.iter().find(|m| m.id == method).and_then(|m| match &m.kind {
        MethodKind::Terminal { command } => Some(command.clone()),
        _ => None,
    })
}

/// Saves the keys for an env-var method (empty values remove them), then re-checks.
pub async fn set_env(store: &Store, backend: &str, vars: &Map<String, Value>) -> Result<Value> {
    save_env(store, &env_slot(backend), &env_changes(vars)?)?;
    // New keys are a new sign-in: the prompt the old ones got refused no longer counts.
    backends::clear_prompt_refused(backend);
    check(store, backend).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_agent_that_reports_its_login_is_never_judged_by_a_session() {
        let reports = json!({"agentCapabilities": {"_meta": {"authStatus": {}}}});
        let (tx, mut rx) = mpsc::unbounded_channel();
        tx.send(Incoming::Notification {
            method: "_auth/status_update".into(),
            params: json!({"authStatus": {"kind": "none"}}),
        })
        .unwrap();
        assert_eq!(login_without_session(&reports, &mut rx, "x", &[]).await, ControlFlow::Break(Some(false)));
        drop(tx);
        // No report and no CLI check: it can't tell, and still opens no session.
        assert_eq!(login_without_session(&reports, &mut rx, "x", &[]).await, ControlFlow::Break(None));
        assert_eq!(login_without_session(&json!({}), &mut rx, "x", &[]).await, ControlFlow::Continue(()));
    }
}
