//! The workflow list, outline, and run. No canvas.

use super::super::app::App;
use super::super::l10n::tr;
use super::super::manage::{
    Lane, WorkflowEdit, WorkflowPage, WorkflowSheet, add_kinds, feeds, field_rows, kind_name, step_rows,
};
use super::super::md::truncate;
use super::{centered, frame_box, put, restyle, rput, theme, when};
use crate::workflows::{EditMode, Graph, Node, NodeKind, edit_mode, field_str};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

pub(in crate::tui::view) fn workflows(buf: &mut Buffer, area: Rect, app: &App, sheet: &WorkflowSheet) {
    match &sheet.page {
        WorkflowPage::List => list(buf, area, sheet),
        WorkflowPage::Detail(detail) => detail_page(buf, area, app, sheet, detail),
        WorkflowPage::Edit(edit) => edit_page(buf, area, app, sheet, edit),
        WorkflowPage::Add(add) => add_page(buf, area, app, sheet, &add.edit, add.cursor),
        WorkflowPage::Run(run) => run_page(buf, area, sheet, run),
    }
}

fn list(buf: &mut Buffer, area: Rect, sheet: &WorkflowSheet) {
    let t = theme();
    let inner = frame(buf, area, &tr("Workflows"));
    match &sheet.list.items {
        None => {
            put(buf, inner.x, inner.y, inner.width, &tr("Loading…"), t.dim.patch(t.panel));
        }
        Some(items) if items.is_empty() => {
            put(buf, inner.x, inner.y, inner.width, &tr("No workflows yet"), t.secondary.patch(t.panel));
            put(buf, inner.x, inner.y + 1, inner.width, &tr("New workflow"), t.text.patch(t.panel));
        }
        Some(items) => {
            let rows = usize::from(inner.height.saturating_sub(3));
            let start = sheet.list.cursor.saturating_sub(rows.saturating_sub(1));
            for (y, (i, item)) in (inner.y..).zip(items.iter().enumerate().skip(start).take(rows)) {
                let base = hilite(buf, inner, y, i == sheet.list.cursor);
                let title = item["title"].as_str().unwrap_or("Workflow");
                put(buf, inner.x, y, inner.width.saturating_sub(16), title, t.bold.patch(base));
                let status = row_status(item);
                if !status.is_empty() {
                    rput(buf, inner.right(), y, &tr(&status), t.secondary.patch(base));
                }
            }
        }
    }
    note(buf, inner, sheet, "j k move · ↵ open · n new · r run · e edit · d delete · p pause · esc");
}

fn detail_page(
    buf: &mut Buffer,
    area: Rect,
    app: &App,
    sheet: &WorkflowSheet,
    detail: &super::super::manage::WorkflowDetail,
) {
    let t = theme();
    let title = detail.body.as_ref().and_then(|b| b["title"].as_str()).unwrap_or("Workflow");
    let inner = frame(buf, area, title);
    let running = detail.body.as_ref().and_then(|b| b["running"].as_str()).is_some_and(open_status);
    let action = if running { tr("Stop") } else { tr("Run now") };
    put(buf, inner.x, inner.y, inner.width / 2, action.as_ref(), t.bold.patch(t.panel));
    put(buf, inner.x + inner.width / 2, inner.y, inner.width / 2, &tr("Edit"), t.text.patch(t.panel));
    let mut y = inner.y + 2;
    if let Some(graph) = &detail.graph {
        let rows = step_rows(graph);
        let room = usize::from(inner.height.saturating_sub(6));
        let start = detail.cursor.saturating_sub(room.saturating_sub(1));
        for (i, row) in rows.iter().enumerate().skip(start).take(room) {
            if y >= inner.bottom().saturating_sub(2) {
                break;
            }
            let Some(node) = graph.nodes.iter().find(|n| n.id == row.id) else { continue };
            let base = hilite(buf, inner, y, i == detail.cursor);
            let name = step_name(app, node);
            put(buf, inner.x, y, 12, &tr(kind_name(node.kind)), t.dim.patch(base));
            put(buf, inner.x + 12, y, inner.width.saturating_sub(12), &name, t.text.patch(base));
            y += 1;
            let summary = step_summary(app, graph, node, row.lane);
            if !summary.is_empty() && y < inner.bottom().saturating_sub(2) {
                put(
                    buf,
                    inner.x + 12,
                    y,
                    inner.width.saturating_sub(12),
                    &truncate(&summary, usize::from(inner.width.saturating_sub(12))),
                    t.secondary.patch(t.panel),
                );
                y += 1;
            }
        }
    } else {
        put(buf, inner.x, y, inner.width, &tr("Loading…"), t.dim.patch(t.panel));
    }
    if let Some(body) = &detail.body {
        let revision = body["revision"].as_i64().unwrap_or(0);
        let saved = body["updatedAt"].as_i64().map(when).unwrap_or_default();
        let line = format!("Version {revision} · Saved {saved}");
        put(buf, inner.x, inner.bottom() - 2, inner.width, &tr(&line), t.dim.patch(t.panel));
    }
    let keys = if running {
        "j k steps · r run · s stop · e edit · d delete · p pause · ↵ run · esc"
    } else {
        "j k steps · r run · e edit · d delete · p pause · ↵ run · esc"
    };
    note(buf, inner, sheet, keys);
}

