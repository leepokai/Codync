//! Validated schedules. Calendar schedules retain their IANA time zone across restarts.
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use croner::Cron;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// This computer's IANA time zone (routines run on the host's clock in it): `TZ`, else the system's, else UTC.
pub fn local_zone() -> String {
    let valid = |z: &str| Tz::from_str(z).is_ok();
    if let Ok(tz) = std::env::var("TZ")
        && valid(tz.trim_start_matches(':'))
    {
        return tz.trim_start_matches(':').to_owned();
    }
    iana_time_zone::get_timezone().ok().filter(|z| valid(z)).unwrap_or_else(|| "UTC".into())
}

/// How long a run may be queued after its occurrence without counting as missed: one tick of slack.
pub const GRACE_MS: i64 = 60_000;
/// The largest count a missed-run record names exactly; a larger one means "more than this".
pub const MAX_MISSED: u32 = 999;

/// The occurrences that came due and did not run; the run's `status` says what happened to them.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Missed {
    pub count: u32,
    /// The last of them, when there was more than one.
    pub until: Option<i64>,
}

/// What a due occurrence at `due` stands for when the scheduler first sees it at `now`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissedWindow {
    /// The occurrences in [due, now], at most `MAX_MISSED`.
    pub count: u32,
    /// The newest of them.
    pub last: i64,
    /// The one before `last`, when there was more than one.
    pub before_last: Option<i64>,
}

impl Trigger {
    /// The occurrences this trigger let come due between `due` and `now`, when the host was away
    /// past `GRACE_MS`; `None` when the run is on time, or the trigger has no clock.
    pub fn missed_window(&self, due: i64, now: i64) -> Option<MissedWindow> {
        if now.saturating_sub(due) <= GRACE_MS {
            return None;
        }
        let (count, last, before_last) = match self {
            Self::Interval { seconds } => {
                let step = seconds.checked_mul(1000).filter(|s| *s > 0)?;
                let steps = now.saturating_sub(due) / step;
                let count = u32::try_from(steps.saturating_add(1)).unwrap_or(MAX_MISSED).min(MAX_MISSED);
                let last = due.saturating_add(steps * step);
                (count, last, (steps > 0).then(|| last - step))
            }
            Self::Cron { expression, time_zone } => {
                let zone = time_zone.parse::<Tz>().ok()?;
                let cron = Cron::from_str(expression).ok()?;
                let from = DateTime::<Utc>::from_timestamp_millis(due)?.with_timezone(&zone);
                let to = DateTime::<Utc>::from_timestamp_millis(now)?.with_timezone(&zone);
                let mut count = 1u32;
                let mut cursor = from;
                while let Ok(next) = cron.find_next_occurrence(&cursor, false) {
                    if next > to {
                        break;
                    }
                    count = count.saturating_add(1);
                    if count > MAX_MISSED {
                        break;
                    }
                    cursor = next;
                }
                let last = cron.find_previous_occurrence(&to, true).ok()?;
                let before = (count > 1)
                    .then(|| {
                        DateTime::<Utc>::from_timestamp_millis(last.timestamp_millis() - 1000)
                            .map(|t| t.with_timezone(&zone))
                            .and_then(|t| cron.find_previous_occurrence(&t, true).ok())
                    })
                    .flatten();
                (count.min(MAX_MISSED), last.timestamp_millis(), before.map(|d| d.timestamp_millis()))
            }
            Self::Once { .. } => (1, due, None),
            Self::Webhook | Self::Event { .. } | Self::Watch { .. } => return None,
        };
        Some(MissedWindow { count, last, before_last })
    }

