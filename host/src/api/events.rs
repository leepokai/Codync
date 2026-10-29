//! Event streams shared by SSE and channels: catch-up then live events, and setup terminals.

use super::devices::Caller;
use crate::LockExt;
use crate::hub::Hub;
use anyhow::Result;
use futures::{Stream, StreamExt, stream};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio_stream::wrappers::BroadcastStream;

/// Entries a fresh client receives per bot before paging with `history`.
const CATCH_UP_PER_BOT: i64 = 200;

/// A setup terminal's output: everything so far, then live, ending after `exit`.
pub fn term_events(hub: &Hub, id: &str) -> Option<impl Stream<Item = Value> + Send + use<>> {
    let term = hub.terms.get(id)?;
    let (head, rx) = term.attach();
    let done = head.iter().any(|v| v["type"] == "exit");
    let live = BroadcastStream::new(rx).filter_map(|msg| async move { msg.ok() }).scan(done, |done, v| {
        let out = (!*done).then(|| {
            *done = v["type"] == "exit";
            v
        });
        async move { out }
    });
    Some(stream::iter(head).chain(live))
}

/// Counts a connected phone (pushes are held while one is connected) for as long as its stream lives.
struct PhoneClientGuard(Arc<Hub>);

impl PhoneClientGuard {
    fn new(hub: Arc<Hub>) -> Self {
        hub.phone_clients.fetch_add(1, Ordering::Relaxed);
        Self(hub)
    }
}

impl Drop for PhoneClientGuard {
    fn drop(&mut self) {
        self.0.phone_clients.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Event types only this computer's own apps receive.
const LOCAL_EVENTS: &[&str] = &["accessRequests", "cloud"];

#[cfg(test)]
mod tests;

/// Subscribes first, then yields a catch-up (everything after `since`, in rev order),
/// then live events. Duplicates are fine: clients upsert by id/rev. Shared by SSE and channels.
pub fn events_stream(
    hub: &Arc<Hub>,
    since: i64,
    client: Option<&str>,
    caller: &Caller,
) -> Result<impl Stream<Item = Value> + Send + use<>> {
    let live = BroadcastStream::new(hub.events.subscribe());
    let mut catch_up: Vec<Value> = vec![];
    for bot in hub.bots_json(since)? {
        catch_up.push(json!({"type": "bot", "rev": bot["rev"], "bot": bot}));
    }
    for e in hub.store.entries_since(since, CATCH_UP_PER_BOT)? {
        catch_up.push(json!({"type": "entry", "rev": e.rev, "entry": e}));
    }
    catch_up.extend(crate::chat::artifacts::since(&hub.store, since));
    catch_up.sort_by_key(|v| v["rev"].as_i64().unwrap_or(0));
    let hello = json!({
        "type": "hello",
        "hostId": hub.host_id,
        "computerId": hub.identity.computer_id(),
        "rev": hub.store.current_rev(),
        "usage": hub.usage.locked().clone(),
        "screen": hub.screen.state(),
        // Live state like the two above: a `network` event sent before this stream began is in it.
        "network": crate::network::status(hub),
    });
    catch_up.insert(0, hello);
    // What the voice of a call this device holds should have heard while it was away.
    catch_up.extend(crate::chat::calls::replay(caller.device_key()));

    let local = matches!(caller, Caller::Local);
    let key = caller.device_key().to_owned();
    let guard = Arc::new(matches!(client, Some("ios" | "android")).then(|| PhoneClientGuard::new(hub.clone())));
    let tail = live.filter_map(move |msg| {
        let _keep = guard.clone();
        let key = key.clone();
        async move {
            match msg {
                Ok(v) if !local && v["type"].as_str().is_some_and(|t| LOCAL_EVENTS.contains(&t)) => None,
                // What a call's voice should say goes to the device holding that call only.
                Ok(v) if v["type"] == "call" && v["owner"] != key.as_str() => None,
                Ok(v) => Some(v),
                // Lagged: tell the client to resync from its last rev.
                Err(_) => Some(json!({"type": "resync"})),
            }
        }
    });
    Ok(stream::iter(catch_up).chain(tail))
}
