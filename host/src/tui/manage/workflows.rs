//! Workflows on this computer: the list, a step outline, and a run.
//! The host's `Workflows` methods answer the same JSON the other clients use.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::super::app::{App, Confirm, ConfirmAct, Editor, Overlay};
use super::workflow_sync::{
    delete_confirm, detail_snapshot, edit_from_detail, open_status, run_snapshot, selected, selected_id, text,
};
use super::{Reply, WorkflowReply};
use crate::workflows::Graph;

pub struct WorkflowList {
    pub items: Option<Vec<Value>>,
    pub cursor: usize,
}

pub struct WorkflowDetail {
    pub id: String,
    pub body: Option<Value>,
    pub graph: Option<Graph>,
    pub runs: Vec<Value>,
    pub cursor: usize,
}

impl WorkflowDetail {
    pub(in crate::tui) fn pending(id: String) -> Self {
        Self { id, body: None, graph: None, runs: Vec::new(), cursor: 0 }
    }
}

pub struct WorkflowEdit {
    pub id: String,
    pub title: String,
    pub base: i64,
    pub graph: Graph,
    pub source: String,
    #[allow(dead_code)]
    pub script: String,
    #[allow(dead_code)]
    pub phases: Vec<String>,
    pub cursor: usize,
    pub field: usize,
    pub text: Editor,
    pub saving: bool,
    pub dirty: bool,
    /// The editor stays up until its last save lands, then the sheet returns to the detail page.
    pub leaving: bool,
}

pub struct WorkflowAdd {
    pub edit: WorkflowEdit,
    pub cursor: usize,
}

pub struct WorkflowRun {
    pub workflow: String,
    pub run: Option<Value>,
    pub cursor: usize,
    pub output: bool,
}

pub enum WorkflowPage {
    List,
    Detail(Box<WorkflowDetail>),
    Edit(Box<WorkflowEdit>),
    Add(Box<WorkflowAdd>),
    Run(Box<WorkflowRun>),
}

pub struct WorkflowSheet {
    pub list: WorkflowList,
    pub page: WorkflowPage,
    pub notice: Option<String>,
    pub error: Option<String>,
    pub versions: Option<Vec<Value>>,
    pub version_cursor: usize,
    /// Line view indents each step under the one that feeds it. Otherwise sources are labels.
    pub lines: bool,
}

impl WorkflowSheet {
    pub(in crate::tui) fn text_editor(&mut self) -> Option<&mut Editor> {
        let edit = match &mut self.page {
            WorkflowPage::Edit(edit) => edit.as_mut(),
            WorkflowPage::Add(add) => &mut add.edit,
            _ => return None,
        };
        edit.text_field().then_some(&mut edit.text)
    }

    pub(super) fn edit_mut(&mut self) -> Option<&mut WorkflowEdit> {
        match &mut self.page {
            WorkflowPage::Edit(edit) => Some(edit),
            WorkflowPage::Add(add) => Some(&mut add.edit),
            _ => None,
        }
    }
}

impl App {
    pub(in crate::tui) fn open_workflows(&mut self) {
        if self.host_version.is_empty() {
            self.flash("Connecting…");
            return;
        }
        if !self.workflows {
            self.flash("Update Codync on this computer to edit this workflow.");
            return;
        }
        self.sheet("workflows", json!({}), Reply::Workflow(WorkflowReply::List));
        self.overlays.push(Overlay::Workflows(Box::new(WorkflowSheet {
            list: WorkflowList { items: None, cursor: 0 },
            page: WorkflowPage::List,
            notice: None,
            error: None,
            versions: None,
            version_cursor: 0,
            lines: false,
        })));
    }

    /// A `workflow` event is not a protocol error. An open page loads again.
    pub(in crate::tui) fn on_workflow_event(&mut self, v: &Value) {
        if !self.overlays.iter().any(|o| matches!(o, Overlay::Workflows(_))) {
            return;
        }
        let id = v["workflowId"].as_str().unwrap_or("");
        match v["change"].as_str() {
            Some("deleted") => self.workflow_deleted(id),
            Some("graph" | "enabled" | "run" | "step" | "phase") => self.reload_workflows(),
            _ => {}
        }
    }

