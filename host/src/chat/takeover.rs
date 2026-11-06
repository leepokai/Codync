//! Human takeover requests (the `request_handoff` tool): the bot stops at a step only the person can do
//! (a CAPTCHA, a login, a payment) and posts a card. The card tells the person once, and lapses after 30
//! minutes (`expired`), so a forgotten request neither stays "waiting" forever nor looks answerable long
//! after anyone remembers what it was.
//!
//! This build's request does not hold the bot's computer: the screen's own takeover (`screenTakeover`) is
//! the person's choice from the remote screen and is not touched here.

use crate::hub::Hub;
use crate::remote::push::{self, AlertKind};
use crate::store::{Store, now_ms};
use anyhow::Result;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

/// How long a request waits for the person.
pub const TTL_MS: i64 = 30 * 60 * 1000;
/// How often waiting requests are checked.
const SWEEP_EVERY: Duration = Duration::from_secs(60);

/// The data of a new request's card at `now`. `notified` is set because the person is told the moment it is posted.
pub fn card(bot_id: &str, reason: &str, url: Option<&str>, now: i64) -> Value {
    json!({
        "text": format!("Human takeover requested: {reason}"),
        "style": "info",
        "author": bot_id,
        "takeoverRequest": {
            "reason": reason,
            "url": url,
            "status": "waiting",
            "requestedAt": now,
            "expiresAt": now + TTL_MS,
            "notified": true,
        }
    })
}

/// The card's data once its time is up at `now`; `None` while it still waits (or already ended).
pub fn lapsed(data: &Value, now: i64) -> Option<Value> {
    let request = data.get("takeoverRequest")?;
    if request["status"] != "waiting" || request["expiresAt"].as_i64().is_none_or(|at| at > now) {
        return None;
    }
    let mut next = data.clone();
    next["takeoverRequest"]["status"] = "expired".into();
    let reason = request["reason"].as_str().unwrap_or("a step for you");
    next["text"] = format!("Takeover request expired: {reason}").into();
    Some(next)
}

/// Every waiting request's card id and data.
fn waiting(store: &Store) -> Result<Vec<(String, Value)>> {
    store.with_connection(|c| {
        let mut st = c.prepare(
            "SELECT id, data FROM entries WHERE kind = 'notice' AND json_extract(data, '$.takeoverRequest.status') = 'waiting'",
        )?;
        let rows =
            st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().filter_map(|(id, data)| Some((id, serde_json::from_str(&data).ok()?))).collect())
    })
}

/// The waiting requests whose time is up at `now`, with their lapsed data.
pub fn due(store: &Store, now: i64) -> Result<Vec<(String, Value)>> {
    Ok(waiting(store)?.into_iter().filter_map(|(id, data)| Some((id, lapsed(&data, now)?))).collect())
}

/// Tells the person a bot needs them, once, as the card is posted.
pub fn notify(hub: &Hub, bot_id: &str, reason: &str) {
    let Some(config) = hub.store.bot(bot_id).ok().flatten().map(|row| row.config) else { return };
    push::notify(hub, &config, Some(bot_id), &format!("{} needs you", config.name), reason, AlertKind::NeedsInput);
}

/// Closes the requests nobody answered, for as long as the host runs.
pub async fn sweep_loop(hub: Arc<Hub>) {
    let mut tick = tokio::time::interval(SWEEP_EVERY);
    loop {
        tick.tick().await;
        let found = hub.clone();
        match tokio::task::spawn_blocking(move || due(&found.store, now_ms())).await {
            Ok(Ok(lapsed)) => {
                for (id, data) in lapsed {
                    hub.set_entry(&id, &data);
                }
            }
            Ok(Err(error)) => {
                tracing::warn!(error = format!("{error:#}"), "couldn't look for lapsed takeover requests");
            }
            Err(error) => tracing::warn!(error = %error, "the takeover sweep stopped"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{EntryKind, Lane};

    #[test]
    fn a_request_lapses_after_thirty_minutes_and_only_once() {
        let data = card("b1", "log in", Some("https://example.com"), 1_000);
        assert_eq!(data["takeoverRequest"]["notified"], true);
        assert!(lapsed(&data, 1_000 + TTL_MS - 1).is_none(), "still waiting just before the limit");
        let next = lapsed(&data, 1_000 + TTL_MS).expect("lapses at the limit");
        assert_eq!(next["takeoverRequest"]["status"], "expired");
        assert!(next["text"].as_str().is_some_and(|t| t.contains("log in")));
        assert!(lapsed(&next, 9_999_999_999).is_none(), "an expired request does not lapse again");
    }

    #[test]
    fn a_resolved_request_or_one_without_a_deadline_never_lapses() {
        let mut data = card("b1", "x", None, 0);
        data["takeoverRequest"]["status"] = "resolved".into();
        assert!(lapsed(&data, i64::MAX).is_none());
        let old = json!({"takeoverRequest": {"reason": "x", "status": "waiting"}});
        assert!(lapsed(&old, i64::MAX).is_none(), "a card from before deadlines is left alone");
    }

    #[test]
    fn the_sweep_finds_only_the_waiting_requests_that_are_due() {
        let path = std::env::temp_dir().join(format!("takeover-{}.db", uuid::Uuid::new_v4()));
        let store = Store::open(&path).expect("store");
        let config = serde_json::from_value(json!({"id": "b1", "name": "B", "backend": "claude"})).expect("bot");
        store.save_bot(&config).expect("saved");
        let lane = Lane::main("b1");
        store.insert_entry(&lane, EntryKind::Notice, 1, &card("b1", "old", None, 0)).expect("old card");
        store.insert_entry(&lane, EntryKind::Notice, 1, &card("b1", "new", None, TTL_MS)).expect("new card");
        let found = due(&store, TTL_MS + 1).expect("due");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].1["takeoverRequest"]["reason"], "old");
    }
}
