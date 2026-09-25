//! Who is calling and what they may do (spec §4.1, §4.3, §6.6): the caller kinds,
//! per-method permissions, the device authorization check and QR pairing codes.
//! The authorized-device table itself lives in `store`.

use crate::crypto;
use crate::hub::Hub;
use crate::store::{Device, DeviceSource, Scope, now_ms};
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

pub enum Caller {
    /// This computer's own apps and helpers: loopback + the bearer token.
    Local,
    /// An authorized device over an E2E channel.
    Device { key: String, scopes: Vec<Scope> },
    /// A channel opened to redeem a pairing code; may only call `pair`.
    Pairing { key: String },
}

impl Caller {
    /// The key push and Live Activity tickets are bound to.
    pub fn device_key(&self) -> &str {
        match self {
            Self::Local => "local",
            Self::Device { key, .. } | Self::Pairing { key } => key,
        }
    }
}

/// Methods only this computer may call.
const LOCAL_ONLY: &[&str] = &[
    "setScreenEnabled",
    "pairing",
    "computerCall",
    "teamCall",
    "claimSign",
    "unclaim",
    "devices",
    "revokeDevice",
    "accessRequests",
    "decideAccessRequest",
    "cloudStatus",
    "setCloud",
];
const SCREEN: &[&str] = &["screenOffer", "screenClose", "screenTakeover"];

/// A method the caller isn't allowed to call (403).
#[derive(Debug)]
pub struct Forbidden;

impl std::fmt::Display for Forbidden {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("This can only be done at the computer itself.")
    }
}

impl std::error::Error for Forbidden {}

pub fn permit(caller: &Caller, method: &str) -> Result<(), Forbidden> {
    let ok = match caller {
        Caller::Local => method != "pair",
        Caller::Pairing { .. } => false,
        Caller::Device { scopes, .. } => {
            let needs = if SCREEN.contains(&method) { Scope::Screen } else { Scope::Control };
            !LOCAL_ONLY.contains(&method) && method != "pair" && scopes.contains(&needs)
        }
    };
    if ok { Ok(()) } else { Err(Forbidden) }
}

/// Why a channel refuses a device (`reject.code`).
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RejectCode {
    Unauthorized,
    Revoked,
    LeaseExpired,
    BadSignature,
    UnsupportedVersion,
    PairingClosed,
    RateLimited,
}

impl RejectCode {
    /// The WebSocket close code sent after the `reject`.
    pub fn close_code(self) -> u16 {
        match self {
            Self::Revoked => 4003,
            Self::RateLimited => 4008,
            Self::UnsupportedVersion => 4400,
            Self::PairingClosed => 4410,
            Self::Unauthorized | Self::LeaseExpired | Self::BadSignature => 4001,
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::Unauthorized => "This device isn't allowed on this computer. Pair it again.",
            Self::Revoked => "This device's access was removed on the computer.",
            Self::LeaseExpired => "The computer can't confirm this device's access right now. Try again shortly.",
            Self::BadSignature => "The device's signature didn't check out.",
            Self::UnsupportedVersion => "Update Codync to connect to this computer.",
            Self::PairingClosed => "Pairing isn't open. Show a new code on the computer.",
            Self::RateLimited => "Too many pairing attempts. Wait a minute and try again.",
        }
    }
}

/// Whether `key` may use a channel right now (§4.3): a local device always, an account
/// device only while its lease runs and the cloud state was applied on this connection.
pub fn authorize(hub: &Hub, key: &str) -> Result<Device, RejectCode> {
    let d = hub.store.device(key).ok_or(RejectCode::Unauthorized)?;
    if d.source == DeviceSource::Account
        && (!hub.cloud_synced.load(Ordering::Acquire) || d.lease_until.is_none_or(|t| t <= now_ms()))
    {
        return Err(RejectCode::LeaseExpired);
    }
    Ok(d)
}

// MARK: pairing codes

const CODE_TTL_MS: i64 = 10 * 60 * 1000;
const MAX_CODES: usize = 5;
const PER_SOURCE_PER_MINUTE: usize = 3;
const TOTAL_PER_MINUTE: usize = 10;

struct PairingCode {
    code: [u8; 16],
    expires_at: i64,
}

/// One-time QR codes (memory only) and the attempt limits that guard them.
#[derive(Default)]
pub struct Pairing {
    codes: Vec<PairingCode>,
    attempts: Vec<(String, Instant)>,
}

/// A code just issued, for the QR.
pub struct Issued {
    pub code: String,
    pub expires_at: i64,
}

