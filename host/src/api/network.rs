//! Methods for this computer's networks (codync-network): only this computer's own apps call
//! them (`devices::LOCAL_ONLY`), except `networkView`, which paired phones read. `network` names
//! one by its id or name, and may be left out while the computer is in only one.

use super::str_arg;
use crate::hub::Hub;
use crate::network;
use anyhow::Result;
use serde_json::Value;
use std::ops::ControlFlow;
use std::sync::Arc;

pub(super) async fn call(hub: &Arc<Hub>, method: &str, b: Value) -> Result<ControlFlow<Value, Value>> {
    let text = |k: &str| b[k].as_str().filter(|s| !s.trim().is_empty());
    let which = text("network");
    Ok(ControlFlow::Break(match method {
        "networkStatus" => network::status(hub),
        "networkView" => network::view(hub),
        "networkCreate" => network::create(hub, which, text("name"), text("connector")).await?,
        "networkJoin" => network::join(hub, str_arg(&b, "invite")?, which, text("name"), text("connector")).await?,
        "networkLeave" => network::leave(hub, which).await?,
        "networkInvite" => network::invite(hub, which)?,
        "networkPeers" => network::peers(hub, which).await?,
        "networkRemove" => network::remove(hub, which, str_arg(&b, "computer")?, b["cascade"] == true)?,
        "networkPromote" => network::promote(hub, which, str_arg(&b, "computer")?)?,
        "networkRecover" => network::recover(hub, which, str_arg(&b, "code")?)?,
        "setNetworkRules" => network::set_rules(hub, which, &b["rules"]).await?,
        _ => return Ok(ControlFlow::Continue(b)),
    }))
}
