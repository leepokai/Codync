//! The models an agent offers, read from a session it opens for that alone.

use super::{Started, probe_dir, start_with, text};
use crate::LockExt;
use crate::hub::Hub;
use crate::store::Store;
use anyhow::{Context, Result, bail};
use codync_agent::acp::Incoming;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::OnceCell;

type InFlightKey = (String, Option<String>);

/// Model lists being fetched, by agent and login profile: a settings screen asks for one from several places at once
/// (the model picker, Approvals), and each would otherwise start the agent. An ask after one came fetches afresh.
#[derive(Default)]
pub struct InFlight(Mutex<HashMap<InFlightKey, Slot>>);

/// One fetch's answer, for every ask that came while it ran (a failure as its text).
type Slot = Arc<OnceCell<Result<Value, String>>>;

impl InFlight {
    async fn get(
        &self,
        backend: &str,
        login_profile: Option<&str>,
        fetch: impl Future<Output = Result<Value>>,
    ) -> Result<Value> {
        let key = (backend.to_owned(), login_profile.map(str::to_owned));
        let slot = self.0.locked().entry(key.clone()).or_default().clone();
        let answer = slot
            .get_or_init(|| async {
                let answer = fetch.await.map_err(|e| format!("{e:#}"));
                self.0.locked().remove(&key);
                answer
            })
            .await;
        answer.clone().map_err(anyhow::Error::msg)
    }
}

/// [`models`], shared with the asks for the same agent and login profile meanwhile.
pub async fn shared_models(hub: &Hub, backend: &str, login_profile: Option<&str>) -> Result<Value> {
    let login_profile = login_profile.filter(|s| !s.is_empty());
    Box::pin(hub.model_lists.get(backend, login_profile, Box::pin(models(&hub.store, backend, login_profile)))).await
}

