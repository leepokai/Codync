//! Tools a bot on this computer uses to read and rewrite workflows.

use super::model::Graph;
use super::service::Workflows;
use crate::hub::Hub;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use uuid::Uuid;

pub const INSTRUCTIONS: &str = "Workflows on this computer are graphs you can also read and rewrite as a script. Use list_workflows and read_workflow before changing one. save_workflow sends the whole script and the revision you read; a conflict means someone else saved first, so read it again. A save changes the next run, not the one already going. run_workflow starts it. Do not put webhook secrets in the script.";

pub fn tools() -> Value {
    json!([
        {"name":"list_workflows","description":"List workflows on this computer.","inputSchema":{"type":"object","properties":{}}},
        {"name":"read_workflow","description":"Read one workflow, including its script and revision.","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}},
        {"name":"save_workflow","description":"Replace a workflow's script. Send baseRevision from read_workflow. Omit id to create one.","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"baseRevision":{"type":"integer"},"script":{"type":"string"},"title":{"type":"string"},"triggers":{"type":"array"}},"required":["script"]}},
        {"name":"run_workflow","description":"Start a workflow. Fails when one run is already open.","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"input":{"type":"string"}},"required":["id"]}}
    ])
}

pub fn call(workflows: &Workflows, hub: &Hub, name: &str, args: &Value) -> Result<Value> {
    match name {
        "list_workflows" => workflows.list(hub),
        "read_workflow" => workflows.workflow(hub, required(args, "id")?, false),
        "save_workflow" => {
            let script = args["script"].as_str();
            if let Some(id) = args["id"].as_str() {
                let base = args["baseRevision"].as_i64().context("`baseRevision` is required")?;
                workflows.save_script(hub, id, base, script, triggers_of(args)?)
            } else {
                workflows.create(hub, args["title"].as_str().unwrap_or("Workflow"), None, script)
            }
        }
        "run_workflow" => {
            let id = required(args, "id")?;
            workflows.start_run(hub, id, &Uuid::new_v4().to_string(), None, args["input"].as_str().unwrap_or(""))
        }
        _ => bail!("unknown workflow tool"),
    }
}

fn required<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args[key].as_str().filter(|s| !s.is_empty()).with_context(|| format!("`{key}` is required"))
}

fn triggers_of(args: &Value) -> Result<Option<Graph>> {
    if args.get("triggers").is_none() || args["triggers"].is_null() {
        return Ok(None);
    }
    let nodes = serde_json::from_value(args["triggers"].clone()).context("`triggers` is invalid")?;
    Ok(Some(Graph { nodes, edges: Vec::new(), strokes: Vec::new() }))
}
