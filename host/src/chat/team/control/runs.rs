//! The owner's own work an outside app may list, and with Full access start: routine and workflow runs, which pick
//! their own approvals. Workflows aren't limited by bot: an app with the workflows scope sees, and at Full runs,
//! every one (docs/features/mcp-access.md).

use super::access::{Code, Refusal};
use super::{Caller, calls, reachable, text, usable};
use crate::hub::Hub;
use anyhow::Result;
use serde_json::{Value, json};
use std::sync::Arc;

/// The bot the call names, if the caller may use it.
fn bot(hub: &Hub, me: &Caller, args: &Value) -> Result<String> {
    let id = text(args, "botId")?;
    usable(hub, me.principal, id).map(|c| c.id).ok_or_else(|| Refusal::err(Code::NotFound, "no such bot you may use"))
}

pub fn routines(hub: &Arc<Hub>, me: &Caller<'_>, args: &Value) -> Result<Value> {
    let bots: Vec<String> = match args["botId"].as_str() {
        Some(_) => vec![bot(hub, me, args)?],
        None => reachable(hub, me.principal).into_iter().map(|c| c.id).collect(),
    };
    let listed: Vec<Value> = bots
        .iter()
        .map(|id| {
            let mut v = hub.routines.list(id);
            v["botId"] = id.clone().into();
            v
        })
        .filter(|v| v["routines"].as_array().is_some_and(|r| !r.is_empty()))
        .collect();
    Ok(json!({"bots": listed}))
}

pub async fn run_routine(hub: &Arc<Hub>, me: &Caller<'_>, args: &Value) -> Result<Value> {
    let bot = bot(hub, me, args)?;
    let routine = text(args, "routineId")?.to_owned();
    let (run, replayed) = calls::once_blocking(hub, me, "routine_run", args, move |hub| {
        hub.routines.enqueue(&hub.store, &bot, &routine, json!({"source": "mcp"}), None, false)
    })
    .await?;
    Ok(json!({"run": run, "replayed": replayed}))
}

pub fn workflows(hub: &Arc<Hub>) -> Result<Value> {
    hub.workflows.list(hub)
}

pub async fn run_workflow(hub: &Arc<Hub>, me: &Caller<'_>, args: &Value) -> Result<Value> {
    let id = text(args, "workflowId")?.to_owned();
    let input = args["input"].as_str().unwrap_or_default().to_owned();
    let (run, replayed) = calls::once_blocking(hub, me, "workflow_run", args, move |hub| {
        hub.workflows.start_run(hub, &id, &uuid::Uuid::new_v4().to_string(), None, &input)
    })
    .await?;
    Ok(json!({"run": run, "replayed": replayed}))
}

#[cfg(test)]
mod tests;
