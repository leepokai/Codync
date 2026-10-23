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
pub(crate) async fn select_model(acp: &Acp, res: &Value, sid: &str, cfg: &BotConfig) -> Result<Selected> {
    let model = cfg.model.as_deref().filter(|m| !m.is_empty());
    let mut current = res.clone();
    // Whether `current` lists the options of the model the session runs now.
    let mut fresh = true;
    // Models listed the older way (`models.availableModels`) are chosen with `session/set_model`; the settings follow.
    let legacy = model.filter(|_| crate::agent::auth::model_config(res).is_none());
    if let Some(model) = legacy {
        let r = acp.request("session/set_model", json!({"sessionId": sid, "modelId": model})).await;
        let answer = r.map_err(|e| model_error(model, &e))?;
        // Without a fresh list, what the session listed before describes the model it ran then.
        if answer["configOptions"].is_array() {
            current = answer;
        } else {
            fresh = false;
        }
    }
    let mut left_out = Vec::new();
    // Settings set since the session last listed its options: it didn't say what it set.
    let mut unconfirmed = Vec::new();
    // Cursor lists each model's options only through its own method; asked once, when needed (`None`: not asked).
    let mut listing: Option<Value> = None;
    // A model switch can be the first to list the settings, so plan from what the session lists now.
    let listed = current.clone();
    let steps = config_steps(&listed, model, &cfg.model_options);
    for (i, (id, value)) in steps.iter().copied().enumerate() {
        let is_model = model.is_some() && legacy.is_none() && i == 0;
        if !is_model {
            if cfg.backend == "cursor" && listing.is_none() && !(fresh && lists(&current, id)) {
                listing = Some(acp.request("cursor/list_available_models", json!({})).await.unwrap_or_default());
            }
            // The model the session itself says it runs, from its fresh list: never the one asked for, which the
            // agent may not have taken. No such report: no model's list vouches for anything.
            let running = if fresh { current_model(&current) } else { None };
            let listing = listing.as_ref().unwrap_or(&Value::Null);
            if let Some(offered) = unoffered(&current, fresh, listing, running, id, value) {
                left_out.push((id.to_owned(), value.to_owned(), offered));
                continue;
            }
        }
        let answer = match acp
            .request("session/set_config_option", json!({"sessionId": sid, "configId": id, "value": value}))
            .await
        {
            Ok(answer) => answer,
            Err(e) if is_model => return Err(model_error(model.unwrap_or_default(), &e)),
            Err(e) => {
                let error = match model {
                    Some(model) => model_error(&format!("{model} with {id}={value}"), &e),
                    None => option_error(id, value, &e),
                };
                let rejected = Some((id.to_owned(), value.to_owned(), error));
                return Ok(Selected { options: current, left_out, unconfirmed, model_unlisted: !fresh, rejected });
            }
        };
        if answer["configOptions"].is_array() {
            // A full list is the session's word again, whatever came before it.
            current = answer;
            fresh = true;
            unconfirmed.clear();
        } else if is_model {
            fresh = false;
        } else {
            unconfirmed.push(id.to_owned());
        }
    }
    Ok(Selected { options: current, left_out, unconfirmed, model_unlisted: !fresh, rejected: None })
}

/// What `select_model` did: the session's options as it last listed them, the settings it left out (id, value and
/// the notice saying why), the ones the agent took without listing what it set, whether a model change went
/// unlisted, and the setting the agent refused (id, value and the error the session start fails with).
#[derive(Default)]
pub(crate) struct Selected {
    pub options: Value,
    pub left_out: Vec<(String, String, String)>,
    pub unconfirmed: Vec<String>,
    pub model_unlisted: bool,
    pub rejected: Option<(String, String, anyhow::Error)>,
}

/// Whether the session lists select option `id`.
fn lists(session: &Value, id: &str) -> bool {
    session["configOptions"].as_array().into_iter().flatten().any(|o| o["id"] == id && o["type"] == "select")
}

