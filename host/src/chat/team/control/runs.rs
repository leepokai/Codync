//! The owner's own work an outside app may list, and with Full access start: routine and workflow runs, which pick
//! their own approvals.

use super::access::{Code, Refusal};
use super::clients;
use super::threads::Caller;
use crate::hub::Hub;
use anyhow::Result;
use serde_json::{Value, json};
use std::sync::Arc;

fn bot<'a>(hub: &Hub, me: &Caller, args: &'a Value) -> Result<&'a str> {
    let id = args["botId"].as_str().ok_or_else(|| Refusal::err(Code::InvalidRequest, "`botId` is required"))?;
    let reaches =
        hub.store.bot(id)?.filter(|r| !r.deleted && !r.config.plain).is_some_and(|r| me.principal.reaches(&r.config));
    if !reaches {
        return Err(Refusal::err(Code::NotFound, "no such bot you may use"));
    }
    Ok(id)
}

pub fn routines(hub: &Arc<Hub>, me: &Caller<'_>, args: &Value) -> Result<Value> {
    let bots: Vec<String> = match args["botId"].as_str() {
        Some(_) => vec![bot(hub, me, args)?.to_owned()],
        None => hub
            .store
            .bots()?
            .into_iter()
            .filter(|r| !r.deleted && !r.config.plain && !r.config.is_thread() && me.principal.reaches(&r.config))
            .map(|r| r.config.id)
            .collect(),
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
    let bot = bot(hub, me, args)?.to_owned();
    let routine =
        args["routineId"].as_str().ok_or_else(|| Refusal::err(Code::InvalidRequest, "`routineId` is required"))?;
    let (hub2, client, key, routine) =
        (hub.clone(), me.principal.id.clone(), args["clientRequestId"].as_str().map(str::to_owned), routine.to_owned());
    let (run, replayed) = tokio::task::spawn_blocking(move || {
        clients::once(&hub2.store, &client, key.as_deref(), || {
            hub2.routines.enqueue(&hub2.store, &bot, &routine, json!({"source": "mcp"}), None, false)
        })
    })
    .await??;
    Ok(json!({"run": run, "replayed": replayed}))
}

pub fn workflows(hub: &Arc<Hub>) -> Result<Value> {
    hub.workflows.list(hub)
}

pub async fn run_workflow(hub: &Arc<Hub>, me: &Caller<'_>, args: &Value) -> Result<Value> {
    let id =
        args["workflowId"].as_str().ok_or_else(|| Refusal::err(Code::InvalidRequest, "`workflowId` is required"))?;
    let input = args["input"].as_str().unwrap_or_default().to_owned();
    let (hub2, client, key, id) =
        (hub.clone(), me.principal.id.clone(), args["clientRequestId"].as_str().map(str::to_owned), id.to_owned());
    let (run, replayed) = tokio::task::spawn_blocking(move || {
        clients::once(&hub2.store, &client, key.as_deref(), || {
            hub2.workflows.start_run(&hub2, &id, &uuid::Uuid::new_v4().to_string(), None, &input)
        })
    })
    .await??;
    Ok(json!({"run": run, "replayed": replayed}))
}
