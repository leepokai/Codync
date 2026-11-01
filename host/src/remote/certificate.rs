//! Device certificates (docs/plans/require-sign-in.md, phase 2): the Codync cloud vouches that a device key
//! is in an account. Checking one is a signature check with a key compiled in, so pairing in person works
//! with the cloud off, on a private relay, or with no network at all.

use super::crypto;
#[cfg(not(test))]
use crate::environment::Environment;
use serde::Deserialize;
use serde_json::Value;

/// Prefixed to the payload before signing, so the key signs nothing else by accident.
const DOMAIN: &str = "codync/device-cert/v1\n";

/// What a valid certificate says about the device that sent it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Certified {
    pub user_id: String,
    pub email: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// No certificate: an app older than this check.
    Missing,
    /// Not signed by the cloud, expired, or for another key.
    Invalid,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Payload {
    v: u8,
    device_key: String,
    user_id: String,
    email: Option<String>,
    exp: i64,
}

/// The cloud's public key: this build's environment, or `CODYNC_DEVICE_CERT_KEY` for a self-hosted cloud.
#[cfg(not(test))]
fn cloud_key() -> Option<[u8; 32]> {
    let key =
        std::env::var("CODYNC_DEVICE_CERT_KEY").unwrap_or_else(|_| Environment::current().device_cert_key().into());
    crypto::unb64_n::<32>(&key).ok()
}

/// Tests sign certificates with [`TEST_CLOUD_SEED`].
#[cfg(test)]
#[expect(clippy::unnecessary_wraps, reason = "the same signature as the build's")]
fn cloud_key() -> Option<[u8; 32]> {
    Some(ed25519_dalek::SigningKey::from_bytes(&TEST_CLOUD_SEED).verifying_key().to_bytes())
}

#[cfg(test)]
pub const TEST_CLOUD_SEED: [u8; 32] = [9; 32];

/// A certificate as the cloud would sign it, for tests.
#[cfg(test)]
pub fn test_certificate(device_key: &str, user_id: &str) -> Value {
    let payload = serde_json::json!({"v": 1, "deviceKey": device_key, "userId": user_id,
        "email": format!("{user_id}@example.com"), "exp": crate::store::now_ms() + 60_000})
    .to_string();
    let key = ed25519_dalek::SigningKey::from_bytes(&TEST_CLOUD_SEED);
    let sig = crypto::sign(&key, format!("{DOMAIN}{payload}").as_bytes());
    serde_json::json!({"payload": payload, "sig": crypto::b64(&sig)})
}

/// Checks `certificate` (`{payload, sig}`) for the device whose channel key is `device_key`, at `now` (ms).
pub fn check(certificate: &Value, device_key: &str, now: i64) -> Result<Certified, Refusal> {
    if certificate.is_null() {
        return Err(Refusal::Missing);
    }
    check_with(cloud_key().ok_or(Refusal::Invalid)?, certificate, device_key, now)
}

fn check_with(key: [u8; 32], certificate: &Value, device_key: &str, now: i64) -> Result<Certified, Refusal> {
    let (Some(text), Some(sig)) = (certificate["payload"].as_str(), certificate["sig"].as_str()) else {
        return Err(Refusal::Invalid);
    };
    let sig = crypto::unb64_n::<64>(sig).map_err(|_| Refusal::Invalid)?;
    crypto::verify(&key, format!("{DOMAIN}{text}").as_bytes(), &sig).map_err(|_| Refusal::Invalid)?;
    let p: Payload = serde_json::from_str(text).map_err(|_| Refusal::Invalid)?;
    if p.v != 1 || p.device_key != device_key || p.exp <= now || p.user_id.is_empty() {
        return Err(Refusal::Invalid);
    }
    Ok(Certified { user_id: p.user_id, email: p.email.filter(|e| !e.is_empty()) })
}

/// `kevin@example.com` → `k•••@example.com`: enough to recognize your own account, not to read someone else's.
pub fn masked(email: &str) -> String {
    match email.split_once('@') {
        Some((name, domain)) => format!("{}•••@{domain}", name.chars().next().unwrap_or('•')),
        None => "•••".into(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use ed25519_dalek::SigningKey;
    use serde_json::json;

    fn certificate(key: &SigningKey, device_key: &str, exp: i64) -> Value {
        let payload =
            json!({"v": 1, "deviceKey": device_key, "userId": "user_1", "email": "kevin@example.com", "exp": exp})
                .to_string();
        let sig = crypto::sign(key, format!("{DOMAIN}{payload}").as_bytes());
        json!({"payload": payload, "sig": crypto::b64(&sig)})
    }

    #[test]
    fn only_the_clouds_unexpired_certificate_for_this_key_passes() {
        let cloud = SigningKey::from_bytes(&[7; 32]);
        let key = cloud.verifying_key().to_bytes();
        let good = certificate(&cloud, "dk", 2_000);
        assert_eq!(
            check_with(key, &good, "dk", 1_000),
            Ok(Certified { user_id: "user_1".into(), email: Some("kevin@example.com".into()) })
        );
        assert_eq!(check_with(key, &good, "other", 1_000), Err(Refusal::Invalid), "another device's");
        assert_eq!(check_with(key, &good, "dk", 2_000), Err(Refusal::Invalid), "expired");
        let forged = certificate(&SigningKey::from_bytes(&[8; 32]), "dk", 2_000);
        assert_eq!(check_with(key, &forged, "dk", 1_000), Err(Refusal::Invalid), "not the cloud's key");
        let mut edited = good.clone();
        edited["payload"] = good["payload"].as_str().unwrap().replace("user_1", "user_2").into();
        assert_eq!(check_with(key, &edited, "dk", 1_000), Err(Refusal::Invalid), "changed after signing");
        assert_eq!(check(&Value::Null, "dk", 1_000), Err(Refusal::Missing));
    }

    #[test]
    fn emails_are_masked() {
        assert_eq!(masked("kevin@example.com"), "k•••@example.com");
        assert_eq!(masked("nope"), "•••");
    }
}
