//! The lock around a sweep and a person's action, and the methods the API calls.

#![allow(clippy::needless_pass_by_value)]

use super::model::Graph;
use super::outline::{self, EditMode};
use super::run;
use super::store::{self, Saved};
use super::{agent, hooks, schedule};
use crate::LockExt;
use crate::hub::Hub;
use crate::store::now_ms;
use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::sync::watch;

pub struct Workflows {
    gate: Mutex<()>,
    /// Runs whose script replay is in progress. A second replay waits, then runs.
    replaying: Mutex<HashSet<String>>,
    hooks: watch::Sender<u64>,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Default for Workflows {
    fn default() -> Self {
        Self {
            gate: Mutex::new(()),
            replaying: Mutex::new(HashSet::new()),
            hooks: watch::channel(0).0,
            task: Mutex::new(None),
        }
    }
}

impl Workflows {
    pub fn subscribe_hooks(&self) -> watch::Receiver<u64> {
        self.hooks.subscribe()
    }

    pub(crate) fn enter(&self) -> MutexGuard<'_, ()> {
        self.gate.locked()
    }

    /// `true` when this call owns the run's replay. A replay already in progress returns `false`.
    pub(crate) fn claim_replay(&self, run_id: &str) -> bool {
        self.replaying.locked().insert(run_id.to_owned())
    }

    pub(crate) fn release_replay(&self, run_id: &str) {
        self.replaying.locked().remove(run_id);
    }

    pub fn keeps_awake(store: &crate::store::Store) -> bool {
        schedule::keeps_awake(store)
    }

    pub fn create(
        &self,
        hub: &Hub,
        title: &str,
        graph: Option<Graph>,
        script: Option<&str>,
        owner: Option<&str>,
    ) -> Result<Value> {
        let _gate = self.gate.locked();
        let id = store::create(&hub.store, title, graph, script, owner)?;
        self.changed();
        emit(hub, "graph", &id, json!({}));
        detail(hub, &id, false)
    }

    pub fn save(&self, hub: &Hub, id: &str, base: i64, graph: &Graph) -> Result<Value> {
        let _gate = self.gate.locked();
        match store::save(&hub.store, id, base, graph)? {
            Saved::Conflict => {
                Ok(json!({"ok": false, "conflict": true, "workflow": detail(hub, id, false)?["workflow"]}))
            }
            Saved::Ok { revision } => {
                self.changed();
                emit(hub, "graph", id, json!({}));
                let workflow = detail(hub, id, false)?["workflow"].clone();
                Ok(json!({"ok": true, "revision": revision, "workflow": workflow}))
            }
            Saved::Unchanged { revision } => {
                let workflow = detail(hub, id, false)?["workflow"].clone();
                Ok(json!({"ok": true, "revision": revision, "workflow": workflow}))
            }
        }
    }

    pub fn save_script(
        &self,
        hub: &Hub,
        id: &str,
        base: i64,
        script: Option<&str>,
        triggers: Option<Graph>,
    ) -> Result<Value> {
        let _gate = self.gate.locked();
        match store::save_script(&hub.store, id, base, script, triggers.as_ref())? {
            Saved::Conflict => {
                Ok(json!({"ok": false, "conflict": true, "workflow": detail(hub, id, false)?["workflow"]}))
            }
            Saved::Ok { revision } => {
                self.changed();
                emit(hub, "graph", id, json!({}));
                Ok(json!({"ok": true, "revision": revision, "workflow": detail(hub, id, false)?["workflow"]}))
            }
            Saved::Unchanged { revision } => {
                Ok(json!({"ok": true, "revision": revision, "workflow": detail(hub, id, false)?["workflow"]}))
            }
        }
    }

    pub fn set_enabled(&self, hub: &Hub, id: &str, enabled: bool) -> Result<Value> {
        let _gate = self.gate.locked();
        store::set_enabled(&hub.store, id, enabled)?;
        self.changed();
        emit(hub, "enabled", id, json!({"enabled": enabled}));
        hub.update_keep_awake();
        Ok(json!({"workflow": summary(hub, id)?}))
    }

    pub fn rename(&self, hub: &Hub, id: &str, title: &str) -> Result<Value> {
        let _gate = self.gate.locked();
        store::rename(&hub.store, id, title)?;
        emit(hub, "graph", id, json!({}));
        Ok(json!({"workflow": summary(hub, id)?}))
    }

