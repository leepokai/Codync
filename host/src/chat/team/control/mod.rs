//! Outside apps drive this computer's workers ([MCP access](../../../../../docs/features/mcp-access.md)): the worker
//! tools of the `codync` MCP server (threads, routines, workflows), for a coding agent's session here
//! (`codync-host mcp network`, with an app's token or without) and the user's cloud assistant alike. Each call is
//! one principal's ([`clients`]), admitted by its tool's class and level ([`access`]) before its handler runs; none
//! ever approves a request.

pub mod access;
pub mod clients;
mod runs;
mod schema;
mod threads;

pub use access::Level;

use crate::hub::Hub;
use access::{Code, Refusal};
use anyhow::Result;
use clients::Principal;
use serde_json::{Value, json};
use std::sync::Arc;

/// What lives while the host runs: each principal's calls of the last minute.
#[derive(Default)]
pub struct Control {
    calls: clients::Calls,
}

pub fn tools() -> Vec<Value> {
    schema::tools()
}

/// Whether `tool` is one of the worker tools.
pub fn has(tool: &str) -> bool {
    access::class(tool, &Value::Null).is_some_and(|class| class != access::Class::Messages)
}

/// Runs worker tool `tool` for `principal`, after it is admitted, and keeps the call in the audit.
pub async fn call(hub: &Arc<Hub>, principal: &Principal, tool: &str, args: &Value) -> Result<Value> {
    let outcome = admitted(hub, principal, tool, args).await;
    let code = outcome.as_ref().err().and_then(|e| e.downcast_ref::<Refusal>()).map(|r| r.code);
    let word = match (&outcome, code) {
        (Ok(_), _) => "ok",
        (Err(_), Some(_)) => "refused",
        (Err(_), None) => "failed",
    };
    let class = access::class(tool, args).map(access::Class::word);
    let at = crate::store::now_ms();
    let record =
        json!({"client": principal.id, "via": principal.via, "tool": tool, "outcome": word, "code": code, "at": at});
    clients::record(&hub.store, &record);
    let properties = json!({"tool": tool, "class": class, "outcome": word, "code": code, "cloud": principal.cloud});
    crate::analytics::capture(
        hub,
        crate::analytics::Event::McpCall,
        properties.as_object().cloned().unwrap_or_default(),
    );
    outcome
}

async fn admitted(hub: &Arc<Hub>, principal: &Principal, tool: &str, args: &Value) -> Result<Value> {
    hub.control.calls.take(&principal.id)?;
    let limit = clients::level_arg(args, "accessLimit")?;
    let ceiling = access::ceiling(principal.approved, clients::max_access(&hub.store), limit);
    let admitted = access::admit(tool, args, ceiling, &principal.scopes)?;
    let me = threads::Caller { principal, admitted: &admitted };
    match tool {
        "worker_capabilities" => threads::capabilities(hub, &me, args).await,
        "thread_list" => threads::list(hub, &me, args).await,
        "thread_launch" => threads::launch(hub, &me, args).await,
        "thread_send" => threads::send(hub, &me, args).await,
        "thread_wait" => threads::wait(hub, &me, args).await,
        "thread_read" => threads::read(hub, &me, args),
        "thread_steer" => threads::steer(hub, &me, args).await,
        "thread_interrupt" => threads::interrupt(hub, &me, args),
        "thread_pending" => threads::pending(hub, &me, args).await,
        "routine_list" => runs::routines(hub, &me, args),
        "routine_run" => runs::run_routine(hub, &me, args).await,
        "workflow_list" => runs::workflows(hub),
        "workflow_run" => runs::run_workflow(hub, &me, args).await,
        _ => Err(Refusal::err(Code::NotFound, format!("unknown tool: {tool}"))),
    }
}
