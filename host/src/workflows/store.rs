//! The workflow tables. Deletes name every child table: SQLite foreign keys are off.

#![allow(clippy::map_unwrap_or)]

use super::graph::{self, BotState};
use super::limits::{MAX_TITLE, MAX_VERSIONS, VERSION_GAP_MS};
use super::model::{Graph, NodeKind, TriggerKind, VersionKind, default_graph, field_str, new_id};
use crate::store::{Store, now_ms};
use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use std::collections::HashSet;

pub enum Saved {
    Ok { revision: i64 },
    Conflict,
    Unchanged { revision: i64 },
}

pub fn bot_state(store: &Store, id: &str) -> BotState {
    match store.bot(id) {
        Ok(Some(row)) if !row.deleted && row.config.is_group() => BotState::Group,
        Ok(Some(row)) if !row.deleted => BotState::Ok,
        _ => BotState::Missing,
    }
}

pub fn title_of(raw: &str) -> String {
    let trimmed = raw.trim();
    let title = if trimmed.is_empty() { "Workflow" } else { trimmed };
    title.chars().take(MAX_TITLE).collect()
}

pub fn create(
    store: &Store,
    title: &str,
    graph: Option<Graph>,
    script: Option<&str>,
    owner: Option<&str>,
) -> Result<String> {
    let mut graph = graph.unwrap_or_else(default_graph);
    let mut title = title_of(title);
    let (source, script_text) = if let Some(script) = script {
        let applied = super::script::prepare(script, &graph, "", &|id| bot_state(store, id))?;
        if let Some(name) = applied.title {
            title = title_of(&name);
        }
        graph = applied.graph;
        (applied.source, applied.script)
    } else {
        ("graph", super::script::generate(&title, &graph))
    };
    graph::validate(&graph, &|id| bot_state(store, id))?;
    let id = new_id();
    let now = now_ms();
    store.with_immediate(|tx| {
        tx.execute(
            "INSERT INTO workflows(id, title, revision, enabled, strokes_json, next_run_number, created_at, updated_at, source, script_text, owner_bot_id) VALUES(?1,?2,1,1,?3,1,?4,?4,?5,?6,?7)",
            params![id, title, serde_json::to_string(&graph.strokes)?, now, source, script_text, owner],
        )?;
        write_graph(tx, &id, &graph, now, &HashSet::new())?;
        Ok(())
    })?;
    Ok(id)
}

