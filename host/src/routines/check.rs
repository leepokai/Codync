//! A routine body as `save` takes it, checked before anything is saved; `chat::manage` checks one this way before
//! its approval card goes up.

use super::{Routine, Routines, ScheduleDraft, State, Trigger, required};
use crate::{LockExt, store::now_ms};
use anyhow::{Context, Result, bail};
use serde_json::Value;

/// A routine body's name, instruction, triggers and timeout, checked.
pub struct Checked {
    pub name: String,
    pub instruction: String,
    pub triggers: Vec<Trigger>,
    pub timeout: Option<u64>,
}

impl Checked {
    pub(super) fn parse(body: &Value) -> Result<Self> {
        let name = required(body, "name")?.trim();
        let instruction = required(body, "instruction")?.trim();
        if name.is_empty() || name.len() > 200 || instruction.is_empty() || instruction.len() > 32_000 {
            bail!("routine name or instruction is empty or too long");
        }
        let triggers: Vec<Trigger> = if let Some(draft) = body.get("schedule") {
            serde_json::from_value::<ScheduleDraft>(draft.clone()).context("invalid schedule")?.triggers()?
        } else {
            serde_json::from_value(body["triggers"].clone()).context("invalid triggers")?
        };
        if triggers.is_empty() || triggers.len() > 20 {
            bail!("a routine requires 1 to 20 triggers");
        }
        let timeout = body
            .get("timeoutSeconds")
            .map(|value| {
                value
                    .as_u64()
                    .or_else(|| value.as_str()?.parse::<u64>().ok())
                    .filter(|seconds| (1..=86_400).contains(seconds))
                    .context("timeoutSeconds must be an integer between 1 and 86400")
            })
            .transpose()?;
        Ok(Self { name: name.into(), instruction: instruction.into(), triggers, timeout })
    }
}

/// The routine of `bot`'s that `body`'s `id` names, if it names one; `triggers` must hold from `now` on, except a
/// one-time trigger that routine already has.
pub(super) fn replaced(s: &State, bot: &str, body: &Value, triggers: &[Trigger], now: i64) -> Result<Option<Routine>> {
    let old = match body["id"].as_str() {
        Some(id) => Some(
            s.routines
                .iter()
                .find(|r| r.id == id && r.bot_id == bot && !r.deleted)
                .cloned()
                .context("routine not found")?,
        ),
        None => None,
    };
    for trigger in triggers {
        if matches!(trigger, Trigger::Once { .. }) && old.as_ref().is_some_and(|r| r.triggers.contains(trigger)) {
            continue;
        }
        trigger.validate(now)?;
    }
    Ok(old)
}

impl Routines {
    /// Checks `body` the way `save` does for `bot`, saving nothing; a bot not created yet has no routine to replace.
    pub fn check(&self, bot: &str, body: &Value) -> Result<Checked> {
        let checked = Checked::parse(body)?;
        replaced(&self.state.locked(), bot, body, &checked.triggers, now_ms())?;
        Ok(checked)
    }
}
