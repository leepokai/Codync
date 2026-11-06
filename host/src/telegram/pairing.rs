//! One-time pairing codes: how a person on Telegram becomes allowed to talk to a bot.
//!
//! The code is made on a client, typed into the Telegram chat (`/start CODE`), works once, expires, and is dropped
//! after a few wrong tries. It is kept in the host's key-value store, one per Telegram bot.

use crate::store::Store;
use anyhow::{Result, anyhow};

/// How long a code can be used.
pub(super) const TTL_MS: i64 = 10 * 60 * 1000;
/// Wrong tries after which the code is void.
const MAX_FAILS: u32 = 5;
const ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const LENGTH: usize = 8;

fn slot(name: &str) -> String {
    format!("telegram.pair.{name}")
}

/// A fresh code for the Telegram bot `name` (replacing any earlier one) and when it expires.
pub(super) fn issue(store: &Store, name: &str, now: i64) -> Result<(String, i64)> {
    let mut bytes = [0_u8; LENGTH];
    getrandom::fill(&mut bytes).map_err(|_| anyhow!("could not make a pairing code"))?;
    let code: String = bytes.iter().map(|b| char::from(ALPHABET[usize::from(b & 31)])).collect();
    let expires = now + TTL_MS;
    store.kv_set(&slot(name), &format!("{code}|{expires}|0"))?;
    Ok((code, expires))
}

/// Whether `attempt` is the live code of `name`; if so the code is used up.
pub(super) fn redeem(store: &Store, name: &str, attempt: &str, now: i64) -> Result<bool> {
    let slot = slot(name);
    let Some(raw) = store.kv_read(&slot)? else { return Ok(false) };
    let mut parts = raw.split('|');
    let (Some(code), Some(expires), Some(fails)) = (parts.next(), parts.next(), parts.next()) else {
        store.kv_delete(&slot)?;
        return Ok(false);
    };
    let (expires, fails): (i64, u32) = (expires.parse().unwrap_or(0), fails.parse().unwrap_or(MAX_FAILS));
    if now > expires || fails >= MAX_FAILS {
        store.kv_delete(&slot)?;
        return Ok(false);
    }
    if attempt.trim().eq_ignore_ascii_case(code) {
        store.kv_delete(&slot)?;
        return Ok(true);
    }
    store.kv_set(&slot, &format!("{code}|{expires}|{}", fails + 1))?;
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store::open(std::path::Path::new(":memory:")).expect("store")
    }

    #[test]
    fn a_code_works_once_and_not_for_another_bot() {
        let s = store();
        let (code, expires) = issue(&s, "home", 1_000).unwrap();
        assert_eq!((code.len(), expires), (LENGTH, 1_000 + TTL_MS));
        assert!(!redeem(&s, "work", &code, 1_001).unwrap());
        assert!(redeem(&s, "home", &code.to_lowercase(), 1_001).unwrap(), "typed in any case");
        assert!(!redeem(&s, "home", &code, 1_002).unwrap(), "used up");
    }

    #[test]
    fn a_code_expires() {
        let s = store();
        let (code, expires) = issue(&s, "home", 0).unwrap();
        assert!(!redeem(&s, "home", &code, expires + 1).unwrap());
        assert!(!redeem(&s, "home", &code, 1).unwrap(), "an expired code is gone, not just refused");
    }

    #[test]
    fn a_few_wrong_tries_void_the_code() {
        let s = store();
        let (code, _) = issue(&s, "home", 0).unwrap();
        for _ in 0..MAX_FAILS {
            assert!(!redeem(&s, "home", "WRONG123", 1).unwrap());
        }
        assert!(!redeem(&s, "home", &code, 2).unwrap());
    }

    #[test]
    fn a_new_code_replaces_the_old_one() {
        let s = store();
        let (old, _) = issue(&s, "home", 0).unwrap();
        let (new, _) = issue(&s, "home", 0).unwrap();
        assert!(old == new || !redeem(&s, "home", &old, 1).unwrap());
    }
}
