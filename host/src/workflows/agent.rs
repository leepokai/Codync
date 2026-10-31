//! One assistant step is a turn in that bot's chat, in a session of its own.

#![allow(clippy::map_unwrap_or, clippy::needless_pass_by_value)]

use super::limits::STEP_TIMEOUT;
use super::model::{NodeKind, field_str};
use super::run;
use crate::hub::Hub;
use crate::store::{EntryKind, Lane, now_ms};
use anyhow::Result;
use rusqlite::OptionalExtension;
use rusqlite::params;
use serde_json::json;
use std::time::Duration;

pub struct Prepared {
    pub lane: Lane,
    pub prompt: String,
    pub timeout: Duration,
}

/// The turn the actor is about to start. `None` when the step was already stopped.
pub fn begin(hub: &Hub, step_id: &str) -> Result<Option<Prepared>> {
    let Some(row) = hub.store.with_immediate(|tx| {
        tx.query_row(
            "SELECT s.status, s.input, s.workflow_id, s.run_id, r.graph_json, w.title, s.node_id, COALESCE(s.bot_id, ''), COALESCE(r.exec, ''), COALESCE(s.passes, 1), COALESCE(s.error, '')
             FROM workflow_steps s
             LEFT JOIN workflow_runs r ON r.id = s.run_id
             JOIN workflows w ON w.id = s.workflow_id
             WHERE s.id = ?1",
            [step_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, String>(8)?,
                    r.get::<_, i64>(9)?,
                    r.get::<_, String>(10)?,
                ))
            },
        )
        .optional()
        .map_err(anyhow::Error::from)
    })?
    else {
        return Ok(None);
    };
    if row.0 != "running" {
        return Ok(None);
    }
    if row.8 == "script" {
        let bot = row.7.trim();
        if bot.is_empty() || !matches!(super::store::bot_state(&hub.store, bot), super::graph::BotState::Ok) {
            run::fail_step(hub, step_id, "Choose a bot for this step")?;
            return Ok(None);
        }
        let prompt = if row.9 >= 2 {
            format!("The last reply didn't match the schema:\n{}\n\nReply with only the JSON value.", row.10)
        } else if row.1.trim().is_empty() {
            run::fail_step(hub, step_id, "The step didn't include a prompt")?;
            return Ok(None);
        } else {
            row.1.clone()
        };
        let lane = Lane::main(bot);
        let turn = hub.store.max_turn(bot) + 1;
        let data = json!({"text": format!("Workflow · {}", row.5), "workflowId": row.2, "workflowTitle": row.5, "runId": row.3, "stepId": step_id, "notice": "workflow"});
        hub.add_entry_once(&format!("workflow-step-{step_id}"), &lane, EntryKind::Notice, turn, &data)?;
        return Ok(Some(Prepared { lane, prompt, timeout: STEP_TIMEOUT }));
    }
    let graph: super::model::Graph = if let Some(raw) = row.4.as_deref().filter(|raw| !raw.is_empty()) {
        serde_json::from_str(raw)?
    } else {
        super::store::load(&hub.store, &row.2)?.map(|(_, graph)| graph).unwrap_or_else(|| super::model::Graph {
            nodes: Vec::new(),
            edges: Vec::new(),
            strokes: Vec::new(),
        })
    };
    let Some(node) = graph.nodes.into_iter().find(|n| n.id == row.6 && n.kind == NodeKind::Agent) else {
        run::fail_step(hub, step_id, "Choose a bot for this step")?;
        return Ok(None);
    };
    let bot = field_str(&node.fields, "botId").unwrap_or("").trim().to_owned();
    if bot.is_empty() || !matches!(super::store::bot_state(&hub.store, &bot), super::graph::BotState::Ok) {
        run::fail_step(hub, step_id, "Choose a bot for this step")?;
        return Ok(None);
    }
    let task = field_str(&node.fields, "task").unwrap_or("").trim();
    let incoming = row.1.trim();
    if task.is_empty() && incoming.is_empty() {
        run::fail_step(hub, step_id, "This step needs instructions")?;
        return Ok(None);
    }
    let lane = Lane::main(&bot);
    let turn = hub.store.max_turn(&bot) + 1;
    let data = json!({"text": format!("Workflow · {}", row.5), "workflowId": row.2, "workflowTitle": row.5, "runId": row.3, "stepId": step_id, "notice": "workflow"});
    hub.add_entry_once(&format!("workflow-step-{step_id}"), &lane, EntryKind::Notice, turn, &data)?;
    let prompt = if task.is_empty() {
        format!("Workflow: {}\n\n{incoming}", row.5)
    } else {
        format!("Workflow: {}\n\n{task}\n\n{incoming}", row.5)
    };
    let _ = turn;
    Ok(Some(Prepared { lane, prompt, timeout: STEP_TIMEOUT }))
}

