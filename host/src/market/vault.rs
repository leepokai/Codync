//! Encrypted records backed by the operating system credential store. The database
//! contains ciphertext only; its random master key lives in Keychain/Secret Service
//! (a disposable host: in a file beside its data, see `key_in_folder`).
use crate::{LockExt, environment::Environment, store::Store};
use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use std::{sync::Arc, time::Duration};
use zeroize::Zeroizing;

const PREFIX: &str = "vault:v1:";
const SERVICE: &str = match Environment::current() {
    Environment::Main => "dev.codync.credentials",
    Environment::Dev => "dev.codync.development.credentials",
};

/// How long an unlock prompt (Keychain, Secret Service) may wait for an answer.
const PROMPT_TIMEOUT: Duration = Duration::from_mins(1);

/// Loads the master key once. The keyring read can wait on an unlock prompt nobody
/// sees (a headless Linux host), so it never holds `secret_key`, which async code
/// locks on runtime workers. One read runs at a time and owns `vault_unlock`: callers
/// arriving meanwhile share its outcome instead of prompting again.
pub async fn unlock(hub: Arc<crate::hub::Hub>) -> Result<()> {
    if hub.store.secret_key.locked().is_some() {
        return Ok(());
    }
    // The read's own prompt is bounded too; this margin covers the D-Bus calls around it.
    let wait = PROMPT_TIMEOUT + Duration::from_secs(5);
    let Ok(guard) = Arc::clone(&hub.store.vault_unlock).try_lock_owned() else {
        let _ = tokio::time::timeout(wait, hub.store.vault_unlock.lock()).await;
        return if hub.store.secret_key.locked().is_some() { Ok(()) } else { Err(unavailable()) };
    };
    let read = tokio::task::spawn_blocking(move || {
        let _guard = guard;
        if hub.store.secret_key.locked().is_some() {
            return Ok(());
        }
        let key = read_key(&hub.store)?;
        *hub.store.secret_key.locked() = Some(key);
        Ok(())
    });
    tokio::time::timeout(wait, read).await.map_err(|_| unavailable())??
}

fn read_key(store: &Store) -> Result<Zeroizing<Vec<u8>>> {
    let path = crate::service::data_dir().join("vault.key");
    let in_folder = key_in_folder(
        Environment::current(),
        std::env::var("CODYNC_VAULT").ok().as_deref(),
        path.try_exists()?,
        store.kv_read("vault.initialized")?.is_some(),
    );
    let key = if in_folder { folder_key(&path)? } else { keyring_key(store)? };
    if key.len() != 32 {
        bail!("Invalid credential key.");
    }
    store.kv_set("vault.initialized", "true")?;
    Ok(Zeroizing::new(key))
}

/// `CODYNC_VAULT=folder`, for data that gets thrown away (tests, worktree sandboxes): the key sits beside the
/// data and goes with it, and the credential store never hears of this host. Only file permissions protect
/// it, so a production build ignores the switch. Data already sealed with a credential store key stays there:
/// it is initialized with no key file (a sandbox from before the switch, or one given a copy of a host's database).
fn key_in_folder(env: Environment, switch: Option<&str>, file_exists: bool, initialized: bool) -> bool {
    env == Environment::Dev && switch == Some("folder") && (file_exists || !initialized)
}

fn folder_key(path: &std::path::Path) -> Result<Vec<u8>> {
    match std::fs::read(path) {
        Ok(key) => Ok(key),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let key = new_key()?;
            crate::remote::identity::write_private(path, &key)?;
            Ok(key)
        }
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

fn new_key() -> Result<Vec<u8>> {
    let mut key = vec![0; 32];
    getrandom::fill(&mut key).map_err(|_| anyhow!("Could not generate credential key"))?;
    Ok(key)
}

fn keyring_key(store: &Store) -> Result<Vec<u8>> {
    let id = if let Some(id) = store.kv_read("vault.id")? {
        id
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        store.kv_set("vault.id", &id)?;
        id
    };
    #[cfg(target_os = "linux")]
    unlock_secret_service().map_err(|_| unavailable())?;
    let entry = keyring::Entry::new(SERVICE, &id).map_err(|_| unavailable())?;
    match entry.get_secret() {
        Ok(key) => Ok(key),
        Err(keyring::Error::NoEntry) => {
            if store.kv_read("vault.initialized")?.is_some() {
                bail!(
                    "The credential key is missing. Restore this computer's keychain; encrypted credentials were preserved."
                );
            }
            let key = new_key()?;
            entry.set_secret(&key).map_err(|_| unavailable())?;
            Ok(key)
        }
        Err(_) => Err(unavailable()),
    }
}

/// keyring keeps its item in the default collection and prompts to unlock it with no
/// time limit. Unlocking it first, with one, means keyring never prompts itself.
#[cfg(target_os = "linux")]
fn unlock_secret_service() -> Result<(), dbus_secret_service::Error> {
    use dbus_secret_service::{EncryptionType, SecretService};
    let service = SecretService::connect_with_max_prompt_timeout(EncryptionType::Plain, PROMPT_TIMEOUT.as_secs())?;
    match service.get_default_collection() {
        Ok(collection) => collection.ensure_unlocked(),
        // No default collection: keyring reports that without prompting.
        Err(_) => Ok(()),
    }
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
    fn only_a_dev_host_that_asks_keeps_its_key_in_a_file() {
        use Environment::{Dev, Main};
        let folder = Some("folder");
        assert!(key_in_folder(Dev, folder, false, false), "a new host makes one");
        assert!(key_in_folder(Dev, folder, true, true), "and keeps it");
        assert!(!key_in_folder(Dev, folder, false, true), "data from before the switch stays on its keyring key");
        assert!(!key_in_folder(Main, folder, false, false), "production ignores the switch");
        assert!(!key_in_folder(Main, folder, true, true), "even with a key file there");
        assert!(!key_in_folder(Dev, None, false, false), "off without the switch");
        assert!(!key_in_folder(Dev, None, true, true), "whatever file is there");
        assert!(!key_in_folder(Dev, Some("keyring"), false, false), "or with another value");
    }

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
