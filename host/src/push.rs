//! Push via the Codync relay (it holds the APNs key; the host only holds opaque
//! per-device tickets the relay issued to the phone).
//!
//! Like Grok Bot there are only two alert kinds: "needs you" and "done", and
//! none are sent while the phone app is connected (it's in the foreground).

use crate::LockExt;
use crate::crypto;
use crate::hub::{BotStatus, Hub, Runtime};
use crate::store::BotConfig;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use x25519_dalek::StaticSecret;

/// The two alert kinds (wire values double as the APNs category).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AlertKind {
    Done,
    NeedsInput,
}

/// (bot id, kind) → last alert.
static LAST_SENT: LazyLock<Mutex<HashMap<(String, AlertKind), Instant>>> = LazyLock::new(Mutex::default);
/// Tickets the relay reported as dead (app uninstalled / token rotated); purged on the next push.
static GONE: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Seconds between the Unix epoch and Swift's `Date` reference date (2001-01-01).
const SWIFT_REFERENCE_EPOCH: f64 = 978_307_200.0;

fn relay_url(hub: &Hub) -> Option<String> {
    std::env::var("CODYNC_RELAY_URL").ok().or_else(|| hub.store.kv_get("relay_url")).filter(|u| !u.is_empty())
}

/// True when `key` was sent less than `min_gap` ago; otherwise records now.
fn throttled(key: (String, AlertKind), min_gap: Duration) -> bool {
    let mut last = LAST_SENT.locked();
    if last.get(&key).is_some_and(|t| t.elapsed() < min_gap) {
        return true;
    }
    last.insert(key, Instant::now());
    false
}

fn post(url: String, body: Value) {
    tokio::spawn(async move {
        match crate::http().post(&url).json(&body).timeout(Duration::from_secs(10)).send().await {
            Ok(r) if r.status().is_success() => {}
            Ok(r) if r.status() == reqwest::StatusCode::GONE => {
                if let Some(t) = body["ticket"].as_str() {
                    GONE.locked().push(t.to_owned());
                }
            }
            Ok(r) => tracing::warn!(%url, status = %r.status(), "relay rejected push"),
            Err(error) => tracing::warn!(%url, %error, "relay unreachable"),
        }
    });
}

pub fn notify(hub: &Hub, bot: &BotConfig, title: &str, body: &str, kind: AlertKind) {
    if bot.notify == Some(false) || bot.hidden || hub.ios_connected() {
        return;
    }
    if throttled((bot.id.clone(), kind), Duration::from_secs(5)) {
        return;
    }
    let Some(relay) = relay_url(hub) else { return };
    for t in std::mem::take(&mut *GONE.locked()) {
        if let Err(error) = hub.store.remove_push_ticket(&t) {
            tracing::warn!(%error, "couldn't forget a dead push ticket");
        }
    }
    // The relay and APNs only see a generic line; the real one is sealed to each device (§6.7).
    let generic = match kind {
        AlertKind::NeedsInput => "Needs you",
        AlertKind::Done => "Done",
    };
    let secret = json!({"title": title, "body": crate::acp::truncate(body, 140)}).to_string();
    let computer_id = hub.identity.computer_id();
    for t in hub.store.push_tickets() {
        let mut data = json!({"botId": bot.id, "computerId": computer_id, "ctx": t.ctx});
        if let Some(key) = t.push_key.as_deref().and_then(|k| crypto::unb64_n::<32>(k).ok()) {
            let eph = StaticSecret::from(crypto::random::<32>());
            match crypto::seal_push(&hub.identity.cid_raw(), &key, secret.as_bytes(), &eph) {
                Ok(sealed) => data["sealed"] = sealed.into(),
                Err(error) => tracing::warn!(error = format!("{error:#}"), "couldn't seal a notification"),
            }
        }
        post(
            format!("{relay}/push"),
            json!({
                "ticket": t.ticket,
                "alert": {"title": "Codync", "body": generic},
                "mutableContent": true,
                "threadId": bot.id,
                "category": kind,
                "data": data,
            }),
        );
    }
}

/// Live Activity status updates for bots a phone is watching. Only the status enum and
/// start time travel: no free text leaves the computer this way.
pub fn live_activity_update(hub: &Hub, bot_id: &str, rt: &Runtime) {
    let tickets = hub.store.activity_tickets(bot_id);
    if tickets.is_empty() {
        return;
    }
    let Some(relay) = relay_url(hub) else { return };
    #[allow(clippy::cast_precision_loss)] // epoch milliseconds fit an f64 mantissa until year 287,396
    let started_at = rt.started_at.map(|ms| ms as f64 / 1000.0 - SWIFT_REFERENCE_EPOCH);
    let state = json!({"status": rt.status, "activity": "", "startedAt": started_at});
    for ticket in tickets {
        post(
            format!("{relay}/push"),
            json!({"ticket": ticket, "liveActivity": {"event": "update", "contentState": state}}),
        );
    }
}

pub fn live_activity_end(hub: &Hub, bot_id: &str) {
    let tickets = hub.store.take_activity_tickets(bot_id);
    let Some(relay) = relay_url(hub) else { return };
    let state = json!({"status": BotStatus::Idle, "activity": "", "startedAt": null});
    for ticket in tickets {
        post(
            format!("{relay}/push"),
            json!({"ticket": ticket, "liveActivity": {"event": "end", "contentState": state}}),
        );
    }
}