/// Discover the agent's advertised models in an isolated, prompt-free session.
/// This uses the same credentials as actual bot sessions and never reads project files.
pub async fn models(store: &Store, backend: &str, login_profile: Option<&str>) -> Result<Value> {
    let login_profile = login_profile.filter(|s| !s.is_empty());
    let running = crate::agent::login_profiles::hold(store, backend, login_profile)?;
    let Started { acp, mut inbox, .. } = start_with(store, backend, login_profile, &[]).await?;
    let result = tokio::time::timeout(Duration::from_mins(1), async {
        let request = acp.request("session/new", json!({"cwd": probe_dir(), "mcpServers": []}));
        tokio::pin!(request);
        let session = loop {
            tokio::select! {
                result = &mut request => break result?,
                incoming = inbox.recv() => match incoming {
                    Some(Incoming::Request(request)) => {
                        request.refuse(codync_agent::acp::METHOD_NOT_FOUND, "not supported during model discovery").await?;
                    }
                    Some(Incoming::Notification { .. } | Incoming::Update(_)) => {}
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
        catalog["modes"] =
            codync_agent::modes::Modes::from_session(&session, backend).map_or(json!([]), |m| m.catalog());
        if backend != "cursor" {
            discover_options(&acp, &session, &mut catalog).await?;
        }
        Ok(catalog)
    })
    .await;
    acp.kill().await;
    drop(running);
    result.context("the agent's model list timed out")?
}

/// Probe model-dependent settings in the disposable discovery session, never in a bot's session.
async fn discover_options(acp: &codync_agent::acp::Acp, session: &Value, catalog: &mut Value) -> Result<()> {
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
        "fastMode": codync_agent::config::fast_mode_config(session).is_some()})
}

/// One model's options as `[{id, name, values: [{id, name}], default}]`, from Cursor's
/// `{models: [{value, configOptions}]}` (ACP select config options).
fn model_options(options: &Value, model: &str) -> Vec<Value> {
    let entry = options["models"].as_array().into_iter().flatten().find(|m| m["value"] == model);
    entry.and_then(|m| m["configOptions"].as_array()).into_iter().flatten().filter_map(parse_select_option).collect()
}

/// Cursor names a model variant `grok-4.7[reasoning_effort=high,fast=true]`: the model and its settings.
pub(crate) fn cursor_variant(model: &str) -> Option<(&str, Vec<(&str, &str)>)> {
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
    async fn asks_at_once_start_the_agent_once_and_later_ones_again() {
        let lists = InFlight::default();
        let started = std::sync::atomic::AtomicUsize::new(0);
        let fetch = |ok: bool| {
            let started = &started;
            async move {
                started.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(20)).await;
                if ok { Ok(json!({"models": []})) } else { Err(anyhow::anyhow!("signed out")) }
            }
        };
        let (a, b) = tokio::join!(lists.get("claude", None, fetch(true)), lists.get("claude", None, fetch(true)));
        assert_eq!((a.unwrap(), b.unwrap()), (json!({"models": []}), json!({"models": []})));
        assert_eq!(started.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            lists.get("claude", None, fetch(false)).await.unwrap_err().to_string(),
            "signed out",
            "a refresh fetches"
        );
        assert!(lists.get("claude", None, fetch(true)).await.is_ok());
        assert_eq!(started.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn login_profiles_do_not_share_in_flight_slots() {
        let lists = InFlight::default();
        let started = std::sync::atomic::AtomicUsize::new(0);
        let fetch = |label: &'static str| {
            let started = &started;
            async move {
                started.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(25)).await;
                Ok(json!({"models": [{"id": label}]}))
            }
        };
        let (d1, d2, p1_a, p1_b, p2) = tokio::join!(
            lists.get("gemini", None, fetch("default")),
            lists.get("gemini", None, fetch("default-dup")),
            lists.get("gemini", Some("pro-7"), fetch("pro")),
            lists.get("gemini", Some("pro-7"), fetch("pro-dup")),
            lists.get("gemini", Some("ultra-8"), fetch("ultra")),
        );
        assert_eq!(d1.unwrap()["models"][0]["id"], "default");
        assert_eq!(d2.unwrap()["models"][0]["id"], "default");
        assert_eq!(p1_a.unwrap()["models"][0]["id"], "pro");
        assert_eq!(p1_b.unwrap()["models"][0]["id"], "pro");
        assert_eq!(p2.unwrap()["models"][0]["id"], "ultra");
        assert_eq!(started.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn an_unavailable_model_keeps_other_models_and_their_options() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/model_options_agent.py");
        let command = codync_agent::shell::python(&fixture);
        let (acp, _inbox) = codync_agent::acp::Acp::spawn(&command, env!("CARGO_MANIFEST_DIR"), &[]).unwrap();
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
    async fn models_holds_and_releases_login_profile_across_errors_and_concurrent_guards() {
        use crate::agent::login_profiles::{self, Kind, Refused};

        let dir = std::env::temp_dir().join(format!("codync-models-hold-unit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let store = Store::open(std::path::Path::new(":memory:")).expect("memory store");
        let own = login_profiles::add(&store, "not-installed-test-agent", Kind::ApiKey).await.expect("own profile");
        let own_id = own["id"].as_str().expect("own id").to_owned();
        let other = login_profiles::add(&store, "claude", Kind::ApiKey).await.expect("other profile");
        let other_id = other["id"].as_str().expect("other id").to_owned();
        let identity = crate::remote::identity::Identity::load_or_create(&dir).expect("identity");
        let hub = Hub::new(store, "models-hold-unit".into(), identity, "test".into(), 0);

        // Missing profile fails cleanly with the same message as `start_with`.
        let missing_err = models(&hub.store, "not-installed-test-agent", Some("missing-profile"))
            .await
            .expect_err("missing profile should fail");
        assert!(missing_err.to_string().contains("removed from this computer"), "{missing_err:#}");

        // Default (`None`), empty (`""` filtered by `models`), and another agent's profile hold no guard.
        assert!(login_profiles::hold(&hub.store, "not-installed-test-agent", None).expect("none").is_none());
        let cross = login_profiles::hold(&hub.store, "not-installed-test-agent", Some(&other_id)).expect("cross");
        assert!(cross.is_none(), "another agent's profile runs on default login without holding");

        // `models()` acquires `Running` in `hold`, then drops it when `start_with` fails so removal succeeds.
        let start_err = models(&hub.store, "not-installed-test-agent", Some(&own_id))
            .await
            .expect_err("unknown backend should fail in start_with");
        assert!(start_err.to_string().contains("isn't set up"), "{start_err:#}");

        // Concurrent/nested `hold` guards keep `remove` refused until every guard drops (no self-lock or early release).
        let first = login_profiles::hold(&hub.store, "not-installed-test-agent", Some(&own_id))
            .expect("first hold")
            .expect("guard");
        let second = login_profiles::hold(&hub.store, "not-installed-test-agent", Some(&own_id))
            .expect("second hold")
            .expect("guard");
        let err = login_profiles::remove(&hub, &own_id).await.expect_err("held by two guards");
        assert_eq!(err.downcast_ref::<Refused>().map(|r| r.category), Some("in_use"));
        drop(first);
        let err = login_profiles::remove(&hub, &own_id).await.expect_err("still held by second guard");
        assert_eq!(err.downcast_ref::<Refused>().map(|r| r.category), Some("in_use"));
        drop(second);

        login_profiles::remove(&hub, &own_id).await.expect("all guards dropped");
        login_profiles::remove(&hub, &other_id).await.expect("other profile was never held");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