    pub fn remove(&self, hub: &Hub, id: &str) -> Result<Value> {
        let _gate = self.gate.locked();
        if let Some(run_id) = run::open_run(&hub.store, id)? {
            let _ = stop_locked(hub, &run_id);
        }
        store::remove(&hub.store, id)?;
        self.changed();
        emit(hub, "deleted", id, json!({}));
        Ok(json!({}))
    }

    #[allow(clippy::unused_self)]
    pub fn start_run(&self, hub: &Hub, id: &str, request: &str, from: Option<&str>, input: &str) -> Result<Value> {
        // `run::start` takes the gate itself and drops it before a script replay.
        let detail = run::start(hub, id, request, from, super::model::RunSource::Manual, input, None)?;
        emit_run(hub, id, detail["run"]["id"].as_str().unwrap_or(""));
        Ok(detail)
    }

    pub fn stop_run(&self, hub: &Hub, run_id: &str) -> Result<Value> {
        let _gate = self.gate.locked();
        stop_locked(hub, run_id)?;
        Ok(json!({}))
    }

    pub fn decide(&self, hub: &Hub, step_id: &str, approved: bool) -> Result<Value> {
        let replay = {
            let _gate = self.gate.locked();
            decide_locked(hub, step_id, approved)?
        };
        if let Some(run_id) = replay {
            super::script_run::replay(hub, &run_id)?;
        }
        Ok(json!({}))
    }

    pub fn workflow(&self, hub: &Hub, id: &str, reveal: bool) -> Result<Value> {
        let _gate = self.gate.locked();
        detail(hub, id, reveal)
    }

    pub fn list(&self, hub: &Hub) -> Result<Value> {
        let _gate = self.gate.locked();
        let mut workflows = store::list(&hub.store)?;
        workflows.retain(|row| !row.is_null());
        Ok(json!({"workflows": workflows}))
    }

    pub fn versions(&self, hub: &Hub, id: &str) -> Result<Value> {
        let _gate = self.gate.locked();
        Ok(json!({"versions": store::versions(&hub.store, id)?}))
    }

    pub fn version(&self, hub: &Hub, id: &str, version: &str) -> Result<Value> {
        let _gate = self.gate.locked();
        Ok(json!({"graph": store::version_graph(&hub.store, id, version)?}))
    }

    pub fn runs(
        &self,
        hub: &Hub,
        id: &str,
        limit: i64,
        before_at: Option<i64>,
        before_id: Option<&str>,
    ) -> Result<Value> {
        let _gate = self.gate.locked();
        run::runs_page(&hub.store, id, limit, before_at, before_id)
    }

    pub fn run(&self, hub: &Hub, run_id: &str) -> Result<Value> {
        let _gate = self.gate.locked();
        run::run_detail(hub, run_id)
    }

    pub fn rotate(&self, hub: &Hub, id: &str, node: &str) -> Result<Value> {
        let _gate = self.gate.locked();
        let secret = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        let changed = hub.store.with_immediate(|tx| {
            Ok(tx.execute("UPDATE workflow_webhook_secrets SET secret = ?3, updated_at = ?4 WHERE workflow_id = ?1 AND node_id = ?2", params![id, node, secret, now_ms()])?)
        })?;
        if changed == 0 {
            bail!("This step has no webhook");
        }
        self.changed();
        Ok(json!({"secret": secret}))
    }

    pub fn run_step(&self, hub: &Hub, id: &str, node: &str, request: &str) -> Result<Value> {
        let _gate = self.gate.locked();
        run::run_solo(hub, id, node, request)
    }

    pub fn stop_step(&self, hub: &Hub, step_id: &str) -> Result<Value> {
        let _gate = self.gate.locked();
        run::stop_one(hub, step_id)?;
        Ok(json!({}))
    }

    #[allow(clippy::unused_self)]
    pub fn retry_step(&self, hub: &Hub, step_id: &str) -> Result<Value> {
        run::retry_one(hub, step_id)
    }

    pub fn finish(
        &self,
        hub: &Hub,
        step_id: &str,
        output: Option<String>,
        error: Option<String>,
        stopped: bool,
    ) -> bool {
        let settled = {
            let _gate = self.gate.locked();
            agent::settle(hub, step_id, output, error, stopped)
        };
        match settled {
            Ok(settled) => {
                if let Some(run_id) = settled.replay
                    && let Err(error) = super::script_run::replay(hub, &run_id)
                {
                    tracing::warn!(step = step_id, error = format!("{error:#}"), "couldn't replay a workflow script");
                }
                settled.keep
            }
            Err(error) => {
                tracing::warn!(step = step_id, error = format!("{error:#}"), "couldn't settle a workflow step");
                false
            }
        }
    }

