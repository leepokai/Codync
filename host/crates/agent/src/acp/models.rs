//! The models an agent offers in a session, with each model's reasoning settings, read from what the session
//! reports (`configOptions`, or the older `models`). Cursor lists each model's options only through its own
//! `cursor/list_available_models`.

use super::{Acp, text};
use anyhow::{Result, bail};
use serde_json::{Value, json};

/// The catalog of `backend`'s models in `session` (a `session/new` answer), for an app's model settings:
/// `{models: [{id, name, description, options}], currentModelId, defaultModelOptions, fastMode, modes}`. Settings
/// that depend on the model are read by switching a session meant for this alone to each model in turn: never a
/// session that does work.
pub async fn catalog(acp: &Acp, session: &Value, backend: &str) -> Result<Value> {
    let options = if backend == "cursor" {
        acp.request("cursor/list_available_models", json!({})).await.unwrap_or_default()
    } else {
        Value::Null
    };
    let mut catalog = model_catalog(session, &options);
    catalog["modes"] = crate::modes::Modes::from_session(session, backend).map_or(json!([]), |m| m.catalog());
    if backend != "cursor" {
        discover_options(acp, session, &mut catalog).await?;
    }
    Ok(catalog)
}

/// Probe model-dependent settings in the disposable discovery session, never in a bot's session.
async fn discover_options(acp: &Acp, session: &Value, catalog: &mut Value) -> Result<()> {
    let Some(sid) = session["sessionId"].as_str() else { return Ok(()) };
    let Some(config) = model_config(session).and_then(|c| c["id"].as_str()) else { return Ok(()) };
    let current = catalog["currentModelId"].clone();
    let Some(models) = catalog["models"].as_array_mut() else { return Ok(()) };
    for model in models {
        let response = if model["id"] == current {
            session.clone()
        } else {
            match acp
                .request(
                    "session/set_config_option",
                    json!({"sessionId": sid,
                "configId": config, "value": model["id"]}),
                )
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    model["options"] = json!([]);
                    model["optionsError"] = format!("Couldn't load model settings: {error}").into();
                    continue;
                }
            }
        };
        model["options"] = json!(reasoning_options(&response));
    }
    Ok(())
}

/// Only reasoning belongs here: permission/mode and Fast mode have their own controls.
fn reasoning_options(session: &Value) -> Vec<Value> {
    session["configOptions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| {
            o["category"] == "thought_level"
                || (o["category"] != "mode"
                    && matches!(
                        o["id"].as_str(),
                        Some("reasoning_effort" | "reasoning-effort" | "thought_level" | "effort")
                    ))
        })
        .filter_map(parse_select_option)
        .map(|mut option| {
            option["kind"] = "reasoning".into();
            // Inherit/reset has one dedicated control in every client, not a second effort level.
            if let Some(values) = option["values"].as_array_mut() {
                values.retain(|v| v["id"] != "default");
            }
            option
        })
        .collect()
}

/// IDs are agent-defined: use the category, with legacy conventional IDs as a fallback.
pub fn model_config(session: &Value) -> Option<&Value> {
    let options = session["configOptions"].as_array()?;
    let is_select = |v: &&Value| v["type"] == "select" || v.get("type").is_none();
    options
        .iter()
        .filter(is_select)
        .find(|v| v["category"] == "model")
        .or_else(|| options.iter().filter(is_select).find(|v| v["id"] == "model" || v["id"] == "models"))
}

/// The model ids the agent lists, in its order.
pub fn model_ids(session: &Value) -> Vec<String> {
    let catalog = model_catalog(session, &Value::Null);
    catalog["models"].as_array().into_iter().flatten().filter_map(|m| text(&m["id"])).collect()
}

/// `{models: [{id, name, description, options}], currentModelId, fastMode}`; `options` is
/// Cursor's `cursor/list_available_models` answer (`Null` for other agents).
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
    let initial_options = reasoning_options(session);
    for model in &mut models {
        let own = model_options(options, model["id"].as_str().unwrap_or_default());
        model["options"] = json!(if options.is_null() { initial_options.clone() } else { own });
    }
    json!({"models": models, "currentModelId": current,
        "defaultModelOptions": true,
        "fastMode": crate::config::fast_mode_config(session).is_some()})
}

/// One model's options as `[{id, name, values: [{id, name}], default}]`, from Cursor's
/// `{models: [{value, configOptions}]}` (ACP select config options).
fn model_options(options: &Value, model: &str) -> Vec<Value> {
    let entry = options["models"].as_array().into_iter().flatten().find(|m| m["value"] == model);
    entry.and_then(|m| m["configOptions"].as_array()).into_iter().flatten().filter_map(parse_select_option).collect()
}