impl Pairing {
    fn prune(&mut self, now: i64) {
        self.codes.retain(|c| c.expires_at > now);
    }

    /// A new code; the oldest is dropped beyond five.
    pub fn issue(&mut self) -> Issued {
        let now = now_ms();
        self.prune(now);
        if self.codes.len() >= MAX_CODES {
            self.codes.remove(0);
        }
        let code = crypto::random::<16>();
        let expires_at = now + CODE_TTL_MS;
        self.codes.push(PairingCode { code, expires_at });
        Issued { code: crypto::b64(&code), expires_at }
    }

    /// The live codes as ACL offers: `(offerId, expiresAt)`.
    pub fn offers(&mut self) -> Vec<(String, i64)> {
        self.prune(now_ms());
        self.codes.iter().map(|c| (crypto::offer_id(&c.code), c.expires_at)).collect()
    }

    /// Whether any code is still unused and unexpired.
    pub fn open(&mut self) -> bool {
        self.prune(now_ms());
        !self.codes.is_empty()
    }

    /// Counts a `pair` attempt from `source` (peer IP or relay link); false when over the limit.
    pub fn attempt(&mut self, source: &str) -> bool {
        let now = Instant::now();
        self.attempts.retain(|(_, t)| now.duration_since(*t) < Duration::from_secs(60));
        let from_source = self.attempts.iter().filter(|(s, _)| s == source).count();
        if from_source >= PER_SOURCE_PER_MINUTE || self.attempts.len() >= TOTAL_PER_MINUTE {
            return false;
        }
        self.attempts.push((source.to_owned(), now));
        true
    }

    /// Uses up `code` if it is valid (constant-time over every live code).
    pub fn redeem(&mut self, code: &str) -> bool {
        self.prune(now_ms());
        let Ok(given) = crypto::unb64_n::<16>(code) else { return false };
        let mut found = None;
        for (i, c) in self.codes.iter().enumerate() {
            if crypto::ct_eq(&c.code, &given) {
                found = Some(i);
            }
        }
        found.map(|i| self.codes.remove(i)).is_some()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn method_permissions_follow_the_caller() {
        let both = Caller::Device { key: "k".into(), scopes: vec![Scope::Control, Scope::Screen] };
        let control = Caller::Device { key: "k".into(), scopes: vec![Scope::Control] };
        for m in LOCAL_ONLY {
            assert!(permit(&Caller::Local, m).is_ok());
            assert!(permit(&both, m).is_err(), "{m} is loopback only");
        }
        assert!(permit(&both, "send").is_ok());
        assert!(permit(&both, "screenOffer").is_ok());
        assert!(permit(&control, "screenOffer").is_err(), "screen needs the screen scope");
        assert!(permit(&control, "screenStatus").is_ok(), "status is control");
        assert!(permit(&Caller::Device { key: "k".into(), scopes: vec![] }, "hello").is_err());
        assert!(permit(&both, "pair").is_err(), "pair only on a pairing channel");
        assert!(permit(&Caller::Pairing { key: "k".into() }, "hello").is_err());
        assert_eq!(both.device_key(), "k");
        assert_eq!(Caller::Local.device_key(), "local");
    }

    #[test]
    fn codes_are_single_use_and_capped() {
        let mut p = Pairing::default();
        assert!(!p.open());
        let first = p.issue();
        assert!(p.open());
        let offer = crypto::offer_id(&crypto::unb64_n(&first.code).unwrap());
        assert_eq!(p.offers(), [(offer, first.expires_at)]);
        assert!(!p.redeem("CAgICAgICAgICAgICAgICA"));
        assert!(p.redeem(&first.code));
        assert!(!p.redeem(&first.code), "single use");
        assert!(!p.open());

        let codes: Vec<Issued> = (0..6).map(|_| p.issue()).collect();
        assert!(!p.redeem(&codes[0].code), "the oldest of six is dropped");
        assert!(p.redeem(&codes[5].code));

        p.codes[0].expires_at = now_ms() - 1;
        assert!(!p.redeem(&codes[1].code), "expired");
    }

    #[test]
    fn attempts_are_limited_per_source_and_in_total() {
        let mut p = Pairing::default();
        assert!((0..3).all(|_| p.attempt("1.2.3.4")));
        assert!(!p.attempt("1.2.3.4"));
        assert!((0..7).all(|i| p.attempt(&format!("link{i}"))));
        assert!(!p.attempt("fresh"), "ten per minute across the host");
    }
}
