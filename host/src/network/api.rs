//! The `network*` API methods: what the apps show of this computer's networks, and the changes
//! made at the computer (loopback only, like pairing a device).

use super::saved::{self, Saved};
use super::{Slot, default_name, start_node, take_in};
use crate::LockExt;
use crate::hub::Hub;
use anyhow::{Context, Result, bail};
use codync_network::{Connector, Node, Role, Rule};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Instant;

/// The `network` event: what `networkView` answers.
pub fn emit(hub: &Hub) {
    let _ = hub.events.send(json!({"type": "network", "network": status(hub)}));
}

/// `networkStatus`, the `network` event and the stream's hello: every network this computer is in
/// (`networks`, the default first), each with its `key` (the network's own id, the same on every
/// computer in it), its computers as last heard, what this computer may do there (`canInvite`,
/// `canRecover`) and to each (`canRemove`, `canPromote`), and its rules;
/// plus the cloud assistants switch, and whether they can reach this computer at all
/// (`assistantsAvailable`). Never an invite: phones read it too.
pub fn status(hub: &Hub) -> Value {
    let networks: Vec<Value> = hub.network.slots().iter().filter(|s| s.node.name().is_some()).map(network).collect();
    json!({
        "joined": !networks.is_empty(), "networks": networks,
        "assistants": crate::chat::team::assistant::enabled(&hub.store),
        "assistantsAvailable": crate::chat::team::assistant::available(hub),
    })
}

fn network(slot: &Slot) -> Value {
    let node = &slot.node;
    let mine = node.role();
    let computers: Vec<Value> = node
        .directory()
        .into_iter()
        .map(|p| {
            let agents: Vec<Value> =
                p.agents.iter().map(|a| json!({"id": a.id, "name": a.name, "kind": a.kind})).collect();
            let may = |rule: fn(Role, Role) -> bool| !p.me && mine.is_some_and(|me| rule(me, p.role));
            json!({
                "name": p.name, "role": p.role, "me": p.me, "online": p.online, "agents": agents,
                "canRemove": may(Role::removes), "canPromote": may(Role::promotes),
            })
        })
        .collect();
    json!({
        "id": slot.id, "key": node.network_id(), "name": slot.name, "computer": node.name(), "role": mine,
        "canInvite": mine.is_some_and(Role::invites), "canRecover": mine.is_some_and(|r| r != Role::Owner),
        "connector": slot.connector,
        "computers": computers, "rules": node.rules(),
    })
}

/// `networkView`: `status`, asking the others in the background when what they offer is stale;
/// the `network` event brings what they answer. Before the first bot listing it asks without
/// counting as asked, so that listing still waits for the answers (`Network::directory`).
pub fn view(hub: &Hub) -> Value {
    let never = hub.network.asked.locked().is_none();
    if never || hub.network.due().is_some() {
        for slot in hub.network.slots() {
            tokio::spawn(async move { slot.node.peers().await });
        }
    }
    status(hub)
}

/// A new network's node in a folder of its own, taken in once `make` (create or join) worked;
/// gone again, folder and all, when it didn't.
async fn add<T>(
    hub: &Arc<Hub>,
    name: &str,
    connector: Option<&str>,
    first: impl AsyncFnOnce(&Node) -> Result<T>,
) -> Result<(Slot, T)> {
    let connector: Connector = connector.map_or(Ok(Connector::Public), str::parse)?;
    let _one = hub.network.changing.lock().await;
    let mut all = saved::load(&hub.store);
    let id = saved::new_id(&all);
    let saved = Saved { id: id.clone(), name: saved::free_name(&all, name)?, connector, rules: Vec::new() };
    let (node, inbox) = start_node(&saved).await?;
    let made = match first(&node).await {
        Ok(it) => it,
        Err(error) => {
            node.shutdown().await;
            let _ = std::fs::remove_dir_all(saved::dir(&id));
            return Err(error);
        }
    };
    all.push(saved.clone());
    saved::save(&hub.store, &all)?;
    let slot = take_in(hub, &saved, node, inbox);
    emit(hub);
    Ok((slot, made))
}

/// `networkCreate`: a new network (`network`, its name) with this computer as its owner, under
/// `name`. Returns its id and name, and the recovery code: the apps show it once, and it is kept
/// nowhere.
pub async fn create(
    hub: &Arc<Hub>,
    network: Option<&str>,
    name: Option<&str>,
    connector: Option<&str>,
) -> Result<Value> {
    let name = name.map_or_else(default_name, str::to_owned);
    let network = network.unwrap_or(saved::DEFAULT_NAME);
    let (slot, code) = add(hub, network, connector, async |node: &Node| node.create(&name)).await?;
    Ok(json!({"network": slot.id, "name": slot.name, "computer": slot.node.name(), "recoveryCode": code}))
}

