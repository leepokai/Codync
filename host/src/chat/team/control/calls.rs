//! What one MCP call leaves behind, apart from the grants (`clients`): its line in the audit (`mcp.audit`), the
//! writes a retry must not repeat (`mcp.request.*`), each principal's calls of the last minute, and the gate every
//! app's writes pass one at a time.

use super::Caller;
use super::access::{Code, Level, Refusal};
use super::clients::{DAY_MS, Principal};
use crate::LockExt;
use crate::hub::Hub;
use crate::store::{Store, now_ms};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

const AUDIT: &str = "mcp.audit";
const REQUESTS: &str = "mcp.request.";
/// Calls an app may make a minute.
pub const CALLS_PER_MINUTE: usize = 60;
/// Calls the audit keeps, newest last, and how many a listing shows per app.
const AUDIT_KEPT: usize = 500;
const AUDIT_SHOWN: usize = 20;
/// How long a write's `clientRequestId` replays it.
const REQUEST_KEPT_MS: i64 = 7 * DAY_MS;

// MARK: audit

pub(super) fn audit(store: &Store) -> Vec<Value> {
    store.kv_get(AUDIT).and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

pub(super) fn recent(audit: &[Value], client: &str) -> Value {
    Value::Array(audit.iter().rev().filter(|c| c["client"] == client).take(AUDIT_SHOWN).cloned().collect())
}

/// Keeps `call` in the audit, the oldest going past [`AUDIT_KEPT`].
pub fn record(store: &Store, call: &Value) {
    static LOCK: Mutex<()> = Mutex::new(());
    let _g = LOCK.locked();
    let mut all = audit(store);
    all.push(call.clone());
    let over = all.len().saturating_sub(AUDIT_KEPT);
    all.drain(..over);
    if let Err(error) = serde_json::to_string(&all).map_err(anyhow::Error::from).and_then(|v| store.kv_set(AUDIT, &v)) {
        tracing::warn!(error = format!("{error:#}"), "couldn't keep an MCP call in the audit");
    }
}

// MARK: writes, one at a time, and the ones a retry repeats

/// Every app's writes that start or reopen a thread pass here one at a time, so a look at how many of an app's
/// threads are going and what it then starts can't interleave. Taken before any box's lock, never inside one.
// ponytail: one gate for every app; one per app if apps write often enough to wait on each other.
pub fn gate() -> MutexGuard<'static, ()> {
    static GATE: Mutex<()> = Mutex::new(());
    GATE.locked()
}

/// The key a `clientRequestId` of `p`'s for `tool` is kept under: each session without a token and each cloud
/// assistant app has ids of its own, and so does each tool.
pub(super) fn request_key(p: &Principal, tool: &str, key: &str) -> String {
    let id = Sha256::digest(json!([p.id, p.session, p.via, tool, key]).to_string().as_bytes());
    crate::chat::files::digest_hex(&id)
}

/// Runs `write` (`tool`'s, for `p`) once per `key`: a retry gets what the first made (`replayed`), never a second
/// one. Under the [`gate`], so two retries at once make one.
pub fn once(
    store: &Store,
    p: &Principal,
    tool: &str,
    key: Option<&str>,
    write: impl FnOnce() -> Result<Value>,
) -> Result<(Value, bool)> {
    let _g = gate();
    let Some(key) = key.filter(|k| !k.is_empty()) else { return Ok((write()?, false)) };
    ensure!(key.len() <= 200, "clientRequestId is at most 200 characters");
    let k = format!("{REQUESTS}{}", request_key(p, tool, key));
    let now = now_ms();
    // One kept past its days counts as never made, though no newer write has swept it away yet.
    let kept = store.kv_get(&k).and_then(|v| serde_json::from_str::<Value>(&v).ok());
    if let Some(done) = kept.filter(|d| d["at"].as_i64().is_some_and(|at| now - at <= REQUEST_KEPT_MS)) {
        return Ok((done["made"].clone(), true));
    }
    let made = write()?;
    store.kv_set(&k, &json!({"made": made, "at": now}).to_string())?;
    // ponytail: scans every kept write on each new one; an index by time if apps write thousands a week.
    for old in store.kv_prefix(REQUESTS) {
        let at = store.kv_get(&old).and_then(|v| serde_json::from_str::<Value>(&v).ok()).and_then(|v| v["at"].as_i64());
        if at.is_none_or(|at| now - at > REQUEST_KEPT_MS) {
            let _ = store.kv_delete(&old);
        }
    }
    Ok((made, false))
}

