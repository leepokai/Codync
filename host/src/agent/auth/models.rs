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
}
