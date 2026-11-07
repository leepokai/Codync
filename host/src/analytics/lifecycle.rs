//! Events outside API calls: the host's own start, a phone added, a phone connecting.

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
}