    pub(crate) fn changed(&self) {
        self.hooks.send_modify(|n| *n = n.saturating_add(1));
    }
}

fn stop_locked(hub: &Hub, run_id: &str) -> Result<()> {
    let workflow: Option<String> = hub.store.with_immediate(|tx| {
        tx.execute("UPDATE workflow_runs SET status = 'stopping', updated_at = ?2 WHERE id = ?1 AND status IN ('running','approval')", params![run_id, now_ms()])?;
        tx.query_row("SELECT workflow_id FROM workflow_runs WHERE id = ?1", [run_id], |r| r.get(0)).optional().map_err(anyhow::Error::from)
    })?;
    let Some(workflow) = workflow else { bail!("This run was deleted") };
    let agents = hub.store.with_immediate(|tx| {
        let mut st = tx.prepare("SELECT bot_id, id FROM workflow_steps WHERE run_id = ?1 AND status = 'running' AND kind = 'agent' AND bot_id IS NOT NULL")?;
        let rows = st.query_map([run_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })?;
    for (bot, step) in agents {
        let _ = hub.send_cmd(&bot, crate::agent::bot::Cmd::WorkflowStop { step_id: step });
    }
    let effects = run::advance(hub, run_id)?;
    run::dispatch_agents(hub, &effects);
    run::dispatch_pushes(hub, &effects);
    emit_run(hub, &workflow, run_id);
    Ok(())
}

pub fn start(hub: &Arc<Hub>) {
    let running = hub.clone();
    let task = tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        if let Err(error) = recover(&running) {
            tracing::warn!(error = format!("{error:#}"), "couldn't resume workflows");
        }
        loop {
            tick.tick().await;
            if let Err(error) = schedule::tick(&running) {
                tracing::warn!(error = format!("{error:#}"), "workflow schedule");
            }
        }
    });
    *hub.workflows.task.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(task);
}

fn decide_locked(hub: &Hub, step_id: &str, approved: bool) -> Result<Option<String>> {
    let now = now_ms();
    let run_id: Option<String> = hub.store.with_immediate(|tx| {
        if approved {
            tx.execute(
                "UPDATE workflow_steps SET status = 'done', output = input, ended_at = ?2, updated_at = ?2 WHERE id = ?1 AND status = 'approval'",
                params![step_id, now],
            )?;
        }
        Ok(tx.query_row("SELECT run_id FROM workflow_steps WHERE id = ?1", [step_id], |r| r.get::<_, Option<String>>(0))?)
    })?;
    let Some(run_id) = run_id else {
        if !approved {
            hub.store.with_immediate(|tx| {
                Ok(tx.execute(
                    "UPDATE workflow_steps SET status = 'stopped', ended_at = ?2, updated_at = ?2 WHERE id = ?1 AND status = 'approval'",
                    params![step_id, now],
                )?)
            })?;
        }
        return Ok(None);
    };
    if !approved {
        stop_locked(hub, &run_id)?;
        return Ok(None);
    }
    if super::script_run::is_script_run(&hub.store, &run_id) {
        return Ok(Some(run_id));
    }
    let effects = run::advance(hub, &run_id)?;
    run::dispatch_agents(hub, &effects);
    run::dispatch_pushes(hub, &effects);
    Ok(None)
}

fn recover(hub: &Arc<Hub>) -> Result<()> {
    let scripts = {
        let _gate = hub.workflows.gate.locked();
        let runs = hub.store.with_immediate(|tx| {
            let mut st =
                tx.prepare("SELECT id FROM workflow_runs WHERE status IN ('running','approval','stopping')")?;
            let rows = st.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })?;
        let mut scripts = Vec::new();
        for run_id in runs {
            let exec: String = hub.store.with_immediate(|tx| {
                Ok(tx
                    .query_row("SELECT COALESCE(exec, 'graph') FROM workflow_runs WHERE id = ?1", [&run_id], |r| {
                        r.get(0)
                    })
                    .optional()?
                    .unwrap_or_else(|| "graph".into()))
            })?;
            if exec == "script" {
                scripts.push(run_id);
                continue;
            }
            let effects = run::advance(hub, &run_id)?;
            run::dispatch_pushes(hub, &effects);
            let started = effects.agents.clone();
            run::dispatch_agents(hub, &run::Effects { agents: started.clone(), pushes: Vec::new() });
            scripts.extend(resume_open(hub, &run_id, &started)?);
        }
        scripts
    };
    for run_id in scripts {
        super::script_run::replay(hub, &run_id)?;
        for again in resume_open(hub, &run_id, &[])? {
            super::script_run::replay(hub, &again)?;
        }
    }
    Ok(())
}

