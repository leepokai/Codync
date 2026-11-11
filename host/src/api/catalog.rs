//! The ready-made bot and team catalog (`market::catalog`): listing it and creating a whole team.

use super::str_arg;
use crate::hub::Hub;
use crate::market::catalog::{self, TeamProfile};
use crate::market::openbot;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::ops::ControlFlow;
use std::sync::Arc;

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;

pub(super) async fn call(hub: &Arc<Hub>, method: &str, b: Value) -> Result<ControlFlow<Value, Value>> {
    Ok(ControlFlow::Break(match method {
        "botCatalog" => json!(catalog::catalog()),
        "installTeam" => {
            let team = catalog::team(str_arg(&b, "id")?)?;
            install_team(hub, team, b["backend"].as_str(), b["model"].as_str()).await?
        }
        "marketAgents" => {
            let installed = installed_agents(hub)?;
            json!({
                "items": openbot::browse_agents(
                    &installed,
                    b["search"].as_str().unwrap_or_default(),
                    b["category"].as_str().unwrap_or_default(),
                )
            })
        }
        "marketAgent" => {
            let id = str_arg(&b, "id")?;
            openbot::agent_details(id, installed_agents(hub)?.iter().any(|a| a == id))?
        }
        "installAgent" => install_agent(hub, str_arg(&b, "id")?, b["backend"].as_str(), b["model"].as_str()).await?,
        "marketSkills" => {
            let installed = installed_skills(hub);
            let mut items = openbot::browse_skills(&installed);
            items.extend(crate::market::hermes::browse_skills(&installed));
            json!({"items": items})
        }
        "marketApps" => {
            let installed = installed_apps(hub)?;
            let search = b["search"].as_str().unwrap_or_default();
            let mut items = openbot::browse_apps(&installed, search);
            items.extend(crate::native_channels::apps::browse(&hub.store, search)?);
            items.extend(crate::market::hermes_apps::browse(
                &hub.store,
                &crate::market::connectors(&hub.store)?,
                search,
                hub.port,
            ));
            json!({"items": items})
        }
        _ => return Ok(ControlFlow::Continue(b)),
    }))
}

/// The marketplace agents this computer already has.
fn installed_agents(hub: &Arc<Hub>) -> Result<Vec<String>> {
    Ok(hub.store.bots()?.into_iter().filter(|b| !b.deleted).filter_map(|b| b.config.catalog).collect())
}

/// The marketplace apps a connector on this computer came from.
fn installed_skills(hub: &Arc<Hub>) -> Vec<String> {
    let items = crate::market::list_skills(&hub.store);
    items["items"].as_array().into_iter().flatten().filter_map(|s| s["source"].as_str().map(str::to_owned)).collect()
}

fn installed_apps(hub: &Arc<Hub>) -> Result<Vec<String>> {
    Ok(crate::market::connectors(&hub.store)?.into_iter().filter_map(|c| c.catalog).collect())
}

/// Creates the bot with the agent's instructions, look and skills, then its routines. All or nothing:
/// a failed routine removes the bot this call created.
async fn install_agent(hub: &Arc<Hub>, id: &str, backend: Option<&str>, model: Option<&str>) -> Result<Value> {
    let agent = openbot::agent(id)?;
    let mut skills = Vec::new();
    for skill in &agent.skills {
        skills.extend(openbot::ensure(&hub.store, skill)?);
    }
    let body = json!({
        "name": agent.name,
        "description": agent.instructions,
        "avatarColor": agent.avatar_color,
        "avatarShape": agent.avatar_shape,
        "backend": backend.unwrap_or_default(),
        "model": model,
        "skills": skills,
        "catalog": agent.id,
    });
    let bot = create(hub, body).await?;
    let bot_id = bot["id"].as_str().context("created bot has an id")?.to_owned();
    for routine in &agent.routines {
        let body = json!({
            "name": routine.name,
            "instruction": routine.instruction,
            "enabled": routine.active,
            "triggers": routine_triggers(&routine.schedule)?,
        });
        if let Err(e) = hub.routines.save(hub, &bot_id, &body) {
            let hub = hub.clone();
            let removed = tokio::task::spawn_blocking(move || hub.delete_bot(&bot_id)).await;
            if let Err(e) = removed.map_err(anyhow::Error::from).and_then(|r| r) {
                tracing::warn!(
                    agent = id,
                    error = format!("{e:#}"),
                    "couldn't remove a bot from a failed agent install"
                );
            }
            return Err(e);
        }
    }
    Ok(json!({"bot": bot}))
}

/// A catalog routine's calendar schedule as the routine's cron trigger, on this computer's clock.
fn routine_triggers(schedule: &Value) -> Result<Value> {
    let kind = schedule["kind"].as_str().unwrap_or_default();
    let time = schedule["time"].as_str().filter(|_| kind != "interval").unwrap_or("09:00");
    let (hour, minute) = time.split_once(':').context("invalid schedule time")?;
    let (hour, minute): (u32, u32) = (hour.parse()?, minute.parse()?);
    let expression = match kind {
        "weekdays" => format!("{minute} {hour} * * 1-5"),
        "weekly" => format!("{minute} {hour} * * {}", schedule["weekday"].as_u64().unwrap_or(1)),
        "monthly" => format!("{minute} {hour} {} * *", schedule["day"].as_u64().unwrap_or(1)),
        "daily" => format!("{minute} {hour} * * *"),
        other => bail!("the agent has an unknown schedule: {other}"),
    };
    Ok(json!([{"type": "cron", "expression": expression, "timeZone": crate::routines::local_zone()}]))
}

/// Creates the coordinator and its teammates, then their group led by the coordinator.
/// All or nothing: a failure removes the bots this call already created.
async fn install_team(
    hub: &Arc<Hub>,
    team: &'static TeamProfile,
    backend: Option<&str>,
    model: Option<&str>,
) -> Result<Value> {
    let mut created = Vec::new();
    let result = create_team(hub, team, backend, model, &mut created).await;
    if result.is_err() {
        for id in created {
            let hub = hub.clone();
            let removed = tokio::task::spawn_blocking(move || hub.delete_bot(&id)).await;
            if let Err(e) = removed.map_err(anyhow::Error::from).and_then(|r| r) {
                tracing::warn!(
                    team = team.id,
                    error = format!("{e:#}"),
                    "couldn't remove a bot from a failed team install"
                );
            }
        }
    }
    result
}

async fn create_team(
    hub: &Arc<Hub>,
    team: &'static TeamProfile,
    backend: Option<&str>,
    model: Option<&str>,
    created: &mut Vec<String>,
) -> Result<Value> {
    for profile in team.profiles()? {
        let bot = create(hub, profile.draft(backend, model)).await?;
        created.push(bot["id"].as_str().context("created bot has an id")?.to_owned());
    }
    let group = json!({
        "kind": "group",
        "name": team.name,
        "description": team.purpose,
        "members": created,
        "chiefBotId": created.first(),
    });
    let group = create(hub, group).await?;
    Ok(json!({"bot": group}))
}

async fn create(hub: &Arc<Hub>, body: Value) -> Result<Value> {
    let cfg = crate::agent::new_bot::with_defaults(hub, body).await?;
    let hub = hub.clone();
    tokio::task::spawn_blocking(move || hub.create_bot(cfg)).await?
}