/// [`once`] off the async runtime, with the call's `clientRequestId`.
pub async fn once_blocking(
    hub: &Arc<Hub>,
    me: &Caller<'_>,
    tool: &'static str,
    args: &Value,
    write: impl FnOnce(&Arc<Hub>) -> Result<Value> + Send + 'static,
) -> Result<(Value, bool)> {
    let (hub, p, key) = (hub.clone(), me.principal.clone(), args["clientRequestId"].as_str().map(str::to_owned));
    tokio::task::spawn_blocking(move || once(&hub.store, &p, tool, key.as_deref(), || write(&hub))).await?
}

// MARK: rate limit

/// Each principal's calls in the last minute.
#[derive(Default)]
pub struct Calls(Mutex<HashMap<String, VecDeque<Instant>>>);

impl Calls {
    /// Counts a call of `principal`'s, refusing it past [`CALLS_PER_MINUTE`].
    pub fn take(&self, principal: &str) -> Result<()> {
        let now = Instant::now();
        let mut all = self.0.locked();
        let calls = all.entry(principal.to_owned()).or_default();
        while calls.front().is_some_and(|at| now.duration_since(*at) > Duration::from_mins(1)) {
            calls.pop_front();
        }
        if calls.len() >= CALLS_PER_MINUTE {
            return Err(Refusal::err(
                Code::RateLimited,
                format!("at most {CALLS_PER_MINUTE} calls a minute: wait a little, then try again"),
            ));
        }
        calls.push_back(now);
        Ok(())
    }
}

/// Refuses a name that isn't one of the levels.
pub fn level_arg(args: &Value, field: &str) -> Result<Option<Level>> {
    match args[field].as_str() {
        None => Ok(None),
        Some(word) => match Level::parse(word) {
            Some(level) => Ok(Some(level)),
            None => Err(Refusal::err(Code::InvalidRequest, format!("{field}: readOnly, ask, auto or full"))),
        },
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::chat::calls::context::tests::hub;

    fn app(id: &str) -> Principal {
        let mut p = super::super::clients::cloud(&hub().store);
        p.id = id.into();
        p
    }

    #[test]
    fn a_write_a_retry_names_again_is_made_once_per_app_session_and_tool() {
        let hub = hub();
        let mut made = 0;
        let mut launch = || {
            made += 1;
            Ok(json!({"taskId": format!("t{made}")}))
        };
        let a = app("app");
        let (first, replayed) = once(&hub.store, &a, "thread_launch", Some("k1"), &mut launch).unwrap();
        assert!(!replayed);
        let (again, replayed) = once(&hub.store, &a, "thread_launch", Some("k1"), &mut launch).unwrap();
        assert!(replayed && again == first, "the same thread, from what the store kept (a restart keeps it too)");
        let (other, _) = once(&hub.store, &app("another app"), "thread_launch", Some("k1"), &mut launch).unwrap();
        assert_ne!(other, first, "each app's ids are its own");
        let (_, replayed) = once(&hub.store, &a, "routine_run", Some("k1"), &mut launch).unwrap();
        assert!(!replayed, "and each tool's");
        let session = Principal { session: Some("s2".into()), ..a.clone() };
        let (_, replayed) = once(&hub.store, &session, "thread_launch", Some("k1"), &mut launch).unwrap();
        assert!(!replayed, "and each session's without a token");
        let (_, replayed) = once(&hub.store, &a, "thread_launch", None, &mut launch).unwrap();
        assert!(!replayed);
        assert_eq!(made, 5);
    }

    #[test]
    fn a_retry_past_seven_days_makes_new_work_even_before_any_other_write() {
        let hub = hub();
        let mut made = 0;
        let mut launch = || {
            made += 1;
            Ok(json!({"taskId": format!("t{made}")}))
        };
        let a = app("app");
        let (first, _) = once(&hub.store, &a, "thread_launch", Some("k1"), &mut launch).unwrap();
        // Kept eight days ago, and nothing since swept it away.
        let k = format!("{REQUESTS}{}", request_key(&a, "thread_launch", "k1"));
        hub.store.kv_set(&k, &json!({"made": first, "at": now_ms() - 8 * DAY_MS}).to_string()).unwrap();
        let (again, replayed) = once(&hub.store, &a, "thread_launch", Some("k1"), &mut launch).unwrap();
        assert!(!replayed && again != first, "too old to answer for: new work");
        assert_eq!(made, 2);
    }

    #[test]
    fn calls_are_limited_per_app() {
        let calls = Calls::default();
        for _ in 0..CALLS_PER_MINUTE {
            calls.take("a").unwrap();
        }
        let refused = calls.take("a").unwrap_err();
        assert_eq!(refused.downcast_ref::<Refusal>().unwrap().code, Code::RateLimited);
        assert!(calls.take("b").is_ok(), "another app's calls count apart");
    }
}
