//! Signing agents in the ACP way, so every agent that speaks ACP can be set up
//! from Codync, not just the ones we know by name.
//!
//! A check starts the agent and reads the `authMethods` it advertises. An agent that
//! reports its login (`_auth/status_update`) is believed; when its report doesn't
//! come, the harness's own CLI check decides, else it can't tell (a session proves
//! nothing for it). Other agents get `session/new`: `-32000` means it needs signing
//! in. Methods come in three kinds:
//! - terminal: a command (the agent's `_meta.terminal-auth`, or its own binary
//!   with `args`), run in a setup terminal (`term`);
//! - agent: the agent signs itself in on `authenticate` (usually a browser on
//!   this computer);
//! - env var: keys the user types, kept in the store and handed to the agent's
//!   environment on every launch.

use crate::LockExt;
use crate::agent::acp::{AUTH_REQUIRED, Acp, Incoming, RpcError};
use crate::agent::backends;
use crate::agent::registry::{Cmd, env_prefix};
use crate::shell::quote as shell_quote;
use crate::store::Store;
use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::ops::ControlFlow;
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
pub fn env(store: &Store, backend: &str) -> Result<Vec<(String, String)>> {
    let Some(raw) = crate::market::vault::read(store, &env_slot(backend))? else {
        return Ok(Vec::new());
    };
    let values: BTreeMap<String, String> = serde_json::from_str(&raw)?;
    Ok(values.into_iter().collect())
}

/// Names of the saved keys (values never leave the host).
fn saved_env(store: &Store, backend: &str) -> Result<Vec<String>> {
    Ok(env(store, backend)?.into_iter().map(|(k, _)| k).collect())
}

/// Runs a check and returns `{signedIn, detail, methods, savedEnv}`.
pub async fn check(store: &Store, backend: &str) -> Result<Value> {
    let status = probe(store, backend).await?;
    backends::set_signed_in(backend, status.signed_in);
    CHECKED.locked().insert(backend.to_owned(), status.clone());
    let mut v = serde_json::to_value(&status)?;
    v["savedEnv"] = json!(saved_env(store, backend)?);
    // Codync's own sign-in command (phone-friendly device flows where the CLI has one).
    v["login"] = json!(backends::login_available(backend));
    Ok(v)
}

/// Discover the agent's advertised models in an isolated, prompt-free session.
/// This uses the same credentials as actual bot sessions and never reads project files.
pub async fn models(store: &Store, backend: &str) -> Result<Value> {
    let (acp, mut inbox, _, _) = start(store, backend).await?;
    let result = tokio::time::timeout(Duration::from_secs(60), async {
        let request = acp.request("session/new", json!({"cwd": probe_dir(), "mcpServers": []}));
        tokio::pin!(request);
        let session = loop {
            tokio::select! {
                result = &mut request => break result?,
                incoming = inbox.recv() => match incoming {
                    Some(Incoming::Request { id, .. }) => {
                        acp.respond_error(id, -32601, "not supported during model discovery").await?;
                    }
                    Some(Incoming::Notification { .. }) => {}
                    Some(Incoming::Closed { .. }) | None => bail!("agent closed during model discovery"),
                }
            }
        };
        // Cursor lists each model's options (effort, fast…) only through its own method.
        let options = if backend == "cursor" {
            acp.request("cursor/list_available_models", json!({})).await.unwrap_or_default()
        } else {
            Value::Null
        };
        Ok((session, options))
    })
    .await;
    acp.kill().await;
    let (session, options) = result.context("the agent's model list timed out")??;
    Ok(model_catalog(&session, &options))
}

/// IDs are agent-defined: use the category, with legacy conventional IDs as a fallback.
pub(crate) fn model_config(session: &Value) -> Option<&Value> {
    let options = session["configOptions"].as_array()?;
    let is_select = |v: &&Value| v["type"] == "select" || v.get("type").is_none();
    options
        .iter()
        .filter(is_select)
        .find(|v| v["category"] == "model")
        .or_else(|| options.iter().filter(is_select).find(|v| v["id"] == "model" || v["id"] == "models"))
}

/// The model ids the agent lists, in its order.
pub(crate) fn model_ids(session: &Value) -> Vec<String> {
    let catalog = model_catalog(session, &Value::Null);
    catalog["models"].as_array().into_iter().flatten().filter_map(|m| text(&m["id"])).collect()
}

