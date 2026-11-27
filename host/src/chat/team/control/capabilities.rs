//! `worker_capabilities`: what an outside app may do here, on which bots and agents, before it starts anything.

use super::threads::{MAX_LIVE, MAX_MESSAGE, wait_secs};
use super::{Caller, calls, clients, reachable};
use crate::hub::Hub;
use anyhow::Result;
use serde_json::{Value, json};
use std::sync::Arc;

pub async fn capabilities(hub: &Arc<Hub>, me: &Caller<'_>, args: &Value) -> Result<Value> {
    let p = me.principal;
    let mut agents: Vec<Value> = codync_agent::backends::list()
        .into_iter()
        .filter(|a| a["installed"] == true)
        .map(|a| json!({"id": a["id"], "name": a["name"], "installed": a["installed"], "signedIn": a["signedIn"]}))
        .collect();
    crate::agent::login_profiles::annotate(&hub.store, &mut agents);
    for agent in &mut agents {
        // Only what picking a login needs.
        let profiles: Vec<Value> = agent["loginProfiles"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| json!({"id": p["id"], "name": p["name"]}))
            .collect();
        *agent = json!({"id": agent["id"], "name": agent["name"], "installed": agent["installed"],
            "signedIn": agent["signedIn"], "loginProfiles": profiles});
    }
    let bots: Vec<Value> = reachable(hub, p)
        .iter()
        .map(|c| json!({"botId": c.id, "name": c.name, "agent": c.backend, "folder": c.cwd, "projects": c.projects}))
        .collect();
    let mut out = json!({
        "you": {
            "name": p.label, "level": me.ceiling(), "approved": p.approved,
            "computerMaximum": clients::max_access(&hub.store), "scopes": p.scopes,
            "threadsWithoutABot": p.plain_threads, "folders": p.folders,
        },
        "bots": bots,
        "agents": agents,
        "approvals": {
            "ask": "every request is an approval card on the user's devices (needs level ask)",
            "auto": "the agent's own auto mode; what it asks for is a card (needs level auto)",
            "full": "requests approved by themselves, in the agent mode you pick (needs level full)",
        },
        "approvalsAnsweredHere": false,
        "limits": {
            "callsPerMinute": calls::CALLS_PER_MINUTE, "liveThreads": MAX_LIVE, "messageBytes": MAX_MESSAGE,
            "waitSecs": wait_secs(p),
        },
    });
    if let Some(agent) = args["agent"].as_str().filter(|a| !a.is_empty()) {
        if p.cloud {
            out["models"] = Value::Null;
            out["note"] = "Model lists aren't fetched over the cloud: leave model out for the agent's default.".into();
        } else {
            out["models"] = crate::agent::auth::models(&hub.store, agent, None).await?["models"].clone();
        }
    }
    Ok(out)
}
