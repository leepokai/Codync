//! Push via the Codync relay (it holds the APNs key; the host only holds opaque
//! per-device tickets the relay issued to the phone).
//!
//! Alerts cover completion, input requests, and failures;
//! none are sent while the phone app is connected (it's in the foreground).

use crate::LockExt;
use crate::hub::{BotStatus, Hub, Runtime};
use crate::remote::crypto;
use crate::store::BotConfig;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use x25519_dalek::StaticSecret;

/// Alert kinds (wire values double as the APNs category).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AlertKind {
    Done,
    NeedsInput,
    Failed,
}

/// (bot id, kind) → last alert. Workflow steps keep a separate table so two steps on one bot don't swallow each other.
static LAST_SENT: LazyLock<Mutex<HashMap<(String, AlertKind), Instant>>> = LazyLock::new(Mutex::default);
static WORKFLOW_SENT: LazyLock<Mutex<HashMap<(String, AlertKind), Instant>>> = LazyLock::new(Mutex::default);

/// Where a workflow notification opens. The ids go on the APNs `data`, which the phone reads as `userInfo`.
pub struct WorkflowLink {
    pub workflow: String,
    pub run: Option<String>,
    pub step: String,
    pub node: String,
}
/// Tickets the relay reported as dead (app uninstalled / token rotated); purged on the next push.
static GONE: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Seconds between the Unix epoch and Swift's `Date` reference date (2001-01-01).
const SWIFT_REFERENCE_EPOCH: f64 = 978_307_200.0;

/// The push relay's former address: `*.workers.dev` is blocked in mainland China (#570), and phone builds from
/// before the move still register it.
const WORKERS_DEV_RELAY: &str = "https://codync-relay.kevin2005ha.workers.dev";
const RELAY: &str = "https://push.codync.dev";

fn relay_url(hub: &Hub) -> Option<String> {
    std::env::var("CODYNC_RELAY_URL")
        .ok()
        .or_else(|| hub.store.kv_get("relay_url").map(reachable_relay))
        .filter(|u| !u.is_empty())
}

/// The same relay Worker on its custom domain, which mainland China reaches.
fn reachable_relay(url: String) -> String {
    if url == WORKERS_DEV_RELAY { RELAY.to_owned() } else { url }
}

/// The least time between two alerts of one kind for one bot (or one workflow step).
const ALERT_GAP: Duration = Duration::from_secs(5);

/// True when `key` was sent less than `ALERT_GAP` ago; otherwise records now.
fn throttled(key: (String, AlertKind)) -> bool {
    throttled_in(&LAST_SENT, key, ALERT_GAP)
}

fn throttled_in(
    table: &Mutex<HashMap<(String, AlertKind), Instant>>,
    key: (String, AlertKind),
    min_gap: Duration,
) -> bool {
    let mut last = table.locked();
    // Keys past the gap can't hold anything back: dropping them keeps one entry per recent alert, not per step ever.
    last.retain(|_, at| at.elapsed() < min_gap);
    if last.contains_key(&key) {
        return true;
    }
    last.insert(key, Instant::now());
    false
}

