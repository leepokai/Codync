use crate::{LockExt, agent::bot::Cmd, hub::Hub, store::Store};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Settings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub root: String,
    #[serde(default)]
    pub python: String,
    #[serde(default)]
    pub terminal: Terminal,
    #[serde(default = "native")]
    pub memory_provider: String,
    #[serde(default)]
    pub delegation: Delegation,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            root: String::new(),
            python: String::new(),
            terminal: Terminal::default(),
            memory_provider: native(),
            delegation: Delegation::default(),
        }
    }
}

fn native() -> String {
    "native".into()
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Terminal {
    #[serde(default)]
    pub backend: String,
    #[serde(default)]
    pub ssh_host: String,
    #[serde(default)]
    pub ssh_user: String,
    #[serde(default)]
    pub ssh_port: Option<u16>,
    #[serde(default)]
    pub ssh_key: String,
    #[serde(default)]
    pub docker_image: String,
    #[serde(default)]
    pub modal_mode: String,
    #[serde(default)]
    pub vercel_runtime: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Delegation {
    #[serde(default)]
    pub routes: Vec<Route>,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub max_iterations: Option<u32>,
    #[serde(default)]
    pub max_concurrent_children: Option<u32>,
    #[serde(default)]
    pub child_timeout_seconds: Option<u32>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Route {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Saved {
    pub revision: String,
    pub config: Settings,
}

pub(crate) fn load(store: &Store, bot: &str) -> Result<Option<Saved>> {
    store
        .kv_read(&format!("hermes-runtime:{bot}"))?
        .map(|text| serde_json::from_str(&text).map_err(Into::into))
        .transpose()
}

fn save(store: &Store, bot: &str, revision: Option<&str>, config: Settings) -> Result<Saved> {
    config.validate()?;
    let _guard = store.connector_lock.locked();
    let current = load(store, bot)?;
    ensure!(
        revision == current.as_ref().map(|saved| saved.revision.as_str()),
        "Hermes settings changed; reopen before applying"
    );
    let saved = Saved { revision: uuid::Uuid::new_v4().to_string(), config };
    store.kv_set(&format!("hermes-runtime:{bot}"), &serde_json::to_string(&saved)?)?;
    Ok(saved)
}

impl Settings {
    fn validate(&self) -> Result<()> {
        ensure!(
            ["", "local", "docker", "ssh", "modal", "daytona", "vercel_sandbox"]
                .contains(&self.terminal.backend.as_str()),
            "Unknown Hermes environment"
        );
        ensure!(
            ["native", "builtin", "honcho"].contains(&self.memory_provider.as_str()),
            "Unknown Hermes memory provider"
        );
        ensure!(self.delegation.routes.len() <= 16, "Save at most 16 subagent routes");
        let mut ids = std::collections::HashSet::new();
        for route in &self.delegation.routes {
            ensure!(
                !route.id.is_empty()
                    && route.id.len() <= 64
                    && route.id != "default"
                    && route.id.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
                    && ids.insert(&route.id),
                "Subagent routes require unique ids"
            );
            ensure!(
                !route.label.trim().is_empty() && route.label.len() <= 128,
                "Name each subagent route (up to 128 bytes)"
            );
            ensure!(
                !route.provider.trim().is_empty() || !route.model.trim().is_empty(),
                "Choose a provider or model for each saved route"
            );
            for text in [&route.label, &route.provider, &route.model] {
                ensure!(
                    text.len() <= 512 && !text.chars().any(char::is_control),
                    "Subagent route contains invalid text"
                );
            }
        }
        ensure!(self.terminal.ssh_port != Some(0), "SSH port must be 1–65535");
        if self.terminal.backend == "ssh" {
            ensure!(
                !self.terminal.ssh_host.trim().is_empty() && !self.terminal.ssh_user.trim().is_empty(),
                "SSH host and user are required"
            );
            ensure!(
                !self.terminal.ssh_host.starts_with('-') && !self.terminal.ssh_user.starts_with('-'),
                "Invalid SSH host or user"
            );
        }
        for value in [
            &self.root,
            &self.python,
            &self.terminal.ssh_host,
            &self.terminal.ssh_user,
            &self.terminal.ssh_key,
            &self.terminal.docker_image,
            &self.delegation.provider,
            &self.delegation.model,
        ] {
            ensure!(
                value.len() <= 4096 && !value.chars().any(char::is_control),
                "Hermes settings contain invalid text"
            );
        }
        ensure!(
            self.delegation.provider.len() <= 512 && self.delegation.model.len() <= 512,
            "Use short native provider and model names (up to 512 bytes)"
        );
        for (value, max, label) in [
            (self.delegation.max_iterations, 200, "iterations"),
            (self.delegation.max_concurrent_children, 10, "concurrent children"),
        ] {
            ensure!(value.is_none_or(|n| (1..=max).contains(&n)), "Invalid Hermes delegation {label}");
        }
        ensure!(
            self.delegation.child_timeout_seconds.is_none_or(|n| (30..=3600).contains(&n)),
            "Child inactivity timeout must be 30–3600 seconds"
        );
        ensure!(["", "auto", "managed", "direct"].contains(&self.terminal.modal_mode.as_str()), "Invalid Modal mode");
        ensure!(
            ["", "node24", "node22", "python3.13"].contains(&self.terminal.vercel_runtime.as_str()),
            "Invalid Vercel runtime"
        );
        Ok(())
    }
}

pub(crate) async fn api(hub: &Arc<Hub>, method: &str, body: &Value) -> Result<Value> {
    let bot = body["botId"].as_str().context("botId is required")?;
    let row = hub
        .store
        .bot(bot)?
        .filter(|row| !row.deleted && row.config.backend == "hermes")
        .context("Choose a Hermes bot")?;
    if method == "hermesEnvironmentHealth" {
        return super::health::api(hub, &row.config, body).await;
    }
    let saved = if method == "setHermesRuntime" {
        let config: Settings = serde_json::from_value(body["config"].clone())?;
        let saved = save(&hub.store, bot, body["ifRevision"].as_str(), config)?;
        hub.send_cmd(&row.config.id, Cmd::HermesRuntimeChanged)?;
        Some(saved)
    } else {
        load(&hub.store, bot)?
    };
    let mut result = json!({"revision":saved.as_ref().map(|s| &s.revision),"config":saved.as_ref().map_or_else(|| Settings {memory_provider:native(),..Settings::default()}, |s| s.config.clone()),"applyAfterCurrentTurn":true});
    if method == "hermesRuntimeStatus" {
        let config = saved.map(|s| s.config).unwrap_or_default();
        result["installation"] = match super::Installation::resolve(&config).await {
            Ok(installation) => {
                let mut view = json!({"available":true,"root":installation.root,"python":installation.python});
                let metadata = async {
                    let (mut env, _grants) = crate::agent::auth::env(&hub.store, "hermes", Some(bot))?;
                    if let Some(account) = row.config.account.as_deref() {
                        for (key, value) in crate::agent::account::env(account, "hermes")? {
                            env.retain(|(saved, _)| saved != &key);
                            env.push((key, value));
                        }
                    }
                    installation.profile_metadata(&env).await
                }
                .await;
                match metadata {
                    Ok(metadata) => {
                        let provider = metadata["memoryProvider"].as_str().unwrap_or("unknown");
                        view["profileSummary"] = format!("Hermes memory: {provider}. Honcho plugin {}. Profile metadata only; connection health has not been checked.", if metadata["honchoInstalled"] == true {"installed"} else {"not installed"}).into();
                        view["profile"] = metadata;
                    }
                    Err(_) => view["profileSummary"] = "Hermes profile metadata is unavailable. Unlock the existing connection and check its profile configuration.".into(),
                }
                view
            }
            Err(error) => json!({"available":false,"error":format!("{error:#}")}),
        };
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_apply_cannot_overwrite_the_newer_complete_route() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let first = save(&store, "a", None, Settings::default()).unwrap();
        let mut config = first.config.clone();
        config.terminal.backend = "docker".into();
        config.delegation.model = "user-chosen-model".into();
        let second = save(&store, "a", Some(&first.revision), config).unwrap();
        assert!(save(&store, "a", Some(&first.revision), first.config).is_err());
        let current = load(&store, "a").unwrap().unwrap();
        assert_eq!(current.revision, second.revision);
        assert_eq!(current.config.terminal.backend, "docker");
        assert_eq!(current.config.delegation.model, "user-chosen-model");
        assert!(load(&store, "b").unwrap().is_none());
    }

    #[test]
    fn invalid_routes_and_limits_are_rejected_before_any_save() {
        for input in [
            json!({"delegation":{"max_iterations":0}}),
            json!({"delegation":{"child_timeout_seconds":29}}),
            json!({"terminal":{"backend":"ssh","ssh_host":"-proxy","ssh_user":"name"}}),
            json!({"delegation":{"routes":[{"id":"default","label":"x","model":"m"}]}}),
            json!({"delegation":{"routes":[{"id":"x","label":"x","model":"m"},{"id":"x","label":"y","model":"n"}]}}),
        ] {
            let config: Settings = serde_json::from_value(input).unwrap();
            assert!(config.validate().is_err());
        }
        assert_eq!(Settings::default().memory_provider, "native");
    }
}
