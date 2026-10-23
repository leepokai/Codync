//! The agent process: which commands start it, the ACP handshake, the bot's model, and how its instructions are
//! attached to a session request.

use super::{Adapter, Conn, SessionCaps};
use crate::agent::acp::{self, Acp};
use crate::store::BotConfig;
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
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
fn claude_options(cfg: &BotConfig) -> Value {
    // The bot's replies, memory and team management are Codync's own tools, which follow the bot's permission inside
    // Codync: the user's Claude Code auto-mode classifier mustn't refuse them (it blocked a reply naming the user's
    // daughter), and a team change asks on Codync's own card.
    let mut settings = json!({"permissions": {"allow": ["mcp__chat", "mcp__memory", "mcp__manage"]}});
    if !crate::agent::own_memory::kept(cfg) {
        settings["autoMemoryEnabled"] = false.into();
        settings["autoDreamEnabled"] = false.into();
    }
    json!({"options": {"settings": settings}})
}

/// Shell commands that may start the bot's agent over ACP, best first.
/// `progress` receives short status lines ("Downloading Cursor…").
pub(crate) async fn launch_commands(cfg: &BotConfig, progress: impl Fn(&str)) -> Result<Vec<String>> {
    Ok(match cfg.command.as_deref().map(str::trim) {
        Some(c) if !c.is_empty() => vec![c.to_owned()],
        _ => crate::agent::backends::launch_candidates(&cfg.backend, progress)
            .await?
            .iter()
            .map(crate::agent::registry::Cmd::acp)
            .collect(),
    })
}

/// Switches a session (`session/new` or `session/load` answered `res`) to the bot's
/// chosen model, then sets that model's options (effort, fast…). A bot on the agent's default
/// model gets its options too. Returns the answer holding the session's config options now (a
/// model can change them), and what it left out: a saved value the session lists other values
/// for, as when the agent's offer changed since it was saved, is never sent.
pub(crate) async fn select_model(acp: &Acp, res: &Value, sid: &str, cfg: &BotConfig) -> Result<(Value, Vec<String>)> {
    let model = cfg.model.as_deref().filter(|m| !m.is_empty());
    if let Some(model) = model
        && crate::agent::auth::model_config(res).is_none()
    {
        let r = acp.request("session/set_model", json!({"sessionId": sid, "modelId": model})).await;
        return r.map(|_| (res.clone(), Vec::new())).map_err(|e| model_error(model, &e));
    }
    let mut current = res.clone();
    let mut left_out = Vec::new();
    for (i, (id, value)) in config_steps(res, model, &cfg.model_options).into_iter().enumerate() {
        let is_model = model.is_some() && i == 0;
        if !is_model && let Some(offered) = unoffered(&current, id, value) {
            left_out.push(offered);
            continue;
        }
        let answer = acp
            .request("session/set_config_option", json!({"sessionId": sid, "configId": id, "value": value}))
            .await
            .map_err(|e| match model {
                Some(model) if i == 0 => model_error(model, &e),
                Some(model) => model_error(&format!("{model} with {id}={value}"), &e),
                None => option_error(id, value, &e),
            })?;
        if answer["configOptions"].is_array() {
            current = answer;
        }
    }
    Ok((current, left_out))
}

/// Why `value` can't be set: the session lists option `id` without it. An option the session doesn't list goes to
/// the agent as saved (Cursor's own model options arrive that way), and the agent decides.
fn unoffered(session: &Value, id: &str, value: &str) -> Option<String> {
    let options = session["configOptions"].as_array()?;
    let option = options.iter().find(|o| o["id"] == id && o["type"] == "select")?;
    // Values may come in groups, as model lists do.
    let listed = option["options"].as_array().into_iter().flatten();
    let values: Vec<&str> = listed
        .flat_map(|v| std::iter::once(v).chain(v["options"].as_array().into_iter().flatten()))
        .filter_map(|v| v["value"].as_str())
        .collect();
    if values.contains(&value) {
        return None;
    }
    let name = option["name"].as_str().unwrap_or(id);
    Some(format!("{name} {value} isn't offered for this model, so the agent uses its own setting."))
}