fn edit_page(buf: &mut Buffer, area: Rect, app: &App, sheet: &WorkflowSheet, edit: &WorkflowEdit) {
    let t = theme();
    let inner = frame(buf, area, &edit.title);
    let mode = edit_mode(&edit.graph, &edit.source);
    let mut y = inner.y;
    if mode == EditMode::Fields {
        let line = tr("This workflow was arranged on a computer. You can edit each step's settings.");
        put(
            buf,
            inner.x,
            y,
            inner.width,
            &truncate(line.as_ref(), usize::from(inner.width)),
            t.secondary.patch(t.panel),
        );
        y += 2;
    } else if mode == EditMode::Script {
        let line = tr("This workflow is a script. You can change its trigger. The steps appear when it runs.");
        put(
            buf,
            inner.x,
            y,
            inner.width,
            &truncate(line.as_ref(), usize::from(inner.width)),
            t.secondary.patch(t.panel),
        );
        y += 1;
        for phase in &edit.phases {
            if y + 1 >= inner.bottom().saturating_sub(2) {
                break;
            }
            put(buf, inner.x, y, inner.width, &truncate(phase, usize::from(inner.width)), t.text.patch(t.panel));
            y += 1;
        }
        y += 1;
    } else if mode == EditMode::ReadOnly {
        let line = tr("Update Codync on this computer to edit this workflow.");
        put(
            buf,
            inner.x,
            y,
            inner.width,
            &truncate(line.as_ref(), usize::from(inner.width)),
            t.secondary.patch(t.panel),
        );
        y += 2;
    }
    let rows = step_rows(&edit.graph);
    let fields = field_rows(app, edit);
    let used = y.saturating_sub(inner.y).saturating_add(4);
    let room = usize::from(inner.height.saturating_sub(used));
    let start = edit.cursor.saturating_sub(room.saturating_sub(1));
    for (i, row) in rows.iter().enumerate().skip(start).take(room.max(1)) {
        if y + 1 >= inner.bottom().saturating_sub(2) {
            break;
        }
        let Some(node) = edit.graph.nodes.iter().find(|n| n.id == row.id) else { continue };
        let sel = i == edit.cursor;
        let base = hilite(buf, inner, y, sel);
        let prefix = lane_prefix(row.lane);
        let name = step_name(app, node);
        put(buf, inner.x, y, inner.width, &format!("{prefix}{name}"), if sel { t.bold } else { t.text }.patch(base));
        y += 1;
        if sel {
            for (f, (label, value)) in fields.iter().enumerate() {
                if y >= inner.bottom().saturating_sub(2) {
                    break;
                }
                let on = f == edit.field;
                put(buf, inner.x + 2, y, 10, &tr(label), if on { t.bold } else { t.dim }.patch(t.panel));
                let shown = if on && edit.text_field() { edit.text.text.as_str() } else { value.as_str() };
                put(
                    buf,
                    inner.x + 12,
                    y,
                    inner.width.saturating_sub(12),
                    &truncate(shown, usize::from(inner.width.saturating_sub(12))),
                    t.text.patch(t.panel),
                );
                y += 1;
            }
            if node.kind == NodeKind::Prompt && !feeds(&edit.graph, &node.id) {
                let line = tr("This prompt doesn't feed a step");
                put(
                    buf,
                    inner.x + 2,
                    y,
                    inner.width.saturating_sub(2),
                    &truncate(line.as_ref(), usize::from(inner.width.saturating_sub(2))),
                    t.secondary.patch(t.panel),
                );
                y += 1;
            }
        }
    }
    let keys = match mode {
        EditMode::Outline => "j k step · space field · [ ] move · a add · x delete · ← → change · esc",
        EditMode::Fields => "j k step · space field · ← → change · esc",
        EditMode::ReadOnly => "j k step · esc",
        EditMode::Script => "space field · esc",
    };
    note(buf, inner, sheet, keys);
}

