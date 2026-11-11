//! The agent process: which commands start it, the ACP handshake, the bot's model, and how its instructions are
//! attached to a session request.

use super::{Adapter, Conn, SessionCaps};
use crate::agent::acp::{self, Acp};
use crate::agent::registry::{Candidate, Cmd};
use crate::store::BotConfig;
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

/// ACP's `systemPrompt`: the bot's instructions added to the agent's own.
pub(crate) fn system_prompt(system: &str) -> Value {
    json!({"mode": "append", "content": [{"type": "text", "text": system}]})
}

/// Claude's `_meta`: the bot's instructions appended to Claude Code's own system prompt, and its options. The
/// per-session parts of Claude Code's prompt (working directory, git status) move to the first message, so a
/// bot's chat, groups and routines share one cached system prompt.
pub(super) fn claude_meta(cfg: &BotConfig, system: &str) -> Value {
    json!({"systemPrompt": {"append": system, "excludeDynamicSections": true}, "claudeCode": claude_options(cfg)})
}

/// Claude Code's own memory and its auto-dream stay off, so Codync's is the bot's one memory, unless the user kept
/// them on; then Claude Code sets both itself ([design](../../../../docs/design/bot-memory.md#claude-codes-own-memory)).
/// Claude's thinking shows as summaries, which the working steps a chat unfolds show: an SDK session otherwise
/// leaves its text out. Whether Claude thinks stays the user's Claude Code setting.
fn claude_options(cfg: &BotConfig) -> Value {
    // The bot's replies, memory and team management are Codync's own tools, which follow the bot's permission inside
    // Codync: the user's Claude Code auto-mode classifier mustn't refuse them (it blocked a reply naming the user's
    // daughter), and a team change asks on Codync's own card.
    let mut settings = json!({"permissions": {"allow": ["mcp__chat", "mcp__memory", "mcp__manage"]}});
    if !crate::agent::own_memory::kept(cfg) {
        settings["autoMemoryEnabled"] = false.into();
        settings["autoDreamEnabled"] = false.into();
    }
    json!({"options": {"settings": settings, "extraArgs": {"thinking-display": "summarized"}}})
}

/// Ways to start the bot's agent over ACP, best first: its own command, else its backend's.
pub(crate) fn launch_commands(cfg: &BotConfig) -> Result<Vec<Candidate>> {
    Ok(match cfg.command.as_deref().map(str::trim) {
        Some(c) if !c.is_empty() => vec![Candidate::Ready(Cmd { program: c.to_owned(), args: String::new() })],
        _ => crate::agent::backends::launch_candidates(&cfg.backend)?,
    })
}

/// Switches a session (`session/new`, `session/load` or `session/resume` answered `res`) to the bot's
/// chosen model, then sets that model's options (effort, fast…). Returns the answer holding
/// the session's config options now: a model can change them.
pub(crate) async fn select_model(acp: &Acp, res: &Value, sid: &str, cfg: &BotConfig) -> Result<Value> {
    let selected = cfg.model.as_deref().filter(|m| !m.is_empty());
    let (model, options) = model_choice(res, selected.unwrap_or("Default"), &cfg.model_options);
    let set = |id: &str, value: &str| {
        acp.request("session/set_config_option", json!({"sessionId": sid, "configId": id, "value": value}))
    };
    let mut current = res.clone();
    if selected.is_some() {
        if let Some(config_id) = crate::agent::auth::model_config(res).and_then(|c| c["id"].as_str()) {
            let answer = set(config_id, model).await.map_err(|e| model_error(model, &e))?;
            if answer["configOptions"].is_array() {
                current = answer;
            }
        } else {
            acp.request("session/set_model", json!({"sessionId": sid, "modelId": model}))
                .await
                .map_err(|e| model_error(model, &e))?;
        }
    }
    for (id, value) in options {
        let answer = set(id, value).await.map_err(|e| model_error(&format!("{model} with {id}={value}"), &e))?;
        if answer["configOptions"].is_array() {
            current = answer;
        }
    }
    // The older `modes` field comes only with the session (`agent::modes`).
    if current.get("modes").is_none()
        && let Some(modes) = res.get("modes")
    {
        current["modes"] = modes.clone();
    }
    Ok(current)
}

