//! Proactive check-ins: a bot with them on is woken every `every_hours`, outside its quiet hours and at most
//! `max_per_day` times a day, to look at what's open and tell the person only if there is something worth saying.

use crate::api::apps_runtime::dispatch_bot_turn;
use crate::hub::{BotStatus, Hub};
use crate::store::{ProactiveSettings, now_ms};
use chrono::{DateTime, Local, Timelike, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

const TICK: Duration = Duration::from_secs(60);
const HOUR_MS: i64 = 3_600_000;
const PROMPT: &str = "[Proactive check-in, started by you on a schedule, not by the user] Review your open items, \
routines and recent conversation. If something needs the user's attention or you have useful progress to report, \
say it briefly. If not, reply with exactly NO_UPDATE.";

/// When a bot was last woken and how many times on `day` (its local date).
#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Eq)]
struct Ledger {
    last: i64,
    day: String,
    count: u32,
}

/// The hour of day and the date in `zone` (the computer's own zone when unset or unknown).
fn local_clock(now: i64, zone: Option<&str>) -> (u32, String) {
    let at = DateTime::<Utc>::from_timestamp_millis(now).unwrap_or_default();
    if let Some(tz) = zone.and_then(|z| z.parse::<Tz>().ok()) {
        let t = at.with_timezone(&tz);
        (t.hour(), t.format("%Y-%m-%d").to_string())
    } else {
        let t = at.with_timezone(&Local);
        (t.hour(), t.format("%Y-%m-%d").to_string())
    }
}

/// Whether `hour` falls in the quiet window `start..end`, which may wrap past midnight.
fn is_quiet(hour: u32, start: u8, end: u8) -> bool {
    let (start, end) = (u32::from(start), u32::from(end));
    match start.cmp(&end) {
        std::cmp::Ordering::Equal => false,
        std::cmp::Ordering::Less => (start..end).contains(&hour),
        std::cmp::Ordering::Greater => hour >= start || hour < end,
    }
}

/// Decides one bot's check-in at `now`: the ledger to keep, and whether to wake the bot.
fn decide(settings: &ProactiveSettings, ledger: Option<Ledger>, now: i64) -> (Ledger, bool) {
    let (hour, day) = local_clock(now, settings.timezone.as_deref());
    // A bot just switched on waits one interval before its first check-in.
    let Some(mut ledger) = ledger else { return (Ledger { last: now, day, count: 0 }, false) };
    if ledger.day != day {
        ledger.day = day;
        ledger.count = 0;
    }
    let waited = now - ledger.last >= i64::from(settings.every_hours.max(1)) * HOUR_MS;
    let due =
        waited && ledger.count < settings.max_per_day && !is_quiet(hour, settings.quiet_start, settings.quiet_end);
    if due {
        ledger.last = now;
        ledger.count += 1;
    }
    (ledger, due)
}

fn key(bot_id: &str) -> String {
    format!("proactive:{bot_id}")
}

fn check_in(hub: &Hub, now: i64) {
    let bots = match hub.store.bots() {
        Ok(bots) => bots,
        Err(error) => {
            tracing::warn!(error = format!("{error:#}"), "couldn't list bots for proactive check-ins");
            return;
        }
    };
    for row in bots.into_iter().filter(|r| !r.deleted) {
        let cfg = &row.config;
        let id = &cfg.id;
        if !cfg.proactive.enabled || cfg.is_group() || crate::external::is_external(cfg) {
            if let Err(error) = hub.store.kv_delete(&key(id)) {
                tracing::warn!(bot = id, error = format!("{error:#}"), "couldn't clear the proactive ledger");
            }
            continue;
        }
        let ledger = hub.store.kv_get(&key(id)).and_then(|raw| serde_json::from_str::<Ledger>(&raw).ok());
        let (next, due) = decide(&cfg.proactive, ledger, now);
        // A busy bot is asked again on the next tick, without using up today's budget.
        if due && hub.runtime(id).status != BotStatus::Idle {
            continue;
        }
        if due && let Err(error) = dispatch_bot_turn(hub, id, PROMPT, "proactive") {
            tracing::warn!(bot = id, error = format!("{error:#}"), "proactive check-in failed");
            continue;
        }
        if let Ok(raw) = serde_json::to_string(&next)
            && let Err(error) = hub.store.kv_set(&key(id), &raw)
        {
            tracing::warn!(bot = id, error = format!("{error:#}"), "saving the proactive ledger failed");
        }
    }
}

pub async fn run(hub: Arc<Hub>) {
    let mut tick = tokio::time::interval(TICK);
    loop {
        tick.tick().await;
        let hub = hub.clone();
        if let Err(error) = tokio::task::spawn_blocking(move || check_in(&hub, now_ms())).await {
            tracing::warn!(error = format!("{error:#}"), "proactive check-ins stopped for a tick");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(every_hours: u32, max_per_day: u32) -> ProactiveSettings {
        ProactiveSettings {
            enabled: true,
            every_hours,
            quiet_start: 22,
            quiet_end: 8,
            timezone: Some("UTC".into()),
            max_per_day,
            apps_in_background: true,
        }
    }

    /// 2026-01-01 at `hour`:00 UTC.
    fn at(hour: i64) -> i64 {
        1_767_225_600_000 + hour * HOUR_MS
    }

    #[test]
    fn quiet_hours_wrap_past_midnight() {
        assert!(is_quiet(23, 22, 8) && is_quiet(3, 22, 8) && !is_quiet(8, 22, 8) && !is_quiet(12, 22, 8));
        assert!(is_quiet(13, 12, 14) && !is_quiet(14, 12, 14));
        assert!(!is_quiet(3, 5, 5), "equal bounds mean no quiet hours");
    }

    #[test]
    fn a_new_bot_waits_one_interval_then_wakes() {
        let s = settings(4, 3);
        let (ledger, due) = decide(&s, None, at(9));
        assert!(!due);
        let (ledger, due) = decide(&s, Some(ledger), at(12));
        assert!(!due, "3 hours is less than the interval");
        let (ledger, due) = decide(&s, Some(ledger), at(13));
        assert!(due);
        assert_eq!(ledger.count, 1);
    }

    #[test]
    fn quiet_hours_and_the_daily_budget_hold_it_back() {
        let s = settings(1, 2);
        let seed = Ledger { last: at(8), day: "2026-01-01".into(), count: 0 };
        let (ledger, due) = decide(&s, Some(seed), at(23));
        assert!(!due, "23:00 is quiet");
        assert_eq!(ledger.count, 0);
        let full = Ledger { last: at(9), day: "2026-01-01".into(), count: 2 };
        assert!(!decide(&s, Some(full), at(12)).1, "budget used up");
        let yesterday = Ledger { last: at(9) - 24 * HOUR_MS, day: "2025-12-31".into(), count: 2 };
        assert!(decide(&s, Some(yesterday), at(12)).1, "a new day starts a new budget");
    }

    /// A host with one bot that has check-ins on every hour.
    fn hub_with_check_ins() -> (Arc<Hub>, String, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("codync-proactive-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("store");
        let identity = crate::remote::identity::Identity::load_or_create(&dir).expect("identity");
        let hub = Hub::new(store, "proactive-test".into(), identity, "t".into(), 0);
        let mut cfg: crate::store::BotConfig =
            serde_json::from_value(serde_json::json!({"id": "b1", "name": "B", "backend": "claude"})).expect("bot");
        cfg.proactive = settings(1, 4);
        hub.store.save_bot(&cfg).expect("saved");
        (hub, cfg.id, dir)
    }

    #[test]
    fn turning_check_ins_on_starts_the_ledger_without_saying_anything_and_the_first_one_stays_internal() {
        let (hub, id, dir) = hub_with_check_ins();
        // The first look only starts the ledger: a new bot waits one interval.
        check_in(&hub, at(10));
        let ledger = hub.store.kv_get(&key(&id)).expect("a ledger for the bot");
        assert!(ledger.contains("\"count\":0"), "{ledger}");
        assert!(
            hub.store.entries_since(0, 50).expect("entries").iter().all(|e| e.kind != "user"),
            "nothing was sent yet"
        );
        // An hour later it is due: the message it sends is internal, from the scheduler, and never shown as a chat message.
        check_in(&hub, at(11) + 1);
        let sent: Vec<_> =
            hub.store.entries_since(0, 50).expect("entries").into_iter().filter(|e| e.kind == "user").collect();
        assert_eq!(sent.len(), 1, "one check-in");
        assert_eq!(
            (sent[0].data["from"].as_str(), sent[0].data["internal"].as_bool()),
            (Some("proactive"), Some(true))
        );
        assert!(crate::store::is_internal(&sent[0].data), "no client lists it");
        // This host has no running bot to take the message, so the check-in could not start: it is not counted, and the
        // next tick asks again.
        assert!(hub.store.kv_get(&key(&id)).expect("ledger").contains("\"count\":0"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
