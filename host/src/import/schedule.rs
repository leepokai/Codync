//! An exported routine's schedule as Codync triggers. The shapes are the ones the export skill writes (and
//! the other apps' exports use): hourly, daily, weekdays, weekly, monthly, interval, advanced, custom and once.

use crate::routines::Trigger;
use anyhow::{Context, Result, bail};
use chrono::TimeZone as _;
use chrono_tz::Tz;
use serde_json::Value;

/// The trigger for `schedule` in `zone` (an IANA name), checked from `now` on.
pub fn trigger(schedule: &Value, zone: &str, now: i64) -> Result<Trigger> {
    let int = |key: &str| schedule[key].as_i64().with_context(|| format!("`{key}` is required"));
    let cron = |expression: String| Trigger::Cron { expression, time_zone: zone.to_owned() };
    let trigger = match schedule["kind"].as_str().unwrap_or_default() {
        "hourly" => cron(format!("{} * * * *", ranged(int("minute")?, 0, 59, "minute")?)),
        "daily" => cron(at(schedule, "* * *")?),
        "weekdays" => cron(at(schedule, "* * 1-5")?),
        "weekly" => cron(at(schedule, &format!("* * {}", ranged(int("weekday")?, 0, 6, "weekday")?))?),
        "monthly" => cron(at(schedule, &format!("{} * *", ranged(int("day")?, 1, 31, "day")?))?),
        "interval" => Trigger::Interval { seconds: int("amount")?.checked_mul(unit(schedule)?).context("too long")? },
        "custom" => cron(schedule["expression"].as_str().context("`expression` is required")?.trim().to_owned()),
        "advanced" => cron(advanced(schedule)?),
        "once" => Trigger::Once { at: once(schedule, zone)? },
        other => bail!("unknown schedule `{other}`"),
    };
    trigger.validate(now)?;
    Ok(trigger)
}

/// `minute hour` from the schedule's `time` (`HH:MM`), then the cron fields after them.
fn at(schedule: &Value, rest: &str) -> Result<String> {
    let (hour, minute) = hh_mm(schedule["time"].as_str().unwrap_or_default())?;
    Ok(format!("{minute} {hour} {rest}"))
}

fn hh_mm(time: &str) -> Result<(i64, i64)> {
    let (h, m) = time.split_once(':').context("a time is `HH:MM`")?;
    let hour = ranged(h.trim().parse().context("a time is `HH:MM`")?, 0, 23, "hour")?;
    Ok((hour, ranged(m.trim().parse().context("a time is `HH:MM`")?, 0, 59, "minute")?))
}

fn ranged(value: i64, low: i64, high: i64, what: &str) -> Result<i64> {
    if !(low..=high).contains(&value) {
        bail!("{what} must be {low} to {high}");
    }
    Ok(value)
}

/// Seconds in one of an interval's `unit`s.
fn unit(schedule: &Value) -> Result<i64> {
    Ok(match schedule["unit"].as_str().unwrap_or_default() {
        "minutes" => 60,
        "hours" => 3_600,
        "days" => 86_400,
        other => bail!("unknown interval unit `{other}`"),
    })
}

/// Months, days of the week or month, and a time or a repeat within the day, as one cron expression.
fn advanced(schedule: &Value) -> Result<String> {
    let list = |value: &Value, low: i64, high: i64, what: &str| -> Result<String> {
        let days: Vec<String> = value
            .as_array()
            .into_iter()
            .flatten()
            .map(|d| ranged(d.as_i64().context("a list holds numbers")?, low, high, what).map(|d| d.to_string()))
            .collect::<Result<_>>()?;
        Ok(if days.is_empty() { "*".into() } else { days.join(",") })
    };
    let months = list(&schedule["months"], 1, 12, "month")?;
    let days = &schedule["days"];
    let (day_of_month, day_of_week) = match days["kind"].as_str().unwrap_or_default() {
        "every-day" => ("*".into(), "*".into()),
        "days-of-week" => ("*".into(), list(&days["days"], 0, 6, "weekday")?),
        "days-of-month" => (list(&days["days"], 1, 31, "day")?, "*".into()),
        other => bail!("unknown day choice `{other}`"),
    };
    let time = &schedule["time"];
    let (minute, hour) = match time["kind"].as_str().unwrap_or_default() {
        "at-time" => {
            let (hour, minute) = hh_mm(time["time"].as_str().unwrap_or_default())?;
            (minute.to_string(), hour.to_string())
        }
        "every" => {
            let amount = time["amount"].as_i64().context("`amount` is required")?;
            match time["unit"].as_str().unwrap_or_default() {
                "minutes" => (format!("*/{}", ranged(amount, 1, 59, "minutes")?), "*".into()),
                "hours" => ("0".into(), format!("*/{}", ranged(amount, 1, 23, "hours")?)),
                other => bail!("unknown repeat unit `{other}`"),
            }
        }
        other => bail!("unknown time choice `{other}`"),
    };
    Ok(format!("{minute} {hour} {day_of_month} {months} {day_of_week}"))
}

