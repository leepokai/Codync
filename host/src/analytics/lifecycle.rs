//! Events outside API calls: the host's own start (and a cloud computer's keeper and sleep), a phone added, a phone
//! connecting.

use super::{Analytics, Event, capture, platform, platform_of};
use crate::LockExt as _;
use crate::hub::Hub;
use crate::remote::pairing_requests::Answer;
use serde_json::{Map, Value};

/// The version this host last started as (`host_started.updated_from`); empty when the install
/// was just made and hasn't started yet (`open_store`).
pub const KV_STARTED: &str = "analytics_version";

/// How a phone was added (`device_paired`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Via {
    /// A pairing code redeemed.
    Qr,
    /// An account access request approved.
    Account,
}

/// `host_started`, once per start of the host; `service`: run by launchd, systemd or Windows' runner.
pub fn host_started(hub: &Hub, service: bool) {
    let version = env!("CARGO_PKG_VERSION");
    let p = started(hub.store.kv_get(KV_STARTED).as_deref(), version, service);
    if let Err(e) = hub.store.kv_set(KV_STARTED, version) {
        tracing::warn!(error = format!("{e:#}"), "couldn't save the version this host started as");
    }
    capture(hub, Event::HostStarted, p);
}

/// `host_started`'s properties, from the version the host last started as (`None`: a host
/// from before this was kept, which is neither new nor known to have changed).
fn started(last: Option<&str>, version: &str, service: bool) -> Map<String, Value> {
    let mut p = Map::new();
    p.insert("first_run".into(), (last == Some("")).into());
    if let Some(from) = last.filter(|v| !v.is_empty() && *v != version) {
        p.insert("updated_from".into(), from.into());
    }
    p.insert("service".into(), service.into());
    p
}

/// How a Codync Cloud computer's keeper stood when its host started (`cloud_keeper_checked`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeeperState {
    /// It had its vault key.
    Unlocked,
    /// Still waiting for its key from the Codync cloud after the host's short wait.
    Locked,
    /// Nothing listening on its socket.
    Missing,
}

/// `cloud_keeper_checked`, once per start of a cloud computer's host: how its keeper stood, and whether `sudo` still
/// worked for its agents (which would leave the keeper's secrets open to them).
pub fn cloud_keeper_checked(hub: &Hub, state: KeeperState, sudo: bool) {
    let state = match state {
        KeeperState::Unlocked => "unlocked",
        KeeperState::Locked => "locked",
        KeeperState::Missing => "missing",
    };
    capture(
        hub,
        Event::CloudKeeperChecked,
        Map::from_iter([("state".into(), state.into()), ("sudo".into(), sudo.into())]),
    );
}

/// What came of a cloud computer's host asking the cloud to put it to sleep (`cloud_sleep_asked`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SleepOutcome {
    /// The cloud took it: the machine stops.
    Asleep,
    /// The cloud answered with an update to install instead.
    Update,
    /// Refused while a device is connected.
    InUse,
    /// Any other error.
    Failed,
}

/// `cloud_sleep_asked`: an idle cloud computer's host asked to sleep; `routine`: to be woken for its next routine.
pub fn cloud_sleep_asked(hub: &Hub, outcome: SleepOutcome, routine: bool) {
    let outcome = match outcome {
        SleepOutcome::Asleep => "asleep",
        SleepOutcome::Update => "update",
        SleepOutcome::InUse => "in_use",
        SleepOutcome::Failed => "failed",
    };
    capture(
        hub,
        Event::CloudSleepAsked,
        Map::from_iter([("outcome".into(), outcome.into()), ("routine".into(), routine.into())]),
    );
}

/// How a phone stands with this computer's account after pairing (`device_paired`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairedAccount {
    /// In the computer's account, or the computer joined the phone's.
    Joined,
    /// Signed in, and the computer stayed out of its account.
    PhoneOnly,
    /// A phone with no account.
    SignedOut,
}

