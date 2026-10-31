//! One run: a snapshot of the graph, then steps until only a bot or a person is left.

#![allow(
    clippy::too_many_arguments,
    clippy::map_unwrap_or,
    clippy::needless_pass_by_value,
    clippy::collapsible_if,
    clippy::redundant_closure,
    clippy::redundant_closure_for_method_calls
)]

use super::graph::BotState;
use super::graph::loop_edges;
use super::limits::{MAX_RUN_PAGE, MAX_STEPS, MAX_TEXT};
use super::model::{Branch, Graph, Node, NodeKind, OutputTarget, RunSource, RunStatus, StepStatus, field_str, new_id};
use super::store;
use super::walk::{self, Action, Recorded};
use crate::hub::Hub;
use crate::remote::push::{AlertKind, WorkflowLink};
use crate::store::{EntryKind, Lane, Store, now_ms, run_session_key};
use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use std::collections::HashSet;

pub struct Effects {
    pub agents: Vec<(String, String)>,
    pub pushes: Vec<Push>,
}

pub struct Push {
    pub bot: String,
    pub title: String,
    pub body: String,
    pub kind: AlertKind,
    pub workflow: String,
    pub run: Option<String>,
    pub step: String,
}

pub fn start(
    hub: &Hub,
    id: &str,
    request: &str,
    from: Option<&str>,
    source: RunSource,
    payload: &str,
    scheduled_for: Option<i64>,
) -> Result<Value> {
    let key = match source {
        RunSource::Manual => format!("manual:{request}"),
        RunSource::Schedule => format!("schedule:{}:{}", from.unwrap_or(""), scheduled_for.unwrap_or(0)),
        RunSource::Webhook => format!("webhook:{}:{request}", from.unwrap_or("")),
    };
    if let Some(existing) = find_key(&hub.store, id, &key)? {
        return run_detail(hub, &existing);
    }
    let Some((summary, graph)) = store::load(&hub.store, id)? else { bail!("This workflow was deleted") };
    if !can_start(&graph, summary["source"].as_str() == Some("script"), source) {
        bail!("Add a trigger first");
    }
    let enabled = summary["enabled"].as_bool().unwrap_or(false);
    if !enabled && source != RunSource::Manual {
        bail!("This workflow is paused");
    }
    if open_run(&hub.store, id)?.is_some() {
        bail!("A run is already in progress");
    }
    let snapshot = graph.clone();
    if summary["source"].as_str() == Some("script") {
        return super::script_run::start_script(
            hub,
            id,
            &summary,
            &snapshot,
            request,
            from,
            source,
            payload,
            scheduled_for,
            &key,
        );
    }
    let scope = scope_of(&snapshot, from)?;
    check_ready(&snapshot, &scope)?;
    let run_id = new_id();
    let now = now_ms();
    let number = hub.store.with_immediate(|tx| {
        let number: i64 = tx.query_row("SELECT next_run_number FROM workflows WHERE id = ?1", [id], |r| r.get(0))?;
        tx.execute("UPDATE workflows SET next_run_number = ?2 WHERE id = ?1", params![id, number + 1])?;
        tx.execute(
            "INSERT INTO workflow_runs(id, workflow_id, number, trigger_node_id, from_node_id, source, scheduled_for, idempotency_key, revision, graph_json, payload, status, started_at, updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'running',?12,?12)",
            params![
                run_id,
                id,
                number,
                from.filter(|node| snapshot.nodes.iter().any(|n| n.id == *node && n.kind == NodeKind::Trigger)),
                from,
                source.as_str(),
                scheduled_for,
                key,
                summary["revision"].as_i64().unwrap_or(0),
                serde_json::to_string(&snapshot)?,
                payload,
                now,
            ],
        )?;
        if from.is_some_and(|node| snapshot.nodes.iter().any(|n| n.id == node && n.kind != NodeKind::Trigger)) {
            reuse(tx, id, &run_id, &snapshot, &scope, now)?;
        }
        Ok(number)
    })?;
    let _ = number;
    let _ = store::record_version(
        &hub.store,
        id,
        summary["revision"].as_i64().unwrap_or(0),
        super::model::VersionKind::Run,
        &snapshot,
    );
    let effects = advance(hub, &run_id)?;
    dispatch_agents(hub, &effects);
    dispatch_pushes(hub, &effects);
    hub.emit_workflow(json!({"type": "workflow", "change": "run", "workflowId": id, "runId": run_id}));
    run_detail(hub, &run_id)
}

fn scope_of(graph: &Graph, from: Option<&str>) -> Result<HashSet<String>> {
    let steps: HashSet<String> = graph.nodes.iter().filter(|n| n.kind.is_step()).map(|n| n.id.clone()).collect();
    let Some(from) = from else { return Ok(steps) };
    if !steps.contains(from) {
        bail!("That step is not in this workflow");
    }
    let forward = walk(graph, from, true);
    let kind = graph.nodes.iter().find(|n| n.id == from).map(|n| n.kind);
    if kind == Some(NodeKind::Trigger) {
        let mut scope = forward;
        scope.extend(walk_back(graph, &scope));
        Ok(scope.into_iter().filter(|id| steps.contains(id)).collect())
    } else {
        Ok(forward.into_iter().filter(|id| steps.contains(id)).collect())
    }
}

fn walk(graph: &Graph, start: &str, forward: bool) -> HashSet<String> {
    let mut seen = HashSet::from([start.to_owned()]);
    let mut queue = vec![start.to_owned()];
    while let Some(id) = queue.pop() {
        for edge in &graph.edges {
            let next = if forward { edge.from == id } else { edge.to == id };
            let other = if forward { edge.to.clone() } else { edge.from.clone() };
            if next && seen.insert(other.clone()) {
                queue.push(other);
            }
        }
    }
    seen
}

fn walk_back(graph: &Graph, seeds: &HashSet<String>) -> HashSet<String> {
    let mut seen = HashSet::new();
    let mut queue: Vec<String> = seeds.iter().cloned().collect();
    while let Some(id) = queue.pop() {
        for edge in &graph.edges {
            if edge.to != id {
                continue;
            }
            let other = graph.nodes.iter().find(|n| n.id == edge.from);
            if other.is_some_and(|n| n.kind == NodeKind::Trigger) {
                continue;
            }
            if seen.insert(edge.from.clone()) {
                queue.push(edge.from.clone());
            }
        }
    }
    seen
}