pub fn save(store: &Store, id: &str, base: i64, graph: &Graph) -> Result<Saved> {
    let source = store.with_immediate(|tx| {
        Ok(tx.query_row("SELECT source FROM workflows WHERE id = ?1", [id], |r| r.get::<_, String>(0)).optional()?)
    })?;
    let Some(source) = source else { bail!("This workflow was deleted") };
    if source == "script" && super::script::has_body(graph) {
        bail!("This workflow is a script. Edit the script to change its steps.");
    }
    graph::validate(graph, &|bot| bot_state(store, bot))?;
    let now = now_ms();
    store.with_immediate(|tx| {
        let row: Option<(i64, String, String)> = tx
            .query_row("SELECT revision, source, title FROM workflows WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .optional()?;
        let Some((current, source, title)) = row else { bail!("This workflow was deleted") };
        if current != base {
            return Ok(Saved::Conflict);
        }
        if source == "script" && super::script::has_body(graph) {
            bail!("This workflow is a script. Edit the script to change its steps.");
        }
        let revision = current + 1;
        let kept = due_dates(tx, id)?;
        let script = if source == "script" { None } else { Some(super::script::generate(&title, graph)) };
        tx.execute(
            "UPDATE workflows SET revision = ?2, strokes_json = ?3, updated_at = ?4, script_text = COALESCE(?5, script_text) WHERE id = ?1",
            params![id, revision, serde_json::to_string(&graph.strokes)?, now, script],
        )?;
        write_graph(tx, id, graph, now, &kept)?;
        maybe_version(tx, id, revision, VersionKind::Auto, graph, now)?;
        Ok(Saved::Ok { revision })
    })
}

pub fn save_script(
    store: &Store,
    id: &str,
    base: i64,
    script: Option<&str>,
    triggers: Option<&Graph>,
) -> Result<Saved> {
    if script.is_none() && triggers.is_none() {
        bail!("This script couldn't be read");
    }
    let Some((summary, mut graph)) = load(store, id)? else { bail!("This workflow was deleted") };
    let stored = summary["script"].as_str().unwrap_or("").to_owned();
    let same_script = script.is_some_and(|next| next == stored);
    let same_triggers = triggers.is_none_or(|incoming| trigger_json(&graph) == trigger_json(incoming));
    if same_script && same_triggers {
        return Ok(Saved::Unchanged { revision: summary["revision"].as_i64().unwrap_or(base) });
    }
    if let Some(incoming) = triggers {
        let kept: Vec<_> = incoming.nodes.iter().filter(|n| n.kind == NodeKind::Trigger).cloned().collect();
        if !kept.is_empty() {
            graph.nodes.retain(|n| n.kind != NodeKind::Trigger);
            for (index, node) in kept.into_iter().enumerate() {
                graph.nodes.insert(index, node);
            }
        }
    }
    let now = now_ms();
    if let Some(script) = script {
        let applied = super::script::prepare(script, &graph, &stored, &|bot| bot_state(store, bot))?;
        graph::validate(&applied.graph, &|bot| bot_state(store, bot))?;
        return store.with_immediate(|tx| write_script(tx, id, base, &applied, now));
    }
    graph::validate(&graph, &|bot| bot_state(store, bot))?;
    store.with_immediate(|tx| {
        let current: Option<i64> =
            tx.query_row("SELECT revision FROM workflows WHERE id = ?1", [id], |r| r.get(0)).optional()?;
        let Some(current) = current else { bail!("This workflow was deleted") };
        if current != base {
            return Ok(Saved::Conflict);
        }
        let revision = current + 1;
        let kept = due_dates(tx, id)?;
        tx.execute(
            "UPDATE workflows SET revision = ?2, strokes_json = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, revision, serde_json::to_string(&graph.strokes)?, now],
        )?;
        write_graph(tx, id, &graph, now, &kept)?;
        maybe_version(tx, id, revision, VersionKind::Auto, &graph, now)?;
        Ok(Saved::Ok { revision })
    })
}

fn trigger_json(graph: &Graph) -> String {
    let nodes: Vec<_> = graph.nodes.iter().filter(|n| n.kind == NodeKind::Trigger).collect();
    serde_json::to_string(&nodes).unwrap_or_default()
}

fn write_script(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
    base: i64,
    applied: &super::script::Applied,
    now: i64,
) -> Result<Saved> {
    let current: Option<i64> =
        tx.query_row("SELECT revision FROM workflows WHERE id = ?1", [id], |r| r.get(0)).optional()?;
    let Some(current) = current else { bail!("This workflow was deleted") };
    if current != base {
        return Ok(Saved::Conflict);
    }
    let revision = current + 1;
    tx.execute(
        "UPDATE workflows SET revision = ?2, source = ?3, script_text = ?4, title = COALESCE(?5, title), strokes_json = ?6, updated_at = ?7 WHERE id = ?1",
        params![
            id,
            revision,
            applied.source,
            applied.script,
            applied.title.as_deref(),
            serde_json::to_string(&applied.graph.strokes)?,
            now
        ],
    )?;
    let kept = due_dates(tx, id)?;
    write_graph(tx, id, &applied.graph, now, &kept)?;
    maybe_version(tx, id, revision, VersionKind::Auto, &applied.graph, now)?;
    Ok(Saved::Ok { revision })
}

pub fn rename(store: &Store, id: &str, title: &str) -> Result<()> {
    let title = title_of(title);
    let changed = store.with_immediate(|tx| {
        Ok(tx.execute("UPDATE workflows SET title = ?2, updated_at = ?3 WHERE id = ?1", params![id, title, now_ms()])?)
    })?;
    if changed == 0 {
        bail!("This workflow was deleted");
    }
    Ok(())
}

pub fn set_enabled(store: &Store, id: &str, enabled: bool) -> Result<()> {
    let changed = store.with_immediate(|tx| {
        Ok(tx.execute(
            "UPDATE workflows SET enabled = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, i64::from(enabled), now_ms()],
        )?)
    })?;
    if changed == 0 {
        bail!("This workflow was deleted");
    }
    Ok(())
}

pub fn remove(store: &Store, id: &str) -> Result<()> {
    store.with_immediate(|tx| {
        tx.execute("DELETE FROM workflow_webhook_receipts WHERE workflow_id = ?1", [id])?;
        tx.execute(
            "DELETE FROM workflow_run_notes WHERE run_id IN (SELECT id FROM workflow_runs WHERE workflow_id = ?1)",
            [id],
        )?;
        tx.execute("DELETE FROM workflow_steps WHERE workflow_id = ?1", [id])?;
        tx.execute("DELETE FROM workflow_runs WHERE workflow_id = ?1", [id])?;
        tx.execute("DELETE FROM workflow_versions WHERE workflow_id = ?1", [id])?;
        tx.execute("DELETE FROM workflow_webhook_secrets WHERE workflow_id = ?1", [id])?;
        tx.execute("DELETE FROM workflow_triggers WHERE workflow_id = ?1", [id])?;
        tx.execute("DELETE FROM workflow_edges WHERE workflow_id = ?1", [id])?;
        tx.execute("DELETE FROM workflow_nodes WHERE workflow_id = ?1", [id])?;
        tx.execute("DELETE FROM workflows WHERE id = ?1", [id])?;
        Ok(())
    })
}

/// A deleted bot leaves the step in place, with no bot chosen.
pub fn clear_bot(store: &Store, bot: &str) -> Result<Vec<String>> {
    store.with_immediate(|tx| {
        let ids: Vec<String> = tx
            .prepare("SELECT DISTINCT workflow_id FROM workflow_nodes WHERE bot_id = ?1")?
            .query_map([bot], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        tx.execute(
            "UPDATE workflow_nodes SET bot_id = NULL, fields_json = json_remove(fields_json, '$.botId') WHERE bot_id = ?1",
            [bot],
        )?;
        let now = now_ms();
        for id in &ids {
            tx.execute("UPDATE workflows SET revision = revision + 1, updated_at = ?2 WHERE id = ?1", params![id, now])?;
        }
        Ok(ids)
    })
}

pub fn load(store: &Store, id: &str) -> Result<Option<(Value, Graph)>> {
    store.with_immediate(|tx| load_tx(tx, id))
}

fn load_tx(tx: &rusqlite::Transaction<'_>, id: &str) -> Result<Option<(Value, Graph)>> {
    let row = tx
        .query_row(
            "SELECT title, revision, enabled, strokes_json, created_at, updated_at, source, script_text FROM workflows WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                ))
            },
        )
        .optional()?;
    let Some((title, revision, enabled, strokes, created, updated, source, script)) = row else { return Ok(None) };
    let graph = read_graph(tx, id, &strokes)?;
    let running: Option<String> = tx
        .query_row(
            "SELECT status FROM workflow_runs WHERE workflow_id = ?1 AND status IN ('running','approval','stopping') ORDER BY started_at DESC LIMIT 1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    let last: Option<(i64, String, Option<i64>)> = tx
        .query_row(
            "SELECT number, status, ended_at FROM workflow_runs WHERE workflow_id = ?1 ORDER BY started_at DESC, id DESC LIMIT 1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let summary = json!({
        "id": id,
        "title": title,
        "revision": revision,
        "source": source,
        "script": script,
        "enabled": enabled == 1,
        "updatedAt": updated,
        "createdAt": created,
        "running": running,
        "lastRun": last.map(|(number, status, ended)| json!({"number": number, "status": status, "endedAt": ended})),
    });
    Ok(Some((summary, graph)))
}

/// The bot that created this workflow, when a bot did. User-created workflows are unowned.
pub fn owner(store: &Store, id: &str) -> Result<Option<String>> {
    store.with_immediate(|tx| {
        Ok(tx
            .query_row("SELECT owner_bot_id FROM workflows WHERE id = ?1", [id], |r| r.get::<_, Option<String>>(0))
            .optional()?
            .flatten())
    })
}

pub fn list_owned(store: &Store, bot: &str) -> Result<Vec<Value>> {
    let ids: Vec<String> = store.with_immediate(|tx| {
        let mut st = tx.prepare("SELECT id FROM workflows WHERE owner_bot_id = ?1 ORDER BY updated_at DESC, id")?;
        let ids = st.query_map([bot], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(ids)
    })?;
    ids.into_iter()
        .map(|id| {
            Ok(load(store, &id)?
                .map(|(mut summary, _)| {
                    if let Some(obj) = summary.as_object_mut() {
                        obj.remove("script");
                    }
                    summary
                })
                .unwrap_or(Value::Null))
        })
        .collect()
}

pub fn list(store: &Store) -> Result<Vec<Value>> {
    let ids: Vec<String> = store.with_immediate(|tx| {
        let mut st = tx.prepare("SELECT id FROM workflows ORDER BY updated_at DESC, id")?;
        let ids = st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(ids)
    })?;
    ids.into_iter()
        .map(|id| {
            Ok(load(store, &id)?
                .map(|(mut summary, _)| {
                    if let Some(obj) = summary.as_object_mut() {
                        obj.remove("script");
                    }
                    summary
                })
                .unwrap_or(Value::Null))
        })
        .collect()
}

pub fn versions(store: &Store, id: &str) -> Result<Vec<Value>> {
    store.with_immediate(|tx| {
        let mut st = tx.prepare("SELECT id, revision, kind, created_at FROM workflow_versions WHERE workflow_id = ?1 ORDER BY created_at DESC, id DESC")?;
        let rows = st
            .query_map([id], |r| Ok(json!({"id": r.get::<_, String>(0)?, "revision": r.get::<_, i64>(1)?, "kind": r.get::<_, String>(2)?, "createdAt": r.get::<_, i64>(3)?})))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
}

pub struct VersionSnapshot {
    pub graph: Graph,
    pub script: String,
    pub source: String,
}

pub fn version_snapshot(store: &Store, id: &str, version: &str) -> Result<VersionSnapshot> {
    let (raw, script, source): (String, String, String) = store.with_immediate(|tx| {
        Ok(tx.query_row(
            "SELECT graph_json, COALESCE(script_text, ''), COALESCE(source, '') FROM workflow_versions WHERE id = ?1 AND workflow_id = ?2",
            params![version, id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?)
    })?;
    Ok(VersionSnapshot { graph: serde_json::from_str(&raw)?, script, source })
}

pub fn version_graph(store: &Store, id: &str, version: &str) -> Result<Graph> {
    Ok(version_snapshot(store, id, version)?.graph)
}

/// Puts a saved graph and its script back. The caller records the current copy first.
pub fn restore_saved(store: &Store, id: &str, base: i64, graph: &Graph, script: &str, source: &str) -> Result<Saved> {
    let Some((summary, _)) = load(store, id)? else { bail!("This workflow was deleted") };
    let title = summary["title"].as_str().unwrap_or("Workflow");
    let applied = if restores_graph(source, script, graph, title) {
        super::script::Applied {
            source: "graph",
            script: if script.trim().is_empty() { super::script::generate(title, graph) } else { script.to_owned() },
            graph: graph.clone(),
            title: None,
        }
    } else {
        super::script::prepare(script, graph, "", &|bot| bot_state(store, bot))?
    };
    graph::validate(&applied.graph, &|bot| bot_state(store, bot))?;
    let now = now_ms();
    store.with_immediate(|tx| write_script(tx, id, base, &applied, now))
}

/// A graph version is copied back as saved. Older rows have no source, so a script that still
/// matches the generator is treated as that graph. Anything else is compiled again.
fn restores_graph(source: &str, script: &str, graph: &Graph, title: &str) -> bool {
    if source == "graph" || script.trim().is_empty() {
        return true;
    }
    if !source.is_empty() {
        return false;
    }
    let named = super::script::script_title(script).unwrap_or_else(|| title.to_owned());
    script == super::script::generate(&named, graph)
}

fn read_graph(tx: &rusqlite::Transaction<'_>, id: &str, strokes: &str) -> Result<Graph> {
    let mut nodes = Vec::new();
    let mut st = tx.prepare(
        "SELECT node_id, kind, x, y, width, height, locked, fields_json, schedule_json FROM workflow_nodes WHERE workflow_id = ?1 ORDER BY position, node_id",
    )?;
    let rows = st.query_map([id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, f64>(2)?,
            r.get::<_, f64>(3)?,
            r.get::<_, Option<f64>>(4)?,
            r.get::<_, Option<f64>>(5)?,
            r.get::<_, i64>(6)?,
            r.get::<_, String>(7)?,
            r.get::<_, Option<String>>(8)?,
        ))
    })?;
    for row in rows {
        let (node_id, kind, x, y, width, height, locked, fields, schedule) = row?;
        let kind: NodeKind = serde_json::from_value(Value::String(kind))?;
        nodes.push(super::model::Node {
            id: node_id,
            kind,
            at: super::model::Point { x, y },
            size: width.zip(height).map(|(width, height)| super::model::Size { width, height }),
            locked: locked == 1,
            fields: serde_json::from_str(&fields)?,
            schedule: schedule.map(|raw| serde_json::from_str(&raw)).transpose()?,
        });
    }
    let mut edges = Vec::new();
    let mut st = tx.prepare(
        "SELECT edge_id, from_node_id, to_node_id, port_type, branch FROM workflow_edges WHERE workflow_id = ?1 ORDER BY position, edge_id",
    )?;
    let rows = st.query_map([id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<String>>(4)?,
        ))
    })?;
    for row in rows {
        let (edge_id, from, to, port, branch) = row?;
        edges.push(super::model::Edge {
            id: edge_id,
            from,
            to,
            port: serde_json::from_value(Value::String(port))?,
            branch: branch.map(|raw| serde_json::from_value(Value::String(raw))).transpose()?,
        });
    }
    Ok(Graph { nodes, edges, strokes: serde_json::from_str(strokes).unwrap_or_default() })
}

fn due_dates(tx: &rusqlite::Transaction<'_>, id: &str) -> Result<HashSet<(String, String)>> {
    let mut st = tx
        .prepare("SELECT node_id, schedule_json FROM workflow_triggers WHERE workflow_id = ?1 AND kind = 'schedule'")?;
    let rows = st.query_map([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))?;
    let mut kept = HashSet::new();
    for row in rows {
        let (node, schedule) = row?;
        if let Some(schedule) = schedule {
            kept.insert((node, schedule));
        }
    }
    Ok(kept)
}

fn write_graph(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
    graph: &Graph,
    now: i64,
    kept_due: &HashSet<(String, String)>,
) -> Result<()> {
    let keep: Vec<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();
    tx.execute("DELETE FROM workflow_webhook_secrets WHERE workflow_id = ?1 AND node_id NOT IN (SELECT value FROM json_each(?2))", params![id, serde_json::to_string(&keep)?])?;
    tx.execute(
        "DELETE FROM workflow_triggers WHERE workflow_id = ?1 AND node_id NOT IN (SELECT value FROM json_each(?2))",
        params![id, serde_json::to_string(&keep)?],
    )?;
    tx.execute("DELETE FROM workflow_edges WHERE workflow_id = ?1", [id])?;
    tx.execute(
        "DELETE FROM workflow_nodes WHERE workflow_id = ?1 AND node_id NOT IN (SELECT value FROM json_each(?2))",
        params![id, serde_json::to_string(&keep)?],
    )?;
    for (position, node) in graph.nodes.iter().enumerate() {
        let bot = node_bot(node);
        let schedule = node.schedule.as_ref().map(serde_json::to_string).transpose()?;
        tx.execute(
            "INSERT INTO workflow_nodes(workflow_id, node_id, position, kind, bot_id, x, y, width, height, locked, fields_json, schedule_json)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
             ON CONFLICT(workflow_id, node_id) DO UPDATE SET position=?3, kind=?4, bot_id=?5, x=?6, y=?7, width=?8, height=?9, locked=?10, fields_json=?11, schedule_json=?12",
            params![
                id,
                node.id,
                i64::try_from(position).unwrap_or(i64::MAX),
                node.kind.as_str(),
                bot,
                node.at.x,
                node.at.y,
                node.size.map(|s| s.width),
                node.size.map(|s| s.height),
                i64::from(node.locked),
                serde_json::to_string(&node.fields)?,
                schedule,
            ],
        )?;
        if node.kind == NodeKind::Trigger {
            sync_trigger(tx, id, node, now, kept_due)?;
        } else {
            tx.execute(
                "DELETE FROM workflow_webhook_secrets WHERE workflow_id = ?1 AND node_id = ?2",
                params![id, node.id],
            )?;
            tx.execute("DELETE FROM workflow_triggers WHERE workflow_id = ?1 AND node_id = ?2", params![id, node.id])?;
        }
    }
    for (position, edge) in graph.edges.iter().enumerate() {
        tx.execute(
            "INSERT INTO workflow_edges(workflow_id, edge_id, position, from_node_id, to_node_id, port_type, branch) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![id, edge.id, i64::try_from(position).unwrap_or(i64::MAX), edge.from, edge.to, "text", edge.branch.map(super::model::Branch::as_str)],
        )?;
    }
    Ok(())
}

fn node_bot(node: &super::model::Node) -> Option<&str> {
    match node.kind {
        NodeKind::Agent => field_str(&node.fields, "botId"),
        NodeKind::Output if field_str(&node.fields, "target") == Some("chat") => field_str(&node.fields, "botId"),
        _ => None,
    }
    .map(str::trim)
    .filter(|id| !id.is_empty())
}

fn sync_trigger(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
    node: &super::model::Node,
    now: i64,
    kept_due: &HashSet<(String, String)>,
) -> Result<()> {
    let kind = field_str(&node.fields, "trigger").and_then(TriggerKind::parse).unwrap_or(TriggerKind::Manual);
    let schedule = node.schedule.as_ref().map(serde_json::to_string).transpose()?;
    let zone = field_str(&node.fields, "timezone");
    let same = schedule.as_ref().is_some_and(|raw| kept_due.contains(&(node.id.clone(), raw.clone())));
    let due = if kind == TriggerKind::Schedule && !same {
        node.schedule.as_ref().and_then(|cron| cron.next(now).ok().flatten())
    } else if kind == TriggerKind::Schedule {
        tx.query_row(
            "SELECT next_due_at FROM workflow_triggers WHERE workflow_id = ?1 AND node_id = ?2",
            params![id, node.id],
            |r| r.get(0),
        )
        .optional()?
        .flatten()
    } else {
        None
    };
    if kind != TriggerKind::Webhook {
        tx.execute(
            "DELETE FROM workflow_webhook_secrets WHERE workflow_id = ?1 AND node_id = ?2",
            params![id, node.id],
        )?;
    }
    tx.execute(
        "INSERT INTO workflow_triggers(workflow_id, node_id, kind, timezone, schedule_json, next_due_at, updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(workflow_id, node_id) DO UPDATE SET kind=?3, timezone=?4, schedule_json=?5, next_due_at=?6, updated_at=?7",
        params![id, node.id, kind.as_str(), zone, schedule, due, now],
    )?;
    if kind == TriggerKind::Webhook {
        let exists: i64 = tx.query_row(
            "SELECT COUNT(*) FROM workflow_webhook_secrets WHERE workflow_id = ?1 AND node_id = ?2",
            params![id, node.id],
            |r| r.get(0),
        )?;
        if exists == 0 {
            let (hook, secret) = fresh_hook(tx)?;
            tx.execute(
                "INSERT INTO workflow_webhook_secrets(workflow_id, node_id, hook_id, secret, created_at, updated_at) VALUES(?1,?2,?3,?4,?5,?5)",
                params![id, node.id, hook, secret, now],
            )?;
        }
    }
    Ok(())
}

fn fresh_hook(tx: &rusqlite::Transaction<'_>) -> Result<(String, String)> {
    for _ in 0..5 {
        let hook = new_id();
        let taken: i64 =
            tx.query_row("SELECT COUNT(*) FROM workflow_webhook_secrets WHERE hook_id = ?1", [&hook], |r| r.get(0))?;
        if taken == 0 {
            let secret = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
            return Ok((hook, secret));
        }
    }
    bail!("couldn't mint a webhook id")
}

fn maybe_version(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
    revision: i64,
    kind: VersionKind,
    graph: &Graph,
    now: i64,
) -> Result<()> {
    let latest: Option<i64> = tx
        .query_row(
            "SELECT created_at FROM workflow_versions WHERE workflow_id = ?1 ORDER BY created_at DESC, id DESC LIMIT 1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    if kind == VersionKind::Auto && latest.is_some_and(|at| now.saturating_sub(at) < VERSION_GAP_MS) {
        return Ok(());
    }
    let (script, source): (String, String) = tx
        .query_row("SELECT script_text, source FROM workflows WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap_or_default();
    tx.execute(
        "INSERT INTO workflow_versions(id, workflow_id, revision, kind, graph_json, script_text, source, created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![new_id(), id, revision, kind.as_str(), serde_json::to_string(graph)?, script, source, now],
    )?;
    tx.execute(
        "DELETE FROM workflow_versions WHERE workflow_id = ?1 AND id NOT IN (SELECT id FROM workflow_versions WHERE workflow_id = ?1 ORDER BY created_at DESC, id DESC LIMIT ?2)",
        params![id, MAX_VERSIONS],
    )?;
    Ok(())
}

pub fn record_version(store: &Store, id: &str, revision: i64, kind: VersionKind, graph: &Graph) -> Result<()> {
    let now = now_ms();
    store.with_immediate(|tx| maybe_version(tx, id, revision, kind, graph, now))
}

pub fn published_hooks(store: &Store) -> Vec<crate::routines::Hook> {
    hook_rows(store).unwrap_or_default()
}

fn hook_rows(store: &Store) -> Result<Vec<crate::routines::Hook>> {
    store.with_immediate(|tx| {
        let mut st = tx.prepare(
            "SELECT s.hook_id, s.secret, w.enabled FROM workflow_webhook_secrets s JOIN workflows w ON w.id = s.workflow_id",
        )?;
        let rows = st
            .query_map([], |r| {
                Ok(crate::routines::Hook { id: r.get(0)?, key: r.get(1)?, enabled: r.get::<_, i64>(2)? == 1 })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conflict_does_not_write_and_a_quick_save_skips_a_version() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let id = create(&store, "  ", None, None, None).unwrap();
        let (_, graph) = load(&store, &id).unwrap().unwrap();
        assert!(matches!(save(&store, &id, 0, &graph).unwrap(), Saved::Conflict));
        assert!(matches!(save(&store, &id, 1, &graph).unwrap(), Saved::Ok { revision: 2 }));
        assert_eq!(versions(&store, &id).unwrap().len(), 1, "the first save is a version");
        assert!(matches!(save(&store, &id, 2, &graph).unwrap(), Saved::Ok { revision: 3 }));
        assert_eq!(versions(&store, &id).unwrap().len(), 1, "five minutes have not passed");
        remove(&store, &id).unwrap();
        assert!(load(&store, &id).unwrap().is_none());
        let left: i64 = store
            .with_immediate(|tx| {
                Ok(tx.query_row("SELECT COUNT(*) FROM workflow_nodes WHERE workflow_id = ?1", [&id], |r| r.get(0))?)
            })
            .unwrap();
        assert_eq!(left, 0);
    }

    #[test]
    fn restoring_a_graph_version_keeps_its_steps() {
        use super::super::model::{Branch, Edge, Node, Point, PortType};
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let id = create(&store, "Branch", None, None, None).unwrap();
        let step = |node_id: &str, kind: NodeKind, fields: Value| Node {
            id: node_id.into(),
            kind,
            at: Point { x: 80.0, y: 80.0 },
            size: None,
            locked: false,
            fields: fields.as_object().cloned().unwrap(),
            schedule: None,
        };
        let line = |edge_id: &str, from: &str, to: &str, branch: Option<Branch>| Edge {
            id: edge_id.into(),
            from: from.into(),
            to: to.into(),
            port: PortType::Text,
            branch,
        };
        let graph = Graph {
            nodes: vec![
                step("t", NodeKind::Trigger, json!({"trigger": "manual"})),
                step("c", NodeKind::Condition, json!({"op": "empty"})),
                step("yes", NodeKind::Output, json!({"target": "notify"})),
                step("no", NodeKind::Output, json!({"target": "notify"})),
            ],
            edges: vec![
                line("t>c", "t", "c", None),
                line("c>yes", "c", "yes", Some(Branch::Yes)),
                line("c>no", "c", "no", Some(Branch::No)),
            ],
            strokes: Vec::new(),
        };
        let Saved::Ok { revision } = save(&store, &id, 1, &graph).unwrap() else { panic!("save") };
        let bare = Graph {
            nodes: graph.nodes.iter().filter(|node| node.kind == NodeKind::Trigger).cloned().collect(),
            edges: Vec::new(),
            strokes: Vec::new(),
        };
        let Saved::Ok { revision } = save(&store, &id, revision, &bare).unwrap() else { panic!("replace") };
        let versions = versions(&store, &id).unwrap();
        let version = versions[0]["id"].as_str().unwrap();
        let snap = version_snapshot(&store, &id, version).unwrap();
        assert_eq!(snap.source, "graph");
        assert!(snap.script.contains("let "), "{}", snap.script);
        let Saved::Ok { .. } = restore_saved(&store, &id, revision, &snap.graph, &snap.script, &snap.source).unwrap()
        else {
            panic!("restore")
        };
        let (summary, restored) = load(&store, &id).unwrap().unwrap();
        assert_eq!(summary["source"].as_str(), Some("graph"));
        assert!(restored.nodes.iter().any(|node| node.kind == NodeKind::Condition));
        assert!(restored.nodes.iter().any(|node| node.id == "yes"));
        assert!(restored.edges.iter().any(|edge| edge.from == "c" && edge.to == "yes"));
    }
}
