//! The host owns editor hydration, cron generation, validation and preview.
use super::Trigger;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ScheduleDraft {
    pub kind: String,
    pub amount: String,
    pub unit: i64,
    pub calendar_style: String,
    pub weekday: u8,
    /// The days a `days`-style run takes; a `hourly` run uses it as its weekday field, `None` for every day.
    pub selected_days: Option<BTreeSet<u8>>,
    pub month_day: u8,
    pub year_month: u8,
    pub minute_step: u8,
    /// Hourly: repeat every N hours (`1, 2, 3, 4, 6, 8, 12`); 1 runs on the hour.
    pub every_hours: u8,
    /// Hourly: the window, in minutes from midnight; `None` on either end runs through the day.
    pub window_start: Option<u16>,
    pub window_end: Option<u16>,
    pub hour: u8,
    pub minute: u8,
    pub at: i64,
    pub expression: String,
    pub zone: String,
    pub original: Vec<Trigger>,
}

impl Default for ScheduleDraft {
    fn default() -> Self {
        Self {
            kind: "cron".into(),
            amount: "1".into(),
            unit: 3600,
            calendar_style: "daily".into(),
            weekday: 1,
            selected_days: None,
            month_day: 1,
            year_month: 1,
            minute_step: 15,
            every_hours: 1,
            window_start: None,
            window_end: None,
            hour: 9,
            minute: 0,
            at: 0,
            expression: "0 9 * * *".into(),
            zone: "UTC".into(),
            original: Vec::new(),
        }
    }
}

impl ScheduleDraft {
    fn hydrate(triggers: &[Trigger], zone: String, now: i64) -> Self {
        let mut draft = Self { original: triggers.to_owned(), zone, at: now + 3_600_000, ..Self::default() };
        let [trigger] = triggers else {
            if !triggers.is_empty() {
                draft.kind = "keep".into();
            }
            return draft;
        };
        match trigger {
            Trigger::Interval { seconds } => {
                draft.kind = "interval".into();
                draft.unit = [86400, 3600, 60, 1].into_iter().find(|unit| seconds % unit == 0).unwrap_or(1);
                draft.amount = (seconds / draft.unit).to_string();
            }
            Trigger::Once { at } => {
                draft.kind = "once".into();
                draft.at = *at;
            }
            Trigger::Webhook => draft.kind = "webhook".into(),
            Trigger::Event { .. } => draft.kind = "keep".into(),
            Trigger::Cron { expression, time_zone } => {
                draft.zone.clone_from(time_zone);
                draft.expression.clone_from(expression);
                draft.calendar_style = "custom".into();
                draft.read_cron();
            }
        }
        draft
    }

    /// The style (and its fields) a Custom `expression` is, when the editor has one for it.
    pub(crate) fn read_cron(&mut self) {
        let fields: Vec<_> = self.expression.split_whitespace().collect();
        let [minute, hour, day, month, weekday] = fields.as_slice() else { return };
        if [hour, day, month, weekday].iter().all(|field| **field == "*")
            && let Some(step) = minute.strip_prefix("*/").and_then(|value| value.parse::<u8>().ok())
            && [1, 5, 10, 15, 20, 30].contains(&step)
        {
            self.calendar_style = "minutes".into();
            self.minute_step = step;
            return;
        }
        let Ok(minute) = minute.parse::<u8>() else { return };
        if minute > 59 {
            return;
        }
        self.minute = minute;
        // Hourly: `M * * * *`, `M */N * * DAYS`, or a window `M H1-H2(/N) * * DAYS`. The window's
        // minutes come from its start, so `minute` is kept only for the windowless run.
        if *day == "*"
            && *month == "*"
            && let Some((every, start, end)) = hourly_field(hour, minute)
        {
            self.calendar_style = "hourly".into();
            self.every_hours = every;
            self.window_start = start;
            self.window_end = end;
            self.selected_days = cron_days(weekday);
            return;
        }
        let Ok(hour) = hour.parse::<u8>() else { return };
        if hour > 23 {
            return;
        }
        self.hour = hour;
        if *day == "*" {
            if *weekday == "*" {
                self.calendar_style = "daily".into();
            } else if *weekday == "1-5" {
                self.calendar_style = "weekdays".into();
            } else if let Ok(day) = weekday.parse::<u8>() {
                if day <= 6 {
                    self.calendar_style = "weekly".into();
                    self.weekday = day;
                }
            } else if let Ok(days) = weekday.split(',').map(str::parse::<u8>).collect::<Result<Vec<_>, _>>()
                && !days.is_empty()
                && days.iter().all(|day| *day <= 6)
                && days.windows(2).all(|pair| pair[0] < pair[1])
            {
                self.calendar_style = "days".into();
                self.selected_days = Some(days.into_iter().collect());
            }
        } else if *weekday == "*"
            && let Ok(day) = day.parse::<u8>()
            && let Ok(mo) = month.parse::<u8>()
            && (1..=31).contains(&day)
        {
            self.month_day = day;
            if *month == "*" {
                self.calendar_style = "monthly".into();
            } else if (1..=12).contains(&mo) && day <= days_in_month(mo) {
                self.year_month = mo;
                self.calendar_style = "yearly".into();
            }
        }
    }