/// The model to select and the options to set on it: the model id's variant, then the bot's own. How the agent
/// approves tools is the bot's Approvals, never a model option (`opus[mode=acceptEdits]`).
fn model_choice<'a>(
    res: &Value,
    model: &'a str,
    own: &'a BTreeMap<String, String>,
) -> (&'a str, Vec<(&'a str, &'a str)>) {
    let (model, mut options) = split_variant(res, model);
    options.extend(own.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    let mode = res["configOptions"].as_array().into_iter().flatten().find(|o| o["category"] == "mode");
    options.retain(|(id, _)| mode.is_none_or(|m| m["id"] != *id));
    (model, options)
}

fn model_error(model: &str, e: &anyhow::Error) -> anyhow::Error {
    tracing::warn!(model, error = format!("{e:#}"), "couldn't set model");
    acp::Failure::error(acp::FailureKind::NoModel, format!("Couldn't select model {model}: {e:#}"))
}

/// Cursor names a model variant `grok-4.7[reasoning_effort=high,fast=true]`. When the agent
/// lists plain models instead, the bracket becomes options of the plain model.
fn split_variant<'a>(res: &Value, model: &'a str) -> (&'a str, Vec<(&'a str, &'a str)>) {
    let listed = |id: &str| crate::agent::auth::model_ids(res).iter().any(|m| m == id);
    if let Some((name, params)) = model.strip_suffix(']').and_then(|m| m.split_once('['))
        && let Some(params) =
            params.split(',').filter(|p| !p.is_empty()).map(|p| p.split_once('=')).collect::<Option<Vec<_>>>()
        && !listed(model)
        && listed(name)
    {
        return (name, params);
    }
    (model, Vec::new())
}

/// Starts the first of `candidates` (best first) that `start` gets running; `start` gets each with its environment
/// and whether another is left after it. An OpenCode older than 2 never ends a turn its model refused
/// ([OpenCode](../../../../docs/upstream-agents/opencode.md)): it runs only when nothing after it starts, and the
/// registry build standing in for it keeps a database of its own, since on the user's its first start would migrate
/// their 1.x sessions one way.
pub(crate) async fn start_first<F: Future<Output = Result<Conn>>>(
    candidates: Vec<Candidate>,
    env: &[(String, String)],
    mut start: impl FnMut(Candidate, Vec<(String, String)>, bool) -> F,
) -> Result<Conn> {
    let mut env = env.to_vec();
    let (mut last_err, mut superseded) = (None, None::<Conn>);
    let count = candidates.len();
    for (i, candidate) in candidates.into_iter().enumerate() {
        let fallback_left = i + 1 < count;
        match start(candidate, env.clone(), fallback_left).await {
            Ok(conn) if conn.superseded && fallback_left => {
                let dir = crate::service::data_dir().join("opencode");
                if let Err(e) = tokio::fs::create_dir_all(&dir).await {
                    tracing::warn!(
                        error = format!("{e:#}"),
                        "OpenCode 1.x: no folder for the registry build's database"
                    );
                    return Ok(conn);
                }
                tracing::info!("OpenCode 1.x: starting the registry build");
                env.push(("OPENCODE_DB".into(), dir.join("opencode.db").to_string_lossy().into_owned()));
                superseded = Some(conn);
            }
            Ok(conn) => {
                if let Some(old) = superseded {
                    old.acp.kill().await;
                }
                return Ok(conn);
            }
            Err(e) => last_err = Some(e),
        }
    }
    if let Some(conn) = superseded {
        let error = last_err.map(|e| format!("{e:#}")).unwrap_or_default();
        tracing::warn!(error, "OpenCode 1.x: the registry build didn't start, so the installed one runs");
        return Ok(conn);
    }
    Err(last_err.unwrap_or_else(|| anyhow!("agent failed to start")))
}