    pub(in crate::tui) fn reload_workflows(&mut self) {
        if !self.overlays.iter().any(|o| matches!(o, Overlay::Workflows(_))) {
            return;
        }
        self.sheet("workflows", json!({}), Reply::Workflow(WorkflowReply::List));
        let open = self.overlays.iter().find_map(|o| match o {
            Overlay::Workflows(s) => s.open_id(),
            _ => None,
        });
        if let Some((id, run)) = open {
            self.sheet(
                "workflow",
                json!({"id": id, "reveal": false}),
                Reply::Workflow(WorkflowReply::Refresh(id.clone())),
            );
            if let Some(run) = run {
                self.sheet("workflowRun", json!({"runId": run}), Reply::Workflow(WorkflowReply::Run(run)));
            } else {
                self.sheet("workflowRuns", json!({"id": id, "limit": 20}), Reply::Workflow(WorkflowReply::Runs(id)));
            }
        }
    }

    pub(in crate::tui) fn workflows_key(&mut self, sheet: WorkflowSheet, k: KeyEvent) -> Option<Overlay> {
        if sheet.versions.is_some() {
            return self.workflow_versions_key(sheet, k);
        }
        match sheet.page {
            WorkflowPage::List => self.workflow_list_key(sheet, k),
            WorkflowPage::Detail(_) => self.workflow_detail_key(sheet, k),
            WorkflowPage::Edit(_) => Some(self.workflow_edit_key(sheet, k)),
            WorkflowPage::Add(_) => Some(self.workflow_add_key(sheet, k)),
            WorkflowPage::Run(_) => self.workflow_run_key(sheet, k),
        }
    }

    pub(in crate::tui) fn workflow_reply(&mut self, reply: WorkflowReply, v: &Value) {
        match reply {
            WorkflowReply::List => self.workflow_list_loaded(v),
            WorkflowReply::Open { id, edit } => self.workflow_opened(&id, edit, v),
            WorkflowReply::Refresh(id) => self.workflow_refreshed(&id, v),
            WorkflowReply::Runs(id) => self.workflow_runs_loaded(&id, v),
            WorkflowReply::Saved(id) => self.workflow_saved(&id, v),
            WorkflowReply::Started => self.workflow_started(v),
            WorkflowReply::Run(id) => self.workflow_run_loaded(&id, v),
            WorkflowReply::Deleted => self.workflow_deleted(""),
            WorkflowReply::Versions(id) => self.workflow_versions_loaded(&id, v),
        }
    }

    fn workflow_versions_loaded(&mut self, id: &str, v: &Value) {
        let items = v["versions"].as_array().cloned().unwrap_or_default();
        for overlay in &mut self.overlays {
            if let Overlay::Workflows(sheet) = overlay
                && sheet.open_id().is_some_and(|(open, _)| open == id)
            {
                sheet.versions = Some(items);
                sheet.version_cursor = 0;
                return;
            }
        }
    }