/// `{models: [{id, name, description, options}], currentModelId, fastMode}`; `options` is
/// Cursor's `cursor/list_available_models` answer (`Null` for other agents, whose current
/// model gets the session's own settings, such as Codex's reasoning effort).
fn model_catalog(session: &Value, options: &Value) -> Value {
    fn collect(options: &Value, out: &mut Vec<Value>) {
        for option in options.as_array().into_iter().flatten() {
            if option["options"].is_array() {
                collect(&option["options"], out);
            } else if let Some(id) = text(&option["value"])
                && !out.iter().any(|v| v["id"] == id)
            {
                out.push(json!({"id": id, "name": text(&option["name"]).unwrap_or(id),
                    "description": text(&option["description"])}));
            }
        }
    }
    let mut models = Vec::new();
    let current = if let Some(config) = model_config(session) {
        collect(&config["options"], &mut models);
        text(&config["currentValue"])
    } else {
        for model in session["models"]["availableModels"].as_array().into_iter().flatten() {
            if let Some(id) = text(&model["modelId"])
                && !models.iter().any(|v| v["id"] == id)
            {
                models.push(json!({"id": id, "name": text(&model["name"]).unwrap_or(id),
                    "description": text(&model["description"])}));
            }
        }
        text(&session["models"]["currentModelId"])
    };
    let own = session_options(session);
    for model in &mut models {
        let id = model["id"].as_str().unwrap_or_default();
        model["options"] = json!(if options.is_null() && current.as_deref() == Some(id) {
            own.clone()
        } else {
            model_options(options, id)
        });
    }
    json!({"models": models, "currentModelId": current,
        "fastMode": crate::agent::config::fast_mode_config(session).is_some()})
}

/// One model's options as `[{id, name, values: [{id, name}], default}]`, from Cursor's
/// `{models: [{value, configOptions}]}` (ACP select config options).
fn model_options(options: &Value, model: &str) -> Vec<Value> {
    let entry = options["models"].as_array().into_iter().flatten().find(|m| m["value"] == model);
    selects(entry.and_then(|m| m["configOptions"].as_array()).into_iter().flatten())
}

/// The session's own settings for the model it runs, in the same shape: every select config
/// option except the model itself, how the agent approves tools (the bot's Approvals) and
/// Fast mode (its own switch).
pub(crate) fn session_options(session: &Value) -> Vec<Value> {
    let model = model_config(session).map(|c| &c["id"]);
    let fast = crate::agent::config::fast_mode_config(session).map(|c| &c["id"]);
    let own = session["configOptions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| o["category"] != "mode" && Some(&o["id"]) != model && Some(&o["id"]) != fast);
    selects(own)
}

fn selects<'a>(options: impl Iterator<Item = &'a Value>) -> Vec<Value> {
    options
        .filter(|o| o["type"] == "select")
        .filter_map(|o| {
            let id = text(&o["id"])?;
            // Values may come in groups, as model lists do.
            let listed = o["options"].as_array()?.iter();
            let values: Vec<Value> = listed
                .flat_map(|v| std::iter::once(v).chain(v["options"].as_array().into_iter().flatten()))
                .filter_map(|v| {
                    let value = text(&v["value"])?;
                    Some(json!({"id": value, "name": text(&v["name"]).unwrap_or(value)}))
                })
                .collect();
            let mut setting = json!({"id": id, "name": text(&o["name"]).unwrap_or(id), "values": values,
                "default": text(&o["currentValue"])});
            // Which kind of setting it is (`thought_level`: the effort level), when the agent says.
            if let Some(category) = text(&o["category"]) {
                setting["category"] = category.into();
            }
            Some(setting)
        })
        .collect()
}

