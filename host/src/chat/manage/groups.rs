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
    // Same members reopen an existing group (`Hub::create_bot`) and ignore the requested name and
    // topic. The card has to describe the group that write will return, not a new one.
    let reuse = crate::chat::group::with_members(&hub.store, &cfg.members)?;
    let (title, lines) = if let Some(existing) = &reuse {
        let have = group_bot(hub, existing)?;
        (format!("Open group {}", have.name), group_card(&have.name, &have.description, &have.members, hub))
    } else {
        (format!("Create group {name}"), group_card(name, &cfg.description, &cfg.members, hub))
    };
    approve(hub, source, title, lines, deadline).await?;
    // A member can be deleted while the card waits. `valid_members` would silently drop it and create
    // a group short of what the card shown; refuse instead.
    group_members(hub, "", &cfg.members)?;
    let created = {
        let hub = hub.clone();
        tokio::task::spawn_blocking(move || hub.create_bot(cfg)).await??
    };
    let id = created["id"].as_str().context("the new group has no id")?;
    // Same members reopen an existing group: the name and the notice are the group's, not the request's.
    let created_name = created["name"].as_str().unwrap_or(name);
    let opened = reuse.as_deref() == Some(id);
    let verb = if opened { "Opened" } else { "Created" };
    notice(hub, &source.id, &format!("{verb} group {created_name}"));
    Ok(json!({"group": group_detail(hub, id)?}))
}

pub(crate) async fn update_group(hub: &Arc<Hub>, source: &BotConfig, args: &Value) -> Result<Value> {
    let deadline = Instant::now() + APPROVAL_TIMEOUT;
    let id = text(args, "groupId")?.context("groupId is required")?;
    let target = group_bot(hub, &id)?;
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
        // The same members is not an error: a name or topic change can still go through.
        if wanted != existing {
            patch.insert("members".into(), json!(members));
            changed.push("members");
        }
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
    // The user may have edited the group while the card waited. Recheck the saved name, topic and
    // members inside the same `emit_lock` as the write. The card itself bumps `rev` (in this room the
    // member goes NeedsInput and `emit_bot` touches the group) without changing what is saved, so a
    // rev compare would refuse a change the user just allowed.
    patch.insert("id".into(), id.clone().into());
    let patch = Value::Object(patch);
    let writing = hub.clone();
    let checking = hub.clone();
    let saved_name = target.name.clone();
    let saved_topic = target.description.clone();
    let saved_members = target.members.clone();
    let wanted_members = patch
        .get("members")
        .and_then(Value::as_array)
        .map(|ids| ids.iter().filter_map(Value::as_str).map(str::to_owned).collect::<Vec<String>>());
    tokio::task::spawn_blocking(move || {
        writing.update_bot_if(&patch, move |row| {
            let same = row.config.name == saved_name
                && row.config.description == saved_topic
                && row.config.members == saved_members;
            if !same {
                bail!("the group changed while your card was up; look at its members again and retry");
            }
            // A member added on the card may have been deleted while it waited: refuse rather than
            // let `valid_members` silently drop it and write a group short of what was shown.
            if let Some(ids) = &wanted_members {
                group_members(&checking, &row.config.id, ids)?;
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

/// A group id must name an undeleted group on this computer.
fn group_bot(hub: &Hub, id: &str) -> Result<BotConfig> {
    hub.store
        .bot(id)?
        .filter(|row| !row.deleted && row.config.is_group())
        .map(|row| row.config)
        .ok_or_else(|| anyhow!("unknown group"))
}

/// What an approval card shows for a group: its name, topic (when it has one) and members.
fn group_card(name: &str, topic: &str, members: &[String], hub: &Hub) -> String {
    let mut lines = vec![format!("Name: {name}")];
    if !topic.is_empty() {
        lines.push(format!("Topic: {topic}"));
    }
    lines.push(format!("Members: {}", member_names(hub, members)));
    lines.join("\n")
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