/// A manual run with steps can start from those steps. A blank canvas, a script, or a
/// schedule or webhook still needs a trigger node.
fn can_start(graph: &Graph, scripted: bool, source: RunSource) -> bool {
    if graph.nodes.iter().any(|node| node.kind == NodeKind::Trigger) {
        return true;
    }
    if scripted || source != RunSource::Manual {
        return false;
    }
    graph.nodes.iter().any(|node| {
        matches!(
            node.kind,
            NodeKind::Prompt | NodeKind::Agent | NodeKind::Condition | NodeKind::Approval | NodeKind::Output
        )
    })
}

fn check_ready(graph: &Graph, scope: &HashSet<String>) -> Result<()> {
    for node in graph.nodes.iter().filter(|n| scope.contains(&n.id)) {
        match node.kind {
            NodeKind::Agent => {
                if field_str(&node.fields, "botId").unwrap_or("").trim().is_empty() || bot_state_bad(node) {
                    bail!("Choose a bot for this step");
                }
                let task = field_str(&node.fields, "task").unwrap_or("").trim();
                let fed = graph.edges.iter().any(|edge| edge.to == node.id);
                if task.is_empty() && !fed {
                    bail!("This step needs instructions");
                }
            }
            NodeKind::Output
                if field_str(&node.fields, "target") == Some("chat")
                    && field_str(&node.fields, "botId").unwrap_or("").trim().is_empty() =>
            {
                bail!("Choose a bot for this step");
            }
            NodeKind::Trigger
                if field_str(&node.fields, "trigger") == Some("schedule")
                    && !matches!(node.schedule, Some(crate::routines::Trigger::Cron { .. })) =>
            {
                bail!("Pick when this workflow runs");
            }
            _ => {}
        }
    }
    Ok(())
}

fn bot_state_bad(node: &Node) -> bool {
    field_str(&node.fields, "botId").unwrap_or("").trim().is_empty()
}

fn reuse(
    tx: &rusqlite::Transaction<'_>,
    workflow: &str,
    run: &str,
    graph: &Graph,
    scope: &HashSet<String>,
    now: i64,
) -> Result<()> {
    let mut needed = HashSet::new();
    for id in scope {
        for edge in &graph.edges {
            if edge.to == *id && !scope.contains(&edge.from) {
                needed.insert(edge.from.clone());
            }
        }
    }
    for node in needed {
        let Some(previous) = tx.query_row(
            "SELECT output, branch FROM workflow_steps WHERE workflow_id = ?1 AND node_id = ?2 AND status = 'done' ORDER BY ended_at DESC, id DESC LIMIT 1",
            params![workflow, node],
            |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?)),
        ).optional()? else { continue };
        let kind = graph.nodes.iter().find(|n| n.id == node).map(|n| n.kind.as_str()).unwrap_or("prompt");
        insert_step(
            tx,
            &new_id(),
            workflow,
            Some(run),
            &node,
            1,
            &format!("{run}:{node}:reused"),
            kind,
            None,
            StepStatus::Done,
            "",
            previous.0.as_deref().unwrap_or(""),
            previous.1.as_deref(),
            1,
            false,
            None,
            now,
        )?;
    }
    Ok(())
}

pub fn advance(hub: &Hub, run_id: &str) -> Result<Effects> {
    let mut effects = Effects { agents: Vec::new(), pushes: Vec::new() };
    let cap = usize::try_from(MAX_STEPS).unwrap_or(2000);
    for _ in 0..cap {
        let Some(run) = load_run(&hub.store, run_id)? else { return Ok(effects) };
        if !RunStatus::parse(&run.status).is_some_and(RunStatus::is_open) {
            return Ok(effects);
        }
        let graph: Graph = serde_json::from_str(&run.graph)?;
        let steps = load_steps(&hub.store, run_id)?;
        let open = steps.iter().any(|s| s.status.is_open());
        let actions = if run.status == "stopping" {
            Vec::new()
        } else {
            let scope: HashSet<String> =
                graph.nodes.iter().filter(|n| n.kind.is_step()).map(|n| n.id.clone()).collect();
            walk::next_actions(&graph, &recorded(&steps), &scope_for_run(&graph, &run, &steps), &loop_edges(&graph))
                .into_iter()
                .filter(|action| scope.contains(action_node(action)))
                .collect()
        };
        if actions.is_empty() {
            settle_run(hub, &run, &steps, &mut effects)?;
            return Ok(effects);
        }
        let _ = open;
        for action in actions {
            apply(hub, &run, &graph, &action, &mut effects)?;
        }
    }
    Ok(effects)
}

fn scope_for_run(graph: &Graph, run: &RunRow, steps: &[StepRow]) -> HashSet<String> {
    if run.from.is_none() {
        return graph.nodes.iter().filter(|n| n.kind.is_step()).map(|n| n.id.clone()).collect();
    }
    let mut scope = HashSet::new();
    if let Some(from) = &run.from {
        scope = scope_of(graph, Some(from)).unwrap_or_default();
    }
    for step in steps {
        if step.request_id.ends_with(":reused") {
            scope.remove(&step.node_id);
        }
    }
    scope
}

fn action_node(action: &Action) -> &str {
    match action {
        Action::Start { node_id, .. } | Action::Skip { node_id } | Action::Fail { node_id, .. } => node_id,
    }
}