/// The model the session runs, as it lists it.
fn current_model(session: &Value) -> Option<&str> {
    crate::agent::auth::model_config(session)
        .and_then(|c| c["currentValue"].as_str())
        .or_else(|| session["models"]["currentModelId"].as_str())
}

/// Why `value` can't be set, from the agent's own schema: the session's options or, for an option the session
/// doesn't list, Cursor's per-model `listing` for the `model` it runs. A setting no schema offers is never sent.
fn unoffered(
    session: &Value,
    fresh: bool,
    listing: &Value,
    model: Option<&str>,
    id: &str,
    value: &str,
) -> Option<String> {
    let select = |o: &&Value| o["id"] == id && o["type"] == "select";
    // A list from before a model change describes the old model.
    let in_session = session["configOptions"].as_array().filter(|_| fresh).into_iter().flatten().find(select);
    let per_model = || {
        let entry =
            listing["models"].as_array().into_iter().flatten().find(|m| model.is_some_and(|id| m["value"] == id));
        entry.and_then(|m| m["configOptions"].as_array()).into_iter().flatten().find(select)
    };
    let Some(option) = in_session.or_else(per_model) else {
        return Some(if fresh {
            format!("{id} {value} isn't offered by this agent, so it isn't set.")
        } else {
            format!("{id} {value} couldn't be confirmed for this model, so it isn't set.")
        });
    };
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
/// options. Without config options there is nothing to set them on; on Default the options need
/// no model selector (an agent may list its models the older way).
fn config_steps<'a>(
    res: &'a Value,
    model: Option<&'a str>,
    own: &'a BTreeMap<String, String>,
) -> Vec<(&'a str, &'a str)> {
    if !res["configOptions"].is_array() {
        return Vec::new();
    }
    match model {
        Some(model) => {
            let (model, options) = model_choice(res, model, own);
            // Without a model selector the model goes by `session/set_model` (see `select_model`).
            match crate::agent::auth::model_config(res).and_then(|c| c["id"].as_str()) {
                Some(config_id) => std::iter::once((config_id, model)).chain(options).collect(),
                None => options,
            }
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
/// Numbers agent processes (`Conn::process`) from 1.
static NEXT_PROCESS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

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
        process: NEXT_PROCESS.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
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
        assert_eq!(unoffered(&session, true, &Value::Null, None, "effort", "low"), None);
        assert_eq!(unoffered(&session, true, &Value::Null, None, "effort", "high"), None, "grouped values count");
        assert_eq!(
            unoffered(&session, true, &Value::Null, None, "effort", "max").as_deref(),
            Some("Effort max isn't offered for this model, so the agent uses its own setting.")
        );
        // Without a schema that lists it (see `a_saved_setting_no_schema_offers_is_never_sent`), nothing is sent.
        assert!(unoffered(&session, true, &Value::Null, None, "fast", "true").is_some());
    }

    #[test]
    fn default_gets_its_options_from_an_agent_without_a_model_selector() {
        // Reviewer DJ102-DEFAULT-OPTIONS-WITHOUT-MODEL-SELECTOR: models listed the older way, settings as config
        // options; ACP allows one without the other.
        let res = json!({"models": {"availableModels": [{"modelId": "m"}], "currentModelId": "m"},
            "configOptions": [{"id": "reasoning_effort", "type": "select", "currentValue": "low",
                "options": [{"value": "low"}, {"value": "high"}]}]});
        let own = [("reasoning_effort", "high")].map(|(k, v)| (k.to_owned(), v.to_owned())).into();
        assert_eq!(config_steps(&res, None, &own), vec![("reasoning_effort", "high")]);
    }

    #[test]
    fn a_saved_setting_no_schema_offers_is_never_sent() {
        let session = json!({"configOptions": [
            {"id": "model", "category": "model", "type": "select", "currentValue": "grok",
             "options": [{"value": "grok"}]},
            {"id": "effort", "type": "select", "options": [{"value": "low"}]},
        ]});
        // Not on the session and no per-model list: nothing says the agent has it.
        assert_eq!(
            unoffered(&session, true, &Value::Null, Some("grok"), "verbosity", "terse").as_deref(),
            Some("verbosity terse isn't offered by this agent, so it isn't set.")
        );
        // Cursor's own per-model list is the schema for its model options.
        let listing = json!({"models": [{"value": "grok", "configOptions": [
            {"id": "fast", "type": "select", "options": [{"value": "false"}, {"value": "true"}]}]}]});
        assert_eq!(unoffered(&session, true, &listing, Some("grok"), "fast", "true"), None);
        assert_eq!(
            unoffered(&session, true, &listing, Some("grok"), "fast", "maybe").as_deref(),
            Some("fast maybe isn't offered for this model, so the agent uses its own setting.")
        );
        // Another model's list doesn't vouch for this one.
        assert!(unoffered(&session, true, &listing, Some("auto"), "fast", "true").is_some());
    }

    #[test]
    fn after_a_model_change_without_a_fresh_list_the_old_list_vouches_for_nothing() {
        // The model setter answered without the new model's options: what the session listed before is the old
        // model's, and can't approve a value for the new one.
        let before = json!({"configOptions": [{"id": "effort", "type": "select", "options": [{"value": "high"}]}]});
        assert_eq!(
            unoffered(&before, false, &Value::Null, Some("mini"), "effort", "high").as_deref(),
            Some("effort high couldn't be confirmed for this model, so it isn't set.")
        );
        assert_eq!(unoffered(&before, true, &Value::Null, Some("mini"), "effort", "high"), None);
    }

    #[test]
    fn a_chosen_model_listed_the_older_way_still_gets_its_settings() {
        // Reviewer DJ103-LEGACY-EXPLICIT-OPTIONS: the model goes by `session/set_model`, the settings after it.
        let res = json!({"models": {"availableModels": [{"modelId": "m"}, {"modelId": "n"}], "currentModelId": "n"},
            "configOptions": [{"id": "reasoning_effort", "type": "select", "currentValue": "low",
                "options": [{"value": "low"}, {"value": "high"}]}]});
        let own = [("reasoning_effort", "high")].map(|(k, v)| (k.to_owned(), v.to_owned())).into();
        assert_eq!(config_steps(&res, Some("m"), &own), vec![("reasoning_effort", "high")]);
    }

    /// An ACP agent over stdio for `select_model`: answers `cursor/list_available_models` with A offering high and
    /// B only low, reports model `reported` after the model setter, and logs every setter.
    fn wire_agent(dir: &std::path::Path, reported: &str) -> String {
        let script = dir.join("agent.py");
        let body = r#"
import json, sys
REPORTED = sys.argv[1]
def send(v): print(json.dumps({"jsonrpc": "2.0", **v}), flush=True)
for line in sys.stdin:
    m = json.loads(line); method, rid, p = m.get("method"), m.get("id"), m.get("params", {})
    if rid is None: continue
    if method == "initialize": send({"id": rid, "result": {"protocolVersion": 1}})
    elif method == "cursor/list_available_models":
        send({"id": rid, "result": {"models": [
            {"value": "A", "configOptions": [{"id": "effort", "type": "select", "options": [{"value": "low"}, {"value": "high"}]}]},
            {"value": "B", "configOptions": [{"id": "effort", "type": "select", "options": [{"value": "low"}]}]}]}})
    elif method == "session/set_config_option":
        open("calls.log", "a").write(p["configId"] + "=" + p["value"] + "\n")
        model = [] if REPORTED == "-" else [{"id": "model", "category": "model", "type": "select",
            "currentValue": REPORTED, "options": [{"value": "A"}, {"value": "B"}]}]
        send({"id": rid, "result": {"configOptions": model}})
    else: send({"id": rid, "result": {}})
"#;
        std::fs::write(&script, body).unwrap();
        format!("python3 {} {reported}", script.display())
    }

    async fn selected_with_reported_model(reported: &str) -> (Selected, String) {
        let dir = std::env::temp_dir().join(format!("codync-select-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = start_agent(&wire_agent(&dir, reported), dir.to_str().unwrap(), &[], Duration::from_secs(20))
            .await
            .unwrap();
        let res = json!({"sessionId": "s1", "configOptions": [{"id": "model", "category": "model", "type": "select",
            "currentValue": "B", "options": [{"value": "A"}, {"value": "B"}]}]});
        let cfg: BotConfig = serde_json::from_value(json!({"id": "b", "name": "B", "backend": "cursor",
            "model": "A", "modelOptions": {"effort": "high"}}))
        .unwrap();
        let selected = select_model(&conn.acp, &res, "s1", &cfg).await.unwrap();
        conn.acp.kill().await;
        let calls = std::fs::read_to_string(dir.join("calls.log")).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        (selected, calls)
    }

    #[tokio::test]
    async fn the_model_the_session_reports_decides_its_settings_not_the_one_asked_for() {
        // Reviewer DJ103-REQUESTED-MODEL-AS-CAPABILITY: asked for A, the session says it runs B, where high isn't
        // offered; A's list must not vouch for it.
        let (selected, calls) = selected_with_reported_model("B").await;
        assert_eq!(calls, "model=A\n", "high never reaches B");
        assert_eq!(selected.left_out.len(), 1);
        // A report without the model's identity confirms nothing either.
        let (_, calls) = selected_with_reported_model("-").await;
        assert_eq!(calls, "model=A\n");
        // When the session does run A, A's own list allows it.
        let (_, calls) = selected_with_reported_model("A").await;
        assert_eq!(calls, "model=A\neffort=high\n");
    }

    #[tokio::test]
    async fn settings_the_model_switch_first_lists_are_applied() {
        // Reviewer: models listed the older way, no settings until `session/set_model` answers with them.
        let dir = std::env::temp_dir().join(format!("codync-select-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("agent.py");
        std::fs::write(&script, r#"
import json, sys
def send(v): print(json.dumps({"jsonrpc": "2.0", **v}), flush=True)
for line in sys.stdin:
    m = json.loads(line); method, rid, p = m.get("method"), m.get("id"), m.get("params", {})
    if rid is None: continue
    if method == "initialize": send({"id": rid, "result": {"protocolVersion": 1}})
    elif method == "session/set_model":
        open("calls.log", "a").write("model=" + p["modelId"] + "\n")
        send({"id": rid, "result": {"models": {"currentModelId": p["modelId"]}, "configOptions": [
            {"id": "effort", "type": "select", "currentValue": "low", "options": [{"value": "low"}, {"value": "high"}]}]}})
    elif method == "session/set_config_option":
        open("calls.log", "a").write(p["configId"] + "=" + p["value"] + "\n")
        send({"id": rid, "result": {}})
    else: send({"id": rid, "result": {}})
"#).unwrap();
        let conn =
            start_agent(&format!("python3 {}", script.display()), dir.to_str().unwrap(), &[], Duration::from_secs(20))
                .await
                .unwrap();
        let res = json!({"sessionId": "s1", "models": {"availableModels": [{"modelId": "m"}, {"modelId": "n"}],
            "currentModelId": "n"}});
        let cfg: BotConfig = serde_json::from_value(json!({"id": "b", "name": "B", "backend": "other",
            "model": "m", "modelOptions": {"effort": "high"}}))
        .unwrap();
        select_model(&conn.acp, &res, "s1", &cfg).await.unwrap();
        conn.acp.kill().await;
        let calls = std::fs::read_to_string(dir.join("calls.log")).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(calls, "model=m\neffort=high\n");
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
