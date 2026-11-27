//! Local group tools for the `manage` MCP server: list, create and update this computer's groups.
//! The bot-facing tools and dispatch stay in `manage.rs`; this module holds the group rules.

use super::{
    APPROVAL_TIMEOUT, Arc, BTreeSet, BotConfig, Context, Hub, Instant, Map, Result, Value, anyhow, approve, bail,
    checked_instructions, checked_name, json, notice, text,
};

/// A group's setup: what a manager can learn about it, never its chat.
pub(crate) fn group_list(hub: &Hub) -> Result<Vec<Value>> {
    hub.store
        .bots()?
        .into_iter()
        .filter(|row| !row.deleted && row.config.is_group())
        .map(|row| {
            let members: Vec<Value> = row
                .config
                .members
                .iter()
                .map(|id| {
                    // A missing member keeps a placeholder; a storage failure is real and must not be hidden.
                    let name = match hub.store.bot(id)? {
                        Some(row) => row.config.name,
                        None => "(unknown)".to_owned(),
                    };
                    Ok(json!({"id": id, "name": name}))
                })
                .collect::<Result<_>>()?;
            Ok(json!({
                "id": row.config.id, "name": row.config.name, "topic": row.config.description,
                "members": members, "createdBy": row.config.created_by,
            }))
        })
        .collect::<Result<_>>()
}

pub(crate) async fn create_group(hub: &Arc<Hub>, source: &BotConfig, args: &Value) -> Result<Value> {
    let deadline = Instant::now() + APPROVAL_TIMEOUT;
    let name = text(args, "name")?.context("name is required")?;
    let name = checked_name(&name)?;
    let topic = group_topic(args)?;
    let members = group_members(hub, "", &member_ids(args, "members")?)?;
    // Leave `description` out when there is no topic: a group's description is a plain string, and
    // null would fail deserialization (`create_group` with just name + members must work).
    let mut body = json!({"kind": "group", "name": name, "members": members, "createdBy": source.id});
    if let Some(topic) = topic {
        body["description"] = json!(topic);
    }
    let cfg = crate::agent::new_bot::with_defaults(hub, body).await?;
    crate::hub::validate(&cfg)?;
    let mut lines = vec![format!("Name: {name}")];
    if !cfg.description.is_empty() {
        lines.push(format!("Topic: {}", cfg.description));
    }
    lines.push(format!("Members: {}", member_names(hub, &cfg.members)));
    approve(hub, source, format!("Create group {name}"), lines.join("\n"), deadline).await?;
    let created = {
        let hub = hub.clone();
        tokio::task::spawn_blocking(move || hub.create_bot(cfg)).await??
    };
    let id = created["id"].as_str().context("the new group has no id")?;
    // Same members reopen an existing group: report the name it actually has, not the requested one.
    let created_name = created["name"].as_str().unwrap_or(name);
    notice(hub, &source.id, &format!("Created group {created_name}"));
    Ok(json!({"group": group_detail(hub, id)?}))
}

