//! The Monday recap: once a week, on Monday morning by this computer's clock, a push tells the person what the
//! week before held (the counts of `Store::week`: no message text). What was said is kept in `recap:<week>`, so the
//! same week is never told twice, not even after a restart.
//!
//! Idea from Coucou's Monday recap; no code copied.

use crate::hub::Hub;
use crate::remote::push::{self, AlertKind};
use crate::store::{Week, now_ms};
use chrono::{DateTime, Datelike, Local, Timelike, Weekday};
use std::sync::Arc;
use std::time::Duration;

/// How often the clock is looked at.
const TICK: Duration = Duration::from_secs(600);
/// Monday's morning: from this hour (included) to [`MORNING_END`] (excluded).
const MORNING_START: u32 = 9;
const MORNING_END: u32 = 12;
const DAY_MS: i64 = 86_400_000;
const WEEK_DAYS: i64 = 7;

/// The ISO week `at` falls in, as the recap's key: `2026-W44`.
pub fn week_key<Tz: chrono::TimeZone>(at: &DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    at.format("%G-W%V").to_string()
}

/// Whether the recap goes out at `at`: Monday morning, when this week's has not gone out yet.
pub fn due<Tz: chrono::TimeZone>(at: &DateTime<Tz>, already_told: bool) -> bool {
    !already_told && at.weekday() == Weekday::Mon && (MORNING_START..MORNING_END).contains(&at.hour())
}

/// What the push says, or `None` when the week held nothing to tell.
pub fn body(week: &Week) -> Option<String> {
    if week.active_bots == 0 {
        return None;
    }
    let mut parts = vec![
        format!("{} messages from you", week.messages),
        format!("{} replies", week.replies),
        format!("{} file edits", week.file_edits),
        format!("{} of {} approvals answered", week.approvals_answered, week.approvals_asked),
    ];
    if week.approvals_waiting > 0 {
        parts.push(format!("{} still waiting", week.approvals_waiting));
    }
    let busiest: Vec<&str> = week.top_bots.iter().map(|b| b.name.as_str()).filter(|n| !n.is_empty()).collect();
    let mut text = parts.join(" · ");
    if !busiest.is_empty() {
        text = format!("{text}. Busiest: {}", busiest.join(", "));
    }
    Some(text)
}

fn key(week: &str) -> String {
    format!("recap:{week}")
}

/// Tells the week at most once; returns whether a push went out.
fn tell(hub: &Hub, at: DateTime<Local>) -> bool {
    let week = week_key(&at);
    if !due(&at, hub.store.kv_get(&key(&week)).is_some()) {
        return false;
    }
    let looked_back = match hub.store.week(now_ms() - WEEK_DAYS * DAY_MS) {
        Ok(week) => week,
        Err(error) => {
            tracing::warn!(error = format!("{error:#}"), "couldn't count the week for the recap");
            return false;
        }
    };
    let text = body(&looked_back);
    // Written before the push: a crash in between loses a recap, never repeats one.
    let snapshot =
        serde_json::json!({"week": week, "sentAt": now_ms(), "told": text.is_some(), "summary": looked_back});
    if let Err(error) = hub.store.kv_set(&key(&week), &snapshot.to_string()) {
        tracing::warn!(error = format!("{error:#}"), "couldn't keep the recap, so it isn't sent");
        return false;
    }
    let (Some(text), Some(config)) = (text, hub.store.bots().ok().and_then(first_bot)) else {
        return false;
    };
    push::notify(hub, &config, None, "Your week with Codync", &text, AlertKind::Done);
    true
}

/// The bot the push is sent through: the push needs one, and every bot reaches the same phone.
fn first_bot(bots: Vec<crate::store::BotRow>) -> Option<crate::store::BotConfig> {
    bots.into_iter().filter(|row| !row.deleted && !row.config.is_group()).map(|row| row.config).next()
}

/// Looks at the clock for as long as the host runs.
pub async fn run(hub: Arc<Hub>) {
    let mut tick = tokio::time::interval(TICK);
    loop {
        tick.tick().await;
        let hub = hub.clone();
        if let Err(error) = tokio::task::spawn_blocking(move || tell(&hub, Local::now())).await {
            tracing::warn!(error = %error, "the weekly recap stopped for a tick");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::week::TopBot;
    use chrono::{TimeZone, Utc};

    fn at(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, 0, 0).single().expect("a date")
    }

    #[test]
    fn it_goes_out_on_monday_morning_once_a_week() {
        // 2026-11-02 is a Monday.
        assert!(due(&at(2026, 11, 2, 9), false));
        assert!(due(&at(2026, 11, 2, 11), false));
        assert!(!due(&at(2026, 11, 2, 8), false), "too early");
        assert!(!due(&at(2026, 11, 2, 12), false), "morning is over");
        assert!(!due(&at(2026, 11, 3, 9), false), "Tuesday");
        assert!(!due(&at(2026, 11, 2, 10), true), "already told this week");
    }

    #[test]
    fn a_week_is_named_by_its_iso_week() {
        assert_eq!(week_key(&at(2026, 11, 2, 9)), "2026-W45");
        assert_eq!(week_key(&at(2026, 11, 8, 23)), "2026-W45", "Sunday ends the week");
        assert_eq!(week_key(&at(2026, 11, 9, 0)), "2026-W46");
    }

    #[test]
    fn the_push_counts_and_names_but_says_nothing_for_an_empty_week() {
        assert_eq!(body(&Week::default()), None);
        let week = Week {
            messages: 12,
            replies: 11,
            file_edits: 3,
            approvals_asked: 4,
            approvals_answered: 3,
            approvals_waiting: 1,
            active_bots: 2,
            top_bots: vec![TopBot { id: "a".into(), name: "Reviewer".into(), entries: 9 }],
            ..Week::default()
        };
        let text = body(&week).expect("a week with activity");
        assert!(text.contains("12 messages from you") && text.contains("3 of 4 approvals answered"), "{text}");
        assert!(text.ends_with("Busiest: Reviewer"), "{text}");
        assert!(text.contains("1 still waiting"));
    }
}
