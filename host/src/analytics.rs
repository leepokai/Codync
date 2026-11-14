//! Product analytics (`PostHog`), on unless the computer's owner turned it off
//! (`setAnalytics`). Events name the feature that was used and never carry content:
//! no message text, prompts, file names, paths or bot names.
//! What is collected and why: docs/features/analytics.md.

use crate::LockExt as _;
use crate::hub::Hub;
use crate::store::Store;
use anyhow::Result;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, oneshot};

pub mod errors;
mod event;
mod lifecycle;
mod observe;
pub mod reported;
mod session;
mod turns;

pub use event::Event;
pub use lifecycle::{
    DiscoveryStage, KV_STARTED, KeeperState, PairedAccount, SleepOutcome, Via, cloud_keeper_checked, cloud_sleep_asked,
    device_paired, discovery_failed, host_started, join_from_phone_answered, phone_connected, remote_screen_finished,
};
pub use observe::{answered, failed, observe_call};
pub use reported::track;
pub use turns::{
    Carried, Retired, Steer, TurnOutcome, TurnSource, agent_fell_back, agent_retired, context_compacted,
    handoff_carried, message_steered, turn_finished,
};

const KV_ENABLED: &str = "analytics_enabled";
/// Set to `dev` on test machines (`CODYNC_ANALYTICS_ENV=dev`): their events say `env: dev`, outside every chart.
const KV_ENV: &str = "analytics_env";
/// The account the install id was merged into. An id merges into one account and a second is
/// refused, so a later account on this computer starts its own profile.
const KV_IDENTIFIED: &str = "analytics_identified";
/// Write-only project token: safe to ship in public builds.
const TOKEN: &str = "phc_rQHvxdSFKnMquPfMjmiMhXhM4tqJ5qCYsWuVEoFxCCWX";
const BATCH_URL: &str = "https://us.i.posthog.com/batch/";
const FLUSH_EVERY: Duration = Duration::from_secs(30);
/// Unsent events kept while offline; the oldest go first.
const MAX_PENDING: usize = 500;

/// Where a network of the user's computers failed (`network_failed`'s `stage`); a failed
/// `network*` call is `action` (`observe::failed`).
#[derive(Clone, Copy, Debug)]
pub enum NetworkStage {
    /// A saved network's node didn't start with the host.
    Start,
    /// A bot's letter to another computer didn't go (refused, or no such computer).
    Send,
    /// The outcome of another computer's request didn't go back.
    Reply,
    /// A letter from another computer couldn't be taken in.
    Receive,
}

/// Reports a network step that failed; never the error's text, a name or an address.
pub fn network_failed(hub: &Hub, stage: NetworkStage) {
    let stage = match stage {
        NetworkStage::Start => "start",
        NetworkStage::Send => "send",
        NetworkStage::Reply => "reply",
        NetworkStage::Receive => "receive",
    };
    capture(hub, Event::NetworkFailed, Map::from_iter([("stage".into(), stage.into())]));
}

pub struct Analytics {
    enabled: Mutex<bool>,
    /// Every event's `env`: the build's, or `dev` on a test machine.
    env: &'static str,
    tx: mpsc::UnboundedSender<Msg>,
    rx: Mutex<Option<mpsc::UnboundedReceiver<Msg>>>,
    session: Mutex<session::Session>,
    /// The UTC day each phone last connected (`phone_connected`), in memory only.
    phones: Mutex<HashMap<String, chrono::NaiveDate>>,
}

enum Msg {
    Event(Value),
    /// A panic, captured off the panicking thread (`errors::report_panics`).
    Report(errors::Report),
    /// Send what's queued now and answer when done.
    Flush(oneshot::Sender<()>),
}