/// A one-time run's `date` (`YYYY-MM-DD`) and `time` in `zone`, in Unix milliseconds.
fn once(schedule: &Value, zone: &str) -> Result<i64> {
    let date = schedule["date"].as_str().unwrap_or_default();
    let time = schedule["time"].as_str().unwrap_or_default();
    let local = chrono::NaiveDateTime::parse_from_str(&format!("{date} {time}"), "%Y-%m-%d %H:%M")
        .context("a one-time run has a `date` and a `time`")?;
    let tz: Tz = zone.parse().context("invalid IANA time zone")?;
    let at = tz.from_local_datetime(&local).earliest().context("that time doesn't exist in the time zone")?;
    Ok(at.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cron(schedule: &Value) -> String {
        match trigger(schedule, "Europe/Warsaw", 0).expect("valid schedule") {
            Trigger::Cron { expression, time_zone } => {
                assert_eq!(time_zone, "Europe/Warsaw");
                expression
            }
            other => panic!("expected cron, got {other:?}"),
        }
    }

    #[test]
    fn calendar_schedules_become_cron() {
        assert_eq!(cron(&json!({"kind": "hourly", "minute": 15})), "15 * * * *");
        assert_eq!(cron(&json!({"kind": "daily", "time": "09:05"})), "5 9 * * *");
        assert_eq!(cron(&json!({"kind": "weekdays", "time": "08:30"})), "30 8 * * 1-5");
        assert_eq!(cron(&json!({"kind": "weekly", "weekday": 5, "time": "15:00"})), "0 15 * * 5");
        assert_eq!(cron(&json!({"kind": "monthly", "day": 1, "time": "09:00"})), "0 9 1 * *");
        assert_eq!(cron(&json!({"kind": "custom", "expression": "0 9 * * 1-5"})), "0 9 * * 1-5");
        let advanced = json!({"kind": "advanced", "months": [1, 7], "days": {"kind": "days-of-week", "days": [1, 3]},
                              "time": {"kind": "every", "amount": 2, "unit": "hours"}});
        assert_eq!(cron(&advanced), "0 */2 * 1,7 1,3");
    }

    #[test]
    fn intervals_and_one_time_runs() {
        let interval = trigger(&json!({"kind": "interval", "amount": 30, "unit": "minutes"}), "UTC", 0);
        assert_eq!(interval.expect("valid"), Trigger::Interval { seconds: 1_800 });
        let once = trigger(&json!({"kind": "once", "date": "2030-01-02", "time": "03:04"}), "UTC", 0);
        assert_eq!(once.expect("valid"), Trigger::Once { at: 1_893_553_440_000 });
    }

    #[test]
    fn bad_schedules_are_refused() {
        for bad in [
            json!({"kind": "daily", "time": "25:00"}),
            json!({"kind": "weekly", "weekday": 7, "time": "09:00"}),
            json!({"kind": "custom", "expression": "every day"}),
            json!({"kind": "interval", "amount": 3, "unit": "weeks"}),
            json!({"kind": "once", "date": "2001-01-01", "time": "00:00"}),
            json!({"kind": "sometimes"}),
        ] {
            assert!(trigger(&bad, "UTC", 1_700_000_000_000).is_err(), "{bad}");
        }
    }
}