    pub fn triggers(&self) -> Result<Vec<Trigger>> {
        Ok(match self.kind.as_str() {
            "keep" => self.original.clone(),
            "interval" => {
                let amount = self.amount.parse::<i64>().context("enter a whole number for the interval")?;
                if amount <= 0 || ![1, 60, 3600, 86400].contains(&self.unit) {
                    bail!("invalid interval");
                }
                vec![Trigger::Interval { seconds: amount.checked_mul(self.unit).context("interval overflow")? }]
            }
            "once" => vec![Trigger::Once { at: self.at }],
            "webhook" => vec![Trigger::Webhook],
            "cron" => vec![Trigger::Cron { expression: self.cron()?, time_zone: self.zone.clone() }],
            _ => bail!("choose when this routine should run"),
        })
    }

    pub(crate) fn cron(&self) -> Result<String> {
        if self.hour > 23 || self.minute > 59 {
            bail!("choose a valid hour and minute");
        }
        let (minute, hour) = (self.minute, self.hour);
        Ok(match self.calendar_style.as_str() {
            "minutes" => {
                if ![1, 5, 10, 15, 20, 30].contains(&self.minute_step) {
                    bail!("choose a minute interval that divides the hour evenly");
                }
                format!("*/{} * * * *", self.minute_step)
            }
            "hourly" => {
                if !EVERY_HOURS.contains(&self.every_hours) {
                    bail!("choose an hour interval");
                }
                let step = if self.every_hours > 1 { format!("/{}", self.every_hours) } else { String::new() };
                match (self.window_start, self.window_end) {
                    // The window's start sets the minute of each run.
                    (Some(start), Some(end)) => {
                        if !(0..=1439).contains(&start) || !(0..=1439).contains(&end) || end < start {
                            bail!("choose a window that fits the day");
                        }
                        let Ok(minute) = u8::try_from(start % 60) else {
                            bail!("choose a window that fits the day");
                        };
                        let Ok(first) = u8::try_from(start / 60) else {
                            bail!("choose a window that fits the day");
                        };
                        // The last run is the last on-time one at or before the end.
                        let Ok(last) = u8::try_from((end - start % 60) / 60) else {
                            bail!("choose a window that fits the day");
                        };
                        format!("{minute} {first}-{last}{step} * * {}", self.days_field()?)
                    }
                    (None, None) => format!("{} *{step} * * {}", self.minute, self.days_field()?),
                    _ => bail!("choose a full window, from its start to its end"),
                }
            }
            "daily" => format!("{minute} {hour} * * *"),
            "weekdays" => format!("{minute} {hour} * * 1-5"),
            "weekly" => {
                if self.weekday > 6 {
                    bail!("choose a weekday");
                }
                format!("{minute} {hour} * * {}", self.weekday)
            }
            "days" => {
                let Some(days) = self.selected_days.as_ref().filter(|days| !days.is_empty()) else {
                    bail!("choose at least one valid weekday");
                };
                if days.iter().any(|day| *day > 6) {
                    bail!("choose valid weekdays");
                }
                format!("{minute} {hour} * * {}", days.iter().map(u8::to_string).collect::<Vec<_>>().join(","))
            }
            "monthly" => {
                if !(1..=31).contains(&self.month_day) {
                    bail!("choose a day from 1 to 31");
                }
                format!("{minute} {hour} {} * *", self.month_day)
            }
            "yearly" => {
                if !(1..=days_in_month(self.year_month)).contains(&self.month_day) {
                    bail!("choose a date that month has");
                }
                format!("{minute} {hour} {} {} *", self.month_day, self.year_month)
            }
            "custom" => self.expression.split_whitespace().collect::<Vec<_>>().join(" "),
            _ => bail!("choose a schedule frequency"),
        })
    }

