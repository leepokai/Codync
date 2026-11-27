//! The outside apps the user lets drive this computer's work over MCP (`chat::team::control::clients`). Giving an app
//! access, widening it and the computer's own levels are decided at the computer (`devices::LOCAL_ONLY`); a phone may
//! list the apps and take one's access away.

use super::str_arg;
use crate::chat::team::control::clients;
use crate::hub::Hub;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::ops::ControlFlow;
use std::sync::Arc;

pub(super) fn call(hub: &Arc<Hub>, method: &str, b: Value) -> Result<ControlFlow<Value, Value>> {
    let grant = |b: &Value| serde_json::from_value::<clients::Grant>(b.clone()).context("an app's access");
    Ok(ControlFlow::Break(match method {
        "mcpClients" => clients::list(&hub.store),
        // The token is in this answer only: it is never kept, logged or shown again.
        "mcpClientCreate" => {
            let (client, token) = clients::create(hub, grant(&b)?)?;
            json!({"client": client, "command": clients::command(&token), "token": token})
        }
        "mcpClientUpdate" => json!({"client": clients::update(&hub.store, str_arg(&b, "id")?, grant(&b)?)?}),
        "mcpClientRevoke" => {
            clients::revoke(hub, str_arg(&b, "id")?)?;
            json!({})
        }
        "setMcpAccess" => clients::set_levels(&hub.store, &b)?,
        _ => return Ok(ControlFlow::Continue(b)),
    }))
}
