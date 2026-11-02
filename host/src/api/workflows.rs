//! Workflow methods. A conflict is a normal answer: the editor resets from it.

use super::str_arg;
use crate::hub::Hub;
use crate::workflows::Graph;
use anyhow::{Context, Result};
use serde_json::Value;
use std::ops::ControlFlow;
use std::sync::Arc;

pub(super) async fn call(hub: &Arc<Hub>, method: &str, b: Value) -> Result<ControlFlow<Value, Value>> {
    let answer = match method {
        "workflows" => hub.workflows.list(hub)?,
        "workflowCall" => {
            crate::workflows::tool_call(
                &hub.workflows,
                hub,
                str_arg(&b, "botId")?,
                str_arg(&b, "name")?,
                &b["arguments"],
            )
            .await?
        }
        "workflow" => hub.workflows.workflow(hub, str_arg(&b, "id")?, b["reveal"].as_bool().unwrap_or(false))?,
        "createWorkflow" => {
            hub.workflows.create(hub, b["title"].as_str().unwrap_or(""), graph_of(&b)?, b["script"].as_str(), None)?
        }
        "saveWorkflowScript" => hub.workflows.save_script(
            hub,
            str_arg(&b, "id")?,
            b["baseRevision"].as_i64().context("`baseRevision` is required")?,
            b["script"].as_str(),
            triggers_of(&b)?,
        )?,
        "renameWorkflow" => hub.workflows.rename(hub, str_arg(&b, "id")?, str_arg(&b, "title")?)?,
        "deleteWorkflow" => hub.workflows.remove(hub, str_arg(&b, "id")?)?,
        "setWorkflowEnabled" => hub.workflows.set_enabled(
            hub,
            str_arg(&b, "id")?,
            b["enabled"].as_bool().context("`enabled` is required")?,
        )?,
        "saveWorkflowGraph" => {
            let graph = serde_json::from_value(b["graph"].clone()).context("`graph` is required")?;
            hub.workflows.save(
                hub,
                str_arg(&b, "id")?,
                b["baseRevision"].as_i64().context("`baseRevision` is required")?,
                &graph,
            )?
        }
        "workflowVersions" => hub.workflows.versions(hub, str_arg(&b, "id")?)?,
        "workflowVersion" => hub.workflows.version(hub, str_arg(&b, "id")?, str_arg(&b, "versionId")?)?,
        "restoreWorkflowVersion" => {
            crate::workflows::restore(&hub.workflows, hub, str_arg(&b, "id")?, str_arg(&b, "versionId")?)?
        }
        "startWorkflowRun" => hub.workflows.start_run(
            hub,
            str_arg(&b, "id")?,
            str_arg(&b, "requestId")?,
            b["fromNodeId"].as_str(),
            b["input"].as_str().unwrap_or(""),
        )?,
        "stopWorkflowRun" => hub.workflows.stop_run(hub, str_arg(&b, "runId")?)?,
        "decideWorkflowStep" => hub.workflows.decide(
            hub,
            str_arg(&b, "stepId")?,
            b["approved"].as_bool().context("`approved` is required")?,
        )?,
        "workflowRuns" => {
            let before = b["before"].as_object();
            hub.workflows.runs(
                hub,
                str_arg(&b, "id")?,
                b["limit"].as_i64().unwrap_or(20),
                before.and_then(|v| v["startedAt"].as_i64()),
                before.and_then(|v| v["id"].as_str()),
            )?
        }
        "workflowRun" => hub.workflows.run(hub, str_arg(&b, "runId")?)?,
        "rotateWorkflowWebhook" => hub.workflows.rotate(hub, str_arg(&b, "id")?, str_arg(&b, "nodeId")?)?,
        "runWorkflowStep" => {
            hub.workflows.run_step(hub, str_arg(&b, "id")?, str_arg(&b, "nodeId")?, str_arg(&b, "requestId")?)?
        }
        "stopWorkflowStep" => hub.workflows.stop_step(hub, str_arg(&b, "stepId")?)?,
        "retryWorkflowStep" => hub.workflows.retry_step(hub, str_arg(&b, "stepId")?)?,
        _ => return Ok(ControlFlow::Continue(b)),
    };
    Ok(ControlFlow::Break(answer))
}

fn triggers_of(body: &Value) -> Result<Option<Graph>> {
    if body.get("triggers").is_none() || body["triggers"].is_null() {
        return Ok(None);
    }
    if body["triggers"].is_array() {
        let nodes = serde_json::from_value(body["triggers"].clone()).context("`triggers` is invalid")?;
        return Ok(Some(Graph { nodes, edges: Vec::new(), strokes: Vec::new() }));
    }
    Ok(Some(serde_json::from_value(body["triggers"].clone()).context("`triggers` is invalid")?))
}

fn graph_of(body: &Value) -> Result<Option<Graph>> {
    if body.get("graph").is_none() || body["graph"].is_null() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_value(body["graph"].clone()).context("`graph` is invalid")?))
}