    /// The cron weekday field: `*` when no day or all seven are chosen, else the days, 0 for Sunday.
    fn days_field(&self) -> Result<String> {
        let Some(days) = self.selected_days.as_ref().filter(|days| !days.is_empty()) else {
            return Ok("*".into());
        };
        if days.iter().any(|day| *day > 6) {
            bail!("choose valid weekdays");
        }
        if (0..7).all(|day| days.contains(&day)) {
            return Ok("*".into());
        }
        Ok(days.iter().map(u8::to_string).collect::<Vec<_>>().join(","))
    }
}

/// The steps an hourly run repeats at, the choices every editor offers.
const EVERY_HOURS: [u8; 7] = [1, 2, 3, 4, 6, 8, 12];

/// An hourly cron's hour field: `*`, `*/N`, `H1-H2` or `H1-H2/N`. A window comes back as the
/// minutes of the day it starts and ends at, on the run's minute.
fn hourly_field(field: &str, minute: u8) -> Option<(u8, Option<u16>, Option<u16>)> {
    let (range, step) = match field.split_once('/') {
        Some((range, step)) => (range, step.parse::<u8>().ok()?),
        None => (field, 1),
    };
    if !EVERY_HOURS.contains(&step) {
        return None;
    }
    let (start, end) = if range == "*" {
        (None, None)
    } else {
        let (first, last) = range.split_once('-')?;
        let first: u8 = first.parse().ok()?;
        let last: u8 = last.parse().ok()?;
        if first > last || last > 23 {
            return None;
        }
        (Some(u16::from(first) * 60 + u16::from(minute)), Some(u16::from(last) * 60 + u16::from(minute)))
    };
    Some((step, start, end))
}

/// The days a cron weekday field names, 0 for Sunday: `*` is all seven; none when the field
/// is not a plain list of days and ranges.
fn cron_days(field: &str) -> Option<BTreeSet<u8>> {
    if field == "*" {
        return Some((0..7).collect());
    }
    let mut days = BTreeSet::new();
    for part in field.split(',') {
        let (first, last) = match part.split_once('-') {
            Some((first, last)) => (first.parse::<u8>().ok()?, last.parse::<u8>().ok()?),
            None => part.parse::<u8>().ok().map(|day| (day, day))?,
        };
        if first > last || last > 7 {
            return None;
        }
        for day in first..=last {
            days.insert(day % 7);
        }
    }
    Some(days)
}

/// The days a month has, counting the leap year, so February 29 is a valid yearly date.
pub(crate) const fn days_in_month(month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => 29,
        _ => 0,
    }
}

