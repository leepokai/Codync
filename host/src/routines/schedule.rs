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

/// `at` (Unix milliseconds) on this computer's clock, with its zone: what a bot tells the user.
pub(super) fn local_time(at: i64) -> Option<String> {
    let zone = Tz::from_str(&local_zone()).ok()?;
    Some(DateTime::from_timestamp_millis(at)?.with_timezone(&zone).format("%Y-%m-%d %H:%M (%Z)").to_string())
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
                if !["slack", "github", "origin", "microsoftTeams", "linear", "sentry", "pagerduty", "email"]
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
            Self::Cron { expression, time_zone } => {
                let phrase = calendar_phrase(expression).unwrap_or_else(|| {
                    Cron::from_str(expression)
                        .map_or_else(|_| expression.clone(), |cron| cron.describe().trim_end_matches('.').to_owned())
                });
                format!("{phrase} · {time_zone}")
            }
            Self::Webhook => "When a webhook fires".into(),
            Self::Event { source, event, .. } => format!("{source} · {event}"),
        }
    }

    /// The same schedule `description` spells in English, as stable fields a client can say in
    /// the person's own language. Anything it cannot state safely stays its raw cron expression.
    fn presentation(&self) -> serde_json::Value {
        match self {
            Self::Interval { seconds } => serde_json::json!({"kind": "interval", "seconds": seconds}),
            Self::Once { at } => serde_json::json!({"kind": "once", "at": at}),
            Self::Cron { expression, time_zone } => calendar(expression).map_or_else(
                || serde_json::json!({"kind": "cron", "expression": expression, "timeZone": time_zone}),
                |calendar| {
                    serde_json::json!({
                        "kind": "calendar",
                        "weekdays": calendar.weekdays,
                        "hour": calendar.hour,
                        "minute": calendar.minute,
                        "expression": expression,
                        "timeZone": time_zone,
                    })
                },
            ),
            Self::Webhook => serde_json::json!({"kind": "webhook"}),
            // The event source and name read the same in every language; only Codync's wrapper is translated.
            Self::Event { source, event, .. } => serde_json::json!({"kind": "event", "source": source, "event": event}),
        }
    }

    /// What clients render for `triggers` beside `description`'s English, one item per trigger.
    pub fn presentations(triggers: &[Self]) -> serde_json::Value {
        serde_json::json!({"version": 1, "items": triggers.iter().map(Self::presentation).collect::<Vec<_>>()})
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

const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// A calendar schedule a client can say itself: one time on the named weekdays.
struct Calendar {
    weekdays: Vec<&'static str>,
    hour: u8,
    minute: u8,
}

/// The calendar shapes `description` names in fixed sentences (`Every day at 09:00`,
/// `Every Monday at 09:00`), as fields a client can put in its own language. Shapes with
/// several times a day, or a day of the month, have no such fields and stay raw cron.
fn calendar(expression: &str) -> Option<Calendar> {
    let fields: Vec<&str> = expression.split_whitespace().collect();
    let [minute, hour, "*", "*", weekday] = fields.as_slice() else { return None };
    let minute = minute.parse::<u8>().ok().filter(|minute| *minute <= 59)?;
    let hour = hour.parse::<u8>().ok().filter(|hour| *hour <= 23)?;
    let weekdays = match *weekday {
        "*" => WEEKDAYS.to_vec(),
        "1-5" => WEEKDAYS[1..=5].to_vec(),
        weekday => vec![*WEEKDAYS.get(weekday.parse::<usize>().ok()?)?],
    };
    Some(Calendar { weekdays, hour, minute })
}

/// The editor's calendar styles in fixed sentences the clients' catalogs translate
/// (`Every day at 09:00, 18:00`, `Every Monday at 09:00`). Anything else stays on croner's sentence.
fn calendar_phrase(expression: &str) -> Option<String> {
    let fields: Vec<&str> = expression.split_whitespace().collect();
    let [minute, hour, day, "*", weekday] = fields.as_slice() else { return None };
    let minute = minute.parse::<u8>().ok().filter(|minute| *minute <= 59)?;
    if [*hour, *day, *weekday] == ["*"; 3] {
        return Some(format!("Every hour at minute {minute}"));
    }
    let times = hour
        .split(',')
        .map(|hour| hour.parse::<u8>().ok().filter(|hour| *hour <= 23).map(|hour| format!("{hour:02}:{minute:02}")))
        .collect::<Option<Vec<_>>>()?
        .join(", ");
    match (*day, *weekday) {
        ("*", "*") => Some(format!("Every day at {times}")),
        ("*", "1-5") => Some(format!("Every weekday at {times}")),
        ("*", weekday) => Some(format!("Every {} at {times}", WEEKDAYS.get(weekday.parse::<usize>().ok()?)?)),
        (day, "*") => {
            let day = day.parse::<u8>().ok().filter(|day| (1..=31).contains(day))?;
            Some(format!("Every month on day {day} at {times}"))
        }
        _ => None,
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
    fn editor_styles_read_as_fixed_sentences() {
        let described = |expression: &str| {
            Trigger::Cron { expression: expression.into(), time_zone: "Europe/Rome".into() }.description()
        };
        assert_eq!(described("0 9 * * *"), "Every day at 09:00 · Europe/Rome");
        assert_eq!(described("0 9,18 * * *"), "Every day at 09:00, 18:00 · Europe/Rome");
        assert_eq!(described("0 09 * * *"), "Every day at 09:00 · Europe/Rome");
        assert_eq!(described("47 * * * *"), "Every hour at minute 47 · Europe/Rome");
        assert_eq!(described("30 8 * * 1-5"), "Every weekday at 08:30 · Europe/Rome");
        assert_eq!(described("0 9 * * 1"), "Every Monday at 09:00 · Europe/Rome");
        assert_eq!(described("0 9 * * 0"), "Every Sunday at 09:00 · Europe/Rome");
        assert_eq!(described("15 23 31 * *"), "Every month on day 31 at 23:15 · Europe/Rome");
        for other in ["*/15 * * * *", "0 9 * * 1,3", "0 9 1 6 *", "0 9 1 * 1"] {
            let text = described(other);
            assert!(text.ends_with(" · Europe/Rome") && !text.starts_with("Every day at"), "{text}");
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
    fn presentations_carry_each_schedule_beside_its_english() {
        let triggers = vec![
            Trigger::Interval { seconds: 1800 },
            Trigger::Once { at: 1_800_000_000_000 },
            Trigger::Cron { expression: "20 14 * * 0".into(), time_zone: "Asia/Taipei".into() },
            Trigger::Webhook,
            Trigger::Event {
                source: "github".into(),
                event: "pull_request.opened".into(),
                filters: [("messageContains".into(), "secret".into())].into(),
            },
        ];
        let presented = Trigger::presentations(&triggers);
        assert_eq!(presented["version"], 1);
        let items = presented["items"].as_array().unwrap();
        assert_eq!(items.len(), triggers.len());
        assert_eq!(items[0], serde_json::json!({"kind": "interval", "seconds": 1800}));
        assert_eq!(items[1], serde_json::json!({"kind": "once", "at": 1_800_000_000_000i64}));
        assert_eq!(
            items[2],
            serde_json::json!({"kind": "calendar", "weekdays": ["Sunday"], "hour": 14, "minute": 20,
                "expression": "20 14 * * 0", "timeZone": "Asia/Taipei"})
        );
        assert_eq!(items[3], serde_json::json!({"kind": "webhook"}));
        // The filter may hold private text: clients get the wrapper only, never its value.
        assert_eq!(items[4], serde_json::json!({"kind": "event", "source": "github", "event": "pull_request.opened"}));
        // The English this sits beside is unchanged.
        assert_eq!(triggers[0].description(), "Every 30 minutes");
        assert_eq!(triggers[2].description(), "Every Sunday at 14:20 · Asia/Taipei");
    }

    #[test]
    fn presentations_cover_weekday_ranges_and_daily_schedules() {
        let weekdays = |expression: &str| {
            let trigger = Trigger::Cron { expression: expression.into(), time_zone: "UTC".into() };
            Trigger::presentations(&[trigger])["items"][0]["weekdays"].clone()
        };
        assert_eq!(weekdays("0 9 * * *").as_array().unwrap().len(), 7);
        assert_eq!(
            weekdays("0 9 * * 1-5"),
            serde_json::json!(["Monday", "Tuesday", "Wednesday", "Thursday", "Friday"])
        );
        assert_eq!(weekdays("0 9 * * 3"), serde_json::json!(["Wednesday"]));
    }

    #[test]
    fn presentations_keep_cron_shapes_a_client_cannot_say() {
        for expression in ["0 9,18 * * *", "47 * * * *", "*/15 * * * *", "0 9 31 * *", "0 9 * * 1,3"] {
            let trigger = Trigger::Cron { expression: expression.into(), time_zone: "Europe/Rome".into() };
            let item = &Trigger::presentations(std::slice::from_ref(&trigger))["items"][0];
            assert_eq!(
                item,
                &serde_json::json!({"kind": "cron", "expression": expression, "timeZone": "Europe/Rome"}),
                "{expression}"
            );
        }
    }

    #[test]
    fn invalid_schedules_are_rejected() {
        assert!(Trigger::Interval { seconds: 0 }.validate(0).is_err());
        assert!(Trigger::Once { at: 0 }.validate(0).is_err());
        assert!(Trigger::Cron { expression: "* * * * *".into(), time_zone: "not-a-zone".into() }.validate(0).is_err());
    }
}
