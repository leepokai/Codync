//! Bounded, opt-in turns in a bot's own conversation.
use crate::{LockExt, hub::Hub, store::now_ms};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Mutex;

// Serialize read/modify/write across API calls and actors. Emit only after unlocking.
static WRITE: Mutex<()> = Mutex::new(());
pub const DEFAULT_TURNS: u32 = 20;
pub const DEFAULT_MINUTES: u32 = 60;
pub const MAX_TURNS: u32 = 200;
pub const MAX_MINUTES: u32 = 1440;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Goal {
    pub id: String,
    pub text: String,
    pub max_turns: u32,
    pub max_minutes: u32,
    pub started_at: i64,
    pub turns: u32,
    pub stopped: Option<Stopped>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stopped {
    Done,
    NeedsUser,
    Failed,
    User,
    Limit,
    Usage,
    #[serde(rename = "stopped")]
    Manual,
}

fn key(bot: &str) -> String {
    format!("goal.{bot}")
}

pub fn get(hub: &Hub, bot: &str) -> Option<Goal> {
    match read(hub, bot) {
        Ok(goal) => goal,
        Err(error) => {
            tracing::error!(bot, %error, "couldn't read the goal");
            None
        }
    }
}

fn read(hub: &Hub, bot: &str) -> Result<Option<Goal>> {
    hub.store.kv_read(&key(bot))?.map(|s| serde_json::from_str(&s).map_err(Into::into)).transpose()
}

fn save(hub: &Hub, bot: &str, goal: &Goal) -> Result<()> {
    hub.store.kv_set(&key(bot), &serde_json::to_string(goal)?)
}

/// Setting a goal authorizes its first step. Empty text clears it.
pub fn set(hub: &Hub, bot: &str, body: &Value) -> Result<Value> {
    let row = hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow::anyhow!("unknown bot"))?;
    if row.config.is_group() {
        bail!("goals are only available in a bot's own conversation");
    }
    let text = body["goal"].as_str().ok_or_else(|| anyhow::anyhow!("goal is required"))?.trim();
    if text.chars().count() > 2000 {
        bail!("goal must be at most 2000 characters");
    }
    let lock = WRITE.locked();
    if text.is_empty() {
        hub.store.kv_delete(&key(bot))?;
        drop(lock);
        hub.emit_bot(bot);
        return Ok(json!({"goal": null}));
    }
    let goal = Goal {
        id: uuid::Uuid::new_v4().to_string(),
        text: text.to_owned(),
        max_turns: limit(body, "maxTurns", DEFAULT_TURNS, MAX_TURNS)?,
        max_minutes: limit(body, "maxMinutes", DEFAULT_MINUTES, MAX_MINUTES)?,
        started_at: now_ms(),
        turns: 0,
        stopped: None,
    };
    save(hub, bot, &goal)?;
    drop(lock);
    hub.emit_bot(bot);
    if let Err(error) = hub.send_cmd(bot, crate::agent::bot::Cmd::Goal(goal.id.clone())) {
        finish(hub, bot, Some(&goal.id), Stopped::Failed);
        return Err(error);
    }
    crate::analytics::capture(hub, crate::analytics::Event::GoalStarted, serde_json::Map::new());
    Ok(json!({"goal": goal}))
}

fn limit(body: &Value, field: &str, default: u32, maximum: u32) -> Result<u32> {
    if body.get(field).is_none() {
        return Ok(default);
    }
    let n = body[field].as_u64().filter(|n| (1..=u64::from(maximum)).contains(n));
    u32::try_from(n.ok_or_else(|| anyhow::anyhow!("{field} must be between 1 and {maximum}"))?).map_err(Into::into)
}

pub fn stop(hub: &Hub, bot: &str) -> Result<Value> {
    try_finish(hub, bot, None, Stopped::Manual)?;
    Ok(json!({}))
}

pub fn user_spoke(hub: &Hub, bot: &str) -> Result<()> {
    try_finish(hub, bot, None, Stopped::User)
}