pub(crate) async fn update_group(hub: &Arc<Hub>, source: &BotConfig, args: &Value) -> Result<Value> {
    let deadline = Instant::now() + APPROVAL_TIMEOUT;
    let id = text(args, "groupId")?.context("groupId is required")?;
    let (target, expected_rev) = group_bot(hub, &id)?;
    let mut patch = Map::new();
    let mut changed = Vec::new();
    if let Some(name) = text(args, "name")? {
        patch.insert("name".into(), checked_name(&name)?.into());
        changed.push("name");
    }
    if let Some(topic) = group_topic(args)? {
        patch.insert("description".into(), topic.into());
        changed.push("topic");
    }
    let set_members = !args["members"].is_null();
    let adding = !args["addMembers"].is_null();
    let removing = !args["removeMembers"].is_null();
    if set_members && (adding || removing) {
        bail!("use members to replace the list, or addMembers and removeMembers to adjust it, not both");
    }
    let mut members = target.members.clone();
    if set_members {
        members = member_ids(args, "members")?;
    }
    if adding {
        let add = member_ids(args, "addMembers")?;
        for m in &add {
            if members.iter().any(|old| old == m) {
                bail!("{m} is already in the group");
            }
        }
        members.extend(add);
    }
    if removing {
        let remove = member_ids(args, "removeMembers")?;
        for m in &remove {
            if !members.iter().any(|old| old == m) {
                bail!("{m} isn't in the group");
            }
        }
        members.retain(|old| !remove.contains(old));
    }
    if set_members || adding || removing {
        members = group_members(hub, &id, &members)?;
        // Cross-machine ids and bots that vanished since the last read are refused, not silently dropped.
        let wanted: BTreeSet<&str> = members.iter().map(String::as_str).collect();
        let existing: BTreeSet<&str> = target.members.iter().map(String::as_str).collect();
        if wanted == existing {
            bail!("nothing to change in the group's members");
        }
        patch.insert("members".into(), json!(members));
        changed.push("members");
    }
    if changed.is_empty() {
        bail!("nothing to change");
    }
    let mut lines = Vec::new();
    if let Some(name) = patch.get("name").and_then(Value::as_str) {
        lines.push(format!("Name: {name}"));
    }
    if let Some(topic) = patch.get("description").and_then(Value::as_str) {
        lines.push(format!("Topic: {topic}"));
    }
    if let Some(members) = patch.get("members").and_then(Value::as_array) {
        let ids: Vec<String> = members.iter().filter_map(Value::as_str).map(str::to_owned).collect();
        lines.push(format!("Members: {}", member_names(hub, &ids)));
    }
    approve(hub, source, format!("Change group {}", target.name), lines.join("\n"), deadline).await?;
    // The user may have edited the group while the card waited. Recheck the live row and write back
    // in the same `emit_lock` that `update_bot` uses, so a stale member list can't overwrite it.
    patch.insert("id".into(), id.clone().into());
    let patch = Value::Object(patch);
    let writing = hub.clone();
    tokio::task::spawn_blocking(move || {
        writing.update_bot_if(&patch, |row| {
            if row.rev != expected_rev {
                bail!("the group changed while your card was up; look at its members again and retry");
            }
            Ok(())
        })
    })
    .await??;
    notice(hub, &source.id, &format!("Updated {}: {}", target.name, changed.join(", ")));
    Ok(json!({"group": group_detail(hub, &id)?}))
}

/// `topic` is the group's About text; `description` is the older wire name the clients use too.
fn group_topic(args: &Value) -> Result<Option<String>> {
    let topic = text(args, "topic")?;
    let description = text(args, "description")?;
    match (topic, description) {
        (Some(a), Some(b)) if a != b => bail!("topic and description must agree"),
        (Some(a), _) | (_, Some(a)) => Ok(Some(checked_instructions(&a)?.to_owned())),
        _ => Ok(None),
    }
}

/// A group id must name an undeleted group on this computer, with its current store revision.
fn group_bot(hub: &Hub, id: &str) -> Result<(BotConfig, i64)> {
    let row = hub
        .store
        .bot(id)?
        .filter(|row| !row.deleted && row.config.is_group())
        .ok_or_else(|| anyhow!("unknown group"))?;
    Ok((row.config, row.rev))
}

/// The `members` strings: a non-empty list of bot ids (duplicates allowed here, removed below).
fn member_ids(args: &Value, key: &str) -> Result<Vec<String>> {
    let list = args[key].as_array().with_context(|| format!("{key} must be a list of bot ids"))?;
    if list.is_empty() {
        bail!("{key} must name at least one bot");
    }
    list.iter()
        .map(|v| {
            let id = v.as_str().with_context(|| format!("{key} must be a list of bot ids"))?.trim();
            if id.is_empty() || crate::store::is_thread_id(id) {
                bail!("{key} must name a bot of this computer, not a thread or an empty id");
            }
            Ok(id.to_owned())
        })
        .collect()
}

/// Member ids without repeats, keeping their order.
fn members_unique(ids: &[String]) -> BTreeSet<&str> {
    ids.iter().map(String::as_str).collect()
}

/// Members that exist here, are agent bots, and aren't plain boxes or deleted: the group model's own check too.
fn group_members(hub: &Hub, group_id: &str, ids: &[String]) -> Result<Vec<String>> {
    for id in ids {
        match hub.store.bot(id)? {
            Some(row) if !row.deleted && row.config.kind == crate::store::BotKind::Agent && !row.config.plain => {}
            Some(row) if row.config.is_group() => bail!("a group can't be a member of another group"),
            _ => bail!("unknown or unavailable bot {id}"),
        }
    }
    if members_unique(ids).len() != ids.len() {
        bail!("a group can't have the same bot twice");
    }
    crate::chat::group::valid_members(&hub.store, group_id, ids)
}

fn member_names(hub: &Hub, ids: &[String]) -> String {
    ids.iter()
        .map(|id| match hub.store.bot(id) {
            Ok(Some(row)) => row.config.name,
            _ => id.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The group in a tool's result: the same fields `list_groups` shows.
fn group_detail(hub: &Hub, id: &str) -> Result<Value> {
    group_list(hub)?.into_iter().find(|g| g["id"] == id).ok_or_else(|| anyhow!("unknown group"))
}
