//! Bind memory writes to the actual ACP session, including independent reply threads.

use super::{engram, manage::payload};
use crate::{
    hub::Hub,
    store::{Lane, lane_key},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::sync::Arc;

fn key(bot: &str, session: &str) -> String {
    format!("memory.session.{bot}.{session}")
}

pub async fn begin(hub: &Arc<Hub>, bot: &str, session: &str) -> Result<String> {
    let binding = key(bot, session);
    let mut effective = hub.store.kv_get(&binding).filter(|v| !v.is_empty()).unwrap_or_else(|| session.to_owned());
    for attempt in 0..2 {
        let result =
            engram::call(bot, "mem_session_start", &json!({"id":effective, "directory":engram::directory(bot)?}))
                .await?;
        if result["isError"] != true {
            hub.store.kv_set(&binding, &effective)?;
            return Ok(effective);
        }
        if attempt == 0 && payload(&result)["error_code"] == "session_already_ended" {
            effective = format!("{session}:resume:{}", uuid::Uuid::new_v4());
        } else {
            anyhow::bail!("Engram session registration failed: {}", result["content"]);
        }
    }
    anyhow::bail!("could not register memory session")
}

pub async fn bound_args(hub: &Arc<Hub>, bot: &str, lane: Option<&str>, name: &str, args: &Value) -> Result<Value> {
    let row = hub.store.bot(bot)?.filter(|b| !b.deleted).context("unknown bot")?;
    let sid = match lane.filter(|s| !s.is_empty()) {
        Some(root) => hub.store.kv_get(&lane_key("session", bot, &Lane::in_thread(bot, root))),
        None => row.session_id,
    }
    .filter(|s| !s.is_empty());
    let mut args = args.clone();
    ensure!(args.is_object(), "memory arguments must be an object");
    // Search/management still works without a running agent (e.g. the settings screen).
    if let Some(sid) = sid {
        let effective = begin(hub, bot, &sid).await?;
        if matches!(name, "mem_save" | "mem_save_prompt" | "mem_session_summary" | "mem_capture_passive") {
            args["session_id"] = effective.clone().into();
        }
        if matches!(name, "mem_session_start" | "mem_session_end") {
            args["id"] = effective.into();
        }
    }
    if name == "mem_session_start" {
        args["directory"] = json!(engram::directory(bot)?);
    }
    if args.get("project").is_some() {
        args["project"] = engram::project(bot).into();
    }
    Ok(args)
}

pub async fn capture_prompt(bot: &str, session: &str, text: &str) -> Result<()> {
    let result = engram::call(
        bot,
        "mem_save_prompt",
        &json!({"session_id":session, "content":text, "project":engram::project(bot)}),
    )
    .await?;
    ensure!(result["isError"] != true, "Engram prompt capture failed: {}", result["content"]);
    Ok(())
}

/// An explicit edit/forget must reach non-Claude harnesses too, even mid-session.
pub fn change_notice(hub: &Hub, bot: &str, session: &str) -> Option<String> {
    let revision = super::maintenance::revision(&hub.store, bot);
    let key = format!("memory.announced.{bot}.{session}");
    if hub.store.kv_get(&key).unwrap_or_default() == revision {
        return None;
    }
    Some("<memory_update>Saved memories have changed. Earlier memory excerpts may be outdated or deleted. Call mem_context and mem_search before relying on them; respect removals and corrections.</memory_update>".into())
}

/// A notice is acknowledged only after the prompt was delivered to the harness.
pub fn mark_announced(hub: &Hub, bot: &str, session: &str, revision: &str) -> Result<()> {
    hub.store.kv_set(&format!("memory.announced.{bot}.{session}"), revision)
}