pub fn mark_started(hub: &Hub, step_id: &str) -> Result<()> {
    hub.store.with_immediate(|tx| {
        tx.execute(
            "UPDATE workflow_steps SET started_at = COALESCE(started_at, ?2), updated_at = ?2 WHERE id = ?1",
            params![step_id, now_ms()],
        )?;
        Ok(())
    })
}

/// The actor finished. Only a step that is still running changes.
pub fn settle(hub: &Hub, step_id: &str, output: Option<String>, error: Option<String>, stopped: bool) -> Result<()> {
    let status = if stopped {
        "stopped"
    } else if error.is_some() {
        "failed"
    } else {
        "done"
    };
    let now = now_ms();
    let run: Option<String> = hub.store.with_immediate(|tx| {
        tx.execute(
            "UPDATE workflow_steps SET status = ?2, output = ?3, error = ?4, ended_at = ?5, updated_at = ?5 WHERE id = ?1 AND status = 'running'",
            params![step_id, status, output, error, now],
        )?;
        Ok(tx.query_row("SELECT run_id FROM workflow_steps WHERE id = ?1", [step_id], |r| r.get::<_, Option<String>>(0))?)
    })?;
    if let Some(run_id) = run {
        if super::script_run::is_script_run(&hub.store, &run_id) {
            if status == "done" {
                if let Some(retry) = schema_followup(hub, step_id, output.as_deref())? {
                    let _ = hub.send_cmd(&retry, crate::agent::bot::Cmd::WorkflowStep { step_id: step_id.to_owned() });
                    emit_step(hub, &run_id, step_id);
                    return Ok(());
                }
            }
            super::script_run::replay(hub, &run_id)?;
        } else {
            let effects = run::advance(hub, &run_id)?;
            run::dispatch_agents(hub, &effects);
            run::dispatch_pushes(hub, &effects);
        }
        emit_step(hub, &run_id, step_id);
    }
    Ok(())
}

fn schema_followup(hub: &Hub, step_id: &str, output: Option<&str>) -> Result<Option<String>> {
    let Some((schema, passes, bot)) = hub.store.with_immediate(|tx| {
        tx.query_row(
            "SELECT COALESCE(schema_json, ''), COALESCE(passes, 1), COALESCE(bot_id, '') FROM workflow_steps WHERE id = ?1",
            [step_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)),
        )
        .optional()
        .map_err(anyhow::Error::from)
    })?
    else {
        return Ok(None);
    };
    if schema.is_empty() || super::script_run::schema_ok(&schema, output.unwrap_or("")) {
        return Ok(None);
    }
    let now = now_ms();
    if passes >= 2 {
        hub.store.with_immediate(|tx| {
            tx.execute(
                "UPDATE workflow_steps SET status = 'failed', error = ?2, ended_at = ?3, updated_at = ?3 WHERE id = ?1",
                params![step_id, "The step didn't match the expected shape", now],
            )?;
            Ok(())
        })?;
        return Ok(None);
    }
    hub.store.with_immediate(|tx| {
        tx.execute(
            "UPDATE workflow_steps SET status = 'running', passes = 2, error = ?2, ended_at = NULL, updated_at = ?3 WHERE id = ?1",
            params![step_id, "The last reply didn't match the schema", now],
        )?;
        Ok(())
    })?;
    Ok(Some(bot))
}

fn emit_step(hub: &Hub, run_id: &str, step_id: &str) {
    let workflow: Option<String> = hub
        .store
        .with_immediate(|tx| {
            tx.query_row("SELECT workflow_id FROM workflow_runs WHERE id = ?1", [run_id], |r| r.get(0))
                .optional()
                .map_err(anyhow::Error::from)
        })
        .ok()
        .flatten();
    if let Some(workflow) = workflow {
        hub.emit_workflow(
            json!({"type": "workflow", "change": "step", "workflowId": workflow, "runId": run_id, "stepId": step_id}),
        );
    }
}

pub fn session_slot(step_id: &str) -> String {
    format!("workflow-{step_id}")
}
