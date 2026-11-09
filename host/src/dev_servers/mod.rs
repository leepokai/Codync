//! Programs a bot keeps running (a dev server, a watch build). The host runs them, not the agent, so they outlive the
//! agent's idle stop, crash or restart, and every client can show their ports and stop them. What a bot leaves running
//! through its own shell instead can't move out of the agent's tree; [`detected`] only shows it.
//! Design: docs/features/dev-servers.md.

mod detected;
mod logs;
mod platform;
mod ports;
mod run;
mod scan;
#[cfg(test)]
mod tests;
mod tools;

pub use run::{remove, restart, stop};
pub use scan::scan_loop;
pub use tools::{BRIEF, START, TOOLS, call, tools};

use crate::LockExt;
use crate::agent::acp::Acp;
use crate::agent::process::Process;
use crate::analytics::{self, Event};
use crate::hub::Hub;
use crate::store::Store;
use anyhow::{Result, anyhow};
use detected::Detected;
use ports::Port;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio::sync::{Notify, watch};

const KV_PREFIX: &str = "dev-server:";
/// Running at once per bot: a loop of retries can't fill the computer.
const MAX_RUNNING: usize = 5;
/// What a server gets to stop by itself before the rest of its tree is killed.
const STOP_GRACE: Duration = Duration::from_secs(3);
/// How long a stop waits: the grace, then a moment for what's left to be killed.
const STOP_WAIT: Duration = Duration::from_secs(STOP_GRACE.as_secs() + 2);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Starting,
    Running,
    Failed,
    /// Also how a client reads a status from a newer host.
    #[default]
    #[serde(other)]
    Stopped,
}

impl Status {
    /// Whether its program is running, or on its way.
    pub fn active(self) -> bool {
        matches!(self, Self::Starting | Self::Running)
    }
}

/// A server as clients and bots see it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct View {
    id: String,
    name: String,
    command: String,
    folder: Option<String>,
    status: Status,
    ports: Vec<Port>,
    started_at: Option<i64>,
    exit_code: Option<i32>,
    managed: bool,
}

/// A server's first process, told from a later one with the same pid. Windows keeps none: a job ends with the host
/// that owns it.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Leader {
    pid: u32,
    start: String,
}

/// What survives a host restart.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    id: String,
    bot: String,
    name: String,
    command: String,
    folder: String,
    created_at: i64,
    /// Whether it should run: true until someone stops it or it ends by itself.
    wanted: bool,
    /// Whether someone allowed it on a card, so it can start again without one.
    approved: bool,
    leader: Option<Leader>,
    boot_ms: Option<i64>,
}

struct Run {
    stop: watch::Sender<bool>,
    ended: watch::Receiver<bool>,
    process: Arc<tokio::sync::Mutex<Process>>,
}

impl Run {
    fn ask_to_stop(&self) -> watch::Receiver<bool> {
        self.stop.send_replace(true);
        self.ended.clone()
    }
}

struct Live {
    saved: Saved,
    status: Status,
    sockets: Vec<SocketAddr>,
    ports: Vec<Port>,
    started_at: Option<i64>,
    exit_code: Option<i32>,
    run: Option<Run>,
    /// A stop came while its program was still starting, with no run to stop yet.
    stop_asked: bool,
}

impl Live {
    fn stopped(saved: Saved) -> Self {
        Self {
            saved,
            status: Status::Stopped,
            sockets: Vec::new(),
            ports: Vec::new(),
            started_at: None,
            exit_code: None,
            run: None,
            stop_asked: false,
        }
    }

    fn view(&self) -> View {
        View {
            id: self.saved.id.clone(),
            name: self.saved.name.clone(),
            command: self.saved.command.clone(),
            folder: Some(self.saved.folder.clone()),
            status: self.status,
            ports: self.ports.clone(),
            started_at: self.started_at,
            exit_code: self.exit_code,
            managed: true,
        }
    }

    /// A new run begins: nothing of the last one stays.
    fn begin(&mut self, now: i64) {
        self.status = Status::Starting;
        self.exit_code = None;
        self.sockets.clear();
        self.ports.clear();
        self.started_at = Some(now);
    }

    /// Its run ended, because someone `asked` or by itself; one that ends by itself isn't wanted back.
    fn ended(&mut self, asked: bool, code: Option<i32>) {
        self.run = None;
        self.sockets.clear();
        self.ports.clear();
        self.exit_code = code;
        self.status = if asked || code == Some(0) { Status::Stopped } else { Status::Failed };
        if !asked {
            self.saved.wanted = false;
        }
    }

    /// What a scan read: `ports` when its addresses were worked out again. The first socket, or enough time without
    /// one, makes it running. Returns whether clients should hear of it.
    fn observed(&mut self, sockets: Vec<SocketAddr>, ports: Option<Vec<Port>>, now: i64) -> bool {
        let settled = self.status == Status::Starting
            && (!sockets.is_empty() || self.started_at.is_some_and(|at| now - at >= scan::SETTLE_MS));
        if settled {
            self.status = Status::Running;
        }
        let mut moved = false;
        if let Some(ports) = ports {
            self.sockets = sockets;
            moved = ports != self.ports;
            self.ports = ports;
        }
        settled || moved
    }
}

/// An agent that was started, and the programs it runs under.
struct Agent {
    acp: Weak<Acp>,
    programs: Vec<String>,
}

