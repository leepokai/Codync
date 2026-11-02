//! Tools a bot on this computer uses to read and rewrite its own workflows.

use super::model::Graph;
use super::service::Workflows;
use super::store;
use crate::agent::bot::Cmd;
use crate::chat::manage::{APPROVAL_TIMEOUT, MAX_CARD_DETAIL};
use crate::hub::Hub;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::oneshot;
use uuid::Uuid;

pub const INSTRUCTIONS: &str = "Workflows you created on this computer are graphs you can also read and rewrite as a script. list_workflows and read_workflow show only yours. save_workflow and run_workflow wait for the user to allow them, and a new workflow starts enabled. Every agent() call names your own bot; an empty or another bot's id is refused. For another bot's workflow, ask that bot with message_bot. Use list_workflows and read_workflow before changing one. save_workflow sends the whole script and the revision you read; a conflict means someone else saved first, so read it again. A save changes the next run, not the one already going. Do not put webhook secrets in the script.";

pub fn tools() -> Value {
    json!([
        {"name":"list_workflows","description":"List this bot's workflows.","inputSchema":{"type":"object","properties":{}}},
        {"name":"read_workflow","description":"Read one of this bot's workflows, including its script and revision.","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}},
        {"name":"save_workflow","description":"Replace one of this bot's scripts, after the user allows it. Send baseRevision from read_workflow. Omit id to create one.","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"baseRevision":{"type":"integer"},"script":{"type":"string"},"title":{"type":"string"},"triggers":{"type":"array"}},"required":["script"]}},
        {"name":"run_workflow","description":"Start one of this bot's workflows, after the user allows it. Fails when one run is already open.","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"input":{"type":"string"}},"required":["id"]}}
    ])
}

pub async fn call(workflows: &Workflows, hub: &Arc<Hub>, bot: &str, name: &str, args: &Value) -> Result<Value> {
    match name {
        "list_workflows" => {
            let rows = store::list_owned(&hub.store, bot)?;
            let workflows: Vec<_> = rows.into_iter().filter(|row| !row.is_null()).collect();
            Ok(json!({"workflows": workflows}))
        }
        "read_workflow" => {
            let id = required(args, "id")?;
            require_owner(hub, bot, id)?;
            workflows.workflow(hub, id, false)
        }
        "save_workflow" => {
            let script = args["script"].as_str().context("`script` is required")?;
            own_agents(script, bot)?;
            let title = args["title"].as_str().unwrap_or("Workflow");
            if let Some(id) = args["id"].as_str() {
                require_owner(hub, bot, id)?;
                let base = args["baseRevision"].as_i64().context("`baseRevision` is required")?;
                confirm(
                    hub,
                    bot,
                    format!("Save workflow {title}"),
                    format!(
                        "Replace this workflow's script ({len} characters). It stays available to you.\n\n{script}",
                        len = script.chars().count(),
                        script = clip(script)
                    ),
                )
                .await?;
                workflows.save_script(hub, id, base, Some(script), triggers_of(args)?)
            } else {
                confirm(
                    hub,
                    bot,
                    format!("Save workflow {title}"),
                    format!(
                        "Create this workflow ({len} characters). It starts enabled.\n\n{script}",
                        len = script.chars().count(),
                        script = clip(script)
                    ),
                )
                .await?;
                workflows.create(hub, title, None, Some(script), Some(bot))
            }
        }
        "run_workflow" => {
            let id = required(args, "id")?;
            require_owner(hub, bot, id)?;
            let title =
                store::load(&hub.store, id)?.map(|(summary, _)| summary["title"].as_str().unwrap_or(id).to_owned());
            let title = title.unwrap_or_else(|| id.to_owned());
            confirm(
                hub,
                bot,
                format!("Run workflow {title}"),
                "Start this workflow. It can message the bots in its steps, including their tools and computer use."
                    .into(),
            )
            .await?;
            workflows.start_run(hub, id, &Uuid::new_v4().to_string(), None, args["input"].as_str().unwrap_or(""))
        }
        _ => bail!("unknown workflow tool"),
    }
}

fn require_owner(hub: &Hub, bot: &str, id: &str) -> Result<()> {
    match store::owner(&hub.store, id)? {
        Some(owner) if owner == bot => Ok(()),
        _ => bail!(
            "workflow not found among yours: for another bot's workflow, ask that bot with message_bot to run it with \
             its own run_workflow"
        ),
    }
}

fn own_agents(script: &str, bot: &str) -> Result<()> {
    if super::script::agent_ids(script)?.iter().any(|id| id != bot) {
        bail!("Name your own bot in agent()");
    }
    Ok(())
}

async fn confirm(hub: &Hub, bot: &str, title: String, detail: String) -> Result<()> {
    let (reply, answer) = oneshot::channel();
    hub.send_cmd(bot, Cmd::Approve { title, detail: clip(&detail), reply })?;
    match tokio::time::timeout(APPROVAL_TIMEOUT, answer).await {
        Ok(Ok(true)) => Ok(()),
        Ok(_) => bail!("the user didn't allow this change; nothing changed"),
        Err(_) => bail!("the user didn't answer the approval card in time; nothing changed"),
    }
}

fn clip(text: &str) -> String {
    let mut count = 0usize;
    let mut end = text.len();
    for (index, ch) in text.char_indices() {
        let next = count + ch.len_utf16();
        if next > MAX_CARD_DETAIL.saturating_sub(1) {
            end = index;
            break;
        }
        count = next;
    }
    if end == text.len() { text.to_owned() } else { format!("{}…", &text[..end]) }
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
