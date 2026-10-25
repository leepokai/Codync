//! Methods for this computer's network (codync-network): only this computer's own apps call
//! them (`devices::LOCAL_ONLY`).

use super::str_arg;
use crate::hub::Hub;
use crate::network;
use anyhow::Result;
use serde_json::Value;
use std::ops::ControlFlow;
use std::sync::Arc;

pub(super) async fn call(hub: &Arc<Hub>, method: &str, b: Value) -> Result<ControlFlow<Value, Value>> {
    let text = |k: &str| b[k].as_str().filter(|s| !s.trim().is_empty());
    Ok(ControlFlow::Break(match method {
        "networkStatus" => network::status(hub),
        "networkCreate" => network::create(hub, text("name"), text("connector")).await?,
        "networkJoin" => network::join(hub, str_arg(&b, "invite")?, text("name"), text("connector")).await?,
        "networkInvite" => network::invite(hub)?,
        "networkPeers" => network::peers(hub).await?,
        "networkRemove" => network::remove(hub, str_arg(&b, "computer")?, b["cascade"] == true)?,
        "networkPromote" => network::promote(hub, str_arg(&b, "computer")?)?,
        "networkRecover" => network::recover(hub, str_arg(&b, "code")?)?,
        _ => return Ok(ControlFlow::Continue(b)),
    }))
}
