//! Signing agents in the ACP way, so every agent that speaks ACP can be set up
//! from Codync, not just the ones we know by name.
//!
//! A check starts the agent, reads the `authMethods` it advertises and tries
//! `session/new`: `-32000` means it needs signing in. Methods come in three kinds:
//! - terminal: a command (the agent's `_meta.terminal-auth`, or its own binary
//!   with `args`), run in a setup terminal (`term`);
//! - agent: the agent signs itself in on `authenticate` (usually a browser on
//!   this computer);
//! - env var: keys the user types, kept in the store and handed to the agent's
//!   environment on every launch.

use crate::LockExt;
use crate::acp::{AUTH_REQUIRED, Acp, Incoming, RpcError};
use crate::backends;
use crate::registry::{Cmd, env_prefix, shell_quote};
use crate::store::Store;
use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

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
fn probe_dir() -> PathBuf {
    crate::service::data_dir().join("probe")
}

fn env_slot(backend: &str) -> String {
    format!("agent-env:{backend}")
}

/// Keys saved for `backend`, as environment for its process.
pub fn env(store: &Store, backend: &str) -> Vec<(String, String)> {
    store
        .kv_get(&env_slot(backend))
        .and_then(|s| serde_json::from_str::<BTreeMap<String, String>>(&s).ok())
        .map(|m| m.into_iter().collect())
        .unwrap_or_default()
}

/// Names of the saved keys (values never leave the host).
fn saved_env(store: &Store, backend: &str) -> Vec<String> {
    env(store, backend).into_iter().map(|(k, _)| k).collect()
}

/// Runs a check and returns `{signedIn, detail, methods, savedEnv}`.
pub async fn check(store: &Store, backend: &str) -> Result<Value> {
    let status = probe(store, backend).await?;
    backends::set_signed_in(backend, status.signed_in);
    CHECKED.locked().insert(backend.to_owned(), status.clone());
    let mut v = serde_json::to_value(&status)?;
    v["savedEnv"] = json!(saved_env(store, backend));
    // Codync's own sign-in command (phone-friendly device flows where the CLI has one).
    v["login"] = json!(backends::login_available(backend));
    Ok(v)
}

async fn probe(store: &Store, backend: &str) -> Result<Status> {
    let (acp, _inbox, init, cmd) = start(store, backend).await?;
    let methods = methods(backend, &init, &cmd);
    let dir = probe_dir();
    tokio::fs::create_dir_all(&dir).await?;
    let res = tokio::time::timeout(
        Duration::from_secs(60),
        acp.request("session/new", json!({"cwd": dir, "mcpServers": []})),
    )
    .await;
    acp.kill().await;
    let (signed_in, detail) = match res {
        Ok(Ok(_)) => (Some(true), None),
        Ok(Err(e)) => match e.downcast_ref::<RpcError>() {
            Some(r) if r.code == AUTH_REQUIRED => (Some(false), Some(r.message.clone())),
            _ => (None, Some(format!("{e:#}"))),
        },
        Err(_) => (None, None),
    };
    Ok(Status { signed_in, detail, methods })
}

/// Starts `backend` and completes `initialize`, trying each way to launch it.
/// The agent's inbox must stay open: the reader stops when nobody listens.
type Started = (Arc<Acp>, mpsc::UnboundedReceiver<Incoming>, Value, Cmd);

async fn start(store: &Store, backend: &str) -> Result<Started> {
    // The installed CLI alone when there is one: resolving the registry build can mean a long download.
    let candidates = match backends::local_candidate(backend) {
        Some(local) => vec![local],
        None => backends::launch_candidates(backend, |_| {}).await?,
    };
    let env = env(store, backend);
    let dir = probe_dir();
    tokio::fs::create_dir_all(&dir).await?;
    let mut last = None;
    let count = candidates.len();
    for (i, cmd) in candidates.into_iter().enumerate() {
        // A first `npx` run downloads the package: give the last candidate time.
        let budget = Duration::from_secs(if i + 1 < count { 20 } else { 180 });
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
                    // Ask for terminal sign-in methods (spec field and the older `_meta` flag).
                    "auth": {"terminal": true},
                    "_meta": {"terminal-auth": true},
                },
                "clientInfo": {"name": "codync", "title": "Codync", "version": env!("CARGO_PKG_VERSION")},
            }),
        );
        match tokio::time::timeout(budget, init).await {
            Ok(Ok(init)) => return Ok((acp, rx, init, cmd)),
            Ok(Err(e)) => {
                // Its last stderr line usually says why it quit.
                let why = std::iter::from_fn(|| rx.try_recv().ok()).find_map(|inc| match inc {
                    Incoming::Closed { stderr_tail } => {
                        stderr_tail.lines().rev().find(|l| !l.trim().is_empty()).map(str::to_owned)
                    }
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

/// Lets the agent sign itself in (it usually opens a browser on this computer).
pub async fn authenticate(store: &Store, backend: &str, method: &str) -> Result<Value> {
    let (acp, _inbox, _, _) = start(store, backend).await?;
    let res =
        tokio::time::timeout(Duration::from_secs(10 * 60), acp.request("authenticate", json!({"methodId": method})))
            .await;
    acp.kill().await;
    match res {
        Ok(r) => {
            r.context("signing in")?;
        }
        Err(_) => bail!("Sign-in didn't finish within 10 minutes."),
    }
    check(store, backend).await
}

/// Saves the keys for an env-var method (empty values remove them), then re-checks.
pub async fn set_env(store: &Store, backend: &str, vars: &Map<String, Value>) -> Result<Value> {
    let mut saved: BTreeMap<String, String> = env(store, backend).into_iter().collect();
    for (k, v) in vars {
        if !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || k.is_empty() {
            bail!("`{k}` isn't an environment variable name");
        }
        match v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
            Some(v) => saved.insert(k.clone(), v.to_owned()),
            None => saved.remove(k),
        };
    }
    store.kv_set(&env_slot(backend), &serde_json::to_string(&saved)?)?;
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
        assert!(matches!(kinds[0], ("a", MethodKind::Terminal { command }) if command == "/bin/tool login '--x y'"));
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

    #[test]
    fn skips_terminal_commands_of_another_harness() {
        let init = json!({"authMethods": [{"id": "k", "name": "Kilo", "_meta": {"terminal-auth": {"command": "opencode", "args": ["auth", "login"]}}}]});
        assert!(methods("kilo", &init, &cmd()).is_empty());
        assert_eq!(methods("opencode", &init, &cmd()).len(), 1);
    }
}