fn post(url: String, body: Value) {
    tokio::spawn(async move {
        match crate::http().post(&url).json(&body).timeout(Duration::from_secs(10)).send().await {
            Ok(r) if r.status().is_success() => {
                if let Some(notifications) = body["notifications"].as_array() {
                    match r.json::<Value>().await {
                        Ok(reply) => record_batch_results(notifications, &reply),
                        Err(error) => tracing::warn!(%error, "invalid relay batch response"),
                    }
                }
            }
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

fn record_batch_results(notifications: &[Value], reply: &Value) {
    for result in reply["results"].as_array().into_iter().flatten() {
        let Some(index) = result["index"].as_u64().and_then(|index| usize::try_from(index).ok()) else { continue };
        let Some(ticket) = notifications.get(index).and_then(|item| item["ticket"].as_str()) else { continue };
        if result["status"] == 410 || result["superseded"] == true {
            GONE.locked().push(ticket.to_owned());
        } else if result["status"] != 200 {
            tracing::warn!(status = ?result["status"], "relay rejected a batched notification");
        }
    }
}

fn purge_dead_tickets(hub: &Hub) {
    for t in std::mem::take(&mut *GONE.locked()) {
        if let Err(error) = hub.store.remove_push_ticket(&t) {
            tracing::warn!(%error, "couldn't forget a dead push ticket");
        }
    }
}

/// The chat, its first two members (a group's avatar) and the speaker, just enough for the phone
/// to draw their faces even when its roster copy is older than the bot.
fn faces(hub: &Hub, bot: &BotConfig, from: Option<&str>) -> Vec<Value> {
    let face = |c: &BotConfig| {
        json!({"id": c.id, "kind": c.kind, "members": c.members, "name": c.name,
            "avatarColor": c.avatar_color, "avatarShape": c.avatar_shape})
    };
    let mut ids: Vec<&str> = bot.members.iter().take(2).map(String::as_str).collect();
    if let Some(from) = from.filter(|f| *f != bot.id && !ids.contains(f)) {
        ids.push(from);
    }
    std::iter::once(face(bot))
        .chain(ids.into_iter().filter_map(|id| hub.store.bot(id).ok().flatten()).map(|r| face(&r.config)))
        .collect()
}

/// An alert's title and body as a phone's push and a desktop notification show them: cut to fit.
fn alert_text(title: &str, body: &str) -> (String, String) {
    (codync_agent::acp::truncate(title, 80), codync_agent::acp::truncate(body, 400))
}

/// What an alert seals for the phone: its text, the faces to draw and the unread total for the app icon.
fn sealed_alert(hub: &Hub, bot: &BotConfig, from: Option<&str>, title: &str, subtitle: &str, body: &str) -> Value {
    let (title, body) = alert_text(title, body);
    let mut secret = json!({"title": title, "subtitle": subtitle, "body": body});
    if let Some(from) = from.filter(|f| *f != bot.id) {
        secret["from"] = from.into();
    }
    secret["faces"] = faces(hub, bot, from).into();
    // The phone adds its other computers' counts and badges the app icon with the sum.
    secret["unread"] = hub.store.total_unread().into();
    secret
}

/// The `alert` event: what a desktop app shows as a system notification, the same text a phone's push opens to.
/// A workflow's alert names the workflow, so clicking it opens the workflow rather than the bot's chat.
fn alert_event(bot: &BotConfig, title: &str, body: &str, kind: AlertKind, link: Option<&WorkflowLink>) -> Value {
    let (title, body) = alert_text(title, body);
    let mut alert = json!({"botId": bot.id, "kind": kind, "title": title, "body": body});
    if let Some(link) = link {
        alert["workflowId"] = link.workflow.clone().into();
    }
    json!({"type": "alert", "alert": alert})
}

/// `bot` is the chat the alert opens; `from` is the member bot speaking when that chat is a group.
pub fn notify(
    hub: &Hub,
    bot: &BotConfig,
    from: Option<&str>,
    title: &str,
    body: &str,
    kind: AlertKind,
    link: Option<&WorkflowLink>,
) {
    purge_dead_tickets(hub);
    if bot.notify == Some(false) || bot.hidden {
        return;
    }
    // A workflow step's alerts are throttled per step, so two steps of one bot both arrive.
    let repeated = match link {
        Some(link) => throttled_in(&WORKFLOW_SENT, (link.step.clone(), kind), ALERT_GAP),
        None => throttled((bot.id.clone(), kind)),
    };
    if repeated {
        return;
    }
    // Live only (no rev, so no catch-up replays it): a desktop app shows it if its user turned that on.
    let _ = hub.events.send(alert_event(bot, title, body, kind, link));
    if hub.phone_connected() {
        return;
    }
    let Some(relay) = relay_url(hub) else { return };
    // The relay and APNs only see a generic line; the real one is sealed to each device (§6.7).
    let generic = match kind {
        AlertKind::NeedsInput => "A bot needs your response. Open Codync to review.",
        AlertKind::Done => "Your task is complete. Open Codync to read the result.",
        AlertKind::Failed => "A task could not finish. Open Codync to review the issue.",
    };
    let subtitle = match kind {
        AlertKind::Done => "Task complete",
        AlertKind::NeedsInput => "Response needed",
        AlertKind::Failed => "Task failed",
    };
    let secret = sealed_alert(hub, bot, from, title, subtitle, body).to_string();
    let computer_id = hub.identity.computer_id();
    let mut notifications = Vec::new();
    for t in hub.store.push_tickets() {
        let mut data = json!({"botId": bot.id, "computerId": computer_id, "ctx": t.ctx});
        if let Some(link) = link {
            data["workflowId"] = link.workflow.clone().into();
            data["stepId"] = link.step.clone().into();
            if !link.node.is_empty() {
                data["nodeId"] = link.node.clone().into();
            }
            if let Some(run) = &link.run {
                data["runId"] = run.clone().into();
            }
        }
        if let Some(key) = t.push_key.as_deref().and_then(|k| crypto::unb64_n::<32>(k).ok()) {
            let eph = StaticSecret::from(crypto::random::<32>());
            match crypto::seal_push(&hub.identity.cid_raw(), &key, secret.as_bytes(), &eph) {
                Ok(sealed) => data["sealed"] = sealed.into(),
                Err(error) => tracing::warn!(error = format!("{error:#}"), "couldn't seal a notification"),
            }
        }
        notifications.push(json!({
            "ticket": t.ticket,
            "alert": {"title": "Codync", "body": generic},
            "mutableContent": true,
            "threadId": format!("{computer_id}:{}", bot.id),
            "category": kind,
            "data": data,
        }));
    }
    if !notifications.is_empty() {
        post(format!("{relay}/push-batch"), json!({"notifications": notifications}));
    }
}

/// Live Activities carry only status and dates. Task text stays in the encrypted channel.
pub fn live_activity_update(hub: &Hub, bot_id: &str, rt: &Runtime) {
    purge_dead_tickets(hub);
    let Some(relay) = relay_url(hub) else { return };
    let terminal = matches!(rt.status, BotStatus::Idle | BotStatus::Error);
    let tickets = if terminal { hub.store.take_activity_tickets(bot_id) } else { hub.store.activity_tickets(bot_id) };
    let activity = activity_payload(rt);
    for ticket in tickets {
        post(format!("{relay}/push"), json!({"ticket": ticket, "liveActivity": activity}));
    }
}

fn activity_payload(rt: &Runtime) -> Value {
    #[allow(clippy::cast_precision_loss)]
    let started_at = rt.started_at.map(|ms| ms as f64 / 1000.0 - SWIFT_REFERENCE_EPOCH);
    let terminal = matches!(rt.status, BotStatus::Idle | BotStatus::Error);
    let timestamp = crate::store::now_ms() / 1000;
    json!({
        "event": if terminal { "end" } else { "update" },
        "timestamp": timestamp,
        "staleDate": timestamp + 15 * 60,
        "contentState": {"status": rt.status, "activity": "", "startedAt": started_at},
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::calls::context::tests::{bot, hub};
    use crate::store::{EntryKind, Lane};

    #[test]
    fn the_workers_dev_relay_is_reached_on_its_custom_domain() {
        assert_eq!(reachable_relay(WORKERS_DEV_RELAY.to_owned()), RELAY);
        assert_eq!(reachable_relay("https://relay.example.com".to_owned()), "https://relay.example.com");
    }

    #[test]
    fn an_alert_seals_the_computers_unread_total() {
        let hub = hub();
        let bot = bot("b1", "Bot", "/tmp");
        hub.store.save_bot(&bot).unwrap();
        hub.store
            .insert_entry(&Lane::main("b1"), EntryKind::Agent, 1, &json!({"text": "done", "final": true}))
            .unwrap();
        let secret = sealed_alert(&hub, &bot, None, "Bot", "Task complete", "done");
        assert_eq!(secret["unread"], 1);
        assert_eq!(secret["title"], "Bot");
        assert_eq!(secret["faces"][0]["id"], "b1");
    }

    #[test]
    fn alert_records_past_their_gap_are_dropped() {
        let table = Mutex::default();
        assert!(!throttled_in(&table, ("s1".into(), AlertKind::Done), Duration::ZERO));
        assert!(!throttled_in(&table, ("s2".into(), AlertKind::Done), Duration::ZERO));
        assert_eq!(table.locked().len(), 1);
    }

    #[test]
    fn a_workflow_alert_names_its_workflow() {
        let bot = bot("b1", "Bot", "/tmp");
        let link = WorkflowLink { workflow: "w1".into(), run: None, step: "s1".into(), node: String::new() };
        assert_eq!(
            alert_event(&bot, "Flow", "Ship it?", AlertKind::NeedsInput, Some(&link))["alert"]["workflowId"],
            "w1"
        );
        assert!(alert_event(&bot, "Bot", "done", AlertKind::Done, None)["alert"].get("workflowId").is_none());
    }

    #[test]
    fn activity_failure_ends_with_error_and_never_includes_task_text() {
        let payload = activity_payload(&Runtime {
            status: BotStatus::Error,
            activity: "secret file".into(),
            ..Runtime::default()
        });
        assert_eq!(payload["event"], "end");
        assert_eq!(payload["contentState"]["status"], "error");
        assert_eq!(payload["contentState"]["activity"], "");
    }

    #[test]
    fn waiting_for_input_stays_live_with_a_freshness_deadline() {
        let payload = activity_payload(&Runtime {
            status: BotStatus::NeedsInput,
            started_at: Some(978_307_201_000),
            ..Runtime::default()
        });
        assert_eq!(payload["event"], "update");
        assert_eq!(payload["contentState"]["startedAt"], 1.0);
        assert_eq!(payload["staleDate"].as_i64().unwrap() - payload["timestamp"].as_i64().unwrap(), 900);
    }
}