/// Spawns an ACP agent and completes the `initialize` handshake.
pub(crate) async fn start_agent(command: &str, cwd: &str, env: &[(String, String)], budget: Duration) -> Result<Conn> {
    let (acp, rx) = Acp::spawn(command, cwd, env)?;
    let init = tokio::time::timeout(
        budget,
        acp.request(
            "initialize",
            json!({
                "protocolVersion": 1,
                // `session.compaction`: tell us when the agent summarizes its context.
                // `parameterizedModelPicker`: Cursor lists plain models with their options
                // (effort, fast…) instead of one fixed variant each; model discovery asks the same.
                "clientCapabilities": {
                    "fs": {"readTextFile": false, "writeTextFile": false},
                    "terminal": false,
                    "session": {"compaction": {}},
                    "_meta": {"parameterizedModelPicker": true},
                },
                "clientInfo": {"name": "codync", "title": "Codync", "version": env!("CARGO_PKG_VERSION")},
            }),
        ),
    )
    .await;
    let init = match init {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            acp.kill().await;
            return Err(e);
        }
        Err(_) => {
            acp.kill().await;
            let message = format!("the agent didn't answer the ACP handshake within {}s", budget.as_secs());
            return Err(acp::Failure::error(acp::FailureKind::Timeout, message));
        }
    };
    let reopen = if init["agentCapabilities"]["loadSession"].as_bool().unwrap_or(false) {
        Some("session/load")
    } else {
        init["agentCapabilities"]["sessionCapabilities"]["resume"].is_object().then_some("session/resume")
    };
    let sessions = SessionCaps {
        reopen,
        close: init["agentCapabilities"]["sessionCapabilities"]["close"].is_object(),
        system_prompt: init["agentCapabilities"]["sessionCapabilities"]["systemPrompt"].is_object(),
        additional_directories: init["agentCapabilities"]["sessionCapabilities"]["additionalDirectories"].is_object(),
        // Claude's, Codex's and omp's adapters say so in the top-level `_meta`.
        steering: init["_meta"]["steering"]["supported"] == true,
    };
    Ok(Conn {
        acp,
        rx,
        sessions,
        adapter: adapter(&init),
        superseded: superseded(&init),
        loaded: HashMap::new(),
        spent: false,
        started: Instant::now(),
        opening: false,
        fast_options: HashMap::new(),
        modes: HashMap::new(),
        mode_set: HashSet::new(),
        reports_compaction: false,
        usage: HashMap::new(),
        renamed: HashMap::new(),
    })
}

fn adapter(init: &Value) -> Adapter {
    if init["agentCapabilities"]["_meta"]["claudeCode"].is_object() {
        Adapter::Claude
    } else if init["agentInfo"]["name"].as_str().is_some_and(|name| name.ends_with("codex-acp")) {
        Adapter::Codex
    } else {
        Adapter::Other
    }
}

