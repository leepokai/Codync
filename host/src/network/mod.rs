//! This computer on codync-network: the user's other computers and the bots on them.
//!
//! The node stays off until the user creates or joins a network. Once it is on, this computer's
//! visible bots are its agents, each under an address name (`reviewer`, so `reviewer@desk` from
//! elsewhere), and the letters that reach them become bot requests (`chat::team::remote`).

mod handles;

use crate::LockExt;
use crate::hub::Hub;
use anyhow::{Context, Result, anyhow, bail};
use codync_network::{Address, Agent, Connector, Node, Peer, Reach};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

pub use handles::handle;

const KV_CONNECTOR: &str = "network_connector";
/// Every address name ever given to one of this computer's bots (name → bot id).
const KV_HANDLES: &str = "network_handles";
/// How long what the other computers offer is good for before a listing asks them again.
const FRESH_FOR: Duration = Duration::from_mins(1);
/// How long a listing waits for computers it has never heard from.
const FIRST_LOOK: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct Network {
    node: OnceLock<Arc<Node>>,
    asked: Mutex<Option<Instant>>,
    handles: Mutex<handles::Handles>,
}

/// Who a `message_bot` request is for.
pub enum Recipient {
    /// One of this computer's bots, by id (whether it exists is the caller's to check).
    Here(String),
    There(Address),
}

fn dir() -> PathBuf {
    crate::service::data_dir().join("network")
}

impl Network {
    pub fn node(&self) -> Option<Arc<Node>> {
        self.node.get().cloned()
    }

    /// The address name of one of this computer's bots.
    pub fn handle_of(&self, bot: &str) -> Option<String> {
        self.handles.locked().handle(bot)
    }

    /// The other computers and their bots, as last heard; refreshed in the background when stale,
    /// and waited for (briefly) the first time.
    pub async fn directory(&self) -> Vec<Peer> {
        let Some(node) = self.node() else { return Vec::new() };
        let (first, stale) = {
            let mut asked = self.asked.locked();
            let first = asked.is_none();
            let stale = asked.is_none_or(|at| at.elapsed() >= FRESH_FOR);
            if stale {
                *asked = Some(Instant::now());
            }
            (first, stale)
        };
        if first {
            let _ = tokio::time::timeout(FIRST_LOOK, node.peers()).await;
        } else if stale {
            let refresh = node.clone();
            tokio::spawn(async move { refresh.peers().await });
        }
        node.directory().into_iter().filter(|p| !p.me).collect()
    }

    /// Where `name` points: a bot id, an address name, `name@` this computer or a bare name none of
    /// its bots has are this computer's; `name@computer`, or a bare name one other computer offers,
    /// is a bot there.
    pub fn recipient(&self, name: &str) -> Result<Recipient> {
        let (handle, computer) = name.rsplit_once('@').map_or((name, None), |(handle, c)| (handle, Some(c)));
        let here = self.handles.locked().bot(handle);
        let local = Ok(Recipient::Here(here.clone().unwrap_or_else(|| name.to_owned())));
        let Some(node) = self.node() else { return local };
        if computer.is_none() && here.is_some() {
            return local;
        }
        match node.resolve(name) {
            Ok(address) if node.name().as_deref() == Some(&address.computer) => local,
            Ok(address) => Ok(Recipient::There(address)),
            Err(_) if computer.is_none() => local,
            Err(error) => Err(error),
        }
    }

    /// This computer's bot behind an address name, if any.
    pub fn bot_for(&self, handle: &str) -> Option<String> {
        self.handles.locked().bot(handle)
    }
}

/// Starts the node when this computer is in a network.
pub async fn start(hub: &Arc<Hub>) {
    if !dir().join("network.json").exists() {
        return;
    }
    if let Err(error) = boot(hub).await {
        tracing::warn!(error = format!("{error:#}"), "couldn't start the network");
    }
}

async fn boot(hub: &Arc<Hub>) -> Result<Arc<Node>> {
    if let Some(node) = hub.network.node() {
        return Ok(node);
    }
    let connector: Connector = hub.store.kv_get(KV_CONNECTOR).as_deref().unwrap_or("public").parse()?;
    let (node, inbox) = Node::start(&dir(), &connector).await.context("couldn't start the network")?;
    let node = Arc::new(node);
    hub.network.node.set(node.clone()).map_err(|_| anyhow!("the network is already starting; try again"))?;
    publish(hub);
    tokio::spawn(crate::chat::team::remote::listen(hub.clone(), inbox));
    tracing::info!(computer = node.name().as_deref().unwrap_or("(none yet)"), "network on");
    Ok(node)
}

