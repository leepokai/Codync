//! Native Hermes transports; the host owns routing, authorization and durable delivery.

mod approvals;
pub(crate) mod apps;
mod config;
mod ingress;
mod media;
mod outgoing;
pub(crate) mod webhook;
mod worker;

use crate::{LockExt, agent::bot::Cmd, hub::Hub, market::Connector};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

pub(crate) use config::{claim_prompt, prepare as prepare_connector};
pub(crate) use worker::run;
pub(crate) const METHODS: &[&str] = &["nativeChannelStatus", "nativeChannelPairing", "nativeChannelSetAllowed"];

pub(crate) fn is_native(c: &Connector) -> bool {
    config::kind(c).is_some()
}

pub(crate) struct Snapshot {
    connection: Connector,
    generation: Option<String>,
}
pub(crate) fn snapshot(hub: &Hub, id: &str) -> Result<Option<Snapshot>> {
    config::current(&hub.store, id)?
        .map(|connection| Ok(Snapshot { generation: config::generation(&hub.store, &connection)?, connection }))
        .transpose()
}
pub(crate) fn snapshots(hub: &Hub) -> Result<Vec<Snapshot>> {
    crate::market::connectors(&hub.store)?
        .into_iter()
        .filter(is_native)
        .map(|connection| Ok(Snapshot { generation: config::generation(&hub.store, &connection)?, connection }))
        .collect()
}
pub(crate) fn changed(hub: &Hub, previous: Option<Snapshot>) -> Result<()> {
    if let Some(previous) = previous {
        if !config::matches(&hub.store, &previous.connection)? {
            revoke(hub, &previous.connection.id, previous.generation, None)?;
        }
    }
    Ok(())
}

/// Cancel only turns authenticated by this connection's previous identity.
pub(crate) fn revoke(hub: &Hub, connection: &str, generation: Option<String>, user: Option<String>) -> Result<()> {
    for bot in hub.store.bots()?.into_iter().filter(|b| !b.deleted && !b.config.is_group()) {
        // Stopped bots have no active turn; queued input remains fenced at actor start.
        if let Err(error) = hub.send_cmd(
            &bot.config.id,
            Cmd::NativeAuthorizationChanged {
                connection: connection.into(),
                generation: generation.clone(),
                user: user.clone(),
            },
        ) {
            tracing::debug!(%error,bot=%bot.config.id,"Native actor already stopped");
        }
    }
    Ok(())
}

pub(crate) fn call(hub: &Hub, method: &str, body: &Value) -> Result<Value> {
    let id = body["id"].as_str().context("Connection ID is required")?;
    let _guard = hub.store.connector_lock.locked();
    let c = config::current(&hub.store, id)?.context("Unknown native connection")?;
    let generation = config::generation(&hub.store, &c)?.context("Native account is not ready")?;
    match method {
        "nativeChannelStatus" => Ok(
            json!({"id":id,"platform":config::kind(&c),"state":config::state(&hub.store,id)?,"allowedUsers":hub.store.native_users(id,&generation)?}),
        ),
        "nativeChannelPairing" => {
            ensure!(config::kind(&c) != Some("homeassistant"), "Home Assistant uses its selected entity identities");
            let code = uuid::Uuid::new_v4().simple().to_string();
            let expires = crate::store::now_ms() + 10 * 60 * 1000;
            hub.store.native_pairing(id, &generation, &config::digest(&code), expires)?;
            Ok(json!({"code":code,"expiresAt":expires,"command":format!("/pair {code}")}))
        }
        "nativeChannelSetAllowed" => {
            let user = body["user"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 512)
                .context("Native sender is required")?;
            let allowed = body["allowed"].as_bool().context("Allowed decision is required")?;
            hub.store.native_set_user(id, &generation, user, allowed)?;
            if !allowed {
                revoke(hub, id, Some(generation), Some(user.into()))?;
            }
            Ok(json!({"ok":true}))
        }
        _ => anyhow::bail!("Unknown native channel method"),
    }
}
