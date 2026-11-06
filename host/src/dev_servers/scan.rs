//! After a host restart, what was running starts again; from then on, what listens is read and clients told.

use super::run::launch;
use super::{DevServers, KV_PREFIX, Live, Saved, Status, detected, forget, platform, ports, save};
use crate::LockExt;
use crate::agent::process::Process;
use crate::hub::Hub;
use crate::store::now_ms;
use anyhow::Result;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

/// A server that shows no port by now counts as running (a watch build may never listen).
pub(super) const SETTLE_MS: i64 = 5_000;
/// How often ports are read while a server is new, and after that.
const SCAN_FAST: Duration = Duration::from_secs(1);
const SCAN_SLOW: Duration = Duration::from_secs(10);
const NEW_FOR_MS: i64 = 30_000;

/// Brings back what was running before the host stopped, then keeps ports up to date.
pub async fn scan_loop(hub: Arc<Hub>) {
    if let Err(error) = resume(&hub).await {
        tracing::warn!(error = format!("{error:#}"), "couldn't restore dev servers");
    }
    let mut interfaces = Vec::new();
    loop {
        scan(&hub, &mut interfaces).await;
        let wait = if hub.dev_servers.any_new() { SCAN_FAST } else { SCAN_SLOW };
        tokio::select! {
            () = tokio::time::sleep(wait) => {}
            () = hub.dev_servers.wake.notified() => {}
        }
    }
}

/// After a host restart a server that was running starts again. After a computer restart it stays stopped, one tap
/// away: an abandoned server shouldn't come back at every login.
async fn resume(hub: &Arc<Hub>) -> Result<()> {
    let keys = hub.store.kv_prefix(KV_PREFIX);
    let saved: Vec<Saved> = keys
        .iter()
        .filter_map(|key| hub.store.kv_get(key))
        .filter_map(|json| serde_json::from_str(&json).ok())
        .collect();
    let booted = tokio::task::spawn_blocking(platform::boot_ms).await?;
    let mut again = Vec::new();
    for mut server in saved {
        let bot_exists = hub.store.bot(&server.bot)?.is_some_and(|row| !row.deleted);
        if !bot_exists {
            forget(&hub.store, &server.id)?;
            continue;
        }
        let same_boot = platform::same_boot(server.boot_ms, booted);
        // A crashed host on Unix leaves the server holding its port.
        if same_boot && let Some(leader) = server.leader.clone() {
            tokio::task::spawn_blocking(move || platform::kill_if_same(leader.pid, &leader.start)).await?;
        }
        if server.wanted && same_boot {
            again.push(server.id.clone());
        } else if server.wanted {
            server.wanted = false;
            save(&hub.store, &server);
        }
        let bot = server.bot.clone();
        hub.dev_servers.live.locked().insert(server.id.clone(), Live::stopped(server));
        hub.emit_bot(&bot);
    }
    for id in again {
        if let Err(error) = launch(hub, &id).await {
            tracing::warn!(error = format!("{error:#}"), "couldn't restart a dev server");
        }
    }
    Ok(())
}

impl DevServers {
    fn any_new(&self) -> bool {
        let now = now_ms();
        self.live.locked().values().any(|live| {
            live.status == Status::Starting
                || (live.status == Status::Running && live.started_at.is_some_and(|at| now - at < NEW_FOR_MS))
        })
    }

    pub(super) fn agents_running(&self) -> Vec<detected::RunningAgent> {
        let mut agents = self.agents.locked();
        agents.retain(|_, agent| agent.acp.strong_count() > 0);
        agents
            .iter()
            .filter_map(|(bot, agent)| {
                Some(detected::RunningAgent {
                    bot: bot.clone(),
                    acp: agent.acp.upgrade()?,
                    programs: agent.programs.clone(),
                })
            })
            .collect()
    }
}

/// Reads what listens, and tells clients what changed. `known`: this computer's addresses at the last scan.
async fn scan(hub: &Arc<Hub>, known: &mut Vec<IpAddr>) {
    let servers = &hub.dev_servers;
    let running: Vec<(String, Arc<tokio::sync::Mutex<Process>>)> = servers
        .live
        .locked()
        .iter()
        .filter_map(|(id, live)| Some((id.clone(), live.run.as_ref()?.process.clone())))
        .collect();
    let agents = servers.agents_running();
    let had_detected = !servers.detected.locked().is_empty();
    if running.is_empty() && agents.is_empty() && !had_detected {
        return;
    }
    let read = tokio::task::spawn_blocking(|| anyhow::Ok((ports::listening()?, ports::interfaces()))).await;
    let (listening, interfaces) = match read.map_err(anyhow::Error::from).flatten() {
        Ok(read) => read,
        Err(error) => {
            tracing::warn!(error = format!("{error:#}"), "couldn't read listening ports");
            return;
        }
    };
    let moved = *known != interfaces;
    *known = interfaces;
    let pids: Vec<u32> = listening.iter().map(|l| l.pid).collect();
    for (id, process) in running {
        let members = process.lock().await.members(&pids);
        let sockets = ports::sockets_of(&listening, &members);
        update(hub, &id, sockets, moved).await;
    }
    detected::scan(hub, &agents, &listening, moved).await;
}

/// Records a server's sockets. `moved`: this computer's addresses changed, so those of a server on all of them did.
async fn update(hub: &Hub, id: &str, sockets: Vec<SocketAddr>, moved: bool) {
    let stale = hub.dev_servers.live.locked().get(id).is_some_and(|live| {
        live.sockets != sockets || (moved && sockets.iter().any(|socket| socket.ip().is_unspecified()))
    });
    // Reading the addresses starts a process, so only when something changed.
    let ports = if stale {
        let sockets = sockets.clone();
        tokio::task::spawn_blocking(move || ports::urls(&sockets)).await.ok()
    } else {
        None
    };
    let bot = {
        let mut live = hub.dev_servers.live.locked();
        let Some(entry) = live.get_mut(id) else { return };
        // It may have ended while the addresses were read.
        let ports = ports.filter(|_| entry.run.is_some());
        if !entry.observed(sockets, ports, now_ms()) {
            return;
        }
        entry.saved.bot.clone()
    };
    hub.emit_bot(&bot);
}