/// Offers this computer's visible bots to the network, under their address names. Called after
/// every change to a bot's name, description or visibility.
pub fn publish(hub: &Hub) {
    let Some(node) = hub.network.node() else { return };
    let bots = match crate::chat::team::visible_bots(&hub.store) {
        Ok(bots) => bots,
        Err(error) => {
            tracing::warn!(error = format!("{error:#}"), "couldn't offer the bots to the network");
            return;
        }
    };
    let mut handles = hub.network.handles.locked();
    let mut given: HashMap<String, String> =
        hub.store.kv_get(KV_HANDLES).and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default();
    let before = given.len();
    *handles = handles::Handles::of(bots.iter().map(|b| (b.id.clone(), b.name.clone())), &mut given);
    if given.len() != before
        && let Err(error) = hub.store.kv_set(KV_HANDLES, &json!(given).to_string())
    {
        tracing::warn!(error = format!("{error:#}"), "couldn't save the bots' address names");
    }
    let agents: Vec<(Agent, Reach)> = bots
        .iter()
        .filter_map(|b| {
            let id = handles.handle(&b.id)?;
            Some((Agent { id, name: b.name.clone(), about: b.description.clone() }, Reach::App))
        })
        .collect();
    drop(handles);
    node.set_agents(agents);
}

fn joined(hub: &Hub) -> Result<Arc<Node>> {
    hub.network
        .node()
        .filter(|n| n.name().is_some())
        .ok_or_else(|| anyhow!("this computer isn't in a network: create one or join with an invite"))
}

/// `networkStatus`: this computer and the others, as last heard.
pub fn status(hub: &Hub) -> Value {
    let Some(node) = hub.network.node().filter(|n| n.name().is_some()) else {
        return json!({"joined": false});
    };
    let computers: Vec<Value> = node
        .directory()
        .into_iter()
        .map(|p| {
            let bots: Vec<&str> = p.agents.iter().map(|a| a.id.as_str()).collect();
            json!({"name": p.name, "role": p.role, "me": p.me, "online": p.online, "bots": bots})
        })
        .collect();
    json!({
        "joined": true, "computer": node.name(), "role": node.role(),
        "connector": hub.store.kv_get(KV_CONNECTOR).unwrap_or_else(|| "public".into()),
        "computers": computers,
    })
}

/// `networkCreate`: a new network with this computer as its owner; returns the recovery code.
pub async fn create(hub: &Arc<Hub>, name: Option<&str>, connector: Option<&str>) -> Result<Value> {
    if joined(hub).is_ok() {
        bail!("this computer is already in a network");
    }
    set_connector(hub, connector)?;
    let node = boot(hub).await?;
    let code = node.create(&name.map_or_else(default_name, str::to_owned))?;
    publish(hub);
    Ok(json!({"computer": node.name(), "recoveryCode": code}))
}

/// `networkJoin`: joins with an invite from the owner or an admin.
pub async fn join(hub: &Arc<Hub>, invite: &str, name: Option<&str>, connector: Option<&str>) -> Result<Value> {
    if joined(hub).is_ok() {
        bail!("this computer is already in a network");
    }
    set_connector(hub, connector)?;
    let node = boot(hub).await?;
    node.join(invite, &name.map_or_else(default_name, str::to_owned)).await?;
    publish(hub);
    Ok(status(hub))
}

pub fn invite(hub: &Hub) -> Result<Value> {
    Ok(json!({"invite": joined(hub)?.invite()?}))
}

/// `networkPeers`: asks every computer now.
pub async fn peers(hub: &Arc<Hub>) -> Result<Value> {
    joined(hub)?.peers().await?;
    *hub.network.asked.locked() = Some(Instant::now());
    Ok(status(hub))
}

pub fn remove(hub: &Hub, computer: &str, cascade: bool) -> Result<Value> {
    joined(hub)?.remove(computer, cascade)?;
    Ok(json!({}))
}

pub fn promote(hub: &Hub, computer: &str) -> Result<Value> {
    joined(hub)?.promote(computer)?;
    Ok(json!({}))
}

pub fn recover(hub: &Hub, code: &str) -> Result<Value> {
    joined(hub)?.recover(code)?;
    Ok(json!({}))
}

fn set_connector(hub: &Hub, connector: Option<&str>) -> Result<()> {
    if let Some(connector) = connector {
        connector.parse::<Connector>()?;
        hub.store.kv_set(KV_CONNECTOR, connector)?;
    }
    Ok(())
}

/// This computer's host name, as a network name can hold it.
fn default_name() -> String {
    let host = gethostname::gethostname().to_string_lossy().into_owned();
    handle(host.split('.').next().unwrap_or("computer"))
}