pub fn preview(body: &Value, now: i64) -> Result<Value> {
    let draft = if let Some(value) = body.get("draft") {
        serde_json::from_value::<ScheduleDraft>(value.clone()).context("invalid schedule fields")?
    } else {
        let triggers = body.get("triggers").cloned().unwrap_or_else(|| json!([]));
        ScheduleDraft::hydrate(
            &serde_json::from_value::<Vec<Trigger>>(triggers)?,
            body["timeZone"].as_str().unwrap_or("UTC").into(),
            now,
        )
    };
    let triggers = draft.triggers()?;
    if triggers.is_empty() || triggers.len() > 20 {
        bail!("a routine requires 1 to 20 triggers");
    }
    for trigger in &triggers {
        // Existing expired one-shots may be edited without changing their date.
        if matches!(trigger, Trigger::Once { .. }) && draft.original.contains(trigger) {
            continue;
        }
        trigger.validate(now)?;
    }
    let next =
        triggers.iter().map(|trigger| trigger.next(now)).collect::<Result<Vec<_>>>()?.into_iter().flatten().min();
    let warning = (draft.kind == "cron" && draft.calendar_style == "monthly" && draft.month_day > 28)
        .then(|| format!("Months without day {} are skipped.", draft.month_day));
    Ok(
        json!({"draft":draft,"triggers":triggers,"summary":triggers.iter().map(Trigger::description).collect::<Vec<_>>().join(" · "),
        "nextRunAt":next,"warning":warning}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    const NOW: i64 = 1_790_470_800_000;

    #[test]
    fn defaults_and_presets_are_compiled_and_validated_on_the_host() {
        let result = preview(&json!({"timeZone":"Asia/Taipei"}), NOW).unwrap();
        assert_eq!(result["triggers"][0]["type"], "cron");
        assert_eq!(result["triggers"][0]["expression"], "0 9 * * *");
        for (style, draft, expression) in [
            ("minutes", json!({"minuteStep":15}), "*/15 * * * *"),
            ("hourly", json!({"minute":47,"selectedDays":[]}), "47 * * * *"),
            ("hourly", json!({"minute":47,"everyHours":4}), "47 */4 * * *"),
            ("hourly", json!({"windowStart":540,"windowEnd":1080}), "0 9-18 * * *"),
            ("hourly", json!({"windowStart":570,"windowEnd":1110,"everyHours":2}), "30 9-18/2 * * *"),
            (
                "hourly",
                json!({"windowStart":570,"windowEnd":1050,"everyHours":2,"selectedDays":[5,1,3]}),
                "30 9-17/2 * * 1,3,5",
            ),
            ("days", json!({"hour":23,"minute":47,"selectedDays":[5,1,3]}), "47 23 * * 1,3,5"),
            ("monthly", json!({"hour":23,"minute":47,"monthDay":31}), "47 23 31 * *"),
            ("yearly", json!({"hour":23,"minute":47,"yearMonth":2,"monthDay":29}), "47 23 29 2 *"),
        ] {
            let mut body = json!({"draft":{"calendarStyle":style,"zone":"Asia/Taipei"}});
            for (key, value) in draft.as_object().unwrap() {
                body["draft"][key] = value.clone();
            }
            let result = preview(&body, NOW).unwrap();
            assert_eq!(result["triggers"][0]["expression"], expression, "{style}");
            assert!(result["nextRunAt"].as_i64().is_some_and(|next| next > NOW), "{style}");
        }
    }

    #[test]
    fn editor_round_trips_existing_triggers_without_changing_cadence() {
        for expression in [
            "*/15 * * * *",
            "37 * * * *",
            "5 23 * * 1,3,6",
            "45 8 31 * *",
            "*/7 * * * *",
            "0 9 L * *",
            "0 9-18 * * *",
            "30 */4 * * *",
            "15 9-17/2 * * 1,3,5",
            "0 */12 * * 0,6",
            "40 6-23 * * *",
            "30 8 29 2 *",
            "0 9 31 12 *",
        ] {
            let triggers = json!([{"type":"cron","expression":expression,"timeZone":"Asia/Taipei"}]);
            let result = preview(&json!({"triggers":triggers}), NOW).unwrap();
            assert_eq!(result["triggers"], triggers, "{expression}");
        }
        for triggers in [
            json!([{"type":"interval","seconds":5400}]),
            json!([{"type":"once","at":1000}]),
            json!([{"type":"webhook"},{"type":"interval","seconds":90}]),
        ] {
            assert_eq!(preview(&json!({"triggers":triggers}), NOW).unwrap()["triggers"], triggers);
        }
    }

    #[test]
    fn the_hourly_window_and_yearly_date_are_validated() {
        let draft = |style: &str, extra: Value| {
            let mut body = json!({"draft":{"calendarStyle":style,"zone":"Asia/Taipei"}});
            for (key, value) in extra.as_object().unwrap() {
                body["draft"][key] = value.clone();
            }
            body
        };
        for (style, extra) in [
            ("hourly", json!({"everyHours":5})),
            ("hourly", json!({"windowStart":600,"windowEnd":540})),
            ("hourly", json!({"windowStart":540})),
            ("yearly", json!({"yearMonth":2,"monthDay":30})),
            ("yearly", json!({"yearMonth":13,"monthDay":1})),
        ] {
            assert!(preview(&draft(style, extra.clone()), NOW).is_err(), "{style} {extra}");
        }
        // The window's end at the run's own minute still fits that run (09:00-10:00 runs at 09:00).
        let result = preview(&draft("hourly", json!({"windowStart":540,"windowEnd":600})), NOW).unwrap();
        assert_eq!(result["triggers"][0]["expression"], "0 9-10 * * *");
    }

    #[test]
    fn preview_rejects_invalid_schedules_before_saving() {
        for draft in [
            json!({"calendarStyle":"days","selectedDays":[]}),
            json!({"calendarStyle":"minutes","minuteStep":7}),
            json!({"zone":"invalid/zone"}),
            json!({"hour":24}),
            json!({"calendarStyle":"custom","expression":"61 9 * * *"}),
            json!({"calendarStyle":"custom","expression":"0 0 9 * * *"}),
            json!({"kind":"once","at":1}),
        ] {
            assert!(preview(&json!({"draft":draft}), NOW).is_err());
        }
    }
}