/// An older turn must never end a newly set goal.
pub fn finish(hub: &Hub, bot: &str, id: Option<&str>, why: Stopped) {
    if let Err(error) = try_finish(hub, bot, id, why) {
        tracing::error!(bot, %error, "couldn't save the goal's end");
    }
}

fn try_finish(hub: &Hub, bot: &str, id: Option<&str>, why: Stopped) -> Result<()> {
    let lock = WRITE.locked();
    let Some(mut goal) = read(hub, bot)?.filter(|g| g.stopped.is_none() && id.is_none_or(|id| id == g.id)) else {
        return Ok(());
    };
    goal.stopped = Some(why);
    save(hub, bot, &goal)?;
    drop(lock);
    hub.emit_bot(bot);
    crate::analytics::capture(
        hub,
        crate::analytics::Event::GoalStopped,
        serde_json::Map::from_iter([("reason".into(), json!(why))]),
    );
    let data = json!({"text": format!("Goal stopped: {}.", reason(why)), "style": "info", "goalEnded": why});
    hub.add_entry(&crate::store::Lane::main(bot), crate::store::EntryKind::Notice, hub.store.max_turn(bot), &data);
    Ok(())
}

fn reason(why: Stopped) -> &'static str {
    match why {
        Stopped::Done => "the goal was met",
        Stopped::NeedsUser => "the bot needs you",
        Stopped::Failed => "the bot failed",
        Stopped::User => "you wrote",
        Stopped::Limit => "the turn or time limit was reached",
        Stopped::Usage => "the agent's plan is near its usage limit",
        Stopped::Manual => "you stopped it",
    }
}

/// Check at dequeue time: stale steps cost no turns and never start an agent.
pub fn begin(hub: &Hub, bot: &str, id: &str, backend: &str) -> Result<Option<String>> {
    let lock = WRITE.locked();
    let Some(mut goal) = read(hub, bot)?.filter(|g| g.id == id && g.stopped.is_none()) else { return Ok(None) };
    let why = if goal.turns >= goal.max_turns
        || now_ms().saturating_sub(goal.started_at) >= i64::from(goal.max_minutes) * 60_000
    {
        Some(Stopped::Limit)
    } else if crate::usage::near_limit(hub, backend) {
        Some(Stopped::Usage)
    } else {
        None
    };
    if let Some(why) = why {
        drop(lock);
        try_finish(hub, bot, Some(id), why)?;
        return Ok(None);
    }
    goal.turns += 1;
    save(hub, bot, &goal)?;
    drop(lock);
    hub.emit_bot(bot);
    let text = format!(
        "[Codync: goal step {}/{}] Continue with the next step towards this goal: {}\n\nWhen the goal is complete, end with a line containing exactly GOAL_COMPLETE. When you need the user, end with NEEDS_USER. Do not use these markers until the corresponding condition is true. The user's next message stops automatic continuation.",
        goal.turns, goal.max_turns, goal.text,
    );
    let data = json!({"text": format!("Continuing with the goal ({}/{})", goal.turns, goal.max_turns), "style": "info", "goalStep": goal.turns.to_string()});
    hub.add_entry(&crate::store::Lane::main(bot), crate::store::EntryKind::Notice, hub.store.max_turn(bot), &data);
    Ok(Some(text))
}

/// Only an affirmative closing line completes a goal; negated/quoted prose does not.
pub fn done(reply: &str) -> bool {
    matches!(
        last_line(reply).to_lowercase().as_str(),
        "goal_complete" | "goal met" | "goal complete" | "目标已完成" | "目標已完成" | "目标已达成" | "目標已達成"
    )
}

fn last_line(reply: &str) -> &str {
    reply.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or_default().trim()
}

pub fn asks_user(reply: &str) -> bool {
    let last = last_line(reply).to_lowercase();
    last == "needs_user"
        || last.ends_with(['?', '？'])
        || [
            "let me know",
            "please confirm",
            "needs your",
            "you need to",
            "请确认",
            "请提供",
            "等待你",
            "需要你",
            "需要您",
        ]
        .iter()
        .any(|m| last.contains(m))
}

#[cfg(test)]
mod tests;
