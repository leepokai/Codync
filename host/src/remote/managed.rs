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
use crate::analytics::{self, SleepOutcome};
use crate::environment::Environment;
use crate::hub::Hub;
use crate::remote::cloud::{self, CloudError};
use crate::store::Store;
use crate::update;
use anyhow::Result;
use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

mod keeper_checks;
pub use keeper_checks::on_start;

/// Store key: this host is a cloud computer Codync runs (set from its claim ticket).
const MANAGED: &str = "cloud_managed";
/// Store key: the name its user gave it in the cloud (`set_name`).
const NAME_KEY: &str = "cloud_name";
const NAME: &str = "Codync Cloud";
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

/// The name every app shows for this computer: a cloud computer's is the one its user gave it (its machine gets another
/// hostname on each wake).
pub fn computer_name(store: &Store) -> String {
    if !is_managed(store) {
        return crate::service::host_name();
    }
    store.kv_get(NAME_KEY).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| NAME.into())
}

/// The cloud's name for this cloud computer (its state's `name`): apps connected now hear it as a `name` event, the
/// others in their next `hello`.
pub fn set_name(hub: &Hub, name: &str) -> Result<()> {
    if !is_managed(&hub.store) || hub.store.kv_get(NAME_KEY).as_deref() == Some(name) {
        return Ok(());
    }
    hub.store.kv_set(NAME_KEY, name)?;
    let _ = hub.events.send(json!({"type": "name", "name": name}));
    Ok(())
}

