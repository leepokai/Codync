//! The flow tools of the built-in `routines` MCP server: a bot builds its routines' flows by talking.

use super::{MAX_INSTRUCTION_BYTES, api};
use crate::hub::Hub;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::sync::Arc;

pub const TOOLS: &[&str] = &["link_routine_bots", "unlink_routine_bots", "list_routine_flow"];

pub fn tools() -> Vec<Value> {
    let id = json!({"type": "string"});
    vec![
        json!({"name": "link_routine_bots", "description": format!("Make one of your routines hand its reply on to another bot (a routine flow). When the routine's run ends, its reply goes to toBotId with your instruction (at most {MAX_INSTRUCTION_BYTES} bytes), and that bot's reply can go on to the next link. fromBotId is you unless you chain from a bot the flow already reaches. A flow can't loop, can't reach more than a few bots deep, and can't end in a group or a terminal session. Use list_bots for ids. Linking only applies to the routine's later runs."),
            "inputSchema": {"type": "object", "properties": {"routineId": id, "toBotId": id, "fromBotId": id, "instruction": {"type": "string"}}, "required": ["routineId", "toBotId"]}}),
        json!({"name": "unlink_routine_bots", "description": "Remove a link of one of your routines' flows: by linkId, or by routineId with toBotId (and fromBotId when it isn't you).",
            "inputSchema": {"type": "object", "properties": {"linkId": id, "routineId": id, "toBotId": id, "fromBotId": id}}}),
        json!({"name": "list_routine_flow", "description": "List the flows of your routines: each routine's links (who hands to whom, with the instruction) and the steps of its latest run with their status. routineId narrows it to one routine.",
            "inputSchema": {"type": "object", "properties": {"routineId": id}}}),
    ]
}

fn text<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args[key].as_str().filter(|s| !s.is_empty()).with_context(|| format!("{key} is required"))
}

pub fn call(hub: &Arc<Hub>, bot: &str, name: &str, args: &Value) -> Result<Value> {
    match name {
        "link_routine_bots" => {
            let routine_id = text(args, "routineId")?;
            let routine = owned(hub, bot, routine_id)?;
            let from = args["fromBotId"].as_str().unwrap_or(bot);
            let instruction = args["instruction"].as_str().unwrap_or_default().trim();
            if instruction.len() > MAX_INSTRUCTION_BYTES {
                bail!("instruction must be at most {MAX_INSTRUCTION_BYTES} bytes");
            }
            let link = api::connect(hub, &routine, from, text(args, "toBotId")?, instruction.to_owned())?;
            Ok(json!({"link": link, "note": "Linked. The routine's next run hands its reply on."}))
        }
        "unlink_routine_bots" => {
            let link = if let Some(id) = args["linkId"].as_str() {
                hub.store.flow_link(id)?.context("link not found")?
            } else {
                let (routine_id, to) = (text(args, "routineId")?, text(args, "toBotId")?);
                let from = args["fromBotId"].as_str().unwrap_or(bot);
                hub.store
                    .flow_links(routine_id)?
                    .into_iter()
                    .find(|l| l.from_bot_id == from && l.to_bot_id == to)
                    .context("link not found")?
            };
            owned(hub, bot, &link.routine_id)?;
            api::disconnect(hub, &link.id)?;
            Ok(json!({"ok": true}))
        }
        _ => list(hub, bot, args["routineId"].as_str()),
    }
}

/// A routine of this bot; a flow is managed by the bot whose routine starts it.
fn owned(hub: &Hub, bot: &str, routine_id: &str) -> Result<crate::routines::Routine> {
    if !hub.routines.owns(bot, routine_id) {
        bail!("routine not found among yours: only the bot that owns a routine can change its flow");
    }
    api::routine(hub, routine_id)
}

fn list(hub: &Hub, bot: &str, only: Option<&str>) -> Result<Value> {
    let mut flows = vec![];
    for routine in hub.routines.canvas_routines(&hub.store, bot)?.into_iter().filter(|r| r.bot_id == bot) {
        if only.is_some_and(|id| id != routine.id) {
            continue;
        }
        let links: Vec<Value> = hub
            .store
            .flow_links(&routine.id)?
            .into_iter()
            .map(|l| {
                json!({"id": l.id, "fromBotId": l.from_bot_id, "from": hub.bot_name(&l.from_bot_id),
                    "toBotId": l.to_bot_id, "to": hub.bot_name(&l.to_bot_id), "instruction": l.instruction})
            })
            .collect();
        flows.push(json!({"routineId": routine.id, "name": routine.name, "links": links,
            "latestRun": hub.store.flow_last_steps(&routine.id)?}));
    }
    Ok(json!({"flows": flows}))
}