/// OpenCode 1.x retries a model call its provider refused forever (a used-up free quota, a bad key), so the turn
/// never ends and the chat stays on "Thinking…" (anomalyco/opencode#41848); 2.x returns the error at once.
fn superseded(init: &Value) -> bool {
    init["agentInfo"]["name"] == "OpenCode"
        && init["agentInfo"]["version"]
            .as_str()
            .and_then(|v| semver::Version::parse(v).ok())
            .is_some_and(|v| v.major < 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_opencode_before_2_is_superseded() {
        let init = |name: &str, version: &str| json!({"agentInfo": {"name": name, "version": version}});
        assert!(superseded(&init("OpenCode", "1.19.5")));
        assert!(!superseded(&init("OpenCode", "2.2.6")));
        assert!(!superseded(&init("OpenCode", "dev")));
        assert!(!superseded(&init("Kilo", "1.0.0")));
        assert!(!superseded(&json!({})));
    }

    /// Starts each candidate as an agent answering `initialize` as the OpenCode version its command names ("broken"
    /// doesn't start). Returns whether the agent that runs is superseded, and whether each start got its own database.
    #[cfg(unix)]
    async fn first_of(versions: &[&str]) -> (bool, Vec<bool>) {
        const AGENT: &str = r#"read l; id=$(echo "$l" | sed 's/.*"id":\([0-9]*\).*/\1/'); printf '{"jsonrpc":"2.0","id":%s,"result":{"agentInfo":{"name":"OpenCode","version":"%s"}}}\n' "$id" "$0""#;
        let candidates =
            versions.iter().map(|v| Candidate::Ready(Cmd { program: (*v).to_owned(), args: String::new() })).collect();
        let mut own_db = Vec::new();
        let conn = start_first(candidates, &[], |candidate, env, _| {
            own_db.push(env.iter().any(|(k, _)| k == "OPENCODE_DB"));
            async move {
                let Candidate::Ready(Cmd { program: version, .. }) = candidate else { unreachable!() };
                anyhow::ensure!(version != "broken", "didn't start");
                let command = format!("sh -c '{}' {version}", AGENT.replace('\'', "'\\''"));
                start_agent(&command, "/tmp", &env, Duration::from_secs(10)).await
            }
        })
        .await
        .unwrap();
        (conn.superseded, own_db)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_opencode_before_2_runs_only_when_nothing_after_it_starts() {
        assert_eq!(
            first_of(&["1.19.5", "2.2.6"]).await,
            (false, vec![false, true]),
            "the registry build, on its own database"
        );
        assert!(first_of(&["1.19.5", "broken"]).await.0, "the registry build didn't start");
        assert!(first_of(&["1.19.5"]).await.0, "a custom command, or no registry build");
    }

    #[test]
    fn variant_ids_become_options_only_when_the_agent_lists_plain_models() {
        let plain = json!({"configOptions": [{"id": "model", "category": "model", "type": "select",
            "options": [{"value": "grok-4.7"}]}]});
        let variant = "grok-4.7[context=256k,reasoning_effort=medium,fast=false]";
        assert_eq!(
            split_variant(&plain, variant),
            ("grok-4.7", vec![("context", "256k"), ("reasoning_effort", "medium"), ("fast", "false")])
        );
        assert_eq!(split_variant(&plain, "grok-4.7"), ("grok-4.7", vec![]));
        let variants = json!({"configOptions": [{"id": "model", "category": "model", "type": "select",
            "options": [{"value": variant}]}]});
        assert_eq!(split_variant(&variants, variant), (variant, vec![]));
        assert_eq!(split_variant(&plain, "other[x=1]"), ("other[x=1]", vec![]));
        // Claude's `opus[1m]` is a model id of its own, not a variant.
        assert_eq!(split_variant(&json!({"models": {"availableModels": [{"modelId": "opus"}]}}), "opus[1m]").1, vec![]);
        // Cursor's Auto: `default[]` with variants, `default` without.
        let auto = json!({"models": {"availableModels": [{"modelId": "default"}]}});
        assert_eq!(split_variant(&auto, "default[]"), ("default", vec![]));
    }

    #[test]
    fn no_model_option_sets_how_the_agent_approves_tools() {
        let res = json!({"configOptions": [
            {"id": "model", "category": "model", "type": "select", "options": [{"value": "opus"}]},
            {"id": "mode", "category": "mode", "type": "select", "options": [{"value": "default"}, {"value": "auto"}]},
        ]});
        let own = [("mode", "auto"), ("effort", "high")].map(|(k, v)| (k.to_owned(), v.to_owned())).into();
        assert_eq!(model_choice(&res, "opus[mode=acceptEdits]", &own), ("opus", vec![("effort", "high")]));
    }

    #[test]
    fn claude_codes_memory_is_off_in_any_folder_unless_the_user_kept_it() {
        let bot = |kept: Value| -> BotConfig {
            serde_json::from_value(
                json!({"id": "a", "name": "B", "backend": "claude", "cwd": "/work/app", "harnessMemory": kept}),
            )
            .unwrap()
        };
        let settings = |cfg: &BotConfig| claude_options(cfg)["options"]["settings"].clone();
        // `null`: a client sending back a bot that had no switch.
        for off in [json!(false), Value::Null] {
            let off = settings(&bot(off));
            assert_eq!(off["autoMemoryEnabled"], false, "a project folder too");
            assert_eq!(off["autoDreamEnabled"], false);
        }
        let kept = settings(&bot(json!(true)));
        assert!(
            kept.get("autoMemoryEnabled").is_none() && kept.get("autoDreamEnabled").is_none(),
            "Claude Code decides"
        );
    }

    #[test]
    fn claude_streams_thinking_summaries() {
        let cfg: BotConfig =
            serde_json::from_value(json!({"id": "a", "name": "B", "backend": "claude", "cwd": "/work/app"})).unwrap();
        let options = &claude_options(&cfg)["options"];
        assert_eq!(options["extraArgs"]["thinking-display"], "summarized");
        assert!(options.get("thinking").is_none(), "whether Claude thinks stays Claude Code's");
    }
}