/// Cursor names a model variant `grok-4.7[reasoning_effort=high,fast=true]`: the model and its settings.
pub fn cursor_variant(model: &str) -> Option<(&str, Vec<(&str, &str)>)> {
    let (name, params) = model.strip_suffix(']')?.split_once('[')?;
    let params = params.split(',').filter(|p| !p.is_empty()).map(|p| p.split_once('=')).collect::<Option<_>>()?;
    Some((name, params))
}

/// Checks that `options` (option id → value) are ones `model` (none: the agent's current default) lists in `catalog`
/// ([`models`]), as a client's model settings offer them: only reasoning and Cursor's per-model options, never a mode.
pub fn check_options<'a>(
    catalog: &Value,
    model: Option<&str>,
    options: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<()> {
    let model = model.or_else(|| catalog["currentModelId"].as_str()).unwrap_or_default();
    // A saved Cursor model carries its settings; the list names the bare model.
    let bare = cursor_variant(model).map_or(model, |(name, _)| name);
    let models = catalog["models"].as_array().map_or(&[][..], Vec::as_slice);
    let entry = models.iter().find(|m| m["id"] == model).or_else(|| models.iter().find(|m| m["id"] == bare));
    let Some(entry) = entry else {
        bail!("model {model} isn't in the agent's model list, so its settings can't be checked")
    };
    let listed = entry["options"].as_array().map_or(&[][..], Vec::as_slice);
    for (id, value) in options {
        let Some(option) = listed.iter().find(|o| o["id"] == id) else {
            let ids: Vec<&str> = listed.iter().filter_map(|o| o["id"].as_str()).collect();
            bail!(
                "model {model} has no setting {id}; it has: {}",
                if ids.is_empty() { "none".into() } else { ids.join(", ") }
            );
        };
        let values: Vec<&str> =
            option["values"].as_array().into_iter().flatten().filter_map(|v| v["id"].as_str()).collect();
        if !values.contains(&value) {
            bail!("{id} must be one of: {}", values.join(", "));
        }
    }
    Ok(())
}

fn parse_select_option(option: &Value) -> Option<Value> {
    if option["type"] != "select" {
        return None;
    }
    let id = text(&option["id"])?;
    let mut values = Vec::new();
    collect_values(&option["options"], &mut values);
    if values.is_empty() {
        return None;
    }
    Some(json!({"id": id, "name": text(&option["name"]).unwrap_or(id), "values": values,
        "default": text(&option["currentValue"])}))
}

