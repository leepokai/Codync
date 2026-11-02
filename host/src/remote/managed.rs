//! Codync Cloud: a cloud computer Codync runs for its user (docs/plans/codync-cloud.md).
//!
//! It sleeps while nobody needs it: the host decides it is idle and asks the cloud to stop the machine, naming
//! when to wake it (its next routine). The relay wakes it earlier when a device connects or a mailbox message or
//! routine webhook arrives. From the request on, the relay counts this host as asleep; a machine resumed with its
//! memory brings the same process back, which then reconnects so the relay counts it awake again.
//!
//! Its updates come from the cloud, not from the host's own automatic updater: an order in the cloud state (an
//! app needs a newer host, or an emergency floor) is installed at once; before sleeping, the cloud may name a
//! newer release every device of the account can use, installed instead of sleeping. Its owner can still install
//! the latest release from any device (`installHostUpdate`), like on any computer.

use crate::LockExt;
use crate::environment::Environment;
use crate::hub::Hub;
use crate::remote::cloud::{self, CloudError};
use crate::store::Store;
use crate::update;
use anyhow::Result;
use reqwest::Method;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// Store key: this host is a cloud computer Codync runs (set from its claim ticket).
const MANAGED: &str = "cloud_managed";
/// No device, turn, run or setup terminal for this long.
const IDLE: Duration = Duration::from_mins(10);
/// Awake at least this long after starting, so a wake always gets its work done.
const MIN_AWAKE: Duration = Duration::from_mins(2);
/// A routine due sooner than this keeps the computer awake.
const ROUTINE_LEAD_MS: i64 = 5 * 60_000;
/// Wake this long before a routine is due.
const WAKE_EARLY_MS: i64 = 60_000;
const TICK: Duration = Duration::from_secs(30);
/// While asleep, how often this process (if it still runs) asks the cloud whether the machine was woken.
const AWAKE_POLL: Duration = Duration::from_secs(3);

pub fn is_managed(store: &Store) -> bool {
    store.kv_get(MANAGED).as_deref() == Some("1")
}

pub fn set_managed(store: &Store) -> Result<()> {
    store.kv_set(MANAGED, "1")
}

/// `CODYNC_SLEEP_IDLE_SECS` shortens the idle time for end-to-end tests.
fn idle_time() -> Duration {
    std::env::var("CODYNC_SLEEP_IDLE_SECS").ok().and_then(|s| s.parse().ok()).map_or(IDLE, Duration::from_secs)
}

fn in_use(hub: &Hub) -> bool {
    hub.busy() || hub.phone_clients.load(Ordering::Relaxed) > 0 || !hub.connected.locked().is_empty()
}

/// A release the cloud wants this host on. `force` interrupts working bots (their sessions resume).
#[derive(Deserialize, Debug, PartialEq, Eq)]
pub struct UpdateOrder {
    pub version: String,
    #[serde(default)]
    pub force: bool,
}

/// Installs `order` when it is newer than this host; an update already starting is left alone.
pub fn update_to(hub: &Hub, order: UpdateOrder) {
    if !newer(&order.version, env!("CARGO_PKG_VERSION")) {
        return;
    }
    if Environment::current() != Environment::Main {
        tracing::warn!(
            version = order.version,
            "a development build doesn't install releases; ignoring the cloud's update"
        );
        return;
    }
    tracing::info!(version = order.version, force = order.force, "the cloud asked for a newer host; updating");
    let port = hub.port;
    tokio::task::spawn_blocking(move || {
        if let Err(e) = update::spawn_worker(port, order.force, Some(&order.version)) {
            tracing::warn!(error = format!("{e:#}"), "couldn't start the update the cloud asked for");
        }
    });
}

fn newer(version: &str, than: &str) -> bool {
    matches!((semver::Version::parse(version), semver::Version::parse(than)), (Ok(v), Ok(t)) if v > t)
}