impl Analytics {
    pub fn load(store: &Store) -> Self {
        // Test machines (Dev and end-to-end cloud computers) mark their events instead of guessing them from
        // location or version. Stored, so a release the machine updates to keeps the mark.
        if std::env::var("CODYNC_ANALYTICS_ENV").as_deref() == Ok("dev")
            && let Err(e) = store.kv_set(KV_ENV, "dev")
        {
            tracing::warn!(error = format!("{e:#}"), "couldn't mark this machine's analytics as dev");
        }
        let built = if option_env!("CODYNC_ENV") == Some("main") { "main" } else { "dev" };
        let env = if store.kv_get(KV_ENV).as_deref() == Some("dev") { "dev" } else { built };
        let enabled = store.kv_get(KV_ENABLED).as_deref() != Some("0");
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            enabled: Mutex::new(enabled),
            env,
            tx,
            rx: Mutex::new(Some(rx)),
            session: Mutex::default(),
            phones: Mutex::default(),
        }
    }

    pub fn state(&self) -> bool {
        *self.enabled.locked()
    }

    pub fn set_enabled(&self, store: &Store, on: bool) -> Result<()> {
        save_choice(store, on)?;
        *self.enabled.locked() = on;
        Ok(())
    }
}

/// Saves the owner's choice; a host that isn't running reads it when it starts (`Analytics::load`).
pub fn save_choice(store: &Store, on: bool) -> Result<()> {
    store.kv_set(KV_ENABLED, if on { "1" } else { "0" })
}

/// A phone's platform as the catalog names it.
fn platform(p: &str) -> &'static str {
    match p {
        "ios" => "ios",
        "android" => "android",
        "macos" => "macos",
        _ => "other",
    }
}

/// The platform of the paired phone `key` (`other` for one this computer doesn't have).
fn platform_of(hub: &Hub, key: &str) -> &'static str {
    platform(hub.store.device(key).as_ref().map_or("", |d| d.platform.as_str()))
}

/// Queues one event unless the owner turned analytics off. Signed in to a Codync account,
/// events are linked to that account; otherwise to this install's random id. Both get a
/// person profile.
pub fn capture(hub: &Hub, event: Event, mut properties: Map<String, Value>) {
    if !hub.analytics.state() {
        return;
    }
    let account = hub.cloud.status().owner.map(|o| o.user_id);
    if let Some(user) = &account
        && hub.store.kv_get(KV_IDENTIFIED).is_none()
    {
        // Merges this install's earlier, anonymous events into the account.
        let identify = json!({"$anon_distinct_id": hub.host_id, "$process_person_profile": true});
        queue(hub, "$identify", user, identify.as_object().cloned().unwrap_or_default());
        if let Err(e) = hub.store.kv_set(KV_IDENTIFIED, user) {
            tracing::warn!(error = format!("{e:#}"), "couldn't save the analytics identity");
        }
    }
    properties.insert("$process_person_profile".into(), true.into());
    queue(hub, event.name(), account.as_deref().unwrap_or(&hub.host_id), properties);
}

fn queue(hub: &Hub, event: &str, distinct_id: &str, mut properties: Map<String, Value>) {
    properties.extend([
        ("$lib".into(), "codync-host".into()),
        ("app_version".into(), env!("CARGO_PKG_VERSION").into()),
        ("env".into(), hub.analytics.env.into()),
        ("$os".into(), std::env::consts::OS.into()),
        ("arch".into(), std::env::consts::ARCH.into()),
        // Joins this computer's events across accounts and with its phones' `computer_paired`.
        ("computer".into(), computer_tag(&hub.identity.computer_id()).into()),
        ("$session_id".into(), hub.analytics.session.locked().id(SystemTime::now()).to_string().into()),
    ]);
    let _ = hub.analytics.tx.send(Msg::Event(json!({
        "event": event,
        "distinct_id": distinct_id,
        // When it happened, not when its batch left; the uuid makes a resent batch count once.
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "uuid": uuid::Uuid::new_v4().to_string(),
        "properties": properties,
    })));
}

