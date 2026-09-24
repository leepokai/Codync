//! Push via the Codync relay (it holds the APNs key; the host only holds opaque
//! per-device tickets the relay issued to the phone).
//!
//! Like Grok Bot there are only two alert kinds: "needs you" and "done", and
//! none are sent while the phone app is connected (it's in the foreground).

use crate::hub::{Hub, Runtime};
use crate::store::BotConfig;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

static LAST: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);
/// Tickets the relay reported as dead (app uninstalled / token rotated); purged on the next push.
static GONE: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn relay_url(hub: &Hub) -> Option<String> {
    std::env::var("CODYNC_RELAY_URL").ok().or_else(|| hub.store.kv_get("relay_url")).filter(|u| !u.is_empty())
}

fn post(url: String, body: Value) {
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        match client.post(&url).json(&body).timeout(Duration::from_secs(10)).send().await {
            Ok(r) if r.status().is_success() => {}
            Ok(r) if r.status() == reqwest::StatusCode::GONE => {
                if let Some(t) = body["ticket"].as_str() {
                    GONE.lock().unwrap().push(t.to_owned());
                }
            }
            Ok(r) => tracing::warn!("relay {} → {}", url, r.status()),
            Err(e) => tracing::warn!("relay {url}: {e}"),
        }
    });
}

pub fn notify(hub: &Hub, bot: &BotConfig, title: &str, body: &str, kind: &str) {
    if bot.notify == Some(false) || bot.hidden || hub.ios_connected() {
        return;
    }
    // At most one alert per bot+kind every 5 s.
    {
        let mut last = LAST.lock().unwrap();
        let map = last.get_or_insert_with(HashMap::new);
        let key = format!("{}:{kind}", bot.id);
        if map.get(&key).is_some_and(|t| t.elapsed() < Duration::from_secs(5)) {
            return;
        }
        map.insert(key, Instant::now());
    }
    let Some(relay) = relay_url(hub) else { return };
    for t in std::mem::take(&mut *GONE.lock().unwrap()) {
        let _ = hub.store.remove_device(&t);
    }
    for ticket in hub.store.devices() {
        post(
            format!("{relay}/push"),
            json!({
                "ticket": ticket,
                "alert": {"title": title, "body": crate::acp::truncate(body, 140)},
                "threadId": bot.id,
                "category": kind,
                "data": {"botId": bot.id},
            }),
        );
    }
}

/// Throttled Live Activity content updates for bots the phone is watching.
pub fn live_activity_update(hub: &Hub, bot_id: &str, rt: &Runtime) {
    let tickets = hub.activities.lock().unwrap().get(bot_id).cloned().unwrap_or_default();
    if tickets.is_empty() {
        return;
    }
    {
        let mut last = LAST.lock().unwrap();
        let map = last.get_or_insert_with(HashMap::new);
        let key = format!("{bot_id}:la");
        let urgent = rt.status != "working";
        if !urgent && map.get(&key).is_some_and(|t| t.elapsed() < Duration::from_secs(8)) {
            return;
        }
        map.insert(key, Instant::now());
    }
    let Some(relay) = relay_url(hub) else { return };
    let state = json!({
        "status": if rt.status.is_empty() { "idle" } else { &rt.status },
        "activity": rt.activity,
        "startedAt": rt.started_at.map(|ms| ms as f64 / 1000.0 - 978_307_200.0), // Swift Date reference epoch
    });
    for ticket in tickets {
        post(format!("{relay}/push"), json!({"ticket": ticket, "liveActivity": {"event": "update", "contentState": state}}));
    }
}

pub fn live_activity_end(hub: &Hub, bot_id: &str) {
    let tickets = hub.activities.lock().unwrap().remove(bot_id).unwrap_or_default();
    let Some(relay) = relay_url(hub) else { return };
    for ticket in tickets {
        post(
            format!("{relay}/push"),
            json!({"ticket": ticket, "liveActivity": {"event": "end", "contentState": {"status": "idle", "activity": "", "startedAt": null}}}),
        );
    }
}