fn apply(hub: &Hub, run: &RunRow, graph: &Graph, action: &Action, effects: &mut Effects) -> Result<()> {
    let node_id = action_node(action);
    let Some(node) = graph.nodes.iter().find(|n| n.id == node_id).cloned() else { return Ok(()) };
    let now = now_ms();
    match action {
        Action::Skip { .. } => {
            write_terminal(hub, run, &node, StepStatus::Skipped, "", None, None, false, None, 1, now)
        }
        Action::Fail { error, .. } => {
            write_terminal(hub, run, &node, StepStatus::Failed, "", None, None, false, Some(error), 1, now)
        }
        Action::Start { input, truncated, passes, .. } => match node.kind {
            NodeKind::Trigger => write_terminal(
                hub,
                run,
                &node,
                StepStatus::Done,
                input,
                Some(run.payload.clone()),
                None,
                *truncated,
                None,
                *passes,
                now,
            ),
            NodeKind::Prompt => {
                let text = field_str(&node.fields, "text").unwrap_or("").to_owned();
                write_terminal(
                    hub,
                    run,
                    &node,
                    StepStatus::Done,
                    input,
                    Some(text),
                    None,
                    *truncated,
                    None,
                    *passes,
                    now,
                )
            }
            NodeKind::Condition => {
                let branch = condition_branch(&node, input);
                write_terminal(
                    hub,
                    run,
                    &node,
                    StepStatus::Done,
                    input,
                    Some(input.clone()),
                    Some(branch),
                    *truncated,
                    None,
                    *passes,
                    now,
                )
            }
            NodeKind::Approval => {
                let step = write_open(hub, run, &node, StepStatus::Approval, input, *truncated, *passes, now)?;
                if let Some(push) = approval_push(hub, run, &node, &step, input) {
                    effects.pushes.push(push);
                }
                Ok(())
            }
            NodeKind::Output => finish_output(hub, run, &node, input, *truncated, *passes, now, effects),
            NodeKind::Agent => {
                let bot = field_str(&node.fields, "botId").unwrap_or("").trim().to_owned();
                if bot.is_empty() || !matches!(store::bot_state(&hub.store, &bot), BotState::Ok) {
                    return write_terminal(
                        hub,
                        run,
                        &node,
                        StepStatus::Failed,
                        input,
                        None,
                        None,
                        false,
                        Some("Choose a bot for this step"),
                        *passes,
                        now,
                    );
                }
                let step = write_open(hub, run, &node, StepStatus::Running, input, *truncated, *passes, now)?;
                effects.agents.push((bot, step));
                Ok(())
            }
            NodeKind::Text | NodeKind::Note | NodeKind::Shape => Ok(()),
        },
    }
}

fn condition_branch(node: &Node, input: &str) -> Branch {
    let op = field_str(&node.fields, "op").unwrap_or("");
    let value = field_str(&node.fields, "value").unwrap_or("");
    let yes = match op {
        "contains" => input.contains(value),
        "equals" => input == value,
        "regex" => regex::Regex::new(value).is_ok_and(|re| re.is_match(input)),
        "empty" => input.trim().is_empty(),
        _ => false,
    };
    if yes { Branch::Yes } else { Branch::No }
}

fn finish_output(
    hub: &Hub,
    run: &RunRow,
    node: &Node,
    input: &str,
    truncated: bool,
    passes: i64,
    now: i64,
    effects: &mut Effects,
) -> Result<()> {
    if OutputTarget::parse(field_str(&node.fields, "target")).is_chat() {
        let bot = field_str(&node.fields, "botId").unwrap_or("").trim();
        if !matches!(store::bot_state(&hub.store, bot), BotState::Ok) {
            return write_terminal(
                hub,
                run,
                node,
                StepStatus::Failed,
                input,
                None,
                None,
                truncated,
                Some("Choose a bot for this step"),
                passes,
                now,
            );
        }
        let step = new_id();
        let turn = hub.store.max_turn(bot) + 1;
        let data = json!({"text": clip(input), "workflowId": run.workflow, "runId": run.id, "stepId": step, "notice": "workflow-output"});
        hub.add_entry_once(&format!("workflow-output-{step}"), &Lane::main(bot), EntryKind::Notice, turn, &data)?;
        write_step(
            &hub.store,
            run,
            node,
            &step,
            StepStatus::Done,
            input,
            Some(input),
            None,
            truncated,
            None,
            passes,
            now,
        )?;
        return Ok(());
    }
    let step = write_open(hub, run, node, StepStatus::Done, input, truncated, passes, now)?;
    if let Some(bot) = anchor(hub, run, Some(node)) {
        effects.pushes.push(Push {
            bot,
            title: run.title.clone(),
            body: clip(input),
            kind: AlertKind::Done,
            workflow: run.workflow.clone(),
            run: Some(run.id.clone()),
            step,
        });
    }
    Ok(())
}

fn approval_push(hub: &Hub, run: &RunRow, node: &Node, step: &str, input: &str) -> Option<Push> {
    let question = field_str(&node.fields, "question").unwrap_or("").trim();
    let body = if question.is_empty() { "Approve this step?" } else { question };
    let _ = input;
    Some(Push {
        bot: anchor(hub, run, None)?,
        title: run.title.clone(),
        body: body.to_owned(),
        kind: AlertKind::NeedsInput,
        workflow: run.workflow.clone(),
        run: Some(run.id.clone()),
        step: step.to_owned(),
    })
}

fn anchor(hub: &Hub, run: &RunRow, node: Option<&Node>) -> Option<String> {
    if let Some(node) = node.filter(|n| OutputTarget::parse(field_str(&n.fields, "target")).is_chat()) {
        return field_str(&node.fields, "botId").map(str::trim).filter(|id| !id.is_empty()).map(str::to_owned);
    }
    let graph: Graph = serde_json::from_str(&run.graph).ok()?;
    graph
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::Agent && field_str(&n.fields, "botId").is_some())
        .and_then(|n| field_str(&n.fields, "botId").map(str::to_owned))
        .or_else(|| lead(&hub.store))
}

fn lead(store: &Store) -> Option<String> {
    store.bots().ok()?.into_iter().find(|b| !b.deleted && b.config.lead).map(|b| b.config.id)
}

