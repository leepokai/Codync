//! The host's side of the keeper's socket (`super::socket`), on a Codync Cloud computer only. Calls are
//! synchronous, like the store's: one local socket round trip each, so the vault's synchronous API stays as it is
//! (an async client couldn't serve it); `wait` runs them on the blocking pool.

pub use super::protocol::Status;
use super::protocol::{Granted, Request};
use super::record::{Keep, Record};
use crate::hub::Hub;
use crate::store::Store;
use anyhow::{Result, bail};
use serde::de::DeserializeOwned;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// What the user reads while the keeper waits for its key.
pub const LOCKED_MESSAGE: &str =
    "This cloud computer's keeper is still getting its key from Codync. Try again in a minute.";
/// How long a grant for the host's own provider calls lives (voice, Composio, a connector's bridge).
pub const SHORT_GRANT: Duration = Duration::from_mins(10);
const TIMEOUT: Duration = Duration::from_secs(10);
/// A revoke happens as a process ends; a keeper that doesn't answer forgets its grants anyway when it restarts.
#[cfg(unix)]
const REVOKE_TIMEOUT: Duration = Duration::from_secs(2);
/// An update downloads a release before the keeper answers.
const UPDATE_TIMEOUT: Duration = Duration::from_mins(5);
/// How long `wait_unlocked` waits for the keeper's key.
const UNLOCK_WAIT: Duration = Duration::from_secs(15);

/// The keeper's socket; unit tests name their own keeper's in kv `test_keeper_socket`.
#[cfg(unix)]
fn socket(store: &Store) -> std::path::PathBuf {
    if cfg!(test) {
        store.kv_get("test_keeper_socket").map(std::path::PathBuf::from).unwrap_or_default()
    } else {
        std::path::PathBuf::from(super::SOCKET)
    }
}

/// No keeper to reach: no socket, or nobody listening on it.
#[derive(Debug)]
struct NotRunning;

impl std::fmt::Display for NotRunning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("This cloud computer's keeper isn't running, so the keys saved on it can't be used.")
    }
}

impl std::error::Error for NotRunning {}

fn call<T: DeserializeOwned>(store: &Store, request: &Request) -> Result<T> {
    call_within(store, request, TIMEOUT)
}

#[cfg(unix)]
fn call_within<T: DeserializeOwned>(store: &Store, request: &Request, timeout: Duration) -> Result<T> {
    call_at(&socket(store), request, timeout)
}

#[cfg(not(unix))]
fn call_within<T: DeserializeOwned>(_: &Store, _: &Request, _: Duration) -> Result<T> {
    Err(NotRunning.into())
}

#[cfg(unix)]
fn call_at<T: DeserializeOwned>(socket: &std::path::Path, request: &Request, timeout: Duration) -> Result<T> {
    use anyhow::anyhow;
    use std::io::{BufRead as _, Write as _};
    let stream = std::os::unix::net::UnixStream::connect(socket).map_err(|_| NotRunning)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    let mut line = serde_json::to_vec(request)?;
    line.push(b'\n');
    (&stream).write_all(&line).map_err(|_| NotRunning)?;
    let mut answer = String::new();
    std::io::BufReader::new(&stream).read_line(&mut answer).map_err(|_| anyhow!("the keeper didn't answer"))?;
    let unreadable = || anyhow!("the keeper's answer couldn't be read");
    let mut v: serde_json::Value = serde_json::from_str(&answer).map_err(|_| unreadable())?;
    match v["error"].as_str() {
        Some(super::LOCKED) => bail!(LOCKED_MESSAGE),
        Some(error) => bail!("the keeper refused: {error}"),
        None => serde_json::from_value(v["ok"].take()).map_err(|_| unreadable()),
    }
}

/// The keeper's signing key (`crypto::b64`), for the claim to name.
pub fn public_key(store: &Store) -> Result<String> {
    call(store, &Request::Pub)
}

/// The keeper's status; `None` when it isn't running.
pub fn status(store: &Store) -> Result<Option<Status>> {
    match call(store, &Request::Status) {
        Err(e) if e.is::<NotRunning>() => Ok(None),
        answer => answer.map(Some),
    }
}

pub fn put(store: &Store, slot: &str, value: &str) -> Result<()> {
    call(store, &Request::Put { slot: slot.into(), value: value.into() }).map(|_: bool| ())
}

