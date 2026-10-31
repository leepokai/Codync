//! Replies that fill the workflow list, the open page, and a run.

use serde_json::{Value, json};

use super::super::app::{App, Confirm, ConfirmAct, Editor, Overlay};
use super::workflow_edit::step_rows;
use super::{Reply, WorkflowDetail, WorkflowEdit, WorkflowPage, WorkflowReply, WorkflowRun, WorkflowSheet};
use crate::workflows::{EditMode, Graph, edit_mode};

impl App {
    pub(super) fn workflow_list_loaded(&mut self, v: &Value) {
        let Some(mut items) = v.get("workflows").and_then(Value::as_array).cloned() else { return };
        items.sort_by(|a, b| {
            open_status(b["running"].as_str())
                .cmp(&open_status(a["running"].as_str()))
                .then(b["updatedAt"].as_i64().unwrap_or(0).cmp(&a["updatedAt"].as_i64().unwrap_or(0)))
        });
        for overlay in &mut self.overlays {
            if let Overlay::Workflows(sheet) = overlay {
                sheet.list.cursor = sheet.list.cursor.min(items.len().saturating_sub(1));
                sheet.list.items = Some(items);
                return;
            }
        }
    }

    pub(super) fn workflow_opened(&mut self, id: &str, edit: bool, v: &Value) {
        let Some(body) = v.get("workflow").cloned() else { return };
        let opened = body["id"].as_str().unwrap_or(id).to_owned();
        if !id.is_empty() && opened != id {
            return;
        }
        let Ok(graph) = serde_json::from_value::<Graph>(body["graph"].clone()) else { return };
        for overlay in &mut self.overlays {
            let Overlay::Workflows(sheet) = overlay else { continue };
            if edit {
                let mut page = WorkflowEdit {
                    id: opened,
                    title: text(&body, "title"),
                    base: body["revision"].as_i64().unwrap_or(1),
                    graph,
                    source: text(&body, "source"),
                    script: text(&body, "script"),
                    phases: body["phases"]
                        .as_array()
                        .map(|items| items.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
                        .unwrap_or_default(),
                    cursor: 0,
                    field: 0,
                    text: Editor::default(),
                    saving: false,
                    dirty: false,
                };
                page.load_text();
                sheet.page = WorkflowPage::Edit(Box::new(page));
            } else {
                let runs = match &sheet.page {
                    WorkflowPage::Detail(d) if d.id == opened => d.runs.clone(),
                    _ => Vec::new(),
                };
                sheet.page = WorkflowPage::Detail(Box::new(WorkflowDetail {
                    id: opened,
                    body: Some(body),
                    graph: Some(graph),
                    runs,
                    cursor: 0,
                }));
            }
            return;
        }
    }

    pub(super) fn workflow_refreshed(&mut self, id: &str, v: &Value) {
        let Some(body) = v.get("workflow").cloned() else { return };
        if body["graph"].is_null() {
            self.patch_summary(&body);
            return;
        }
        let Ok(graph) = serde_json::from_value::<Graph>(body["graph"].clone()) else { return };
        for overlay in &mut self.overlays {
            let Overlay::Workflows(sheet) = overlay else { continue };
            if sheet.unsaved() {
                return;
            }
            if let WorkflowPage::Detail(d) = &mut sheet.page
                && d.id == id
            {
                d.body = Some(body);
                d.graph = Some(graph);
            } else if let Some(edit) = sheet.edit_mut()
                && edit.id == id
            {
                edit.base = body["revision"].as_i64().unwrap_or(edit.base);
                edit.graph = graph;
                edit.load_text();
            }
            return;
        }
    }

    fn patch_summary(&mut self, summary: &Value) {
        let Some(id) = summary["id"].as_str() else { return };
        for overlay in &mut self.overlays {
            let Overlay::Workflows(sheet) = overlay else { continue };
            if let Some(items) = &mut sheet.list.items
                && let Some(row) = items.iter_mut().find(|row| row["id"].as_str() == Some(id))
            {
                *row = summary.clone();
            }
            if let WorkflowPage::Detail(d) = &mut sheet.page
                && d.id == id
                && let Some(body) = &mut d.body
            {
                for (key, value) in summary.as_object().into_iter().flatten() {
                    if key != "graph" {
                        body[key] = value.clone();
                    }
                }
            }
        }
    }

    pub(super) fn workflow_runs_loaded(&mut self, id: &str, v: &Value) {
        let runs = v["runs"].as_array().cloned().unwrap_or_default();
        for overlay in &mut self.overlays {
            if let Overlay::Workflows(sheet) = overlay
                && let WorkflowPage::Detail(d) = &mut sheet.page
                && d.id == id
            {
                d.runs = runs;
                return;
            }
        }
    }

    pub(super) fn workflow_saved(&mut self, id: &str, v: &Value) {
        let Some(index) = self.overlays.iter().position(|o| matches!(o, Overlay::Workflows(_))) else { return };
        let Overlay::Workflows(mut sheet) = self.overlays.remove(index) else { return };
        let again = apply_save(sheet.as_mut(), id, v);
        let follow =
            again.then(|| sheet.edit_mut().map(|edit| (edit.id.clone(), edit.base, edit.graph.clone()))).flatten();
        if let Some(edit) = sheet.edit_mut()
            && follow.is_some()
        {
            edit.saving = true;
            edit.dirty = false;
        }
        self.overlays.insert(index, Overlay::Workflows(sheet));
        if let Some((wid, base, graph)) = follow
            && let Ok(graph) = serde_json::to_value(&graph)
        {
            self.sheet(
                "saveWorkflowGraph",
                json!({"id": wid, "baseRevision": base, "graph": graph}),
                Reply::Workflow(WorkflowReply::Saved(wid)),
            );
        }
    }

    pub(super) fn workflow_started(&mut self, v: &Value) {
        let Some(run) = v.get("run").cloned() else { return };
        let workflow = text(&run, "workflowId");
        for overlay in &mut self.overlays {
            if let Overlay::Workflows(sheet) = overlay {
                sheet.page =
                    WorkflowPage::Run(Box::new(WorkflowRun { workflow, run: Some(run), cursor: 0, output: false }));
                return;
            }
        }
    }

    pub(super) fn workflow_run_loaded(&mut self, id: &str, v: &Value) {
        let Some(run) = v.get("run").cloned().or_else(|| v.get("step").map(|_| v.clone())) else { return };
        for overlay in &mut self.overlays {
            if let Overlay::Workflows(sheet) = overlay
                && let WorkflowPage::Run(page) = &mut sheet.page
                && (page.run.as_ref().is_none_or(|r| r["id"].as_str() == Some(id)) || text(&run, "id") == id)
            {
                if run.get("steps").is_some() {
                    page.run = Some(run);
                }
                return;
            }
        }
    }

    pub(super) fn workflow_deleted(&mut self, id: &str) {
        for overlay in &mut self.overlays {
            let Overlay::Workflows(sheet) = overlay else { continue };
            let watching = id.is_empty()
                || sheet.open_id().is_some_and(|(open, _)| open == id)
                || selected_id(sheet).as_deref() == Some(id);
            if watching {
                sheet.page = WorkflowPage::List;
                sheet.notice = Some("This workflow was deleted".into());
            }
            if let Some(items) = &mut sheet.list.items {
                items.retain(|row| row["id"].as_str() != Some(id) || id.is_empty());
                if id.is_empty() {
                    // The reply didn't name it; the next list replaces these rows.
                } else {
                    sheet.list.cursor = sheet.list.cursor.min(items.len().saturating_sub(1));
                }
            }
        }
        self.sheet("workflows", json!({}), Reply::Workflow(WorkflowReply::List));
    }
}

impl WorkflowSheet {
    pub(super) fn edit_ref(&self) -> Option<&WorkflowEdit> {
        match &self.page {
            WorkflowPage::Edit(edit) => Some(edit),
            WorkflowPage::Add(add) => Some(&add.edit),
            _ => None,
        }
    }