fn settle_run(hub: &Hub, run: &RunRow, steps: &[StepRow], effects: &mut Effects) -> Result<()> {
    let stopping = run.status == "stopping";
    if stopping {
        for step in steps.iter().filter(|s| s.status.is_open()) {
            hub.store.with_immediate(|tx| {
                tx.execute(
                    "UPDATE workflow_steps SET status = 'stopped', ended_at = ?2, updated_at = ?2 WHERE id = ?1",
                    params![step.id, now_ms()],
                )?;
                Ok(())
            })?;
        }
    } else if steps.iter().any(|s| s.status == StepStatus::Running) {
        hub.store.with_immediate(|tx| {
            Ok(tx.execute(
                "UPDATE workflow_runs SET status = 'running', updated_at = ?2 WHERE id = ?1",
                params![run.id, now_ms()],
            )?)
        })?;
        return Ok(());
    } else if steps.iter().any(|s| s.status == StepStatus::Approval) {
        hub.store.with_immediate(|tx| {
            Ok(tx.execute(
                "UPDATE workflow_runs SET status = 'approval', updated_at = ?2 WHERE id = ?1",
                params![run.id, now_ms()],
            )?)
        })?;
        hub.emit_workflow(json!({"type": "workflow", "change": "run", "workflowId": run.workflow, "runId": run.id}));
        return Ok(());
    }
    let steps = load_steps(&hub.store, &run.id)?;
    let status = if stopping
        || (steps.iter().any(|s| s.status == StepStatus::Stopped)
            && steps.iter().all(|s| s.status != StepStatus::Failed))
    {
        RunStatus::Stopped
    } else if let Some(failed) = steps.iter().find(|s| s.status == StepStatus::Failed) {
        let _ = failed;
        RunStatus::Failed
    } else {
        RunStatus::Done
    };
    let error = steps.iter().find(|s| s.status == StepStatus::Failed).and_then(|s| s.error.clone());
    let now = now_ms();
    hub.store.with_immediate(|tx| {
        tx.execute(
            "UPDATE workflow_runs SET status = ?2, error = ?3, ended_at = ?4, updated_at = ?4 WHERE id = ?1",
            params![run.id, status.as_str(), error, now],
        )?;
        Ok(())
    })?;
    for step in &steps {
        if step.kind == "agent" {
            if let Some(bot) = &step.bot {
                let _ = hub.store.kv_delete(&run_session_key(bot, &format!("workflow-{}", step.id)));
            }
        }
    }
    if status == RunStatus::Failed {
        if let (Some(bot), Some(error)) = (anchor(hub, run, None), error.clone()) {
            effects.pushes.push(Push {
                bot,
                title: run.title.clone(),
                body: clip(&error),
                kind: AlertKind::Failed,
                workflow: run.workflow.clone(),
                run: Some(run.id.clone()),
                step: steps.first().map(|s| s.id.clone()).unwrap_or_default(),
            });
        }
    }
    hub.emit_workflow(json!({"type": "workflow", "change": "run", "workflowId": run.workflow, "runId": run.id}));
    let _ = status;
    Ok(())
}

fn write_terminal(
    hub: &Hub,
    run: &RunRow,
    node: &Node,
    status: StepStatus,
    input: &str,
    output: Option<String>,
    branch: Option<Branch>,
    truncated: bool,
    error: Option<&str>,
    passes: i64,
    now: i64,
) -> Result<()> {
    let step = new_id();
    write_step(&hub.store, run, node, &step, status, input, output.as_deref(), branch, truncated, error, passes, now)?;
    emit_step(hub, &run.workflow, &run.id, &step);
    Ok(())
}

fn write_open(
    hub: &Hub,
    run: &RunRow,
    node: &Node,
    status: StepStatus,
    input: &str,
    truncated: bool,
    passes: i64,
    now: i64,
) -> Result<String> {
    let step = new_id();
    let output = (status == StepStatus::Done).then_some(input);
    write_step(&hub.store, run, node, &step, status, input, output, None, truncated, None, passes, now)?;
    emit_step(hub, &run.workflow, &run.id, &step);
    Ok(step)
}

fn emit_step(hub: &Hub, workflow: &str, run: &str, step: &str) {
    hub.emit_workflow(
        json!({"type": "workflow", "change": "step", "workflowId": workflow, "runId": run, "stepId": step}),
    );
}

fn write_step(
    store: &Store,
    run: &RunRow,
    node: &Node,
    step: &str,
    status: StepStatus,
    input: &str,
    output: Option<&str>,
    branch: Option<Branch>,
    truncated: bool,
    error: Option<&str>,
    passes: i64,
    now: i64,
) -> Result<()> {
    let attempt = next_attempt(store, &run.id, &node.id)?;
    let ended = (!status.is_open()).then_some(now);
    let bot = field_str(&node.fields, "botId").map(str::trim).filter(|id| !id.is_empty());
    store.with_immediate(|tx| {
        insert_step(
            tx,
            step,
            &run.workflow,
            Some(&run.id),
            &node.id,
            attempt,
            &format!("{}:{}:{attempt}", run.id, node.id),
            node.kind.as_str(),
            bot,
            status,
            input,
            output.unwrap_or(""),
            branch.map(Branch::as_str),
            passes,
            truncated,
            error,
            now,
        )
    })?;
    let _ = ended;
    Ok(())
}

fn insert_step(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
    workflow: &str,
    run: Option<&str>,
    node: &str,
    attempt: i64,
    request: &str,
    kind: &str,
    bot: Option<&str>,
    status: StepStatus,
    input: &str,
    output: &str,
    branch: Option<&str>,
    passes: i64,
    truncated: bool,
    error: Option<&str>,
    now: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO workflow_steps(id, workflow_id, run_id, node_id, attempt, request_id, kind, bot_id, status, input, output, branch, passes, truncated, error, started_at, ended_at, created_at, updated_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?18)",
        params![id, workflow, run, node, attempt, request, kind, bot, status.as_str(), input, output, branch, passes, i64::from(truncated), error, (kind != "agent" || !status.is_open()).then_some(now), (!status.is_open()).then_some(now), now],
    )?;
    Ok(())
}

fn next_attempt(store: &Store, run: &str, node: &str) -> Result<i64> {
    let count: i64 = store.with_immediate(|tx| {
        Ok(tx.query_row(
            "SELECT COALESCE(MAX(attempt), 0) FROM workflow_steps WHERE run_id = ?1 AND node_id = ?2",
            params![run, node],
            |r| r.get(0),
        )?)
    })?;
    Ok(count + 1)
}