fn resume_open(hub: &Hub, run_id: &str, started: &[(String, String)]) -> Result<Vec<String>> {
    let open = hub.store.with_immediate(|tx| {
        let mut st = tx.prepare(
            "SELECT id, COALESCE(bot_id, ''), started_at FROM workflow_steps WHERE run_id = ?1 AND kind = 'agent' AND status = 'running'",
        )?;
        let rows = st
            .query_map([run_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })?;
    let mut replay = Vec::new();
    for (step, bot, began) in open {
        if bot.is_empty() || started.iter().any(|(_, id)| id == &step) {
            continue;
        }
        let slot = agent::session_slot(&step);
        let key = crate::store::run_session_key(&bot, &slot);
        if began.is_some() && hub.store.kv_get(&key).is_none() {
            if let Ok(settled) = agent::settle(hub, &step, None, Some("interrupted".into()), false)
                && let Some(run_id) = settled.replay
            {
                replay.push(run_id);
            }
        } else if let Err(error) = hub.send_cmd(&bot, crate::agent::bot::Cmd::WorkflowStep { step_id: step }) {
            tracing::warn!(error = format!("{error:#}"), "couldn't resume a workflow step");
        }
    }
    Ok(replay)
}

fn emit(hub: &Hub, change: &str, workflow: &str, extra: Value) {
    let mut body = json!({"type": "workflow", "change": change, "workflowId": workflow, "runId": Value::Null});
    if let (Some(obj), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            obj.insert(key.clone(), value.clone());
        }
    }
    hub.emit_workflow(body);
}

fn emit_run(hub: &Hub, workflow: &str, run_id: &str) {
    hub.emit_workflow(json!({"type": "workflow", "change": "run", "workflowId": workflow, "runId": run_id}));
}

fn summary(hub: &Hub, id: &str) -> Result<Value> {
    let Some((summary, _)) = store::load(&hub.store, id)? else { bail!("This workflow was deleted") };
    Ok(summary)
}

fn detail(hub: &Hub, id: &str, reveal: bool) -> Result<Value> {
    let Some((mut summary, graph)) = store::load(&hub.store, id)? else { bail!("This workflow was deleted") };
    let mode = outline::edit_mode(&graph, summary["source"].as_str().unwrap_or("graph"));
    summary["graph"] = serde_json::to_value(&graph)?;
    summary["editMode"] = json!(match mode {
        EditMode::Outline => "outline",
        EditMode::Fields => "fields",
        EditMode::Script => "script",
        EditMode::ReadOnly => "readOnly",
    });
    summary["phases"] = json!(super::script::phases_of(summary["script"].as_str().unwrap_or("")));
    let public = crate::remote::cloud::codync_url(hub)
        .ok()
        .map(|base| format!("{}/v1/hooks/{}", base.trim_end_matches('/'), hub.identity.computer_id()));
    summary["webhooks"] =
        json!(hooks::reveal(&hub.store, id, reveal, &format!("http://127.0.0.1:{}", hub.port), public.as_deref(),)?);
    Ok(json!({"workflow": summary}))
}

/// Restores a saved graph. The current one is kept as a `restore` version first.
pub fn restore(workflows: &Workflows, hub: &Hub, id: &str, version: &str) -> Result<Value> {
    let _gate = workflows.gate.locked();
    let saved = store::version_snapshot(&hub.store, id, version)?;
    let Some((summary, current)) = store::load(&hub.store, id)? else { bail!("This workflow was deleted") };
    let revision = summary["revision"].as_i64().unwrap_or(0);
    store::record_version(&hub.store, id, revision, super::model::VersionKind::Restore, &current)?;
    match store::restore_saved(&hub.store, id, revision, &saved.graph, &saved.script, &saved.source) {
        Ok(Saved::Ok { .. }) => {
            workflows.changed();
            emit(hub, "graph", id, json!({}));
            detail(hub, id, false)
        }
        Ok(Saved::Conflict | Saved::Unchanged { .. }) => bail!("This workflow was edited somewhere else"),
        Err(error) => Err(error),
    }
}

pub fn clear_bot(hub: &Hub, bot: &str) {
    if let Ok(ids) = store::clear_bot(&hub.store, bot) {
        for id in ids {
            emit(hub, "graph", &id, json!({}));
        }
    }
}
