//! Source: Hermes `cron/blueprint_catalog.py` at 10d25f8a (MIT).
//! Adaptation: validated drafts for Codync's routine scheduler, without another job engine.

use anyhow::{Context, Result, anyhow, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::LazyLock;

#[derive(Deserialize)]
struct Catalog {
    blueprints: Vec<Blueprint>,
}

#[derive(Deserialize)]
struct Blueprint {
    key: String,
    title: String,
    schedule_template: String,
    prompt_template: String,
    slots: Vec<Slot>,
}

#[derive(Deserialize)]
struct Slot {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    options: Vec<String>,
    #[serde(default = "strict")]
    strict: bool,
    #[serde(default)]
    optional: bool,
}

fn strict() -> bool {
    true
}

static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../market/hermes/blueprints.json")).expect("valid pinned Hermes blueprints")
});

pub fn list() -> Value {
    // The original slot schemas and prompts remain available to all clients and agents.
    serde_json::from_str(include_str!("../market/hermes/blueprints.json")).expect("valid pinned Hermes blueprints")
}

pub fn draft(args: &Value) -> Result<Value> {
    let key = args["template"].as_str().context("template is required")?;
    let blueprint = CATALOG.blueprints.iter().find(|b| b.key == key).context("Unknown routine template")?;
    let supplied = match args.get("values") {
        None | Some(Value::Null) => BTreeMap::new(),
        Some(value) => {
            serde_json::from_value::<BTreeMap<String, String>>(value.clone()).context("values must contain strings")?
        }
    };
    ensure!(supplied.keys().all(|k| blueprint.slots.iter().any(|s| &s.name == k)), "Unknown routine template field");
    let mut values = BTreeMap::new();
    for slot in &blueprint.slots {
        let value = supplied.get(&slot.name).or(slot.default.as_ref());
        let Some(value) = value.filter(|s| !s.trim().is_empty()) else {
            ensure!(slot.optional, "{} is required", slot.name);
            continue;
        };
        ensure!(value.len() <= 4096, "{} is too long", slot.name);
        if slot.kind == "enum" && slot.strict && !slot.options.is_empty() {
            ensure!(slot.options.contains(value), "Invalid value for {}", slot.name);
        }
        values.insert(slot.name.clone(), value.clone());
    }
    let instruction = render(&blueprint.prompt_template, &values)?;
    let mut schedule_values = values.clone();
    if let Some(time) = values.get("time") {
        let (hour, minute) = time.trim().split_once(':').context("time must be HH:MM")?;
        ensure!(hour.len() <= 2 && minute.len() == 2, "time must be HH:MM");
        let hour: u8 = hour.parse().context("Invalid hour")?;
        let minute: u8 = minute.parse().context("Invalid minute")?;
        ensure!(hour < 24 && minute < 60, "Invalid time");
        schedule_values.insert("hour".into(), hour.to_string());
        schedule_values.insert("minute".into(), minute.to_string());
    }
    let dow = values.get("recurrence").or_else(|| values.get("day")).map_or("everyday", String::as_str);
    let dow = match dow.to_ascii_lowercase().as_str() {
        "everyday" => "*",
        "weekdays" => "1-5",
        "weekends" => "0,6",
        "sunday" => "0",
        "monday" => "1",
        "tuesday" => "2",
        "wednesday" => "3",
        "thursday" => "4",
        "friday" => "5",
        "saturday" => "6",
        _ => return Err(anyhow!("Invalid recurrence")),
    };
    schedule_values.insert("dow".into(), dow.into());
    let expression = render(&blueprint.schedule_template, &schedule_values)?;
    let expression = match expression.as_str() {
        "*/60 * * * *" => "0 * * * *".to_owned(),
        "0 */24 * * *" => "0 0 * * *".to_owned(),
        _ => expression,
    };
    croner::Cron::from_str(&expression).context("Invalid template schedule")?;
    let zone = args["timeZone"].as_str().filter(|s| !s.is_empty()).context("timeZone is required")?;
    chrono_tz::Tz::from_str(zone).context("Invalid timeZone")?;
    let deliver = values.get("deliver").map_or("origin", String::as_str);
    ensure!(deliver != "local", "Save-only delivery is not supported; choose origin or a connected platform");
    let outputs = if deliver == "origin" {
        json!([])
    } else {
        let target =
            args["destination"].as_str().filter(|s| !s.trim().is_empty()).context("destination is required")?;
        let output = json!([{"kind": deliver, "target": target, "account": args["account"]}]);
        super::outputs::parse(Some(&output))?;
        output
    };
    Ok(json!({"name": blueprint.title, "instruction": instruction, "enabled": false,
        "triggers": [{"type":"cron", "expression": expression, "timeZone": zone}], "outputs": outputs}))
}

/// Substitute once, so braces in a person's input remain literal and cannot reference other slots.
fn render(template: &str, values: &BTreeMap<String, String>) -> Result<String> {
    let mut rest = template;
    let mut out = String::new();
    while let Some((head, tail)) = rest.split_once('{') {
        out.push_str(head);
        let (key, after) = tail.split_once('}').context("Invalid template placeholder")?;
        out.push_str(values.get(key).with_context(|| format!("Missing template field {key}"))?);
        rest = after;
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hermes_blueprints_all_produce_valid_routine_drafts() {
        assert_eq!(CATALOG.blueprints.len(), 16);
        for blueprint in &CATALOG.blueprints {
            let result = draft(&json!({"template":blueprint.key,"timeZone":"America/Mexico_City"})).unwrap();
            assert_eq!(result["enabled"], false);
            assert_ne!(result["instruction"].as_str().unwrap(), "");
            assert_eq!(result["triggers"][0]["timeZone"], "America/Mexico_City");
        }
    }

    #[test]
    fn hermes_blueprints_refuse_typos_invalid_times_and_undirected_delivery() {
        for values in [
            json!({"tiem":"12:00"}),
            json!({"time":"24:00"}),
            json!({"deliver":"telegram"}),
            json!({"deliver":"local"}),
        ] {
            assert!(draft(&json!({"template":"morning-brief", "timeZone":"UTC", "values":values})).is_err());
        }
        assert!(draft(&json!({"template":"morning-brief", "timeZone":"invalid"})).is_err());
        let result = draft(&json!({"template":"morning-brief","timeZone":"UTC", "values":{"deliver":"telegram"},"destination":"-1001"})).unwrap();
        assert_eq!(result["outputs"][0]["target"], "-1001");
    }

    #[test]
    fn hermes_blueprints_keep_user_braces_literal() {
        let result =
            draft(&json!({"template":"custom-reminder","timeZone":"UTC","values":{"what":"look at {time}"}})).unwrap();
        assert_eq!(result["instruction"], "Remind the user: look at {time}");
    }
}