    fn workflow_versions_key(&mut self, mut sheet: WorkflowSheet, k: KeyEvent) -> Option<Overlay> {
        let n = sheet.versions.as_ref().map_or(0, Vec::len);
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => sheet.versions = None,
            KeyCode::Char('j') | KeyCode::Down => {
                sheet.version_cursor = (sheet.version_cursor + 1).min(n.saturating_sub(1));
            }
            KeyCode::Char('k') | KeyCode::Up => sheet.version_cursor = sheet.version_cursor.saturating_sub(1),
            KeyCode::Enter => {
                let picked = sheet
                    .versions
                    .as_ref()
                    .and_then(|items| items.get(sheet.version_cursor))
                    .and_then(|item| item["id"].as_str().map(str::to_owned));
                let workflow = sheet.open_id().map(|(id, _)| id);
                if let (Some(version_id), Some(id)) = (picked, workflow) {
                    self.overlays.push(Overlay::Workflows(Box::new(sheet)));
                    self.sheet(
                        "restoreWorkflowVersion",
                        json!({"id": id, "versionId": version_id}),
                        Reply::Workflow(WorkflowReply::Refresh(id)),
                    );
                    return None;
                }
            }
            _ => {}
        }
        Some(Overlay::Workflows(Box::new(sheet)))
    }

    pub(in crate::tui) fn workflow_failed(&mut self, reply: &WorkflowReply, error: &str) {
        let deleted = error == "This workflow was deleted";
        for overlay in &mut self.overlays {
            if let Overlay::Workflows(sheet) = overlay {
                if let Some(edit) = sheet.edit_mut() {
                    edit.saving = false;
                }
                sheet.error = Some(error.to_owned());
                if let WorkflowReply::List = reply
                    && sheet.list.items.is_none()
                {
                    sheet.list.items = Some(Vec::new());
                }
            }
        }
        if deleted {
            self.workflow_deleted("");
        }
    }

    fn workflow_list_key(&mut self, mut sheet: WorkflowSheet, k: KeyEvent) -> Option<Overlay> {
        let n = sheet.list.items.as_ref().map_or(0, Vec::len);
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return None,
            KeyCode::Char('j') | KeyCode::Down => sheet.list.cursor = (sheet.list.cursor + 1).min(n.saturating_sub(1)),
            KeyCode::Char('k') | KeyCode::Up => sheet.list.cursor = sheet.list.cursor.saturating_sub(1),
            KeyCode::Char('n') => self.create_workflow(),
            KeyCode::Enter => {
                if let Some(id) = selected_id(&sheet) {
                    self.fetch_workflow(&id, false);
                }
            }
            KeyCode::Char('e') => {
                if let Some(id) = selected_id(&sheet) {
                    self.fetch_workflow(&id, true);
                }
            }
            KeyCode::Char('r') => {
                if let Some(id) = selected_id(&sheet) {
                    self.run_workflow(&sheet, &id);
                }
            }
            KeyCode::Char('p') => {
                if let Some(item) = selected(&sheet).cloned() {
                    let id = text(&item, "id");
                    let enabled = !item["enabled"].as_bool().unwrap_or(true);
                    self.sheet(
                        "setWorkflowEnabled",
                        json!({"id": id, "enabled": enabled}),
                        Reply::Workflow(WorkflowReply::Refresh(id)),
                    );
                }
            }
            KeyCode::Char('d') => {
                if let Some(id) = selected_id(&sheet) {
                    let title = selected(&sheet).map(|item| text(item, "title")).unwrap_or_default();
                    self.overlays.push(Overlay::Workflows(Box::new(sheet)));
                    self.overlays.push(Overlay::Confirm(delete_confirm(&id, &title)));
                    return None;
                }
            }
            _ => {}
        }
        Some(Overlay::Workflows(Box::new(sheet)))
    }

    fn workflow_detail_key(&mut self, mut sheet: WorkflowSheet, k: KeyEvent) -> Option<Overlay> {
        let Some(snapshot) = detail_snapshot(&sheet) else { return Some(Overlay::Workflows(Box::new(sheet))) };
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => sheet.page = WorkflowPage::List,
            KeyCode::Char('j') | KeyCode::Down => {
                if let WorkflowPage::Detail(d) = &mut sheet.page {
                    d.cursor = (d.cursor + 1).min(snapshot.steps.saturating_sub(1));
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let WorkflowPage::Detail(d) = &mut sheet.page {
                    d.cursor = d.cursor.saturating_sub(1);
                }
            }
            KeyCode::Char('e') => {
                let edit = edit_from_detail(&sheet);
                if let Some(edit) = edit {
                    sheet.page = WorkflowPage::Edit(Box::new(edit));
                }
            }
            KeyCode::Char('r') => self.run_workflow(&sheet, &snapshot.id),
            KeyCode::Char('s') if snapshot.running => self.stop_open(&sheet),
            KeyCode::Char('p') => {
                let id = snapshot.id.clone();
                self.sheet(
                    "setWorkflowEnabled",
                    json!({"id": id, "enabled": !snapshot.enabled}),
                    Reply::Workflow(WorkflowReply::Refresh(id)),
                );
            }
            KeyCode::Char('m') => sheet.lines = !sheet.lines,
            KeyCode::Char('v') => {
                let id = snapshot.id.clone();
                self.sheet("workflowVersions", json!({"id": id}), Reply::Workflow(WorkflowReply::Versions(id)));
            }
            KeyCode::Char('d') => {
                self.overlays.push(Overlay::Workflows(Box::new(sheet)));
                self.overlays.push(Overlay::Confirm(delete_confirm(&snapshot.id, &snapshot.title)));
                return None;
            }
            KeyCode::Enter => {
                if let Some(run) = snapshot.run {
                    sheet.page = WorkflowPage::Run(Box::new(WorkflowRun {
                        workflow: snapshot.id,
                        run: None,
                        cursor: 0,
                        output: false,
                    }));
                    self.sheet("workflowRun", json!({"runId": run}), Reply::Workflow(WorkflowReply::Run(run)));
                }
            }
            _ => {}
        }
        Some(Overlay::Workflows(Box::new(sheet)))
    }

    fn workflow_run_key(&mut self, mut sheet: WorkflowSheet, k: KeyEvent) -> Option<Overlay> {
        let Some(snap) = run_snapshot(&sheet) else { return Some(Overlay::Workflows(Box::new(sheet))) };
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                sheet.page = WorkflowPage::Detail(Box::new(WorkflowDetail::pending(snap.workflow.clone())));
                self.fetch_workflow(&snap.workflow, false);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if let WorkflowPage::Run(run) = &mut sheet.page {
                    run.cursor = (run.cursor + 1).min(snap.steps.saturating_sub(1));
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let WorkflowPage::Run(run) = &mut sheet.page {
                    run.cursor = run.cursor.saturating_sub(1);
                }
            }
            KeyCode::Enter => {
                if let WorkflowPage::Run(run) = &mut sheet.page {
                    run.output = !run.output;
                }
            }
            KeyCode::Char('s') => {
                if let Some(id) = &snap.run_id {
                    self.sheet(
                        "stopWorkflowRun",
                        json!({"runId": id}),
                        Reply::Workflow(WorkflowReply::Run(id.clone())),
                    );
                }
            }
            KeyCode::Char('y') => {
                if let Some(id) = &snap.approval {
                    let run_id = snap.run_id.clone().unwrap_or_default();
                    self.sheet(
                        "decideWorkflowStep",
                        json!({"stepId": id, "approved": true}),
                        Reply::Workflow(WorkflowReply::Run(run_id)),
                    );
                }
            }
            KeyCode::Char('u') => {
                if let Some(id) = snap.approval.clone() {
                    self.overlays.push(Overlay::Workflows(Box::new(sheet)));
                    self.overlays.push(Overlay::Confirm(Confirm {
                        title: "Reject this step?".into(),
                        detail: snap.question,
                        note: String::new(),
                        button: "Reject",
                        act: ConfirmAct::RejectWorkflowStep(id),
                    }));
                    return None;
                }
            }
            KeyCode::Char('r') => {
                if snap.failed
                    && let Some(id) = snap.step_id
                {
                    let run_id = snap.run_id.clone().unwrap_or_default();
                    self.sheet("retryWorkflowStep", json!({"stepId": id}), Reply::Workflow(WorkflowReply::Run(run_id)));
                }
            }
            _ => {}
        }
        Some(Overlay::Workflows(Box::new(sheet)))
    }

    fn create_workflow(&mut self) {
        let Ok(graph) = serde_json::to_value(crate::workflows::default_graph()) else { return };
        self.sheet(
            "createWorkflow",
            json!({"title": "Workflow", "graph": graph}),
            Reply::Workflow(WorkflowReply::Open { id: String::new(), edit: true }),
        );
    }

    pub(in crate::tui) fn fetch_workflow(&mut self, id: &str, edit: bool) {
        self.sheet(
            "workflow",
            json!({"id": id, "reveal": false}),
            Reply::Workflow(WorkflowReply::Open { id: id.to_owned(), edit }),
        );
        if !edit {
            self.sheet(
                "workflowRuns",
                json!({"id": id, "limit": 20}),
                Reply::Workflow(WorkflowReply::Runs(id.to_owned())),
            );
        }
    }

    pub(in crate::tui) fn open_workflow_detail(&mut self, sheet: &WorkflowSheet) {
        if let WorkflowPage::Detail(d) = &sheet.page {
            self.fetch_workflow(&d.id, false);
        }
    }

    fn run_workflow(&mut self, sheet: &WorkflowSheet, id: &str) {
        let running = match &sheet.page {
            WorkflowPage::Detail(d) => d.body.as_ref().is_some_and(|b| open_status(b["running"].as_str())),
            WorkflowPage::List => selected(sheet).is_some_and(|item| open_status(item["running"].as_str())),
            _ => false,
        };
        if running {
            self.flash("A run is already in progress");
            return;
        }
        self.sheet(
            "startWorkflowRun",
            json!({"id": id, "requestId": uuid::Uuid::new_v4().to_string()}),
            Reply::Workflow(WorkflowReply::Started),
        );
    }

    fn stop_open(&mut self, sheet: &WorkflowSheet) {
        let WorkflowPage::Detail(d) = &sheet.page else { return };
        let Some(run) = d.runs.iter().find(|r| open_status(r["status"].as_str())) else { return };
        let Some(id) = run["id"].as_str() else { return };
        self.sheet("stopWorkflowRun", json!({"runId": id}), Reply::Workflow(WorkflowReply::Runs(d.id.clone())));
    }
}