/// What every app draws this computer as.
pub async fn computer_device(store: &Store) -> Result<crate::service::Device> {
    if is_managed(store) {
        return Ok(crate::service::Device::Cloud);
    }
    Ok(tokio::task::spawn_blocking(crate::service::device).await?)
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
        let answer = request(&hub, wake_at).await;
        let outcome = sleep_outcome(&answer);
        analytics::cloud_sleep_asked(&hub, outcome, wake_at.is_some());
        match answer {
            Ok(Some(order)) => {
                update_to(&hub, order);
                active = Instant::now();
            }
            Ok(None) => {
                tracing::info!(wake_at, "asked the cloud to put this computer to sleep");
                // The machine stops within seconds: what's queued goes now or never.
                analytics::flush_now(&hub).await;
                wait_for_wake(&hub, idle).await;
                active = Instant::now();
            }
            Err(_) if outcome == SleepOutcome::InUse => {
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
    let (_turn, analytics) = switch_turn(hub).await;
    let body = sleep_body(wake_at, analytics);
    let v = cloud::call(hub, &base, Method::POST, "/v1/host/sleep", Some(&body)).await?;
    Ok(serde_json::from_value(v["update"].clone()).ok())
}

/// Every write of the analytics switch to the cloud (`share_analytics`, the sleep request) waits its turn here, reads
/// the switch only once it has it and keeps it until the cloud answers: a quick on → off can't end with an earlier "on"
/// landing last. A switch turned during a sleep request goes out once the cloud has answered, racing the machine's stop;
/// on a resume without memory it waits for the next sleep request.
async fn switch_turn(hub: &Hub) -> (tokio::sync::MutexGuard<'static, ()>, bool) {
    static SWITCH_WRITES: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let turn = SWITCH_WRITES.lock().await;
    (turn, hub.analytics.state())
}

/// A sleep request also says the owner's analytics switch, `analytics`: the cloud reports this computer's wakes only
/// while it is on (it hears a change at once too, `share_analytics`).
fn sleep_body(wake_at: Option<i64>, analytics: bool) -> Value {
    json!({ "wakeAt": wake_at, "analytics": analytics })
}

/// `POST /v1/host/analytics`: on a cloud computer, the cloud hears the owner's analytics switch as soon as it turns, so
/// a switch turned off stops its reports of this computer's wakes right away. Unreachable, it hears it with the next
/// sleep request.
pub fn share_analytics(hub: &Arc<Hub>) {
    if !is_managed(&hub.store) {
        return;
    }
    let hub = Arc::clone(hub);
    tokio::spawn(async move {
        let sent = async {
            let (_turn, analytics) = switch_turn(&hub).await;
            let body = json!({ "analytics": analytics });
            cloud::call(&hub, &cloud::codync_url(&hub)?, Method::POST, "/v1/host/analytics", Some(&body)).await
        };
        if let Err(e) = sent.await {
            tracing::warn!(error = format!("{e:#}"), "couldn't tell the cloud the analytics switch turned");
        }
    });
}

/// What came of asking to sleep (`cloud_sleep_asked`).
fn sleep_outcome(answer: &Result<Option<UpdateOrder>>) -> SleepOutcome {
    match answer {
        Ok(None) => SleepOutcome::Asleep,
        Ok(Some(_)) => SleepOutcome::Update,
        Err(e) if e.downcast_ref::<CloudError>().is_some_and(|c| c.code == "conflict") => SleepOutcome::InUse,
        Err(_) => SleepOutcome::Failed,
    }
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
    fn a_sleep_request_reports_what_came_of_it() {
        let refused =
            |code: &str| Err(anyhow::Error::new(CloudError { status: 409, code: code.into(), message: String::new() }));
        let order = UpdateOrder { version: "2.50.0".into(), force: false };
        assert_eq!(sleep_outcome(&Ok(None)), SleepOutcome::Asleep);
        assert_eq!(sleep_outcome(&Ok(Some(order))), SleepOutcome::Update);
        assert_eq!(sleep_outcome(&refused("conflict")), SleepOutcome::InUse);
        assert_eq!(sleep_outcome(&refused("notFound")), SleepOutcome::Failed);
        assert_eq!(sleep_outcome(&Err(anyhow::anyhow!("offline"))), SleepOutcome::Failed);
    }

    #[test]
    fn a_sleep_request_says_the_analytics_switch_as_the_cloud_reads_it() {
        assert_eq!(sleep_body(Some(42), true), json!({"wakeAt": 42, "analytics": true}));
        assert_eq!(sleep_body(None, false), json!({"wakeAt": null, "analytics": false}));
    }

    #[test]
    fn a_routine_due_soon_keeps_it_awake() {
        assert!(!routine_soon(None, 0));
        assert!(!routine_soon(Some(ROUTINE_LEAD_MS), 0));
        assert!(routine_soon(Some(ROUTINE_LEAD_MS - 1), 0));
    }

    /// A fake cloud that takes 100 ms over each switch write and notes whether two were ever open at once.
    #[derive(Default)]
    struct SwitchCloud {
        open: std::sync::atomic::AtomicUsize,
        overlapped: std::sync::atomic::AtomicBool,
        heard: std::sync::Mutex<Vec<bool>>,
    }

    async fn hear_switch(
        axum::extract::State(cloud): axum::extract::State<Arc<SwitchCloud>>,
        axum::Json(body): axum::Json<Value>,
    ) -> axum::Json<Value> {
        if cloud.open.fetch_add(1, Ordering::SeqCst) > 0 {
            cloud.overlapped.store(true, Ordering::SeqCst);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        cloud.heard.locked().push(body["analytics"] == true);
        cloud.open.fetch_sub(1, Ordering::SeqCst);
        axum::Json(json!({}))
    }

    #[tokio::test]
    async fn the_cloud_hears_the_switch_one_write_at_a_time_so_a_quick_off_lands_last() {
        let cloud = Arc::new(SwitchCloud::default());
        let router = axum::Router::new()
            .route("/v1/host/analytics", axum::routing::post(hear_switch))
            .route("/v1/host/sleep", axum::routing::post(hear_switch))
            .with_state(Arc::clone(&cloud));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a free port");
        let url = format!("http://{}", listener.local_addr().expect("its address"));
        tokio::spawn(async move { axum::serve(listener, router).await });
        let (hub, dir) = temp_hub();
        hub.store.kv_set("test_cloud_url", &url).expect("cloud url");
        set_managed(&hub.store).expect("set");
        // An "on" from either writer, `share_analytics` or the sleep request, then a quick "off".
        for sleep_first in [false, true] {
            let writes = cloud.heard.locked().len() + 2;
            hub.analytics.set_enabled(&hub.store, true).expect("on");
            if sleep_first {
                let hub = Arc::clone(&hub);
                tokio::spawn(async move { request(&hub, None).await });
            } else {
                share_analytics(&hub);
            }
            // The "on" write reads its value and is on its way before the switch turns off.
            tokio::task::yield_now().await;
            hub.analytics.set_enabled(&hub.store, false).expect("off");
            share_analytics(&hub);
            for _ in 0..50 {
                if cloud.heard.locked().len() == writes {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
        assert!(!cloud.overlapped.load(Ordering::SeqCst), "a write waits for the one before it");
        assert_eq!(*cloud.heard.locked(), [true, false, true, false], "the newest value lands last");
        let _ = std::fs::remove_dir_all(dir);
    }

    fn temp_hub() -> (Arc<Hub>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("codync-managed-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let store = Store::open(&dir.join("t.db")).expect("store");
        let identity = crate::remote::identity::Identity::load_or_create(&dir).expect("identity");
        (Hub::new(store, "test".into(), identity, "test".into(), 0), dir)
    }

    #[tokio::test]
    async fn a_cloud_computer_is_codync_cloud_until_named_with_a_cloud_in_every_app() {
        let (hub, dir) = temp_hub();
        let mut events = hub.events.subscribe();
        set_name(&hub, "Lab").expect("name");
        assert_eq!(
            computer_name(&hub.store),
            crate::service::host_name(),
            "a computer of the user's own keeps its own"
        );
        set_managed(&hub.store).expect("set");
        assert_eq!(computer_name(&hub.store), "Codync Cloud");
        assert_eq!(computer_device(&hub.store).await.expect("device"), crate::service::Device::Cloud);
        set_name(&hub, "Lab").expect("name");
        set_name(&hub, "Lab").expect("name");
        assert_eq!(computer_name(&hub.store), "Lab");
        // Connected apps hear it once, when it changes.
        let heard: Vec<_> = std::iter::from_fn(|| events.try_recv().ok()).filter(|e| e["type"] == "name").collect();
        assert_eq!(heard, [json!({"type": "name", "name": "Lab"})]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