impl PairedAccount {
    /// From whether the phone is signed in, and what the computer answered if it asked (in the phone's account, it
    /// doesn't).
    pub fn of(signed_in: bool, answer: Option<Answer>) -> Self {
        match (signed_in, answer) {
            (false, _) => Self::SignedOut,
            (true, Some(Answer::PhoneOnly)) => Self::PhoneOnly,
            (true, _) => Self::Joined,
        }
    }
}

/// `device_paired`: a phone was added to this computer.
pub fn device_paired(hub: &Hub, via: Via, device_platform: &str, automatic: bool, account: PairedAccount) {
    let mut p = Map::new();
    p.insert("via".into(), (if via == Via::Qr { "qr" } else { "account" }).into());
    p.insert("platform".into(), platform(device_platform).into());
    p.insert("automatic".into(), automatic.into());
    let account = match account {
        PairedAccount::Joined => "joined",
        PairedAccount::PhoneOnly => "phone_only",
        PairedAccount::SignedOut => "signed_out",
    };
    p.insert("account".into(), account.into());
    capture(hub, Event::DevicePaired, p);
}

/// `phone_connected`: a paired phone opened a channel, counted once per phone per UTC day. `relayed`: it came through
/// a relay rather than over Wi-Fi or Tailscale.
pub fn phone_connected(hub: &Hub, key: &str, relayed: bool) {
    if !hub.analytics.first_today(key, chrono::Utc::now().date_naive()) {
        return;
    }
    let mut p = Map::new();
    p.insert("platform".into(), platform_of(hub, key).into());
    p.insert("route".into(), (if relayed { "relay" } else { "direct" }).into());
    capture(hub, Event::PhoneConnected, p);
}

/// `join_from_phone_answered`: the computer answered a paired phone's Add to account.
pub fn join_from_phone_answered(hub: &Hub, approved: bool) {
    let mut p = Map::new();
    p.insert("outcome".into(), (if approved { "approved" } else { "declined" }).into());
    capture(hub, Event::JoinFromPhoneAnswered, p);
}

/// Where advertising on the local network failed (`discovery_failed`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscoveryStage {
    Start,
    Update,
}

/// `discovery_failed`: a computer in no account couldn't advertise on the local network.
pub fn discovery_failed(hub: &Hub, stage: DiscoveryStage) {
    let mut p = Map::new();
    p.insert("stage".into(), (if stage == DiscoveryStage::Start { "start" } else { "update" }).into());
    capture(hub, Event::DiscoveryFailed, p);
}

impl Analytics {
    /// Whether `phone` connects for the first time on `day`. Memory only: a restart may count one more.
    fn first_today(&self, phone: &str, day: chrono::NaiveDate) -> bool {
        self.phones.locked().insert(phone.to_owned(), day) != Some(day)
    }
}