#[derive(Default)]
pub struct DevServers {
    live: Mutex<HashMap<String, Live>>,
    detected: Mutex<HashMap<String, Vec<Detected>>>,
    agents: Mutex<HashMap<String, Agent>>,
    /// Wakes the port scan for a server that just started.
    wake: Notify,
}

impl DevServers {
    /// A bot's servers as clients see them: its own, oldest first, then what its agent left running.
    pub fn wire(&self, bot: &str) -> Value {
        let mut own: Vec<(i64, View)> = self
            .live
            .locked()
            .values()
            .filter(|live| live.saved.bot == bot)
            .map(|live| (live.saved.created_at, live.view()))
            .collect();
        own.sort_by_key(|(created_at, _)| *created_at);
        let mut servers: Vec<View> = own.into_iter().map(|(_, view)| view).collect();
        if let Some(found) = self.detected.locked().get(bot) {
            servers.extend(found.iter().map(Detected::view));
        }
        json!(servers)
    }

    /// `command`: the line it was started with, on `backend`'s harness.
    pub fn agent_started(&self, bot: &str, acp: &Arc<Acp>, backend: &str, command: &str) {
        let agent = Agent { acp: Arc::downgrade(acp), programs: detected::agent_programs(backend, command) };
        self.agents.locked().insert(bot.to_owned(), agent);
    }

    /// A deleted bot's servers end with it: dropping a server's run makes it stop.
    pub fn forget_bot(&self, store: &Store, bot: &str) {
        self.agents.locked().remove(bot);
        self.detected.locked().remove(bot);
        let ids: Vec<String> =
            self.live.locked().iter().filter(|(_, live)| live.saved.bot == bot).map(|(id, _)| id.clone()).collect();
        for id in ids {
            self.live.locked().remove(&id);
            if let Err(error) = forget(store, &id) {
                tracing::warn!(error = format!("{error:#}"), "couldn't forget a dev server");
            }
        }
    }

    /// Stops every server and waits for them, keeping what's wanted so the next start brings it back.
    pub async fn shutdown(&self) {
        let ended: Vec<watch::Receiver<bool>> =
            self.live.locked().values().filter_map(|live| live.run.as_ref()).map(Run::ask_to_stop).collect();
        let waits = ended.into_iter().map(|mut ended| async move {
            let _ = ended.wait_for(|done| *done).await;
        });
        if tokio::time::timeout(STOP_WAIT, futures::future::join_all(waits)).await.is_err() {
            tracing::warn!("some dev servers didn't stop in time");
        }
    }

    fn same(&self, bot: &str, folder: &str, command: &str) -> Option<String> {
        same_server(&self.live.locked(), bot, folder, command)
    }

    fn view(&self, bot: &str, id: &str) -> Result<View> {
        self.live
            .locked()
            .get(id)
            .filter(|live| live.saved.bot == bot)
            .map(Live::view)
            .ok_or_else(|| anyhow!("unknown dev server"))
    }

    fn approved(&self, id: &str) -> bool {
        self.live.locked().get(id).is_some_and(|live| live.saved.approved)
    }

    fn note_approved(&self, store: &Store, id: &str) {
        if let Some(live) = self.live.locked().get_mut(id) {
            live.saved.approved = true;
            save(store, &live.saved);
        }
    }
}

fn key(id: &str) -> String {
    format!("{KV_PREFIX}{id}")
}

fn save(store: &Store, saved: &Saved) {
    let written =
        serde_json::to_string(saved).map_err(anyhow::Error::from).and_then(|json| store.kv_set(&key(&saved.id), &json));
    if let Err(error) = written {
        tracing::warn!(error = format!("{error:#}"), "couldn't save a dev server");
    }
}

/// Leaves nothing of a server behind.
fn forget(store: &Store, id: &str) -> Result<()> {
    logs::delete(id);
    store.kv_delete(&key(id))
}

fn report(hub: &Hub, event: Event, key: &str, value: &str) {
    let mut properties = Map::new();
    properties.insert(key.into(), value.into());
    analytics::capture(hub, event, properties);
}

/// A bot's server by what it runs and where: starting the same thing twice isn't two servers.
fn same_server(live: &HashMap<String, Live>, bot: &str, folder: &str, command: &str) -> Option<String> {
    live.values()
        .find(|live| live.saved.bot == bot && live.saved.folder == folder && live.saved.command == command)
        .map(|live| live.saved.id.clone())
}

/// Whether `bot` may run one more.
fn has_room(live: &HashMap<String, Live>, bot: &str) -> bool {
    live.values().filter(|live| live.saved.bot == bot && live.status.active()).count() < MAX_RUNNING
}

/// The refusal past the limit, saying which ones run. Call it with the servers unlocked.
fn no_room(hub: &Hub, bot: &str) -> anyhow::Error {
    report(hub, Event::DevServerStarted, "outcome", "limit");
    anyhow!("at most {MAX_RUNNING} dev servers run at once; stop one first. Running: {}", hub.dev_servers.wire(bot))
}

/// Refuses another running server past the limit, before asking anyone about it. Only what starts one decides, under
/// the lock that adds it: parallel starts can all pass here, and an approval card can take minutes.
fn check_room(hub: &Hub, bot: &str) -> Result<()> {
    if has_room(&hub.dev_servers.live.locked(), bot) { Ok(()) } else { Err(no_room(hub, bot)) }
}

/// A server's recent output, for the apps.
pub async fn shown_logs(hub: &Hub, bot: &str, id: &str) -> Result<String> {
    hub.dev_servers.view(bot, id)?;
    logs::tail(id, logs::SHOWN_LINES).await
}
