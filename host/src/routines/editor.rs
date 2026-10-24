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
    pub selected_days: BTreeSet<u8>,
    pub month_day: u8,
    pub minute_step: u8,
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
            selected_days: [1, 2, 3, 4, 5].into(),
            month_day: 1,
            minute_step: 15,
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
        if [hour, day, month, weekday].iter().all(|field| **field == "*") {
            self.calendar_style = "hourly".into();
            return;
        }
        let Ok(hour) = hour.parse::<u8>() else { return };
        if hour > 23 || *month != "*" {
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
                self.selected_days = days.into_iter().collect();
            }
        } else if *weekday == "*"
            && let Ok(day) = day.parse::<u8>()
            && (1..=31).contains(&day)
        {
            self.calendar_style = "monthly".into();
            self.month_day = day;
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
            "hourly" => format!("{minute} * * * *"),
            "daily" => format!("{minute} {hour} * * *"),
            "weekdays" => format!("{minute} {hour} * * 1-5"),
            "weekly" => {
                if self.weekday > 6 {
                    bail!("choose a weekday");
                }
                format!("{minute} {hour} * * {}", self.weekday)
            }
            "days" => {
                if self.selected_days.is_empty() || self.selected_days.iter().any(|day| *day > 6) {
                    bail!("choose at least one valid weekday");
                }
                format!(
                    "{minute} {hour} * * {}",
                    self.selected_days.iter().map(u8::to_string).collect::<Vec<_>>().join(",")
                )
            }
            "monthly" => {
                if !(1..=31).contains(&self.month_day) {
                    bail!("choose a day from 1 to 31");
                }
                format!("{minute} {hour} {} * *", self.month_day)
            }
            "custom" => self.expression.split_whitespace().collect::<Vec<_>>().join(" "),
            _ => bail!("choose a schedule frequency"),
        })
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
        for (style, expression) in [
            ("minutes", "*/15 * * * *"),
            ("hourly", "47 * * * *"),
            ("days", "47 23 * * 1,3,5"),
            ("monthly", "47 23 31 * *"),
        ] {
            let result = preview(
                &json!({"draft":{"calendarStyle":style,"hour":23,"minute":47,
                "selectedDays":[5,1,3],"monthDay":31,"zone":"Asia/Taipei"}}),
                NOW,
            )
            .unwrap();
            assert_eq!(result["triggers"][0]["expression"], expression);
            assert!(result["nextRunAt"].as_i64().is_some_and(|next| next > NOW));
        }
    }

    #[test]
    fn editor_round_trips_existing_triggers_without_changing_cadence() {
        for expression in ["*/15 * * * *", "37 * * * *", "5 23 * * 1,3,6", "45 8 31 * *", "*/7 * * * *", "0 9 L * *"] {
            let triggers = json!([{"type":"cron","expression":expression,"timeZone":"Asia/Taipei"}]);
            let result = preview(&json!({"triggers":triggers}), NOW).unwrap();
            assert_eq!(result["triggers"], triggers);
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
