//! When tools run: the ones switched on start with the host, every one stops with it, and a
//! switch survives restarts in the store. ComfyUI also offers its MCP tools to the bots while it runs.

use super::comfy::{self, CONNECTOR_ID};
use super::comfy_job::Jobs;
use super::log::Log;
use super::runner::{Runner, State};
use super::{COMFY, ENGINES, Engine};
use crate::hub::Hub;
use crate::service;
use crate::store::Store;
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct Tools {
    runners: Vec<Arc<Runner>>,
    jobs: Arc<Jobs>,
}

impl Tools {
    /// One runner per declared engine, logging under `<data>/logs`.
    pub fn load(events: &broadcast::Sender<Value>) -> Self {
        let dir = service::data_dir().join("logs");
        Self::with_engines(&ENGINES, &dir, events)
    }

    fn with_engines(engines: &[&'static Engine], logs: &Path, events: &broadcast::Sender<Value>) -> Self {
        Self {
            runners: engines.iter().map(|e| Arc::new(Runner::new(e, Log::open(logs, e.id), events.clone()))).collect(),
            jobs: Arc::new(Jobs::new(events.clone())),
        }
    }

    fn runner(&self, id: &str) -> Result<&Arc<Runner>> {
        self.runners.iter().find(|r| r.engine().id == id).ok_or_else(|| anyhow!("unknown tool `{id}`"))
    }

    pub fn comfy_jobs(&self) -> &Arc<Jobs> {
        &self.jobs
    }

    pub fn comfy_running(&self) -> bool {
        self.runner(COMFY.id).is_ok_and(|r| r.state() == State::Running)
    }

    pub fn comfy_port(&self) -> u16 {
        self.runner(COMFY.id).map_or(COMFY.default_port, |r| r.port())
    }

    /// What clients show for one tool.
    pub async fn status(&self, store: &Store, id: &str) -> Result<Value> {
        let runner = self.runner(id)?;
        let state = runner.state();
        let mut status = json!({
            "id": id,
            "enabled": enabled(store, id),
            "state": state.name(),
            "error": match &state { State::Error(e) => Some(e.clone()), _ => None },
            "port": runner.port(),
            "url": (state == State::Running).then(|| format!("http://127.0.0.1:{}", runner.port())),
            "log": runner.log().path().to_string_lossy(),
            "logTail": runner.log().tail(),
        });
        if id == COMFY.id {
            status["comfy"] = tokio::task::spawn_blocking(comfy::card).await?;
            status["job"] = self.jobs.status();
        }
        Ok(status)
    }

    /// Starts, in the background, every tool the user left switched on.
    pub fn start_enabled(&self, hub: &Arc<Hub>) {
        for runner in &self.runners {
            if enabled(&hub.store, runner.engine().id) {
                spawn_start(runner, hub);
            }
        }
    }

    /// Switches a tool on (it starts with the host from now on) and starts it.
    pub fn start(&self, hub: &Arc<Hub>, id: &str) -> Result<()> {
        let runner = self.runner(id)?;
        hub.store.kv_set(&enabled_key(id), "1")?;
        spawn_start(runner, hub);
        Ok(())
    }

    /// Stops a tool and switches it off.
    pub async fn stop(&self, store: &Store, id: &str) -> Result<()> {
        let runner = self.runner(id)?;
        store.kv_set(&enabled_key(id), "0")?;
        runner.stop().await;
        withdraw(store, id);
        Ok(())
    }

    /// Stops every tool without changing its switch: the host is going away, the tools come back with it.
    pub async fn stop_all(&self) {
        futures::future::join_all(self.runners.iter().map(|r| r.stop())).await;
    }
}

fn spawn_start(runner: &Arc<Runner>, hub: &Arc<Hub>) {
    let (runner, hub) = (runner.clone(), hub.clone());
    tokio::spawn(async move {
        let launch = runner.engine().launch;
        // The runner logs the failure and keeps it in its state; nothing else waits on this task.
        if runner.start(launch).await.is_ok()
            && runner.engine().id == COMFY.id
            && let Err(e) = offer_to_bots(&hub.store)
        {
            tracing::warn!(error = format!("{e:#}"), "offering ComfyUI to the bots");
        }
    });
}

/// ComfyUI is up: its tools appear in the bots' connector list, off for every bot until the owner turns
/// them on. The bot's session adds the `comfy` MCP server itself (`agent/bot/session.rs`).
fn offer_to_bots(store: &Store) -> Result<()> {
    let connector = crate::market::Connector {
        id: CONNECTOR_ID.to_owned(),
        name: "ComfyUI".to_owned(),
        description: "Make images with the ComfyUI on this computer".to_owned(),
        registry_name: None,
        catalog: None,
        command: Some("codync-host".to_owned()),
        args: vec![],
        env: BTreeMap::new(),
        url: None,
        headers: BTreeMap::new(),
        oauth: None,
    };
    crate::market::update_connectors(store, |all| {
        match all.iter_mut().find(|c| c.id == CONNECTOR_ID) {
            Some(existing) => *existing = connector.clone(),
            None => all.push(connector.clone()),
        }
        Ok(())
    })
}

/// A stopped tool has no tools to offer.
fn withdraw(store: &Store, id: &str) {
    if id == COMFY.id
        && let Err(e) = crate::market::remove_connector(store, CONNECTOR_ID)
    {
        tracing::warn!(error = format!("{e:#}"), "removing the ComfyUI connector");
    }
}

fn enabled_key(id: &str) -> String {
    format!("tools.{id}.enabled")
}

fn enabled(store: &Store, id: &str) -> bool {
    store.kv_get(&enabled_key(id)).as_deref() == Some("1")
}