/// The `session/set_config_option` calls, in order: the model when the bot picked one, then its
/// options. Without config options there is nothing to set them on.
fn config_steps<'a>(
    res: &'a Value,
    model: Option<&'a str>,
    own: &'a BTreeMap<String, String>,
) -> Vec<(&'a str, &'a str)> {
    let Some(config_id) = crate::agent::auth::model_config(res).and_then(|c| c["id"].as_str()) else {
        return Vec::new();
    };
    match model {
        Some(model) => {
            let (model, options) = model_choice(res, model, own);
            std::iter::once((config_id, model)).chain(options).collect()
        }
        None => own_options(res, own.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()),
    }
}

/// The model to select and the options to set on it: the model id's variant, then the bot's own.
fn model_choice<'a>(
    res: &Value,
    model: &'a str,
    own: &'a BTreeMap<String, String>,
) -> (&'a str, Vec<(&'a str, &'a str)>) {
    let (model, mut options) = split_variant(res, model);
    options.extend(own.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    (model, own_options(res, options))
}

/// How the agent approves tools is the bot's Approvals, never a model option (`opus[mode=acceptEdits]`).
fn own_options<'a>(res: &Value, mut options: Vec<(&'a str, &'a str)>) -> Vec<(&'a str, &'a str)> {
    let mode = res["configOptions"].as_array().into_iter().flatten().find(|o| o["category"] == "mode");
    options.retain(|(id, _)| mode.is_none_or(|m| m["id"] != *id));
    options
}

fn option_error(id: &str, value: &str, e: &anyhow::Error) -> anyhow::Error {
    tracing::warn!(id, value, error = format!("{e:#}"), "couldn't set model option");
    acp::Failure::error(acp::FailureKind::NoModel, format!("Couldn't set {id} to {value}: {e:#}"))
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
    let sessions = SessionCaps {
        load: init["agentCapabilities"]["loadSession"].as_bool().unwrap_or(false),
        close: init["agentCapabilities"]["sessionCapabilities"]["close"].is_object(),
        system_prompt: init["agentCapabilities"]["sessionCapabilities"]["systemPrompt"].is_object(),
        additional_directories: init["agentCapabilities"]["sessionCapabilities"]["additionalDirectories"].is_object(),
    };
    Ok(Conn {
        acp,
        rx,
        sessions,
        adapter: adapter(&init),
        loaded: HashMap::new(),
        started: Instant::now(),
        fast_options: HashMap::new(),
        reports_compaction: false,
        usage: HashMap::new(),
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_bot_on_the_default_model_still_gets_its_own_options() {
        let res = json!({"configOptions": [
            {"id": "model", "category": "model", "type": "select", "options": [{"value": "gpt-6.1-sol"}]},
            {"id": "mode", "category": "mode", "type": "select", "options": [{"value": "auto"}]},
            {"id": "reasoning_effort", "category": "thought_level", "type": "select", "options": [{"value": "high"}]},
        ]});
        let own = [("mode", "auto"), ("reasoning_effort", "high")].map(|(k, v)| (k.to_owned(), v.to_owned())).into();
        // No model to pick: the agent keeps its default and only the bot's settings are set.
        assert_eq!(config_steps(&res, None, &own), vec![("reasoning_effort", "high")]);
        // With a model, the model comes first.
        assert_eq!(
            config_steps(&res, Some("gpt-6.1-sol"), &own),
            vec![("model", "gpt-6.1-sol"), ("reasoning_effort", "high")]
        );
        // An agent without config options has nothing to set them on.
        assert!(config_steps(&json!({}), None, &own).is_empty());
    }

    #[test]
    fn only_a_value_the_session_lists_for_its_option_is_left_out() {
        let session = json!({"configOptions": [
            {"id": "effort", "name": "Effort", "type": "select", "options": [{"value": "low"}, {"group": "more",
             "options": [{"value": "high"}]}]},
        ]});
        assert_eq!(unoffered(&session, "effort", "low"), None);
        assert_eq!(unoffered(&session, "effort", "high"), None, "grouped values count");
        assert_eq!(
            unoffered(&session, "effort", "max").as_deref(),
            Some("Effort max isn't offered for this model, so the agent uses its own setting.")
        );
        // Cursor lists its model options elsewhere: an option the session doesn't list is the agent's to judge.
        assert_eq!(unoffered(&session, "fast", "true"), None);
        assert_eq!(unoffered(&json!({}), "effort", "max"), None);
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
}