/// A computer's analytics key: a hash of its id, so a phone's `computer_paired` joins the
/// computer's events without revealing the id the relay knows it by.
pub fn computer_tag(computer_id: &str) -> String {
    use sha2::{Digest as _, Sha256};
    use std::fmt::Write as _;
    let hash = Sha256::digest(format!("codync-analytics:{computer_id}"));
    hash[..8].iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Sends queued events in batches every `FLUSH_EVERY`; turning analytics off drops them.
pub fn start(hub: &Arc<Hub>) {
    let Some(mut rx) = hub.analytics.rx.locked().take() else { return };
    errors::report_panics(hub);
    let hub = hub.clone();
    tokio::spawn(async move {
        let mut pending: Vec<Value> = Vec::new();
        let mut tick = tokio::time::interval(FLUSH_EVERY);
        loop {
            tokio::select! {
                msg = rx.recv() => match msg {
                    Some(Msg::Event(e)) => pending.push(e),
                    Some(Msg::Report(r)) => errors::capture_report(&hub, r),
                    Some(Msg::Flush(done)) => {
                        flush(&hub, &mut pending).await;
                        let _ = done.send(());
                    }
                    None => return,
                },
                _ = tick.tick() => flush(&hub, &mut pending).await,
            }
            if pending.len() > MAX_PENDING {
                pending.drain(..pending.len() - MAX_PENDING);
            }
        }
    });
}

async fn flush(hub: &Hub, pending: &mut Vec<Value>) {
    let on = hub.analytics.state();
    let sent = on && !pending.is_empty() && send(pending).await;
    if sent || !on {
        pending.clear();
    }
}

/// Sends what's queued now: before the host exits, so a restart or update loses nothing, and before a cloud computer's
/// machine stops (`remote::managed`).
pub async fn flush_now(hub: &Hub) {
    let (tx, rx) = oneshot::channel();
    if hub.analytics.tx.send(Msg::Flush(tx)).is_ok() {
        let _ = tokio::time::timeout(Duration::from_secs(3), rx).await;
    }
}

async fn send(batch: &[Value]) -> bool {
    // Debug builds (`cargo test`, `cargo run`) are throwaway hosts: each e2e test's fresh
    // home would count as a new user, and one built with `CODYNC_ENV=main` lands in production.
    if cfg!(debug_assertions) {
        return true;
    }
    let body = json!({"api_key": TOKEN, "batch": batch});
    match crate::http().post(BATCH_URL).json(&body).timeout(Duration::from_secs(15)).send().await {
        Ok(r) if r.status().is_success() => true,
        Ok(r) => {
            tracing::debug!(status = %r.status(), "analytics batch refused");
            // Refused for good (bad payload): retrying would only repeat it.
            r.status().is_client_error()
        }
        Err(e) => {
            tracing::debug!(error = format!("{e:#}"), "analytics batch not sent");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_until_the_owner_turns_it_off() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        assert!(Analytics::load(&store).state());
        Analytics::load(&store).set_enabled(&store, false).unwrap();
        assert!(!Analytics::load(&store).state(), "an off stays off");
        save_choice(&store, true).unwrap();
        assert!(Analytics::load(&store).state(), "a choice saved before the host starts counts");
    }

    #[test]
    fn a_test_machine_stays_marked_dev() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let built = if option_env!("CODYNC_ENV") == Some("main") { "main" } else { "dev" };
        assert_eq!(Analytics::load(&store).env, built);
        store.kv_set(KV_ENV, "dev").unwrap();
        assert_eq!(Analytics::load(&store).env, "dev", "the stored mark wins over the build's environment");
    }

    #[test]
    fn computer_tag_matches_the_iphone() {
        // Same vector as the iPhone's AnalyticsTests.
        assert_eq!(computer_tag("AAECAwQFBgcICQoLDA0ODw"), "2d6878bee20aba04");
    }

    #[test]
    fn the_install_id_merges_into_the_first_account_only() {
        use crate::remote::cloud::{Owner, update_status};
        let dir = std::env::temp_dir().join(format!("codync-analytics-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let identity = crate::remote::identity::Identity::load_or_create(&dir).unwrap();
        let hub = Hub::new(store, "h".into(), identity, "t".into(), 0);

        for user in ["user_1", "user_2"] {
            update_status(&hub, |s| s.owner = Some(Owner { user_id: user.into(), email: None }));
            capture(&hub, Event::AppOpened, Map::new());
        }

        let mut rx = hub.analytics.rx.locked().take().unwrap();
        let mut sent = Vec::new();
        while let Ok(Msg::Event(event)) = rx.try_recv() {
            sent.push(event);
        }
        assert_eq!(
            sent.iter().map(|e| (e["event"].as_str().unwrap(), e["distinct_id"].as_str().unwrap())).collect::<Vec<_>>(),
            [("$identify", "user_1"), ("app_opened", "user_1"), ("app_opened", "user_2")]
        );
        assert_eq!(sent[0]["properties"]["$anon_distinct_id"], "h");
        assert_eq!(hub.store.kv_get(KV_IDENTIFIED).as_deref(), Some("user_1"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