pub fn run_detail(hub: &Hub, run_id: &str) -> Result<Value> {
    let Some(run) = load_run(&hub.store, run_id)? else { bail!("This run was deleted") };
    let steps = load_steps(&hub.store, run_id)?;
    let graph: Graph =
        serde_json::from_str(&run.graph).unwrap_or(Graph { nodes: vec![], edges: vec![], strokes: vec![] });
    Ok(json!({
        "run": {
            "id": run.id,
            "number": run.number,
            "source": run.source,
            "status": run.status,
            "exec": run.exec,
            "output": run.output,
            "startedAt": run.started,
            "endedAt": run.ended,
            "error": run.error,
            "graph": graph,
            "steps": steps.into_iter().map(|s| step_json(&s, &graph)).collect::<Vec<_>>(),
        }
    }))
}

fn step_json(step: &StepRow, graph: &Graph) -> Value {
    let title =
        graph.nodes.iter().find(|n| n.id == step.node_id).map(|n| step_title(n)).unwrap_or_else(|| step.kind.clone());
    json!({
        "id": step.id,
        "nodeId": step.node_id,
        "attempt": step.attempt,
        "requestId": step.request_id,
        "kind": step.kind,
        "botId": step.bot,
        "status": step.status.as_str(),
        "input": clip(&step.input),
        "output": step.output.as_ref().map(|t| clip(t)),
        "branch": step.branch.map(|b| b.as_str()),
        "passes": step.passes,
        "truncated": step.truncated,
        "error": step.error.as_ref().map(|t| clip(t)),
        "startedAt": step.started,
        "endedAt": step.ended,
        "title": title,
    })
}

pub fn step_title(node: &Node) -> String {
    match node.kind {
        NodeKind::Trigger => match field_str(&node.fields, "trigger") {
            Some("schedule") => "Schedule",
            Some("webhook") => "Webhook",
            _ => "Manual",
        }
        .into(),
        NodeKind::Prompt => first_line(field_str(&node.fields, "text")).unwrap_or("Prompt").into(),
        NodeKind::Agent => "Assistant".into(),
        NodeKind::Condition => "Condition".into(),
        NodeKind::Approval => first_line(field_str(&node.fields, "question")).unwrap_or("Approval").into(),
        NodeKind::Output if field_str(&node.fields, "target") == Some("chat") => "Chat".into(),
        NodeKind::Output => "Notification".into(),
        NodeKind::Text | NodeKind::Note | NodeKind::Shape => "Note".into(),
    }
}

fn first_line(text: Option<&str>) -> Option<&str> {
    text.map(str::trim).filter(|t| !t.is_empty()).map(|t| t.lines().next().unwrap_or(t))
}

fn clip(text: &str) -> String {
    text.chars().take(MAX_TEXT).collect()
}

struct RunRow {
    id: String,
    workflow: String,
    number: i64,
    from: Option<String>,
    source: String,
    graph: String,
    payload: String,
    status: String,
    error: Option<String>,
    output: Option<String>,
    exec: String,
    started: i64,
    ended: Option<i64>,
    title: String,
}

struct StepRow {
    id: String,
    node_id: String,
    attempt: i64,
    request_id: String,
    kind: String,
    bot: Option<String>,
    status: StepStatus,
    input: String,
    output: Option<String>,
    branch: Option<Branch>,
    passes: i64,
    truncated: bool,
    error: Option<String>,
    started: Option<i64>,
    ended: Option<i64>,
}