/// `networkJoin`: joins a network with an invite from its owner or an admin, under the name the
/// invite gives it unless `network` says otherwise.
pub async fn join(
    hub: &Arc<Hub>,
    invite: &str,
    network: Option<&str>,
    name: Option<&str>,
    connector: Option<&str>,
) -> Result<Value> {
    let ticket = codync_network::read_invite(invite)?;
    if let Some(slot) = hub.network.slots().iter().find(|s| s.node.network_id().as_ref() == Some(&ticket.network)) {
        bail!("this computer is already in that network ({})", slot.name);
    }
    let network = network.or(ticket.name.as_deref()).unwrap_or(saved::DEFAULT_NAME);
    let name = name.map_or_else(default_name, str::to_owned);
    add(hub, network, connector, async |node: &Node| node.join(invite, &name).await).await?;
    Ok(status(hub))
}

/// `networkLeave`: stops this computer's node for a network and forgets it here. The others keep
/// listing this computer until the owner or an admin removes it.
pub async fn leave(hub: &Arc<Hub>, network: Option<&str>) -> Result<Value> {
    let _one = hub.network.changing.lock().await;
    let slot = hub.network.slot(network)?;
    hub.network.slots.locked().retain(|s| s.id != slot.id);
    slot.node.shutdown().await;
    let mut all = saved::load(&hub.store);
    all.retain(|s| s.id != slot.id);
    saved::save(&hub.store, &all)?;
    tokio::task::spawn_blocking(move || std::fs::remove_dir_all(saved::dir(&slot.id)))
        .await?
        .context("couldn't delete the network's folder")?;
    emit(hub);
    Ok(status(hub))
}

fn joined(hub: &Hub, network: Option<&str>) -> Result<Slot> {
    let slot = hub.network.slot(network)?;
    if slot.node.name().is_none() {
        bail!("this computer isn't in {} yet", slot.name);
    }
    Ok(slot)
}

/// `networkInvite`: a one-time invite and when it stops working (Unix ms).
pub fn invite(hub: &Hub, network: Option<&str>) -> Result<Value> {
    let invite = joined(hub, network)?.node.invite()?;
    let lasts = i64::try_from(codync_network::INVITE_FOR.as_millis()).expect("an invite lasts an hour");
    let until = crate::store::now_ms().saturating_add(lasts);
    Ok(json!({"invite": invite, "expiresAt": until}))
}

/// `networkPeers`: asks every computer now, in one network or all of them.
pub async fn peers(hub: &Arc<Hub>, network: Option<&str>) -> Result<Value> {
    let slots = match network {
        Some(_) => vec![joined(hub, network)?],
        None => hub.network.slots(),
    };
    futures::future::try_join_all(slots.iter().map(|s| s.node.peers())).await?;
    *hub.network.asked.locked() = Some(Instant::now());
    Ok(status(hub))
}

pub fn remove(hub: &Hub, network: Option<&str>, computer: &str, cascade: bool) -> Result<Value> {
    joined(hub, network)?.node.remove(computer, cascade)?;
    Ok(json!({}))
}

pub fn promote(hub: &Hub, network: Option<&str>, computer: &str) -> Result<Value> {
    joined(hub, network)?.node.promote(computer)?;
    Ok(json!({}))
}

pub fn recover(hub: &Hub, network: Option<&str>, code: &str) -> Result<Value> {
    joined(hub, network)?.node.recover(code)?;
    Ok(json!({}))
}

/// `setNetworkRules`: who on a network's other computers may write to this computer's bots, in
/// order (the first match decides); an empty list lets everyone.
pub async fn set_rules(hub: &Hub, network: Option<&str>, rules: &Value) -> Result<Value> {
    let rules: Vec<Rule> =
        serde_json::from_value(rules.clone()).context("rules are {fromComputer, fromAgent, toAgent, access}")?;
    // The saved list has one writer at a time: a create or join holds it while its node starts.
    let _one = hub.network.changing.lock().await;
    let slot = joined(hub, network)?;
    slot.node.set_rules(rules.clone())?;
    let mut all = saved::load(&hub.store);
    if let Some(saved) = all.iter_mut().find(|s| s.id == slot.id) {
        saved.rules = rules;
    }
    saved::save(&hub.store, &all)?;
    emit(hub);
    Ok(status(hub))
}