fn add_page(buf: &mut Buffer, area: Rect, app: &App, sheet: &WorkflowSheet, edit: &WorkflowEdit, cursor: usize) {
    let t = theme();
    let inner = frame(buf, area, &tr("New workflow"));
    let has = crate::workflows::outline_of(&edit.graph).is_some_and(|o| o.condition.is_some());
    let mut y = inner.y;
    for (i, kind) in add_kinds().iter().enumerate() {
        let base = hilite(buf, inner, y, i == cursor);
        put(buf, inner.x, y, inner.width, &tr(kind_name(*kind)), t.text.patch(base));
        if *kind == NodeKind::Condition && has {
            y += 1;
            let line = tr("This workflow already has a condition");
            put(buf, inner.x + 2, y, inner.width.saturating_sub(2), line.as_ref(), t.secondary.patch(t.panel));
        }
        y += 1;
    }
    let _ = app;
    note(buf, inner, sheet, "j k · ↵ add · esc");
}

fn run_page(buf: &mut Buffer, area: Rect, sheet: &WorkflowSheet, page: &super::super::manage::WorkflowRun) {
    let t = theme();
    let number = page.run.as_ref().and_then(|r| r["number"].as_i64()).unwrap_or(0);
    let title = format!("Run {number}");
    let heading = tr(&title);
    let inner = frame(buf, area, heading.as_ref());
    let Some(run) = &page.run else {
        put(buf, inner.x, inner.y, inner.width, &tr("Loading…"), t.dim.patch(t.panel));
        note(buf, inner, sheet, "esc");
        return;
    };
    let status = status_word(run["status"].as_str().unwrap_or(""));
    let sub = match run["status"].as_str() {
        Some("done") => {
            let time = run["endedAt"].as_i64().map(when).unwrap_or_default();
            let dur = match (run["startedAt"].as_i64(), run["endedAt"].as_i64()) {
                (Some(start), Some(end)) => format!("{}s", ((end - start) / 1000).max(0)),
                _ => String::new(),
            };
            format!("{status} · {time} · {dur}")
        }
        _ => status.clone(),
    };
    put(buf, inner.x, inner.y, inner.width, &tr(&sub), t.secondary.patch(t.panel));
    if let Some(progress) = progress(run) {
        rput(buf, inner.right(), inner.y, &progress, t.dim.patch(t.panel));
    }
    let steps = run["steps"].as_array().map_or(&[][..], Vec::as_slice);
    let room = usize::from(inner.height.saturating_sub(4));
    let start = page.cursor.saturating_sub(room.saturating_sub(1));
    let mut y = inner.y + 2;
    for (i, step) in steps.iter().enumerate().skip(start).take(room) {
        if y >= inner.bottom().saturating_sub(1) {
            break;
        }
        let base = hilite(buf, inner, y, i == page.cursor);
        let name = step["title"].as_str().unwrap_or("");
        put(buf, inner.x, y, inner.width.saturating_sub(14), name, t.text.patch(base));
        rput(buf, inner.right(), y, &status_word(step["status"].as_str().unwrap_or("")), t.secondary.patch(base));
        y += 1;
        if i == page.cursor && page.output {
            let body = step["output"].as_str().or(step["error"].as_str()).unwrap_or("");
            if !body.is_empty() && y < inner.bottom().saturating_sub(1) {
                put(
                    buf,
                    inner.x + 2,
                    y,
                    inner.width.saturating_sub(2),
                    &truncate(body, usize::from(inner.width.saturating_sub(2))),
                    t.secondary.patch(t.panel),
                );
                y += 1;
            }
        }
    }
    note(buf, inner, sheet, "j k · ↵ output · y approve · u reject · s stop · r retry · esc");
}

fn frame(buf: &mut Buffer, area: Rect, title: &str) -> Rect {
    let t = theme();
    let r = centered(area, area.width.saturating_sub(4).min(88), area.height.saturating_sub(2).min(32));
    frame_box(buf, r, t.text, t.panel, Some((title, t.text)))
}

fn hilite(buf: &mut Buffer, inner: Rect, y: u16, sel: bool) -> ratatui::style::Style {
    let t = theme();
    if sel {
        restyle(buf, Rect::new(inner.x.saturating_sub(1), y, inner.width.saturating_add(2), 1), t.sel);
        t.sel
    } else {
        t.panel
    }
}

fn note(buf: &mut Buffer, inner: Rect, sheet: &WorkflowSheet, keys: &str) {
    let t = theme();
    let by = inner.bottom() - 1;
    if let Some(text) = sheet.error.as_deref().or(sheet.notice.as_deref()) {
        let shown = tr(text);
        put(
            buf,
            inner.x,
            by.saturating_sub(1),
            inner.width,
            &truncate(shown.as_ref(), usize::from(inner.width)),
            if sheet.error.is_some() { t.red } else { t.secondary }.patch(t.panel),
        );
    }
    put(buf, inner.x, by, inner.width, &tr(keys), t.dim.patch(t.panel));
}

