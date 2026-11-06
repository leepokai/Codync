//! Encrypted records backed by the operating system credential store. The database
//! contains ciphertext only; its random master key lives in Keychain/Secret Service.
use crate::{LockExt, store::Store};
use anyhow::{Result, anyhow, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use std::sync::Arc;
use zeroize::Zeroizing;

const PREFIX: &str = "vault:v1:";
const SERVICE: &str = match crate::environment::Environment::current() {
    crate::environment::Environment::Main => "dev.codync.credentials",
    crate::environment::Environment::Dev => "dev.codync.development.credentials",
};

pub async fn unlock(hub: Arc<crate::hub::Hub>) -> Result<()> {
    if hub.store.secret_key.locked().is_some() {
        return Ok(());
    }
    tokio::task::spawn_blocking(move || {
        let store = &hub.store;
        let mut cache = store.secret_key.locked();
        if cache.is_some() { return Ok(()); }
        let id = if let Some(id) = store.kv_read("vault.id")? { id } else {
            let id = uuid::Uuid::new_v4().to_string();
            store.kv_set("vault.id", &id)?;
            id
        };
        let entry = keyring::Entry::new(SERVICE, &id).map_err(|_| unavailable())?;
        let key = match entry.get_secret() {
            Ok(key) => key,
            Err(keyring::Error::NoEntry) => {
                if store.kv_read("vault.initialized")?.is_some() {
                    bail!("The credential key is missing. Restore this computer's keychain; encrypted credentials were preserved.");
                }
                let mut key = vec![0; 32];
                getrandom::fill(&mut key).map_err(|_| anyhow!("Could not generate credential key"))?;
                entry.set_secret(&key).map_err(|_| unavailable())?;
                key
            }
            Err(_) => return Err(unavailable()),
        };
        if key.len() != 32 { bail!("Invalid credential key; restore this computer's keychain."); }
        store.kv_set("vault.initialized", "true")?;
        *cache = Some(Zeroizing::new(key));
        Ok(())
    }).await??;
    Ok(())
}

fn unavailable() -> anyhow::Error {
    anyhow!(
        "Secure credential storage is locked or unavailable. Unlock Keychain on macOS, or start and unlock a Secret Service keyring in this Linux user's D-Bus session (Windows keeps it in Credential Manager). Credentials are never saved as plain text."
    )
}

pub fn read(store: &Store, slot: &str) -> Result<Option<String>> {
    let Some(raw) = store.kv_read(slot)? else {
        return Ok(None);
    };
    if !raw.starts_with(PREFIX) {
        // Move existing host configuration into the vault before returning it.
        write(store, slot, &raw)?;
        return Ok(Some(raw));
    }
    let key = store.secret_key.locked();
    let key = key.as_ref().ok_or_else(unavailable)?;
    let bytes = STANDARD.decode(&raw[PREFIX.len()..]).map_err(|_| anyhow!("Invalid encrypted credential record"))?;
    if bytes.len() < 24 {
        bail!("Invalid encrypted credential record");
    }
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|_| unavailable())?;
    let plain = cipher
        .decrypt(
            &XNonce::try_from(&bytes[..24]).map_err(|_| anyhow!("Invalid credential nonce"))?,
            Payload { msg: &bytes[24..], aad: slot.as_bytes() },
        )
        .map_err(|_| anyhow!("Could not decrypt credentials; the stored record was preserved"))?;
    Ok(Some(String::from_utf8(plain)?))
}

pub fn write(store: &Store, slot: &str, value: &str) -> Result<()> {
    let key = store.secret_key.locked();
    let key = key.as_ref().ok_or_else(unavailable)?;
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|_| unavailable())?;
    let mut nonce = [0; 24];
    getrandom::fill(&mut nonce).map_err(|_| anyhow!("Could not generate credential nonce"))?;
    let sealed = cipher
        .encrypt(&nonce.into(), Payload { msg: value.as_bytes(), aad: slot.as_bytes() })
        .map_err(|_| anyhow!("Could not encrypt credentials"))?;
    let bytes: Vec<u8> = nonce.into_iter().chain(sealed).collect();
    store.kv_set(slot, &format!("{PREFIX}{}", STANDARD.encode(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires an unlocked OS keychain or Secret Service session"]
    fn platform_keyring_roundtrip() {
        let id = format!("test-{}", uuid::Uuid::new_v4());
        let entry = keyring::Entry::new(SERVICE, &id).unwrap();
        let key = [42u8; 32];
        entry.set_secret(&key).unwrap();
        let loaded = entry.get_secret().unwrap();
        entry.delete_credential().unwrap();
        assert_eq!(loaded, key);
        assert!(matches!(entry.get_secret(), Err(keyring::Error::NoEntry)));
    }

    #[test]
    fn existing_plaintext_is_migrated_before_use() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        store.kv_set("record", "private password").unwrap();
        assert_eq!(read(&store, "record").unwrap().as_deref(), Some("private password"));
        assert!(store.kv_read("record").unwrap().unwrap().starts_with(PREFIX));
    }

    #[test]
    fn ciphertext_is_random_and_bound_to_its_record() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        write(&store, "one", "private password").unwrap();
        let first = store.kv_read("one").unwrap().unwrap();
        assert!(!first.contains("private password"));
        assert_eq!(read(&store, "one").unwrap().as_deref(), Some("private password"));
        write(&store, "one", "private password").unwrap();
        assert_ne!(first, store.kv_read("one").unwrap().unwrap());
        store.kv_set("two", &first).unwrap();
        assert!(read(&store, "two").is_err());
    }
    #[test]
    fn locked_storage_preserves_existing_data() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        store.kv_set("record", "existing secret").unwrap();
        *store.secret_key.locked() = None;
        assert!(read(&store, "record").is_err());
        assert!(write(&store, "record", "replacement").is_err());
        assert_eq!(store.kv_read("record").unwrap().as_deref(), Some("existing secret"));
    }
}