/// Background task: on a cloud computer, asks to sleep once it has been idle long enough.
pub async fn sleep_loop(hub: Arc<Hub>) {
    let idle = idle_time();
    let started = Instant::now();
    let mut active = Instant::now();
    loop {
        tokio::time::sleep(TICK.min(idle / 2)).await;
        let (running, next) = hub.routines.sleep_window();
        if !is_managed(&hub.store) || in_use(&hub) || running {
            active = Instant::now();
            continue;
        }
        let status = hub.cloud.status();
        if started.elapsed() < MIN_AWAKE.min(idle) || active.elapsed() < idle || !status.registered || !status.connected
        {
            continue;
        }
        if routine_soon(next, crate::store::now_ms()) {
            continue;
        }
        let wake_at = next.map(|t| t - WAKE_EARLY_MS);
        match request(&hub, wake_at).await {
            Ok(Some(order)) => {
                update_to(&hub, order);
                active = Instant::now();
            }
            Ok(None) => {
                tracing::info!(wake_at, "asked the cloud to put this computer to sleep");
                wait_for_wake(&hub, idle).await;
                active = Instant::now();
            }
            Err(e) if e.downcast_ref::<CloudError>().is_some_and(|c| c.code == "conflict") => {
                tracing::debug!("a device is connected; staying awake");
                active = Instant::now();
            }
            Err(e) => tracing::warn!(error = format!("{e:#}"), "couldn't ask the cloud to put this computer to sleep"),
        }
    }
}

/// Runs while the machine is stopping, and again right after a resume that kept this process. Once the cloud
/// has started the machine again (or the stop never happened and something woke it), reconnects to the relay.
/// Gives up after `limit`, when the loop may ask to sleep again.
async fn wait_for_wake(hub: &Hub, limit: Duration) {
    let since = Instant::now();
    while since.elapsed() < limit {
        tokio::time::sleep(AWAKE_POLL).await;
        match state(hub).await {
            Ok(Some(state)) if !matches!(state.as_str(), "stopping" | "stopped") => {
                tracing::info!(state, "the machine is awake again; reconnecting to the relay");
                hub.cloud.config.send_modify(|v| *v += 1);
                return;
            }
            Ok(_) => {}
            Err(e) => tracing::debug!(error = format!("{e:#}"), "couldn't ask the cloud whether the machine was woken"),
        }
    }
}

/// A routine due this soon keeps the computer awake.
fn routine_soon(next: Option<i64>, now: i64) -> bool {
    next.is_some_and(|t| t - now < ROUTINE_LEAD_MS)
}

/// `GET /v1/host/sleep`: this cloud machine's state as the cloud last saw it.
async fn state(hub: &Hub) -> Result<Option<String>> {
    let base = cloud::codync_url(hub)?;
    let v = cloud::call(hub, &base, Method::GET, "/v1/host/sleep", None).await?;
    Ok(v["state"].as_str().map(str::to_owned))
}

/// `POST /v1/host/sleep`: the cloud refuses with `conflict` while a device is connected, and answers with an
/// update to install instead when a newer release suits every device of the account.
async fn request(hub: &Hub, wake_at: Option<i64>) -> Result<Option<UpdateOrder>> {
    let base = cloud::codync_url(hub)?;
    let v = cloud::call(hub, &base, Method::POST, "/v1/host/sleep", Some(&json!({ "wakeAt": wake_at }))).await?;
    Ok(serde_json::from_value(v["update"].clone()).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_only_forward() {
        assert!(newer("2.26.0", "2.25.0"));
        assert!(!newer("2.25.0", "2.25.0"));
        assert!(!newer("2.24.9", "2.25.0"));
        assert!(!newer("garbage", "2.25.0"));
    }

    #[test]
    fn a_routine_due_soon_keeps_it_awake() {
        assert!(!routine_soon(None, 0));
        assert!(!routine_soon(Some(ROUTINE_LEAD_MS), 0));
        assert!(routine_soon(Some(ROUTINE_LEAD_MS - 1), 0));
    }
}