fn load_run(store: &Store, id: &str) -> Result<Option<RunRow>> {
    store.with_immediate(|tx| {
        tx.query_row(
            "SELECT r.id, r.workflow_id, r.number, r.from_node_id, r.source, r.graph_json, COALESCE(r.payload, ''), r.status, r.error, r.output, COALESCE(r.exec, 'graph'), r.started_at, r.ended_at, w.title
             FROM workflow_runs r JOIN workflows w ON w.id = r.workflow_id WHERE r.id = ?1",
            [id],
            |r| {
                Ok(RunRow {
                    id: r.get(0)?,
                    workflow: r.get(1)?,
                    number: r.get(2)?,
                    from: r.get(3)?,
                    source: r.get(4)?,
                    graph: r.get(5)?,
                    payload: r.get(6)?,
                    status: r.get(7)?,
                    error: r.get(8)?,
                    output: r.get(9)?,
                    exec: r.get(10)?,
                    started: r.get(11)?,
                    ended: r.get(12)?,
                    title: r.get(13)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    })
}

fn load_steps(store: &Store, run: &str) -> Result<Vec<StepRow>> {
    store.with_immediate(|tx| {
        let mut st = tx.prepare(
            "SELECT id, node_id, attempt, request_id, kind, bot_id, status, input, output, branch, passes, truncated, error, started_at, ended_at FROM workflow_steps WHERE run_id = ?1 ORDER BY created_at, id",
        )?;
        let rows = st.query_map([run], |r| {
            Ok(StepRow {
                id: r.get(0)?,
                node_id: r.get(1)?,
                attempt: r.get(2)?,
                request_id: r.get(3)?,
                kind: r.get(4)?,
                bot: r.get(5)?,
                status: StepStatus::parse(&r.get::<_, String>(6)?).unwrap_or(StepStatus::Failed),
                input: r.get(7)?,
                output: r.get(8)?,
                branch: r.get::<_, Option<String>>(9)?.as_deref().and_then(|b| match b { "yes" => Some(Branch::Yes), "no" => Some(Branch::No), _ => None }),
                passes: r.get(10)?,
                truncated: r.get::<_, i64>(11)? == 1,
                error: r.get(12)?,
                started: r.get(13)?,
                ended: r.get(14)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

fn recorded(steps: &[StepRow]) -> Vec<Recorded> {
    steps
        .iter()
        .map(|s| Recorded {
            id: s.id.clone(),
            node_id: s.node_id.clone(),
            status: s.status,
            output: s.output.clone().unwrap_or_default(),
            branch: s.branch,
            ended_at: s.ended.unwrap_or(0),
        })
        .collect()
}

fn find_key(store: &Store, workflow: &str, key: &str) -> Result<Option<String>> {
    store.with_immediate(|tx| {
        tx.query_row(
            "SELECT id FROM workflow_runs WHERE workflow_id = ?1 AND idempotency_key = ?2",
            params![workflow, key],
            |r| r.get(0),
        )
        .optional()
        .map_err(Into::into)
    })
}

pub fn open_run(store: &Store, workflow: &str) -> Result<Option<String>> {
    store.with_immediate(|tx| {
        tx.query_row("SELECT id FROM workflow_runs WHERE workflow_id = ?1 AND status IN ('running','approval','stopping') LIMIT 1", [workflow], |r| r.get(0)).optional().map_err(Into::into)
    })
}

pub fn dispatch_pushes(hub: &Hub, effects: &Effects) {
    for push in &effects.pushes {
        let Ok(Some(bot)) = hub.store.bot(&push.bot) else { continue };
        crate::remote::push::notify(
            hub,
            &bot.config,
            None,
            &push.title,
            &push.body,
            push.kind,
            Some(&WorkflowLink { workflow: push.workflow.clone(), run: push.run.clone(), step: push.step.clone() }),
        );
    }
}

pub fn dispatch_agents(hub: &Hub, effects: &Effects) {
    for (bot, step) in &effects.agents {
        if let Err(error) = hub.send_cmd(bot, crate::agent::bot::Cmd::WorkflowStep { step_id: step.clone() }) {
            tracing::warn!(bot, step, error = format!("{error:#}"), "couldn't start a workflow step");
            let _ = fail_step(hub, step, "Choose a bot for this step");
        }
    }
}

pub fn fail_step(hub: &Hub, step: &str, error: &str) -> Result<()> {
    let now = now_ms();
    let run: Option<String> = hub.store.with_immediate(|tx| {
        tx.execute("UPDATE workflow_steps SET status = 'failed', error = ?2, ended_at = ?3, updated_at = ?3 WHERE id = ?1 AND status = 'running'", params![step, error, now])?;
        Ok(tx.query_row("SELECT run_id FROM workflow_steps WHERE id = ?1", [step], |r| r.get::<_, Option<String>>(0))?)
    })?;
    if let Some(run) = run {
        let effects = advance(hub, &run)?;
        dispatch_agents(hub, &effects);
        dispatch_pushes(hub, &effects);
    }
    Ok(())
}

pub fn runs_page(
    store: &Store,
    workflow: &str,
    limit: i64,
    before_at: Option<i64>,
    before_id: Option<&str>,
) -> Result<Value> {
    let limit = limit.clamp(1, MAX_RUN_PAGE);
    let take = usize::try_from(limit).unwrap_or(20);
    store.with_immediate(|tx| {
        let rows = if let (Some(at), Some(id)) = (before_at, before_id) {
            let mut st = tx.prepare("SELECT id, number, source, status, started_at, ended_at, error FROM workflow_runs WHERE workflow_id = ?1 AND (started_at, id) < (?2, ?3) ORDER BY started_at DESC, id DESC LIMIT ?4")?;
            st.query_map(params![workflow, at, id, limit + 1], run_summary)?.collect::<Result<Vec<_>, _>>()?
        } else {
            let mut st = tx.prepare("SELECT id, number, source, status, started_at, ended_at, error FROM workflow_runs WHERE workflow_id = ?1 ORDER BY started_at DESC, id DESC LIMIT ?2")?;
            st.query_map(params![workflow, limit + 1], run_summary)?.collect::<Result<Vec<_>, _>>()?
        };
        let has_more = rows.len() > take;
        Ok(json!({"runs": rows.into_iter().take(take).collect::<Vec<_>>(), "hasMore": has_more}))
    })
}

fn run_summary(r: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    Ok(
        json!({"id": r.get::<_, String>(0)?, "number": r.get::<_, i64>(1)?, "source": r.get::<_, String>(2)?, "status": r.get::<_, String>(3)?, "startedAt": r.get::<_, i64>(4)?, "endedAt": r.get::<_, Option<i64>>(5)?, "error": r.get::<_, Option<String>>(6)?}),
    )
}

/// One node, outside a full run. An assistant step stays `running` until the bot answers.
pub fn run_solo(hub: &Hub, workflow: &str, node_id: &str, request: &str) -> Result<Value> {
    if let Some(existing) = find_request(&hub.store, workflow, request)? {
        return step_value(hub, &existing);
    }
    let Some((summary, graph)) = store::load(&hub.store, workflow)? else { bail!("This workflow was deleted") };
    if summary["source"].as_str() == Some("script") {
        bail!("This workflow decides its steps when it runs");
    }
    let Some(node) = graph.nodes.iter().find(|n| n.id == node_id).cloned() else {
        bail!("That step is not in this workflow");
    };
    if !matches!(node.kind, NodeKind::Agent | NodeKind::Condition | NodeKind::Approval | NodeKind::Output) {
        bail!("This step can't run on its own");
    }
    let upstream = upstream_text(&hub.store, workflow, &graph, node_id)?;
    let input = match node.kind {
        NodeKind::Agent => upstream.unwrap_or_default(),
        _ => upstream.context_missing()?,
    };
    let title = summary_title(&summary);
    let now = now_ms();
    let step = new_id();
    let attempt = solo_attempt(&hub.store, workflow, node_id)?;
    let bot = field_str(&node.fields, "botId").map(str::trim).filter(|id| !id.is_empty()).map(str::to_owned);
    match node.kind {
        NodeKind::Agent => {
            if bot.as_deref().is_none_or(|id| !matches!(store::bot_state(&hub.store, id), BotState::Ok)) {
                bail!("Choose a bot for this step");
            }
            if field_str(&node.fields, "task").unwrap_or("").trim().is_empty() && input.trim().is_empty() {
                bail!("This step needs instructions");
            }
            hub.store.with_immediate(|tx| {
                insert_step(
                    tx,
                    &step,
                    workflow,
                    None,
                    node_id,
                    attempt,
                    request,
                    "agent",
                    bot.as_deref(),
                    StepStatus::Running,
                    &input,
                    "",
                    None,
                    1,
                    false,
                    None,
                    now,
                )
            })?;
            if let Some(bot) = &bot {
                dispatch_agents(hub, &Effects { agents: vec![(bot.clone(), step.clone())], pushes: Vec::new() });
            }
        }
        NodeKind::Condition => {
            let branch = condition_branch(&node, &input);
            hub.store.with_immediate(|tx| {
                insert_step(
                    tx,
                    &step,
                    workflow,
                    None,
                    node_id,
                    attempt,
                    request,
                    "condition",
                    None,
                    StepStatus::Done,
                    &input,
                    &input,
                    Some(branch.as_str()),
                    1,
                    false,
                    None,
                    now,
                )
            })?;
        }
        NodeKind::Approval => {
            hub.store.with_immediate(|tx| {
                insert_step(
                    tx,
                    &step,
                    workflow,
                    None,
                    node_id,
                    attempt,
                    request,
                    "approval",
                    None,
                    StepStatus::Approval,
                    &input,
                    "",
                    None,
                    1,
                    false,
                    None,
                    now,
                )
            })?;
            if let Some(bot) = lead(&hub.store) {
                let question = field_str(&node.fields, "question").unwrap_or("").trim();
                let body = if question.is_empty() { "Approve this step?" } else { question };
                dispatch_pushes(
                    hub,
                    &Effects {
                        agents: Vec::new(),
                        pushes: vec![Push {
                            bot,
                            title: title.clone(),
                            body: body.to_owned(),
                            kind: AlertKind::NeedsInput,
                            workflow: workflow.to_owned(),
                            run: None,
                            step: step.clone(),
                        }],
                    },
                );
            }
        }
        NodeKind::Output => finish_solo_output(hub, workflow, &title, &node, &step, attempt, request, &input, now)?,
        _ => bail!("This step can't run on its own"),
    }
    hub.emit_workflow(
        json!({"type": "workflow", "change": "step", "workflowId": workflow, "runId": Value::Null, "stepId": step}),
    );
    step_value(hub, &step)
}

fn summary_title(summary: &Value) -> String {
    summary["title"].as_str().unwrap_or("Workflow").to_owned()
}

trait MissingInput {
    fn context_missing(self) -> Result<String>;
}

impl MissingInput for Option<String> {
    fn context_missing(self) -> Result<String> {
        self.ok_or_else(|| anyhow::anyhow!("Run the previous step first"))
    }
}

fn finish_solo_output(
    hub: &Hub,
    workflow: &str,
    title: &str,
    node: &Node,
    step: &str,
    attempt: i64,
    request: &str,
    input: &str,
    now: i64,
) -> Result<()> {
    if OutputTarget::parse(field_str(&node.fields, "target")).is_chat() {
        let bot = field_str(&node.fields, "botId").unwrap_or("").trim();
        if !matches!(store::bot_state(&hub.store, bot), BotState::Ok) {
            bail!("Choose a bot for this step");
        }
        let turn = hub.store.max_turn(bot) + 1;
        let data = json!({"text": clip(input), "workflowId": workflow, "runId": Value::Null, "stepId": step, "notice": "workflow-output"});
        hub.add_entry_once(&format!("workflow-output-{step}"), &Lane::main(bot), EntryKind::Notice, turn, &data)?;
    } else if let Some(bot) = lead(&hub.store) {
        dispatch_pushes(
            hub,
            &Effects {
                agents: Vec::new(),
                pushes: vec![Push {
                    bot,
                    title: title.to_owned(),
                    body: clip(input),
                    kind: AlertKind::Done,
                    workflow: workflow.to_owned(),
                    run: None,
                    step: step.to_owned(),
                }],
            },
        );
    }
    hub.store.with_immediate(|tx| {
        insert_step(
            tx,
            step,
            workflow,
            None,
            &node.id,
            attempt,
            request,
            "output",
            field_str(&node.fields, "botId"),
            StepStatus::Done,
            input,
            input,
            None,
            1,
            false,
            None,
            now,
        )
    })
}

/// Stops one step. An assistant that is still queued or running is cancelled on that bot.
pub fn stop_one(hub: &Hub, step_id: &str) -> Result<()> {
    let Some(row) = step_meta(&hub.store, step_id)? else { bail!("This step was deleted") };
    if !row.status.is_open() {
        return Ok(());
    }
    if row.kind == "agent" && row.status == StepStatus::Running {
        if let Some(bot) = &row.bot {
            if hub.send_cmd(bot, crate::agent::bot::Cmd::WorkflowStop { step_id: step_id.to_owned() }).is_ok() {
                return Ok(());
            }
        }
    }
    let now = now_ms();
    hub.store.with_immediate(|tx| {
        Ok(tx.execute("UPDATE workflow_steps SET status = 'stopped', ended_at = ?2, updated_at = ?2 WHERE id = ?1 AND status IN ('running','approval')", params![step_id, now])?)
    })?;
    if let Some(run) = row.run {
        let effects = advance(hub, &run)?;
        dispatch_agents(hub, &effects);
        dispatch_pushes(hub, &effects);
    }
    Ok(())
}

/// Runs the same node again. A closed run is reopened; a different open run blocks it.
pub fn retry_one(hub: &Hub, step_id: &str) -> Result<Value> {
    let Some(row) = step_meta(&hub.store, step_id)? else { bail!("This step was deleted") };
    if row.status.is_open() {
        bail!("This step is still running");
    }
    let Some(run_id) = row.run.clone() else {
        return run_solo(hub, &row.workflow, &row.node, &new_id());
    };
    if let Some(open) = open_run(&hub.store, &row.workflow)? {
        if open != run_id {
            bail!("A run is already in progress");
        }
        bail!("A run is already in progress");
    }
    let now = now_ms();
    hub.store.with_immediate(|tx| {
        Ok(tx.execute(
            "UPDATE workflow_runs SET status = 'running', error = NULL, ended_at = NULL, updated_at = ?2 WHERE id = ?1",
            params![run_id, now],
        )?)
    })?;
    let Some(run) = load_run(&hub.store, &run_id)? else { bail!("This run was deleted") };
    let graph: Graph = serde_json::from_str(&run.graph)?;
    let mut effects = Effects { agents: Vec::new(), pushes: Vec::new() };
    apply(
        hub,
        &run,
        &graph,
        &Action::Start {
            node_id: row.node.clone(),
            input: row.input.clone(),
            truncated: row.truncated,
            passes: row.passes.saturating_add(1),
        },
        &mut effects,
    )?;
    let more = advance(hub, &run_id)?;
    effects.agents.extend(more.agents);
    effects.pushes.extend(more.pushes);
    dispatch_agents(hub, &effects);
    dispatch_pushes(hub, &effects);
    hub.emit_workflow(json!({"type": "workflow", "change": "run", "workflowId": row.workflow, "runId": run_id}));
    let latest = hub.store.with_immediate(|tx| {
        tx.query_row(
            "SELECT id FROM workflow_steps WHERE run_id = ?1 AND node_id = ?2 ORDER BY attempt DESC, id DESC LIMIT 1",
            params![run_id, row.node],
            |r| r.get::<_, String>(0),
        )
        .map_err(anyhow::Error::from)
    })?;
    step_value(hub, &latest)
}

struct StepMeta {
    workflow: String,
    run: Option<String>,
    node: String,
    kind: String,
    bot: Option<String>,
    status: StepStatus,
    input: String,
    passes: i64,
    truncated: bool,
}

fn step_meta(store: &Store, id: &str) -> Result<Option<StepMeta>> {
    store.with_immediate(|tx| {
        tx.query_row(
            "SELECT workflow_id, run_id, node_id, kind, bot_id, status, input, passes, truncated FROM workflow_steps WHERE id = ?1",
            [id],
            |r| {
                Ok(StepMeta {
                    workflow: r.get(0)?,
                    run: r.get(1)?,
                    node: r.get(2)?,
                    kind: r.get(3)?,
                    bot: r.get(4)?,
                    status: StepStatus::parse(&r.get::<_, String>(5)?).unwrap_or(StepStatus::Failed),
                    input: r.get(6)?,
                    passes: r.get(7)?,
                    truncated: r.get::<_, i64>(8)? == 1,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    })
}

pub fn step_value(hub: &Hub, step: &str) -> Result<Value> {
    let Some(meta) = step_meta(&hub.store, step)? else { bail!("This step was deleted") };
    let graph = match &meta.run {
        Some(run) => match load_run(&hub.store, run)? {
            Some(row) => serde_json::from_str(&row.graph)?,
            None => live_graph(hub, &meta.workflow)?,
        },
        None => live_graph(hub, &meta.workflow)?,
    };
    let row = load_steps_one(&hub.store, step)?.unwrap_or_else(|| StepRow {
        id: step.to_owned(),
        node_id: meta.node,
        attempt: 1,
        request_id: String::new(),
        kind: meta.kind,
        bot: meta.bot,
        status: meta.status,
        input: meta.input,
        output: None,
        branch: None,
        passes: meta.passes,
        truncated: meta.truncated,
        error: None,
        started: None,
        ended: None,
    });
    Ok(json!({"step": step_json(&row, &graph)}))
}

fn live_graph(hub: &Hub, workflow: &str) -> Result<Graph> {
    Ok(store::load(&hub.store, workflow)?.map(|(_, graph)| graph).unwrap_or_else(empty_graph))
}

fn empty_graph() -> Graph {
    Graph { nodes: Vec::new(), edges: Vec::new(), strokes: Vec::new() }
}

fn load_steps_one(store: &Store, id: &str) -> Result<Option<StepRow>> {
    store.with_immediate(|tx| {
        tx.query_row(
            "SELECT id, node_id, attempt, request_id, kind, bot_id, status, input, output, branch, passes, truncated, error, started_at, ended_at FROM workflow_steps WHERE id = ?1",
            [id],
            |r| {
                Ok(StepRow {
                    id: r.get(0)?,
                    node_id: r.get(1)?,
                    attempt: r.get(2)?,
                    request_id: r.get(3)?,
                    kind: r.get(4)?,
                    bot: r.get(5)?,
                    status: StepStatus::parse(&r.get::<_, String>(6)?).unwrap_or(StepStatus::Failed),
                    input: r.get(7)?,
                    output: r.get(8)?,
                    branch: r.get::<_, Option<String>>(9)?.as_deref().and_then(|b| match b {
                        "yes" => Some(Branch::Yes),
                        "no" => Some(Branch::No),
                        _ => None,
                    }),
                    passes: r.get(10)?,
                    truncated: r.get::<_, i64>(11)? == 1,
                    error: r.get(12)?,
                    started: r.get(13)?,
                    ended: r.get(14)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    })
}

fn find_request(store: &Store, workflow: &str, request: &str) -> Result<Option<String>> {
    store.with_immediate(|tx| {
        tx.query_row(
            "SELECT id FROM workflow_steps WHERE workflow_id = ?1 AND request_id = ?2",
            params![workflow, request],
            |r| r.get(0),
        )
        .optional()
        .map_err(Into::into)
    })
}

fn solo_attempt(store: &Store, workflow: &str, node: &str) -> Result<i64> {
    let count: i64 = store.with_immediate(|tx| {
        Ok(tx.query_row("SELECT COALESCE(MAX(attempt), 0) FROM workflow_steps WHERE workflow_id = ?1 AND node_id = ?2 AND run_id IS NULL", params![workflow, node], |r| r.get(0))?)
    })?;
    Ok(count + 1)
}

fn upstream_text(store: &Store, workflow: &str, graph: &Graph, node: &str) -> Result<Option<String>> {
    let incoming: Vec<String> =
        graph.edges.iter().filter(|edge| edge.to == node).map(|edge| edge.from.clone()).collect();
    if incoming.is_empty() {
        return Ok(None);
    }
    let mut parts = Vec::new();
    for from in incoming {
        let output: Option<String> = store.with_immediate(|tx| {
            Ok(tx
                .query_row(
                    "SELECT output FROM workflow_steps WHERE workflow_id = ?1 AND node_id = ?2 AND status = 'done' ORDER BY ended_at DESC, id DESC LIMIT 1",
                    params![workflow, from],
                    |r| r.get::<_, Option<String>>(0),
                )
                .optional()?
                .flatten())
        })?;
        let Some(output) = output else { return Ok(None) };
        parts.push(output);
    }
    Ok(Some(parts.join("\n\n")))
}
