//! The models an agent offers, read from a session it opens for that alone.

use super::{probe_dir, start, text};
use crate::agent::acp::Incoming;
use crate::store::Store;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::time::Duration;

/// Discover the agent's advertised models in an isolated, prompt-free session.
/// This uses the same credentials as actual bot sessions and never reads project files.
pub async fn models(store: &Store, backend: &str) -> Result<Value> {
    let (acp, mut inbox, _, _) = start(store, backend).await?;
    let result = tokio::time::timeout(Duration::from_mins(1), async {
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
        let mut catalog = model_catalog(&session, &options);
        if backend != "cursor" {
            discover_options(&acp, &session, &mut catalog).await?;
        }
        Ok(catalog)
    })
    .await;
    acp.kill().await;
    result.context("the agent's model list timed out")?
}

/// Probe model-dependent settings in the disposable discovery session, never in a bot's session.
async fn discover_options(acp: &crate::agent::acp::Acp, session: &Value, catalog: &mut Value) -> Result<()> {
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
        "fastMode": crate::agent::config::fast_mode_config(session).is_some()})
}

/// One model's options as `[{id, name, values: [{id, name}], default}]`, from Cursor's
/// `{models: [{value, configOptions}]}` (ACP select config options).
fn model_options(options: &Value, model: &str) -> Vec<Value> {
    let entry = options["models"].as_array().into_iter().flatten().find(|m| m["value"] == model);
    entry.and_then(|m| m["configOptions"].as_array()).into_iter().flatten().filter_map(parse_select_option).collect()
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
        let (acp, _inbox) = crate::agent::acp::Acp::spawn(&command, env!("CARGO_MANIFEST_DIR"), &[]).unwrap();
        let session = json!({"sessionId": "s1", "configOptions": [{"id": "model", "category": "model",
            "type": "select", "currentValue": "first", "options": [
                {"value": "first"}, {"value": "unavailable"}, {"value": "second"}]}]});
        let mut catalog = model_catalog(&session, &Value::Null);
        discover_options(&acp, &session, &mut catalog).await.unwrap();
        acp.kill().await;
        assert_eq!(catalog["models"].as_array().unwrap().len(), 3);
        assert!(catalog["models"][1]["optionsError"].as_str().unwrap().contains("model not available"));
        assert_eq!(catalog["models"][1]["options"], json!([]));
        assert_eq!(catalog["models"][2]["options"][0]["default"], "high");
        assert_eq!(catalog["defaultModelOptions"], true);
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
}
