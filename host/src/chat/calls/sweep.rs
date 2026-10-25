//! Calls whose app went away (a crash, a phone switched off): the host keeps the calls in progress and ends one
//! nothing happened on for a long while, writing its line, so what it carried never stays hidden without one.

use super::{CallState, STATE, end_when, state};
use crate::LockExt;
use crate::hub::{BotStatus, Hub};
use crate::store::{Store, now_ms};
use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;

/// A call nothing happened on this long (its app, the user, its bot) has lost its app. Generous: a live call ended by
/// mistake loses what is said after.
const ABANDONED_MS: i64 = 2 * 60 * 60 * 1000;
/// How often the host looks for abandoned calls.
const SWEEP_EVERY: std::time::Duration = std::time::Duration::from_mins(5);
/// The calls in progress (`kv`): what the sweep looks through.
const OPEN_KEY: &str = "calls.open";

pub(super) fn open_calls(store: &Store) -> Vec<String> {
    store.kv_get(OPEN_KEY).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Keeps `call` in the open calls (`open`) or takes it out. Under [`STATE`], like every change to a call.
pub(super) fn mark_open(store: &Store, call: &str, open: bool) -> Result<()> {
    let mut calls = open_calls(store);
    calls.retain(|c| c != call);
    if open {
        calls.push(call.to_owned());
    }
    store.kv_set(OPEN_KEY, &serde_json::to_string(&calls)?)
}

/// Ends abandoned calls (see [`ABANDONED_MS`]), at start-up and every [`SWEEP_EVERY`] after.
pub async fn sweep_loop(hub: Arc<Hub>) {
    loop {
        sweep(&hub, now_ms());
        tokio::time::sleep(SWEEP_EVERY).await;
    }
}

fn sweep(hub: &Arc<Hub>, now: i64) {
    for call in open_calls(&hub.store) {
        let Some(s) = state(&hub.store, &call) else {
            let _one = STATE.locked();
            if state(&hub.store, &call).is_none()
                && let Err(e) = mark_open(&hub.store, &call, false)
            {
                tracing::warn!(call, error = format!("{e:#}"), "couldn't forget a call with no state");
            }
            continue;
        };
        // Decided under the lock that ends it: its app may have resumed it since the read above.
        match end_when(hub, &s.bot, &call, &Value::Null, |s| abandoned(hub, &call, s?, now)) {
            Ok(Some(_)) => tracing::info!(call, bot = s.bot, "ended a call its app abandoned"),
            Ok(None) => {}
            Err(e) => tracing::warn!(call, error = format!("{e:#}"), "couldn't end an abandoned call"),
        }
    }
}

/// How long the call lasted, if nothing happened on it for [`ABANDONED_MS`] by `now` and its bot isn't at work.
fn abandoned(hub: &Hub, call: &str, s: &CallState, now: i64) -> Option<u64> {
    let last = s.last.max(s.started).max(newest_entry(&hub.store, &s.bot, call));
    if now - last < ABANDONED_MS || busy(hub, &s.bot) {
        return None;
    }
    Some(u64::try_from((last - s.started) / 1000).unwrap_or(0))
}

/// When the call's newest message or reply was written (0 for none): the bot answering it is work on the call.
fn newest_entry(store: &Store, bot: &str, call: &str) -> i64 {
    store.call_entries(bot, call, 1).ok().and_then(|e| e.first().map(|e| e.updated_at)).unwrap_or(0)
}

/// Whether the call's bot is at work (a turn under way, or waiting on an approval; a group: a member in it): its
/// reply is still to come on the call.
fn busy(hub: &Hub, bot: &str) -> bool {
    let status = match hub.store.bot(bot) {
        Ok(Some(row)) if row.config.is_group() => hub.group_runtime(&row.config).status,
        _ => hub.runtime(bot).status,
    };
    matches!(status, BotStatus::Working | BotStatus::NeedsInput)
}

#[cfg(test)]
mod tests {
    use super::super::{context, heard, is_live, save};
    use super::*;
    use crate::store::{BotConfig, EntryKind, Lane};
    use serde_json::json;

    fn hub_with_bot() -> (Arc<Hub>, String) {
        let hub = context::tests::hub();
        let id = uuid::Uuid::new_v4().to_string();
        let cfg = BotConfig { backend: "none".into(), ..context::tests::bot(&id, "Mika", "/tmp") };
        hub.store.save_bot(&cfg).unwrap();
        (hub, id)
    }

    #[tokio::test]
    async fn an_abandoned_call_gets_its_line() {
        let (hub, id) = hub_with_bot();
        // A message said on a call whose app then went away (its startCall never reached the host).
        heard(&hub, &id, "gone", "phone").unwrap();
        assert!(is_live(&hub.store, "gone"));
        sweep(&hub, now_ms());
        assert!(is_live(&hub.store, "gone"), "a call heard from just now stays open");
        sweep(&hub, now_ms() + ABANDONED_MS + 1);
        assert!(!is_live(&hub.store, "gone"), "a quiet call is ended");
        let lines = hub.store.history(&id, i64::MAX, 10).unwrap();
        assert!(lines.iter().any(|e| e.data["call"] == "gone" && e.data["callSeconds"].is_number()), "with its line");
        assert_eq!(open_calls(&hub.store), Vec::<String>::new());
    }

    #[tokio::test]
    async fn a_call_resumed_before_the_sweep_ends_it_stays_open() {
        let (hub, id) = hub_with_bot();
        heard(&hub, &id, "back", "phone").unwrap();
        let later = now_ms() + ABANDONED_MS + 1;
        // What the sweep read first: quiet for long enough.
        let read = state(&hub.store, "back").unwrap();
        assert!(abandoned(&hub, "back", &read, later).is_some());
        // Its app speaks again before the sweep takes the lock to end it.
        let mut resumed = state(&hub.store, "back").unwrap();
        resumed.last = later;
        save(&hub.store, "back", &resumed).unwrap();
        let ended = end_when(&hub, &id, "back", &Value::Null, |s| abandoned(&hub, "back", s?, later)).unwrap();
        assert!(ended.is_none() && is_live(&hub.store, "back"), "decided on the call as it is under the lock");
    }

    #[tokio::test]
    async fn a_call_its_bot_works_on_stays_open() {
        let (hub, id) = hub_with_bot();
        heard(&hub, &id, "long", "phone").unwrap();
        // The app last spoke long ago: the user asked for a long job and stayed quiet.
        let mut s = state(&hub.store, "long").unwrap();
        (s.started, s.last) = (0, 0);
        save(&hub.store, "long", &s).unwrap();
        hub.set_runtime(&id, |r| r.status = BotStatus::Working);
        sweep(&hub, now_ms());
        assert!(is_live(&hub.store, "long"), "its bot is still on the job");
        hub.set_runtime(&id, |r| r.status = BotStatus::Idle);
        let reply = json!({"text": "Done.", "final": true, "call": "long"});
        hub.store.insert_entry(&Lane::main(&id), EntryKind::Agent, 1, &reply).unwrap();
        sweep(&hub, now_ms());
        assert!(is_live(&hub.store, "long"), "its reply just came");
        sweep(&hub, now_ms() + ABANDONED_MS + 1);
        assert!(!is_live(&hub.store, "long"), "then quiet for good");
    }
}