fn collect_values(options: &Value, out: &mut Vec<Value>) {
    for value in options.as_array().into_iter().flatten() {
        if value["options"].is_array() {
            collect_values(&value["options"], out);
        } else if let Some(id) = text(&value["value"])
            && !out.iter().any(|v| v["id"] == id)
        {
            out.push(json!({"id": id, "name": text(&value["name"]).unwrap_or(id)}));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_unavailable_model_keeps_other_models_and_their_options() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/model_options_agent.py");
        let command = crate::shell::python(&fixture);
        let (acp, _inbox) = Acp::spawn(&command, env!("CARGO_MANIFEST_DIR"), &[]).unwrap();
        let session = json!({"sessionId": "s1", "configOptions": [{"id": "model", "category": "model",
            "type": "select", "currentValue": "first", "options": [
                {"value": "first"}, {"value": "unavailable"}, {"value": "second"}]}]});
        let catalog = catalog(&acp, &session, "test").await.unwrap();
        acp.kill().await;
        assert_eq!(catalog["models"].as_array().unwrap().len(), 3);
        assert!(catalog["models"][1]["optionsError"].as_str().unwrap().contains("model not available"));
        assert_eq!(catalog["models"][1]["options"], json!([]));
        assert_eq!(catalog["models"][2]["options"][0]["default"], "high");
        assert_eq!(catalog["defaultModelOptions"], true);
    }

    #[test]
    fn options_are_checked_against_their_models_list() {
        let catalog = json!({"currentModelId": "opus", "models": [
            {"id": "opus", "options": [{"id": "effort", "values": [{"id": "low"}, {"id": "high"}]}]},
            {"id": "haiku", "options": []}]});
        let check =
            |model, id: &str, value: &str| check_options(&catalog, model, [(id, value)]).map_err(|e| e.to_string());
        assert!(check(None, "effort", "high").is_ok(), "the default model's");
        assert_eq!(check(Some("opus"), "effort", "max").unwrap_err(), "effort must be one of: low, high");
        assert_eq!(check(Some("opus"), "mode", "plan").unwrap_err(), "model opus has no setting mode; it has: effort");
        assert_eq!(
            check(Some("haiku"), "effort", "low").unwrap_err(),
            "model haiku has no setting effort; it has: none"
        );
        assert!(check(Some("gpt"), "effort", "low").unwrap_err().contains("isn't in the agent's model list"));
        assert!(check_options(&catalog, Some("gpt"), []).is_err(), "an unknown model");
        // A saved Cursor model carries its settings; the list has the bare one.
        assert!(check(Some("opus[effort=low,fast=true]"), "effort", "high").is_ok(), "a saved Cursor model");
    }

    #[test]
    fn reasoning_catalog_preserves_defaults_and_grouped_levels_without_permission_options() {
        let session = json!({"models": {"currentModelId": "opus", "availableModels": [{"modelId": "opus"}]},
        "configOptions": [
            {"id": "effort", "category": "thought_level", "type": "select", "currentValue": "high",
             "options": [{"group": "levels", "options": [{"value": "low", "name": "Low"}, {"value": "high", "name": "High"}]}]},
            {"id": "mode", "category": "mode", "type": "select", "options": [{"value": "bypassPermissions"}]},
            {"id": "fast", "type": "boolean"}
        ]});
        let catalog = model_catalog(&session, &Value::Null);
        let options = catalog["models"][0]["options"].as_array().unwrap();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0]["id"], "effort");
        assert_eq!(options[0]["default"], "high");
        assert_eq!(options[0]["values"].as_array().unwrap().len(), 2);
    }

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
            json!({"models": [], "currentModelId": null, "fastMode": false, "defaultModelOptions": true})
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
                {"id": "reasoning_effort", "name": "Reasoning", "default": "high",
                 "values": [{"id": "medium", "name": "Medium"}, {"id": "high", "name": "High"}]},
                {"id": "fast", "name": "fast", "default": "true",
                 "values": [{"id": "false", "name": "Off"}, {"id": "true", "name": "true"}]}
            ])
        );
        assert_eq!(result["models"][1]["options"], json!([]));
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

    #[tokio::test]
    async fn antigravity_claude_and_gemini_models_pass_through_catalog() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/model_options_agent.py");
        let command = crate::shell::python(&fixture);
        let (acp, _inbox) = Acp::spawn(&command, env!("CARGO_MANIFEST_DIR"), &[]).unwrap();
        let session = json!({
            "sessionId": "s-agy",
            "models": {
                "currentModelId": "gemini-4-hydrogen-high",
                "availableModels": [
                    {"modelId": "gemini-4-hydrogen-high", "name": "Gemini 4 Hydrogen (High)"},
                    {"modelId": "claude-sonnet-4-6", "name": "Claude Sonnet 4.6 (Thinking)"},
                    {"modelId": "claude-opus-4-6-thinking", "name": "Claude Opus 4.6 (Thinking)"},
                    {"modelId": "gpt-oss-120b-medium", "name": "GPT-OSS 120B (Medium)"}
                ]
            },
            "configOptions": [
                {
                    "id": "model",
                    "name": "Model",
                    "category": "model",
                    "type": "select",
                    "currentValue": "gemini-4-hydrogen",
                    "options": [
                        {"value": "gemini-4-hydrogen", "name": "Gemini 4 Hydrogen"},
                        {"value": "claude-sonnet-4-6", "name": "Claude Sonnet 4.6 (Thinking)"},
                        {"value": "claude-opus-4-6-thinking", "name": "Claude Opus 4.6 (Thinking)"},
                        {"value": "gpt-oss-120b-medium", "name": "GPT-OSS 120B (Medium)"}
                    ]
                },
                {
                    "id": "thought_level",
                    "name": "Thinking Level",
                    "category": "thought_level",
                    "type": "select",
                    "currentValue": "high",
                    "options": [{"value": "medium"}, {"value": "high"}]
                }
            ]
        });
        let catalog = catalog(&acp, &session, "antigravity").await.unwrap();
        acp.kill().await;
        let ids: Vec<&str> = catalog["models"].as_array().unwrap().iter().filter_map(|m| m["id"].as_str()).collect();
        assert_eq!(ids, ["gemini-4-hydrogen", "claude-sonnet-4-6", "claude-opus-4-6-thinking", "gpt-oss-120b-medium"]);
        assert_eq!(catalog["currentModelId"], "gemini-4-hydrogen");
    }
}