    /// The record the window keeps: every occurrence it holds, or the ones earlier than `last`
    /// when the newest runs (`keep_newest`).
    pub fn missed_record(window: &MissedWindow, keep_newest: bool) -> Missed {
        if keep_newest {
            let others = window.count.saturating_sub(1).min(MAX_MISSED);
            Missed { count: others, until: window.before_last.filter(|_| others > 1) }
        } else {
            Missed { count: window.count, until: (window.count > 1).then_some(window.last) }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Trigger {
    Interval {
        seconds: i64,
    },
    Once {
        at: i64,
    },
    Cron {
        expression: String,
        time_zone: String,
    },
    Webhook,
    Event {
        source: String,
        event: String,
        #[serde(default)]
        filters: std::collections::BTreeMap<String, String>,
    },
    /// Runs when what `target` names changes, checked every `every_minutes` (see `watch`).
    Watch {
        kind: super::watch::WatchKind,
        target: String,
        every_minutes: i64,
    },
}

impl Trigger {
    pub fn validate(&self, now: i64) -> Result<()> {
        match self {
            Self::Interval { seconds } if !(1..=31_536_000).contains(seconds) => {
                bail!("interval must be between 1 second and 365 days")
            }
            Self::Once { at } if *at <= now => bail!("one-time trigger must be in the future"),
            Self::Cron { expression, time_zone } => {
                if expression.split_whitespace().count() != 5 {
                    bail!("use a five-field cron expression");
                }
                Cron::from_str(expression).context("invalid cron expression")?;
                time_zone.parse::<Tz>().context("invalid IANA time zone")?;
                self.next(now)?.context("schedule has no future occurrence")?;
            }
            Self::Event { source, event, filters } => {
                if ![
                    "slack",
                    "telegram",
                    "discord",
                    "github",
                    "origin",
                    "microsoftTeams",
                    "linear",
                    "sentry",
                    "pagerduty",
                    "email",
                ]
                .contains(&source.as_str())
                {
                    bail!("unsupported event source");
                }
                if event.is_empty()
                    || event.len() > 200
                    || filters.len() > 20
                    || filters.iter().any(|(k, v)| k.is_empty() || k.len() > 200 || v.len() > 4000)
                {
                    bail!("invalid event filter");
                }
            }
            Self::Watch { kind, target, every_minutes } => super::watch::validate(*kind, target, *every_minutes)?,
            _ => {}
        }
        Ok(())
    }

    pub fn next(&self, after: i64) -> Result<Option<i64>> {
        Ok(match self {
            Self::Interval { seconds } => Some(
                after
                    .checked_add(seconds.checked_mul(1000).context("interval overflow")?)
                    .context("schedule overflow")?,
            ),
            Self::Once { at } => (*at > after).then_some(*at),
            Self::Cron { expression, time_zone } => {
                let zone = time_zone.parse::<Tz>().context("invalid time zone")?;
                let date = DateTime::<Utc>::from_timestamp_millis(after)
                    .context("invalid schedule timestamp")?
                    .with_timezone(&zone);
                Some(Cron::from_str(expression)?.find_next_occurrence(&date, false)?.timestamp_millis())
            }
            Self::Watch { every_minutes, .. } => Some(
                after
                    .checked_add(every_minutes.checked_mul(60_000).context("watch interval overflow")?)
                    .context("schedule overflow")?,
            ),
            Self::Webhook | Self::Event { .. } => None,
        })
    }

    /// Skip missed intervals without moving the original cadence forward on each tick.
    pub fn advance(&self, due: i64, now: i64) -> Result<Option<i64>> {
        if let Self::Interval { seconds } = self {
            let step = seconds.checked_mul(1000).filter(|v| *v > 0).context("invalid interval")?;
            let elapsed = now.checked_sub(due).context("schedule overflow")?.max(0);
            let count = elapsed.checked_div(step).and_then(|n| n.checked_add(1)).context("schedule overflow")?;
            return Ok(Some(
                due.checked_add(count.checked_mul(step).context("schedule overflow")?).context("schedule overflow")?,
            ));
        }
        self.next(now)
    }

    pub fn description(&self) -> String {
        match self {
            Self::Interval { seconds } if seconds % 86400 == 0 => format!("Every {} days", seconds / 86400),
            Self::Interval { seconds } if seconds % 3600 == 0 => format!("Every {} hours", seconds / 3600),
            Self::Interval { seconds } if seconds % 60 == 0 => format!("Every {} minutes", seconds / 60),
            Self::Interval { seconds } => format!("Every {seconds} seconds"),
            Self::Once { at } => DateTime::from_timestamp_millis(*at)
                .map_or_else(|| "Once".into(), |d| format!("Once at {}", d.to_rfc3339())),
            Self::Cron { expression, time_zone } => format!(
                "{} · {time_zone}",
                Cron::from_str(expression)
                    .map_or_else(|_| expression.clone(), |cron| cron.describe().trim_end_matches('.').to_owned())
            ),
            Self::Webhook => "When a webhook fires".into(),
            Self::Event { source, event, .. } => format!("{source} · {event}"),
            Self::Watch { target, every_minutes, .. } => {
                format!("When {target} changes · checked every {every_minutes} min")
            }
        }
    }

    pub fn matches(&self, payload: &serde_json::Value) -> bool {
        let Self::Event { source, event, filters } = self else { return false };
        payload["source"].as_str() == Some(source)
            && (event == "*" || payload["event"].as_str() == Some(event))
            && filters.iter().all(|(key, value)| {
                if key == "messageContains" {
                    payload["text"].as_str().is_some_and(|text| text.contains(value))
                } else {
                    payload[key].as_str() == Some(value)
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn taipei_sunday_afternoon_uses_correct_utc_time() {
        let t = Trigger::Cron { expression: "20 14 * * 0".into(), time_zone: "Asia/Taipei".into() };
        let now = DateTime::parse_from_rfc3339("2026-09-26T12:00:00Z").unwrap().timestamp_millis();
        let expected = DateTime::parse_from_rfc3339("2026-09-27T06:20:00Z").unwrap().timestamp_millis();
        assert_eq!(t.next(now).unwrap(), Some(expected));
    }
    #[test]
    fn calendar_presets_follow_clock_boundaries() {
        let now = DateTime::parse_from_rfc3339("2026-09-27T01:07:00Z").unwrap().timestamp_millis();
        for (expression, expected) in [
            ("*/15 * * * *", "2026-09-27T01:15:00Z"),
            ("37 * * * *", "2026-09-27T01:37:00Z"),
            ("0 9 * * 1,3,5", "2026-09-28T01:00:00Z"),
            ("0 9 31 * *", "2026-10-31T01:00:00Z"),
        ] {
            let trigger = Trigger::Cron { expression: expression.into(), time_zone: "Asia/Taipei".into() };
            trigger.validate(now).unwrap();
            let expected = DateTime::parse_from_rfc3339(expected).unwrap().timestamp_millis();
            assert_eq!(trigger.next(now).unwrap(), Some(expected), "{expression}");
        }
    }

    #[test]
    fn event_filters_require_all_fields() {
        let t = Trigger::Event {
            source: "github".into(),
            event: "pull_request.opened".into(),
            filters: [("repo".into(), "owner/repo".into())].into(),
        };
        assert!(t.matches(&serde_json::json!({"source":"github","event":"pull_request.opened","repo":"owner/repo"})));
        assert!(!t.matches(&serde_json::json!({"source":"github","event":"pull_request.opened","repo":"other/repo"})));
    }
    #[test]
    fn invalid_schedules_are_rejected() {
        assert!(Trigger::Interval { seconds: 0 }.validate(0).is_err());
        assert!(Trigger::Once { at: 0 }.validate(0).is_err());
        assert!(Trigger::Cron { expression: "* * * * *".into(), time_zone: "not-a-zone".into() }.validate(0).is_err());
    }

    #[test]
    fn an_on_time_run_misses_nothing() {
        let at = 1_790_470_800_000;
        for trigger in [
            Trigger::Interval { seconds: 3600 },
            Trigger::Cron { expression: "0 9 * * *".into(), time_zone: "UTC".into() },
            Trigger::Once { at },
        ] {
            assert!(trigger.missed_window(at, at + GRACE_MS).is_none(), "{trigger:?}");
        }
    }

    #[test]
    fn a_host_that_was_away_counts_the_occurrences_it_let_come_due() {
        // 9:00 UTC daily; the host comes back at 14:05 the same day.
        let due = DateTime::parse_from_rfc3339("2026-09-27T09:00:00Z").unwrap().timestamp_millis();
        let now = DateTime::parse_from_rfc3339("2026-09-27T14:05:00Z").unwrap().timestamp_millis();
        let cron = Trigger::Cron { expression: "0 9 * * *".into(), time_zone: "UTC".into() };
        let window = cron.missed_window(due, now).unwrap();
        assert_eq!(window, MissedWindow { count: 1, last: due, before_last: None });

        // Hourly, host back at 14:05: 9:00..14:00 came due.
        let hourly = Trigger::Cron { expression: "0 * * * *".into(), time_zone: "UTC".into() };
        let window = hourly.missed_window(due, now).unwrap();
        let last = DateTime::parse_from_rfc3339("2026-09-27T14:00:00Z").unwrap().timestamp_millis();
        assert_eq!(window.count, 6);
        assert_eq!(window.last, last);
        assert_eq!(window.before_last, Some(last - 3_600_000));

        // Interval: three hours of missed 1-hour ticks.
        let interval = Trigger::Interval { seconds: 3600 };
        let window = interval.missed_window(due, now).unwrap();
        assert_eq!(window.count, 6);
        assert_eq!(window.last, last);
        assert_eq!(window.before_last, Some(last - 3_600_000));
    }

    #[test]
    fn a_long_away_counts_at_most_the_limit_and_still_names_the_last() {
        // Every 5 minutes for six days: far more than MAX_MISSED occurrences.
        let due = DateTime::parse_from_rfc3339("2026-09-20T00:00:00Z").unwrap().timestamp_millis();
        let now = DateTime::parse_from_rfc3339("2026-09-26T00:00:00Z").unwrap().timestamp_millis();
        let cron = Trigger::Cron { expression: "*/5 * * * *".into(), time_zone: "UTC".into() };
        let window = cron.missed_window(due, now).unwrap();
        assert_eq!(window.count, MAX_MISSED, "the count caps at the limit");
        assert_eq!(window.last, now, "the last occurrence is still the one at `now`");
        assert!(window.before_last.is_some());
        // The record says "999 or more", the run-once record the same minus the one that runs.
        assert_eq!(Trigger::missed_record(&window, false).count, MAX_MISSED);
        assert_eq!(Trigger::missed_record(&window, true).count, MAX_MISSED - 1);
    }

    #[test]
    fn a_one_time_trigger_misses_itself_and_only_itself() {
        let at = 1_790_470_800_000;
        let window = Trigger::Once { at }.missed_window(at, at + 3_600_000).unwrap();
        assert_eq!(window, MissedWindow { count: 1, last: at, before_last: None });
        assert_eq!(Trigger::missed_record(&window, false), Missed { count: 1, until: None });
        assert_eq!(Trigger::missed_record(&window, true), Missed { count: 0, until: None });
    }
}