/// Saves `record` in `slot`, keeping what `keep` names from the record before it (same upstream only).
pub fn put_proxied(store: &Store, slot: &str, record: &Record, keep: &Keep) -> Result<()> {
    let request = Request::PutProxied { slot: slot.into(), record: Box::new(record.clone()), keep: keep.clone() };
    call(store, &request).map(|_: bool| ())
}

pub fn delete(store: &Store, slot: &str) -> Result<()> {
    call(store, &Request::Delete { slot: slot.into() }).map(|_: bool| ())
}

pub fn has(store: &Store, slot: &str) -> Result<bool> {
    call(store, &Request::Has { slot: slot.into() })
}

/// Whether proxied `slot` still holds a sign-in: the keeper drops one it can't renew.
pub fn signed_in(store: &Store, slot: &str) -> Result<bool> {
    call(store, &Request::SignedIn { slot: slot.into() })
}

/// The slots starting with `prefix`.
pub fn names(store: &Store, prefix: &str) -> Result<Vec<String>> {
    call(store, &Request::Names { prefix: prefix.into() })
}

/// A visible record's value (`None`: nothing saved there). A proxied one is refused.
pub fn reveal(store: &Store, slot: &str) -> Result<Option<String>> {
    call(store, &Request::Reveal { slot: slot.into() })
}

/// A grant that sends requests to `slot`'s upstream with its secret, for `ttl` (at most a day; `None`: until revoked,
/// see [`Grants`]).
pub(super) fn granted(store: &Store, slot: &str, ttl: Option<Duration>) -> Result<Granted> {
    call(store, &Request::Grant { slot: slot.into(), ttl_secs: ttl.map(|t| t.as_secs()) })
}

/// A loopback URL to `slot`'s upstream for [`SHORT_GRANT`], for one of the host's own calls.
pub fn short_grant(store: &Store, slot: &str) -> Result<String> {
    Ok(granted(store, slot, Some(SHORT_GRANT))?.url)
}

/// Has the keeper install release `version` over itself (it restarts).
pub fn update(store: &Store, version: &str) -> Result<()> {
    call_within(store, &Request::Update { version: version.into() }, UPDATE_TIMEOUT).map(|_: bool| ())
}

/// The vault's `unlock` on a cloud computer: waits a little for the keeper to get its key from the cloud.
pub async fn wait_unlocked(hub: &Arc<Hub>) -> Result<()> {
    match wait(hub).await? {
        Some(status) if status.unlocked => Ok(()),
        Some(_) => bail!(LOCKED_MESSAGE),
        None => Err(NotRunning.into()),
    }
}

/// The keeper's status once it has its key, or as it stands after waiting a little for that (`None`: not running).
pub async fn wait(hub: &Arc<Hub>) -> Result<Option<Status>> {
    let until = Instant::now() + UNLOCK_WAIT;
    loop {
        let hub = hub.clone();
        let answer = tokio::task::spawn_blocking(move || status(&hub.store)).await?;
        match answer {
            Ok(Some(status)) if status.unlocked => return Ok(Some(status)),
            answer if Instant::now() >= until => return answer,
            _ => tokio::time::sleep(Duration::from_millis(500)).await,
        }
    }
}

/// The grants an agent process starts with (`agent::auth`). They have no time limit, since a bot's agent runs for
/// days: they are revoked when the process ends instead, as the `Acp` holding them is killed or dropped. A keeper
/// that restarts forgets them all; the host then starts those agents again (`remote::managed`).
#[derive(Default)]
pub struct Grants {
    /// The keeper they came from.
    #[cfg(unix)]
    socket: Option<std::path::PathBuf>,
    tokens: Vec<String>,
}

impl Grants {
    /// A grant on `slot` until revoked, kept here: its URL.
    pub fn grant(&mut self, store: &Store, slot: &str) -> Result<String> {
        let Granted { url, token } = granted(store, slot, None)?;
        #[cfg(unix)]
        self.socket.get_or_insert_with(|| socket(store));
        self.tokens.push(token);
        Ok(url)
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }
}

impl Drop for Grants {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(socket) = &self.socket {
            for token in &self.tokens {
                let revoke = Request::Revoke { token: token.clone() };
                if let Err(e) = call_at::<bool>(socket, &revoke, REVOKE_TIMEOUT) {
                    tracing::debug!(error = format!("{e:#}"), "couldn't revoke an agent's grant");
                }
            }
        }
    }
}