fn row_status(item: &serde_json::Value) -> String {
    let status = item["running"].as_str().filter(|s| !s.is_empty()).or(item["lastRun"]["status"].as_str());
    status.map(status_word).unwrap_or_default()
}

fn status_word(status: &str) -> String {
    match status {
        "running" | "stopping" => "Running",
        "approval" => "Needs you",
        "done" => "Done",
        "failed" => "Failed",
        "stopped" => "Stopped",
        "skipped" => "Skipped",
        _ => "",
    }
    .into()
}

fn open_status(status: &str) -> bool {
    matches!(status, "running" | "approval" | "stopping")
}

fn step_name(app: &App, node: &Node) -> String {
    match node.kind {
        NodeKind::Trigger => match field_str(&node.fields, "trigger") {
            Some("schedule") => tr("Schedule").into_owned(),
            Some("webhook") => tr("Webhook").into_owned(),
            _ => tr("Manual").into_owned(),
        },
        NodeKind::Prompt => first_line(field_str(&node.fields, "text")).unwrap_or_else(|| tr("Prompt").into_owned()),
        NodeKind::Agent => {
            bot_name(app, field_str(&node.fields, "botId")).unwrap_or_else(|| tr("Agent").into_owned())
        }
        NodeKind::Condition => tr("Condition").into_owned(),
        NodeKind::Approval => {
            first_line(field_str(&node.fields, "question")).unwrap_or_else(|| tr("Approval").into_owned())
        }
        NodeKind::Output if field_str(&node.fields, "target") == Some("chat") => {
            bot_name(app, field_str(&node.fields, "botId")).unwrap_or_else(|| tr("Choose a bot").into_owned())
        }
        NodeKind::Output => tr("Notification").into_owned(),
        _ => tr("Note").into_owned(),
    }
}

fn step_summary(app: &App, graph: &Graph, node: &Node, lane: Lane) -> String {
    let lane = match lane {
        Lane::Yes => "Yes",
        Lane::No => "No",
        Lane::After => "After",
        _ => "",
    };
    let body = match node.kind {
        NodeKind::Trigger if field_str(&node.fields, "trigger") == Some("schedule") => next_run(node),
        NodeKind::Trigger => tr("Manual").into_owned(),
        NodeKind::Agent => first_line(field_str(&node.fields, "task")).unwrap_or_else(|| {
            bot_name(app, field_str(&node.fields, "botId")).unwrap_or_else(|| tr("Choose a bot").into_owned())
        }),
        NodeKind::Output if field_str(&node.fields, "target") == Some("chat") => {
            bot_name(app, field_str(&node.fields, "botId")).unwrap_or_else(|| tr("Choose a bot").into_owned())
        }
        NodeKind::Output => tr("Notification").into_owned(),
        NodeKind::Prompt if !feeds(graph, &node.id) => tr("This prompt doesn't feed a step").into_owned(),
        _ => String::new(),
    };
    if lane.is_empty() || body.is_empty() { body } else { format!("{lane} · {body}") }
}

fn bot_name(app: &App, id: Option<&str>) -> Option<String> {
    id.and_then(|id| app.bots.get(id)).map(|b| b.name.clone())
}

fn first_line(text: Option<&str>) -> Option<String> {
    let text = text.map(str::trim).filter(|t| !t.is_empty())?;
    text.lines().next().map(str::to_owned)
}

fn lane_prefix(lane: Lane) -> &'static str {
    match lane {
        Lane::Yes => "Yes · ",
        Lane::No => "No · ",
        Lane::After => "After · ",
        Lane::Trigger | Lane::Main | Lane::Condition | Lane::Loose => "",
    }
}

fn next_run(node: &Node) -> String {
    node.schedule
        .as_ref()
        .and_then(|trigger| trigger.next(crate::store::now_ms()).ok().flatten())
        .map_or_else(|| tr("Schedule").into_owned(), when)
}

fn progress(run: &serde_json::Value) -> Option<String> {
    let steps = run["steps"].as_array()?;
    let graph = run["graph"].as_object();
    let total = graph.and_then(|g| g.get("nodes")).and_then(|n| n.as_array()).map_or(steps.len(), |nodes| {
        nodes.iter().filter(|n| !matches!(n["kind"].as_str(), Some("text" | "note" | "shape"))).count()
    });
    let done = steps
        .iter()
        .filter(|s| matches!(s["status"].as_str(), Some("done" | "failed" | "skipped" | "stopped")))
        .count();
    Some(format!("{done}/{total}"))
}
