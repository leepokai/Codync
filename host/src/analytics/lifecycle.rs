//! Events outside API calls: the host's own start, a phone added, a phone connecting.

use super::{Analytics, Event, capture, platform, platform_of};
use crate::LockExt as _;
use crate::hub::Hub;
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

/// `device_paired`: a phone was added to this computer.
pub fn device_paired(hub: &Hub, via: Via, device_platform: &str, automatic: bool) {
    let mut p = Map::new();
    p.insert("via".into(), (if via == Via::Qr { "qr" } else { "account" }).into());
    p.insert("platform".into(), platform(device_platform).into());
    p.insert("automatic".into(), automatic.into());
    capture(hub, Event::DevicePaired, p);
}

/// `phone_connected`: a paired phone opened a channel, counted once per phone per UTC day.
pub fn phone_connected(hub: &Hub, key: &str) {
    if !hub.analytics.first_today(key, chrono::Utc::now().date_naive()) {
        return;
    }
    let mut p = Map::new();
    p.insert("platform".into(), platform_of(hub, key).into());
    capture(hub, Event::PhoneConnected, p);
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
