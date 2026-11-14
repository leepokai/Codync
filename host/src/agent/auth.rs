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
use crate::agent::acp::{self, AUTH_REQUIRED, Incoming, RpcError};
use crate::agent::backends;
use crate::agent::registry::{Cmd, env_prefix};
use crate::keeper::client::Grants;
use crate::remote::managed::is_managed;
use crate::shell::quote as shell_quote;
use crate::store::Store;
use anyhow::{Result, bail};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::mpsc;

mod agent;
mod models;
mod proxied;
pub use agent::authenticate;
use agent::start;
pub(crate) use agent::{Started, start_with};
pub use models::{InFlight, models, shared_models};
pub(crate) use models::{model_config, model_ids};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum MethodKind {
    /// Shell command for a setup terminal. Never sent to clients.
    Terminal {
        #[serde(skip)]
        command: String,
    },
    Agent,
    EnvVar {
        vars: Vec<EnvVar>,
        link: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvVar {
    pub name: String,
    pub label: String,
    pub secret: bool,
    pub optional: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Method {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(flatten)]
    pub kind: MethodKind,
}

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

fn env_slot(backend: &str) -> String {
    format!("agent-env:{backend}")
}

/// The keys saved for `backend` that its agents may read: all of them, except a cloud computer's proxied ones.
fn visible(store: &Store, backend: &str) -> Result<BTreeMap<String, String>> {
    let Some(raw) = crate::market::vault::read(store, &env_slot(backend))? else {
        return Ok(BTreeMap::new());
    };
    Ok(serde_json::from_str(&raw)?)
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

/// Keys saved for `backend`, as environment for its process, and the grants a cloud computer's proxied ones are on:
/// the process holds those for as long as it runs (`Acp::hold`); dropping them revokes them.
pub fn env(store: &Store, backend: &str) -> Result<(Vec<(String, String)>, Grants)> {
    env_without(store, backend, &[])
}

/// [`env`] without the keys `skip` names, each with the address saved for it.
fn env_without(store: &Store, backend: &str, skip: &[&str]) -> Result<(Vec<(String, String)>, Grants)> {
    let mut saved = visible(store, backend)?;
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
        proxied::add(store, backend, skip, &mut env, &mut grants)?;
    }
    Ok((env, grants))
}

/// One saved value its agent reads as is (a folder, such as `CODEX_HOME`); never a proxied key.
pub fn saved_value(store: &Store, backend: &str, name: &str) -> Result<Option<String>> {
    Ok(visible(store, backend)?.remove(name))
}

/// The environment `backend`'s process starts with: the saved keys, what the harness needs to start, and where
/// `account`'s login is (and [`env`]'s grants).
pub fn launch_env(store: &Store, backend: &str, account: Option<&str>) -> Result<(Vec<(String, String)>, Grants)> {
    let slot = crate::agent::accounts::env(store, backend, account)?;
    // An account's login wins over a key saved for the agent, which the agent would run on instead.
    let login_keys = if slot.is_empty() { &[] } else { crate::agent::accounts::login_keys(backend) };
    let (mut env, grants) = env_without(store, backend, login_keys)?;
    backends::apply_process_env(backend, &mut env);
    // And its folder over one the user saved for the agent.
    env.retain(|(name, _)| !slot.iter().any(|(set, _)| set == name));
    env.extend(slot);
    Ok((env, grants))
}

/// Names of the saved keys (values never leave the host), proxied ones too.
pub(crate) fn saved_env(store: &Store, backend: &str) -> Result<Vec<String>> {
    let mut names: Vec<String> = visible(store, backend)?.into_keys().collect();
    if is_managed(store) {
        names.extend(proxied::names(store, backend)?);
        names.sort();
    }
    Ok(names)
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
    if backends::agent_checks_off() {
        return Ok(Status::default());
    }
    let Started { acp, mut inbox, init, cmd } = start(store, backend).await?;
    let methods = methods(backend, &init, &cmd);
    // Held until the check below is done.
    let (vars, _grants) = env(store, backend)?;
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

/// The agent's first `_auth/status_update` (ACP `authStatus` extension): `kind: "none"` is signed
/// out, any other kind signed in. `None`: it can't tell (it stays silent then).
async fn reported_login(inbox: &mut mpsc::UnboundedReceiver<Incoming>) -> Option<bool> {
    // Claude's adapter gives its `claude auth status` 5 s.
    let report = async {
        while let Some(incoming) = inbox.recv().await {
            if let Incoming::Notification { method, params } = incoming
                && method == "_auth/status_update"
                && let Some(kind) = params["authStatus"]["kind"].as_str()
            {
                return Some(kind != "none");
            }
        }
        None
    };
    tokio::time::timeout(Duration::from_secs(8), report).await.ok().flatten()
}

fn text(v: &Value) -> Option<String> {
    v.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

fn joined_args(v: &Value) -> String {
    v.as_array()
        .map(|a| a.iter().filter_map(Value::as_str).map(shell_quote).collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}

/// The agent's advertised sign-in methods, in Codync's terms.
fn methods(backend: &str, init: &Value, cmd: &Cmd) -> Vec<Method> {
    let empty = Map::new();
    let mut out = init["authMethods"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let id = text(&m["id"])?;
            let name = text(&m["name"]).unwrap_or_else(|| id.clone());
            let meta = m["_meta"].as_object().unwrap_or(&empty);
            let kind = if m["type"] == "env_var" {
                let vars = m["vars"]
                    .as_array()?
                    .iter()
                    .filter_map(|v| {
                        let name = text(&v["name"])?;
                        name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_').then(|| EnvVar {
                            label: text(&v["label"]).unwrap_or_else(|| name.clone()),
                            secret: v["secret"].as_bool().unwrap_or(true),
                            optional: v["optional"].as_bool().unwrap_or(false),
                            name,
                        })
                    })
                    .collect::<Vec<_>>();
                if vars.is_empty() {
                    return None;
                }
                MethodKind::EnvVar { vars, link: text(&m["link"]) }
            } else if let Some(t) = meta.get("terminal-auth").filter(|t| t["command"].is_string()) {
                let program = t["command"].as_str().unwrap_or_default();
                // An adapter pointing at another harness's CLI is a copy-paste slip (Kilo → `opencode`).
                if backends::belongs_to_other(backend, program) {
                    return None;
                }
                let command = format!("{}{} {}", env_prefix(&t["env"]), shell_quote(program), joined_args(&t["args"]));
                MethodKind::Terminal { command }
            } else if m["type"] == "terminal" || meta.get("type").is_some_and(|t| t == "terminal") {
                let args = if m["args"].is_array() { &m["args"] } else { meta.get("args").unwrap_or(&Value::Null) };
                let env = if m["env"].is_object() { &m["env"] } else { meta.get("env").unwrap_or(&Value::Null) };
                MethodKind::Terminal { command: format!("{}{} {}", env_prefix(env), cmd.program, joined_args(args)) }
            } else if meta.contains_key("api-key") || meta.contains_key("gateway") {
                // Keys the agent itself asks for through a UI we don't have.
                return None;
            } else {
                MethodKind::Agent
            };
            Some(Method { id, name, description: text(&m["description"]), kind })
        })
        .collect::<Vec<_>>();
    // Some agents list a key twice: prose-only ("set Z_AI_API_KEY") and as a real env-var method.
    let keyed: Vec<String> =
        out.iter().filter(|m| matches!(m.kind, MethodKind::EnvVar { .. })).map(|m| m.name.clone()).collect();
    out.retain(|m| !matches!(m.kind, MethodKind::Agent) || !keyed.contains(&m.name));
    out
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
    let mut changes = Vec::new();
    for (k, v) in vars {
        if !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || k.is_empty() {
            bail!("`{k}` isn't an environment variable name");
        }
        changes.push((k.clone(), v.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)));
    }
    if is_managed(store) {
        proxied::save(store, backend, &changes)?;
    } else {
        let mut saved = visible(store, backend)?;
        apply(&mut saved, &changes);
        crate::market::vault::write(store, &env_slot(backend), &serde_json::to_string(&saved)?)?;
    }
    // New keys are a new sign-in: the prompt the old ones got refused no longer counts.
    backends::clear_prompt_refused(backend);
    check(store, backend).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd() -> Cmd {
        Cmd { program: "npx -y pkg@1".into(), args: "--acp".into() }
    }

    #[test]
    fn reads_every_kind_of_method() {
        let init = json!({"authMethods": [
            {"id": "a", "name": "Terminal meta", "_meta": {"terminal-auth": {"command": "/bin/tool", "args": ["login", "--x y"]}}},
            {"id": "b", "name": "Terminal args", "type": "terminal", "args": ["--terminal-login"]},
            {"id": "c", "name": "Key", "type": "env_var", "link": "https://k", "vars": [{"name": "Z_KEY", "label": "Key"}, {"name": "bad name"}]},
            {"id": "d", "name": "Browser"},
            {"id": "e", "name": "Api key", "_meta": {"api-key": {"provider": "openai"}}},
            {"id": "f", "name": "Qwen style", "_meta": {"type": "terminal", "args": ["--auth-type=openai"]}},
            {"id": "g", "name": "Key"},
        ]});
        let m = methods("x", &init, &cmd());
        let kinds: Vec<_> = m.iter().map(|m| (m.id.as_str(), &m.kind)).collect();
        assert!(
            matches!(kinds[0], ("a", MethodKind::Terminal { command }) if command == &format!("/bin/tool login {}", shell_quote("--x y")))
        );
        assert!(
            matches!(kinds[1], ("b", MethodKind::Terminal { command }) if command == "npx -y pkg@1 --terminal-login")
        );
        assert!(
            matches!(kinds[2], ("c", MethodKind::EnvVar { vars, .. }) if vars.len() == 1 && vars[0].name == "Z_KEY")
        );
        assert!(matches!(kinds[3], ("d", MethodKind::Agent)));
        assert!(
            matches!(kinds[4], ("f", MethodKind::Terminal { command }) if command == "npx -y pkg@1 --auth-type=openai")
        );
        assert_eq!(m.len(), 5);
        // Commands stay on the host.
        assert!(serde_json::to_value(&m[0]).unwrap().get("command").is_none());
    }

    #[tokio::test]
    async fn takes_the_first_login_the_agent_reports() {
        let report = |kind: &str| Incoming::Notification {
            method: "_auth/status_update".into(),
            params: json!({"authStatus": {"kind": kind, "label": "x"}}),
        };
        let (tx, mut rx) = mpsc::unbounded_channel();
        tx.send(Incoming::Notification { method: "session/update".into(), params: json!({}) }).unwrap();
        tx.send(report("none")).unwrap();
        tx.send(report("account")).unwrap();
        assert_eq!(reported_login(&mut rx).await, Some(false));
        assert_eq!(reported_login(&mut rx).await, Some(true));
        drop(tx);
        assert_eq!(reported_login(&mut rx).await, None, "an agent that goes quiet can't tell");
    }

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

    #[test]
    fn skips_terminal_commands_of_another_harness() {
        let init = json!({"authMethods": [{"id": "k", "name": "Kilo", "_meta": {"terminal-auth": {"command": "opencode", "args": ["auth", "login"]}}}]});
        assert!(methods("kilo", &init, &cmd()).is_empty());
        assert_eq!(methods("opencode", &init, &cmd()).len(), 1);
    }
}