/// `remote_screen_finished`: how a viewer's screen session ended, on which client, whether through TURN, and how long
/// it lasted from `screenPrepare`.
pub fn remote_screen_finished(hub: &Hub, key: &str, ended: &crate::screen::EndedViewer) {
    let mut p = Map::new();
    p.insert("platform".into(), platform_of(hub, key).into());
    p.insert("outcome".into(), ended.outcome.into());
    p.insert("relay".into(), ended.relay.into());
    p.insert("seconds".into(), ended.seconds.into());
    capture(hub, Event::RemoteScreenFinished, p);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_pairing_says_how_the_phone_stands_with_the_account() {
        assert_eq!(PairedAccount::of(false, Some(Answer::PhoneOnly)), PairedAccount::SignedOut);
        assert_eq!(PairedAccount::of(true, Some(Answer::PhoneOnly)), PairedAccount::PhoneOnly);
        assert_eq!(PairedAccount::of(true, Some(Answer::Join)), PairedAccount::Joined);
        assert_eq!(PairedAccount::of(true, None), PairedAccount::Joined, "the computer is in the phone's account");
    }

    #[test]
    fn a_phone_counts_once_a_day() {
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).unwrap();
        let a = Analytics::load(&store);
        let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let next = day.succ_opt().unwrap();
        assert!(a.first_today("k1", day));
        assert!(!a.first_today("k1", day), "reconnecting the same day");
        assert!(a.first_today("k2", day), "another phone");
        assert!(a.first_today("k1", next), "the next day");
        assert!(!a.first_today("k1", next));
    }

    #[test]
    fn a_start_says_whether_it_is_new_or_updated() {
        let p = |last, service| Value::Object(started(last, "2.20.0", service));
        assert_eq!(p(Some(""), true), json!({"first_run": true, "service": true}));
        assert_eq!(p(Some("2.19.3"), false), json!({"first_run": false, "updated_from": "2.19.3", "service": false}));
        assert_eq!(p(Some("2.20.0"), true), json!({"first_run": false, "service": true}));
        assert_eq!(p(None, true), json!({"first_run": false, "service": true}), "a host from before this was kept");
    }

    /// The events `report` queued on a throwaway host.
    fn queued(report: impl FnOnce(&Hub)) -> Vec<Value> {
        use crate::LockExt as _;
        let dir = std::env::temp_dir().join(format!("codync-cloud-event-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).unwrap();
        let identity = crate::remote::identity::Identity::load_or_create(&dir).unwrap();
        let hub = Hub::new(store, "h".into(), identity, "t".into(), 0);
        report(&hub);
        let mut rx = hub.analytics.rx.locked().take().unwrap();
        let mut sent = Vec::new();
        while let Ok(super::super::Msg::Event(event)) = rx.try_recv() {
            sent.push(event);
        }
        std::fs::remove_dir_all(&dir).unwrap();
        sent
    }

    #[test]
    fn a_cloud_computer_reports_its_keeper_and_sudo_once_per_start() {
        let sent = queued(|hub| {
            cloud_keeper_checked(hub, KeeperState::Missing, true);
            cloud_keeper_checked(hub, KeeperState::Unlocked, false);
            cloud_keeper_checked(hub, KeeperState::Locked, false);
        });
        let own = |e: &Value| json!({"event": e["event"], "state": e["properties"]["state"], "sudo": e["properties"]["sudo"]});
        assert_eq!(
            sent.iter().map(own).collect::<Vec<_>>(),
            [
                json!({"event": "cloud_keeper_checked", "state": "missing", "sudo": true}),
                json!({"event": "cloud_keeper_checked", "state": "unlocked", "sudo": false}),
                json!({"event": "cloud_keeper_checked", "state": "locked", "sudo": false}),
            ]
        );
        // Nothing else of its own: no slot, URL or secret, only the properties every host event carries.
        let mut keys: Vec<&str> = sent[0]["properties"].as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "$lib",
                "$os",
                "$process_person_profile",
                "$session_id",
                "app_version",
                "arch",
                "computer",
                "env",
                "state",
                "sudo"
            ]
        );
    }

    #[test]
    fn a_cloud_computer_reports_what_came_of_asking_to_sleep() {
        let sent = queued(|hub| {
            cloud_sleep_asked(hub, SleepOutcome::Asleep, true);
            cloud_sleep_asked(hub, SleepOutcome::Update, false);
            cloud_sleep_asked(hub, SleepOutcome::InUse, false);
            cloud_sleep_asked(hub, SleepOutcome::Failed, true);
        });
        let own = |e: &Value| json!({"event": e["event"], "outcome": e["properties"]["outcome"], "routine": e["properties"]["routine"]});
        assert_eq!(
            sent.iter().map(own).collect::<Vec<_>>(),
            [
                json!({"event": "cloud_sleep_asked", "outcome": "asleep", "routine": true}),
                json!({"event": "cloud_sleep_asked", "outcome": "update", "routine": false}),
                json!({"event": "cloud_sleep_asked", "outcome": "in_use", "routine": false}),
                json!({"event": "cloud_sleep_asked", "outcome": "failed", "routine": true}),
            ]
        );
    }
}