    pub(super) fn unsaved(&self) -> bool {
        self.edit_ref().is_some_and(|e| e.dirty || e.saving)
    }

    pub(super) fn open_id(&self) -> Option<(String, Option<String>)> {
        match &self.page {
            WorkflowPage::Detail(d) => Some((d.id.clone(), None)),
            WorkflowPage::Edit(e) => Some((e.id.clone(), None)),
            WorkflowPage::Add(a) => Some((a.edit.id.clone(), None)),
            WorkflowPage::Run(r) => {
                Some((r.workflow.clone(), r.run.as_ref().and_then(|run| run["id"].as_str().map(str::to_owned))))
            }
            WorkflowPage::List => None,
        }
    }
}

pub(super) struct DetailSnap {
    pub id: String,
    pub title: String,
    pub steps: usize,
    pub running: bool,
    pub enabled: bool,
    pub run: Option<String>,
}

pub(super) fn detail_snapshot(sheet: &WorkflowSheet) -> Option<DetailSnap> {
    let WorkflowPage::Detail(d) = &sheet.page else { return None };
    Some(DetailSnap {
        id: d.id.clone(),
        title: d.body.as_ref().map(|b| text(b, "title")).unwrap_or_default(),
        steps: d.graph.as_ref().map_or(0, |g| step_rows(g).len()),
        running: d.body.as_ref().is_some_and(|b| open_status(b["running"].as_str())),
        enabled: d.body.as_ref().is_none_or(|b| b["enabled"].as_bool().unwrap_or(true)),
        run: d.runs.first().and_then(|r| r["id"].as_str().map(str::to_owned)),
    })
}

pub(super) struct RunSnap {
    pub workflow: String,
    pub steps: usize,
    pub run_id: Option<String>,
    pub step_id: Option<String>,
    pub approval: Option<String>,
    pub failed: bool,
    pub question: String,
}

pub(super) fn run_snapshot(sheet: &WorkflowSheet) -> Option<RunSnap> {
    let WorkflowPage::Run(page) = &sheet.page else { return None };
    let steps = page.run.as_ref().and_then(|r| r["steps"].as_array());
    let current = steps.and_then(|s| s.get(page.cursor));
    let approval = if current.is_some_and(|s| s["status"] == "approval") {
        current.and_then(|s| s["id"].as_str().map(str::to_owned))
    } else {
        steps
            .and_then(|s| s.iter().find(|step| step["status"] == "approval"))
            .and_then(|s| s["id"].as_str().map(str::to_owned))
    };
    Some(RunSnap {
        workflow: page.workflow.clone(),
        steps: steps.map_or(0, Vec::len),
        run_id: page.run.as_ref().and_then(|r| r["id"].as_str().map(str::to_owned)),
        step_id: current.and_then(|s| s["id"].as_str().map(str::to_owned)),
        approval,
        failed: current.is_some_and(|s| s["status"] == "failed"),
        question: current.and_then(|s| s["title"].as_str()).unwrap_or("Approve this step?").to_owned(),
    })
}

fn apply_save(sheet: &mut WorkflowSheet, id: &str, v: &Value) -> bool {
    let Some(edit) = sheet.edit_mut() else {
        if v["conflict"].as_bool().unwrap_or(false) {
            sheet.notice = Some("This workflow was edited somewhere else".into());
        }
        return false;
    };
    if edit.id != id {
        return false;
    }
    edit.saving = false;
    if v["conflict"].as_bool().unwrap_or(false) {
        if let Some(body) = v.get("workflow")
            && let Ok(graph) = serde_json::from_value::<Graph>(body["graph"].clone())
        {
            edit.graph = graph;
            edit.base = body["revision"].as_i64().unwrap_or(edit.base);
            edit.cursor = 0;
            edit.field = 0;
            edit.dirty = false;
            edit.load_text();
        }
        sheet.notice = Some("This workflow was edited somewhere else".into());
        sheet.error = None;
        return false;
    }
    if v["ok"].as_bool().unwrap_or(false) {
        edit.base = v["revision"].as_i64().unwrap_or(edit.base);
        if edit.dirty {
            edit.dirty = false;
            return true;
        }
    }
    false
}

pub(super) fn edit_from_detail(sheet: &WorkflowSheet) -> Option<WorkflowEdit> {
    let WorkflowPage::Detail(d) = &sheet.page else { return None };
    let graph = d.graph.clone()?;
    let source = d.body.as_ref().map(|b| text(b, "source")).unwrap_or_default();
    if edit_mode(&graph, &source) == EditMode::ReadOnly {
        return None;
    }
    let mut edit = WorkflowEdit {
        id: d.id.clone(),
        title: d.body.as_ref().map_or_else(|| "Workflow".into(), |b| text(b, "title")),
        base: d.body.as_ref().and_then(|b| b["revision"].as_i64()).unwrap_or(1),
        graph,
        source,
        script: d.body.as_ref().map(|b| text(b, "script")).unwrap_or_default(),
        phases: d
            .body
            .as_ref()
            .and_then(|b| b["phases"].as_array())
            .map(|items| items.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
            .unwrap_or_default(),
        cursor: 0,
        field: 0,
        text: Editor::default(),
        saving: false,
        dirty: false,
    };
    edit.load_text();
    Some(edit)
}

pub(super) fn selected(sheet: &WorkflowSheet) -> Option<&Value> {
    sheet.list.items.as_ref()?.get(sheet.list.cursor)
}

pub(super) fn selected_id(sheet: &WorkflowSheet) -> Option<String> {
    selected(sheet).and_then(|item| item["id"].as_str().map(str::to_owned))
}

pub(super) fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or("").to_owned()
}

pub(super) fn open_status(status: Option<&str>) -> bool {
    matches!(status, Some("running" | "approval" | "stopping"))
}

pub(super) fn delete_confirm(id: &str, title: &str) -> Confirm {
    Confirm {
        title: format!("Delete {title}?"),
        detail: "Delete workflow".into(),
        note: String::new(),
        button: "Delete workflow",
        act: ConfirmAct::DeleteWorkflow(id.to_owned()),
    }
}
