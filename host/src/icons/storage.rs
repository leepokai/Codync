//! Private R2 transport; domain-separated encryption binds each blob to its slot.
use crate::hub::Hub;
use crate::remote::{cloud, crypto};
use anyhow::{Context, Result, ensure};
use chacha20poly1305::{
    ChaCha20Poly1305, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use reqwest::Method;
use std::time::Duration;

pub(super) async fn put(hub: &Hub, slot: u8, png: &[u8]) -> Result<()> {
    let key = crypto::sha256(&[b"codync/custom-icons/v1", hub.identity.box_secret.as_bytes()]);
    let bytes = seal(&key, slot, png)?;
    request(hub, slot, Method::PUT, bytes).await
}

pub(super) async fn delete(hub: &Hub, slot: u8) -> Result<()> {
    request(hub, slot, Method::DELETE, Vec::new()).await
}

async fn request(hub: &Hub, slot: u8, method: Method, bytes: Vec<u8>) -> Result<()> {
    let base = cloud::codync_url(hub)?;
    let path = format!("/v1/host/icons/{slot}");
    let signature = hub.identity.sign_request(method.as_str(), &cloud::authority(&base)?, &path, &bytes);
    let response = crate::http()
        .request(method, format!("{base}{path}"))
        .header("Codync-Sig", signature)
        .header("Content-Type", "application/octet-stream")
        .timeout(Duration::from_secs(20))
        .body(bytes)
        .send()
        .await
        .context("Icon storage is unreachable; your previous icon has been kept")?;
    ensure!(response.status().is_success(), "{}; your previous icon has been kept", storage_error(response.status()));
    Ok(())
}

fn storage_error(status: reqwest::StatusCode) -> String {
    match status.as_u16() {
        401 | 403 => "Sign in and claim this computer to store custom icons".into(),
        404 => "Custom icon storage is not available on this server yet".into(),
        429 => "Too many icon changes. Wait a moment and try again".into(),
        500..=599 => "Custom icon storage is temporarily unavailable. Try again later".into(),
        _ => format!("Icon storage returned {status}"),
    }
}

fn seal(key: &[u8; 32], slot: u8, png: &[u8]) -> Result<Vec<u8>> {
    let nonce = crypto::random::<12>();
    let encrypted = ChaCha20Poly1305::new(key.into())
        .encrypt(&Nonce::from(nonce), Payload { msg: png, aad: &[1, slot] })
        .map_err(|_| anyhow::anyhow!("Cannot encrypt icon"))?;
    Ok([&[1], nonce.as_slice(), encrypted.as_slice()].concat())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn storage_errors_only_request_sign_in_for_authentication_failures() {
        for code in [401, 403] {
            assert!(storage_error(reqwest::StatusCode::from_u16(code).unwrap()).contains("Sign in"));
        }
        for code in [404, 429, 500, 503] {
            assert!(!storage_error(reqwest::StatusCode::from_u16(code).unwrap()).contains("Sign in"));
        }
        assert!(storage_error(reqwest::StatusCode::NOT_FOUND).contains("not available on this server"));
    }

    #[test]
    fn stored_icons_are_encrypted_and_bound_to_their_slot() {
        let key = [7; 32];
        let blob = seal(&key, 3, b"private image").unwrap();
        let cipher = ChaCha20Poly1305::new((&key).into());
        let nonce = Nonce::try_from(&blob[1..13]).unwrap();
        assert_eq!(cipher.decrypt(&nonce, Payload { msg: &blob[13..], aad: &[1, 3] }).unwrap(), b"private image");
        assert!(cipher.decrypt(&nonce, Payload { msg: &blob[13..], aad: &[1, 4] }).is_err());
        assert_ne!(blob, seal(&key, 3, b"private image").unwrap());
    }
}