async fn probe(store: &Store, backend: &str) -> Result<Status> {
    let (acp, mut inbox, init, cmd) = start(store, backend).await?;
    let methods = methods(backend, &init, &cmd);
    if let ControlFlow::Break(signed_in) =
        login_without_session(&init, &mut inbox, backend, &env(store, backend)?).await
    {
        acp.kill().await;
        return Ok(Status { signed_in, detail: None, methods });
    }
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

/// Starts `backend` and completes `initialize`, trying each way to launch it.
/// The agent's inbox must stay open: the reader stops when nobody listens.
type Started = (Arc<Acp>, mpsc::UnboundedReceiver<Incoming>, Value, Cmd);

async fn start(store: &Store, backend: &str) -> Result<Started> {
    // The installed CLI alone when there is one: resolving the registry build can mean a long download.
    let candidates = match backends::local_candidate(backend) {
        Some(local) => vec![local],
        None => backends::launch_candidates(backend, |_| {}).await?,
    };
    let env = env(store, backend)?;
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
    let mut saved: BTreeMap<String, String> = env(store, backend)?.into_iter().collect();
    for (k, v) in vars {
        if !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || k.is_empty() {
            bail!("`{k}` isn't an environment variable name");
        }
        match v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
            Some(v) => saved.insert(k.clone(), v.to_owned()),
            None => saved.remove(k),
        };
    }
    crate::market::vault::write(store, &env_slot(backend), &serde_json::to_string(&saved)?)?;
    check(store, backend).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_catalog_reads_grouped_config_and_preserves_agent_ids() {
        let session = json!({"configOptions": [{
            "id": "provider-model", "category": "model", "type": "select", "currentValue": "m2",
            "options": [{"group": "a", "options": [
                {"value": "m1", "name": "First"}, {"value": "m2", "name": "Second"},
                {"value": "m1", "name": "Duplicate"}
            ]}]
        }], "models": {"availableModels": [{"modelId": "legacy", "name": "Legacy"}]}});
        assert_eq!(model_config(&session).unwrap()["id"], "provider-model");
        let result = model_catalog(&session, &Value::Null);
        assert_eq!(result["currentModelId"], "m2");
        assert_eq!(result["models"].as_array().unwrap().len(), 2);
        assert_eq!(result["models"][0]["id"], "m1");
        assert_eq!(result["models"][1]["name"], "Second");
        assert_eq!(result["models"][1]["options"], json!([]));
        assert_eq!(model_ids(&session), ["m1", "m2"]);
    }

    #[test]
    fn model_catalog_reads_legacy_and_missing_lists() {
        let result = model_catalog(
            &json!({"models": {
                "currentModelId": "a", "availableModels": [
                    {"modelId": "a", "name": "Agent model", "description": "Available here"},
                    {"name": "No ID"}
                ]
            }}),
            &Value::Null,
        );
        assert_eq!(result["models"].as_array().unwrap().len(), 1);
        assert_eq!(result["models"][0]["description"], "Available here");
        assert_eq!(
            model_catalog(&json!({}), &Value::Null),
            json!({"models": [], "currentModelId": null, "fastMode": false})
        );
    }

    #[test]
    fn model_catalog_adds_cursor_model_options() {
        let session = json!({"configOptions": [{"id": "model", "category": "model", "type": "select",
            "currentValue": "grok-4.7", "options": [{"value": "grok-4.7", "name": "Grok 4.7"}, {"value": "auto"}]}]});
        let listed = json!({"models": [{"value": "grok-4.7", "name": "Grok 4.7", "configOptions": [
            {"id": "reasoning_effort", "name": "Reasoning", "category": "thought_level", "type": "select",
             "currentValue": "high", "options": [{"value": "medium", "name": "Medium"}, {"value": "high", "name": "High"}]},
            {"id": "fast", "type": "select", "currentValue": "true", "options": [{"value": "false", "name": "Off"}, {"value": "true"}]},
            {"id": "verbose", "type": "boolean"}
        ]}]});
        let result = model_catalog(&session, &listed);
        assert_eq!(
            result["models"][0]["options"],
            json!([
                {"id": "reasoning_effort", "name": "Reasoning", "default": "high", "category": "thought_level",
                 "values": [{"id": "medium", "name": "Medium"}, {"id": "high", "name": "High"}]},
                {"id": "fast", "name": "fast", "default": "true",
                 "values": [{"id": "false", "name": "Off"}, {"id": "true", "name": "true"}]}
            ])
        );
        assert_eq!(result["models"][1]["options"], json!([]));
    }

    #[test]
    fn model_catalog_gives_the_current_model_the_sessions_own_options() {
        // Codex (codex-acp) advertises reasoning effort on the session, not per model.
        let session = json!({"configOptions": [
            {"id": "model", "category": "model", "type": "select", "currentValue": "gpt-6.1-sol",
             "options": [{"value": "gpt-6.1-sol", "name": "6.1 Sol"}, {"value": "gpt-6.1-mini", "name": "6.1 Mini"}]},
            {"id": "mode", "category": "mode", "type": "select", "currentValue": "auto",
             "options": [{"value": "read-only"}, {"value": "auto"}]},
            {"id": "reasoning_effort", "name": "Reasoning Effort", "category": "thought_level", "type": "select",
             "currentValue": "low", "options": [{"value": "low", "name": "Low"}, {"value": "high", "name": "High"}]},
            {"id": "fast-mode", "type": "select", "currentValue": "off", "options": [{"value": "on"}, {"value": "off"}]},
            {"id": "verbose", "type": "boolean"}
        ]});
        let result = model_catalog(&session, &Value::Null);
        assert_eq!(
            result["models"][0]["options"],
            json!([{"id": "reasoning_effort", "name": "Reasoning Effort", "default": "low", "category": "thought_level",
                    "values": [{"id": "low", "name": "Low"}, {"id": "high", "name": "High"}]}]),
            "only the agent's own setting, with its category: not the model, approvals or Fast mode"
        );
        // The session only shows the values for the model it runs; another model's may differ.
        assert_eq!(result["models"][1]["options"], json!([]));
    }

    #[test]
    fn model_catalog_reads_any_agents_session_settings_by_schema() {
        // No agent names or ids are special: another agent calls its effort `effort` and adds a setting of its own
        // category. A boolean (other than Fast mode) has no values to pick from, so it isn't offered.
        let session = json!({"configOptions": [
            {"id": "provider-model", "category": "model", "type": "select", "currentValue": "big",
             "options": [{"value": "big"}, {"value": "small"}]},
            {"id": "effort", "category": "thought_level", "type": "select", "currentValue": "medium",
             "options": [{"value": "minimal"}, {"value": "medium"}]},
            {"id": "verbosity", "name": "Verbosity", "category": "_custom", "type": "select",
             "options": [{"value": "terse", "name": "Terse"}]},
            {"id": "sandbox", "type": "boolean", "currentValue": true}
        ]});
        let result = model_catalog(&session, &Value::Null);
        let ids: Vec<&str> =
            result["models"][0]["options"].as_array().unwrap().iter().map(|o| o["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["effort", "verbosity"]);
        assert_eq!(result["models"][0]["options"][1]["default"], Value::Null, "no current value: no default claimed");
        // The category says which setting is the effort level, whatever its id.
        assert_eq!(result["models"][0]["options"][0]["category"], "thought_level");
        assert_eq!(result["models"][0]["options"][1]["category"], "_custom");
        assert_eq!(result["models"][1]["options"], json!([]));
    }

    #[test]
    fn model_catalog_offers_no_settings_when_the_agent_lists_none() {
        let session = json!({"configOptions": [{"id": "model", "category": "model", "type": "select",
            "currentValue": "a", "options": [{"value": "a"}]}]});
        assert_eq!(model_catalog(&session, &Value::Null)["models"][0]["options"], json!([]));
        let legacy = json!({"models": {"currentModelId": "a", "availableModels": [{"modelId": "a"}]}});
        assert_eq!(model_catalog(&legacy, &Value::Null)["models"][0]["options"], json!([]));
    }

    #[test]
    fn session_settings_keep_grouped_values() {
        // Reviewer DJ102-GROUPED-SETTING-VALUES: ACP select values may come in groups.
        let session = json!({"configOptions": [{"id": "effort", "category": "thought_level", "type": "select",
            "currentValue": "low", "options": [{"group": "fast", "name": "Fast", "options": [{"value": "low", "name": "Low"}]},
            {"group": "deep", "name": "Deep", "options": [{"value": "high", "name": "High"}]}]}]});
        assert_eq!(
            session_options(&session),
            json!([{"id": "effort", "name": "effort", "default": "low", "category": "thought_level",
                    "values": [{"id": "low", "name": "Low"}, {"id": "high", "name": "High"}]}])
            .as_array()
            .unwrap()
            .clone()
        );
    }

    #[test]
    fn model_config_prefers_category_and_ignores_unknown_types() {
        let session = json!({"configOptions": [
            {"id": "model", "type": "boolean"},
            {"id": "models", "type": "select"},
            {"id": "engine", "category": "model", "type": "select"}
        ]});
        assert_eq!(model_config(&session).unwrap()["id"], "engine");
        assert!(model_config(&json!({"configOptions": [{"id": "model", "type": "boolean"}]})).is_none());
    }

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
