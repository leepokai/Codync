//! Outline edits. Lines move through `workflows::outline`; the graph is checked with
//! `workflows::graph::validate` before it is saved.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::json;

use super::super::app::{App, Editor, Overlay};
use super::workflow_outline::{
    cycle_bot, cycle_source, cycle_str, cycle_style, cycle_trigger, delete_step, insert_step, move_step, set_schedule,
    style_of,
};
use super::{WorkflowAdd, WorkflowEdit, WorkflowPage, WorkflowSheet};
use crate::routines::Trigger;
use crate::workflows::{
    BotState, EditMode, Graph, Node, NodeKind, edit_mode, field_str, outline_of, reading_order, validate,
};

pub(super) const ADD: [NodeKind; 5] =
    [NodeKind::Agent, NodeKind::Condition, NodeKind::Approval, NodeKind::Prompt, NodeKind::Output];
const OPS: [&str; 4] = ["contains", "equals", "regex", "empty"];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::tui) enum Lane {
    Trigger,
    Main,
    Condition,
    Yes,
    No,
    After,
    Loose,
}

pub(in crate::tui) struct StepRow {
    pub id: String,
    pub lane: Lane,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Setting {
    Trigger,
    Schedule,
    Text(&'static str),
    Bot,
    Op,
    Target,
    Cron,
    Source,
}

pub(in crate::tui) fn step_rows(graph: &Graph) -> Vec<StepRow> {
    if let Some(outline) = outline_of(graph) {
        let mut rows = vec![StepRow { id: outline.trigger, lane: Lane::Trigger }];
        push(&mut rows, outline.main, Lane::Main);
        if let Some(id) = outline.condition {
            rows.push(StepRow { id, lane: Lane::Condition });
        }
        push(&mut rows, outline.yes, Lane::Yes);
        push(&mut rows, outline.no, Lane::No);
        push(&mut rows, outline.after, Lane::After);
        push(&mut rows, outline.loose, Lane::Loose);
        return rows;
    }
    reading_order(graph).into_iter().map(|id| StepRow { id, lane: Lane::Main }).collect()
}

fn push(rows: &mut Vec<StepRow>, ids: Vec<String>, lane: Lane) {
    rows.extend(ids.into_iter().map(|id| StepRow { id, lane }));
}

impl WorkflowEdit {
    pub(in crate::tui) fn text_field(&self) -> bool {
        self.setting().is_some_and(|s| matches!(s, Setting::Text(_) | Setting::Cron))
    }

    fn setting(&self) -> Option<Setting> {
        let node = self.node()?;
        Self::settings(node).get(self.field).copied()
    }

    pub(super) fn node(&self) -> Option<&Node> {
        let id = step_rows(&self.graph).get(self.cursor)?.id.clone();
        self.graph.nodes.iter().find(|n| n.id == id)
    }

    pub(super) fn settings(node: &Node) -> Vec<Setting> {
        match node.kind {
            NodeKind::Trigger => {
                let mut fields = vec![Setting::Trigger];
                if field_str(&node.fields, "trigger") == Some("schedule") {
                    fields.push(Setting::Schedule);
                    if style_of(node) == "custom" {
                        fields.push(Setting::Cron);
                    }
                }
                fields
            }
            NodeKind::Prompt => vec![Setting::Text("text")],
            NodeKind::Agent => vec![Setting::Source, Setting::Bot, Setting::Text("task")],
            NodeKind::Condition => {
                let mut fields = vec![Setting::Source, Setting::Op];
                if field_str(&node.fields, "op") != Some("empty") {
                    fields.push(Setting::Text("value"));
                }
                fields
            }
            NodeKind::Approval => vec![Setting::Source, Setting::Text("question")],
            NodeKind::Output => {
                let mut fields = vec![Setting::Source, Setting::Target];
                if field_str(&node.fields, "target") == Some("chat") {
                    fields.push(Setting::Bot);
                }
                fields
            }
            _ => Vec::new(),
        }
    }

    pub(super) fn load_text(&mut self) {
        let text = self.node().map(|node| field_text(node, self.setting())).unwrap_or_default();
        if self.text.text != text {
            self.text = Editor::with(&text);
        }
    }
}

fn field_text(node: &Node, setting: Option<Setting>) -> String {
    match setting {
        Some(Setting::Text(key)) => field_str(&node.fields, key).unwrap_or("").to_owned(),
        Some(Setting::Cron) => match &node.schedule {
            Some(Trigger::Cron { expression, .. }) => expression.clone(),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

impl App {
    pub(in crate::tui) fn workflow_edit_key(&mut self, mut sheet: WorkflowSheet, k: KeyEvent) -> Overlay {
        let (mode, text_field) = match &sheet.page {
            WorkflowPage::Edit(edit) => (edit_mode(&edit.graph, &edit.source), edit.text_field()),
            _ => return Overlay::Workflows(Box::new(sheet)),
        };
        if mode == EditMode::ReadOnly {
            return read_only(sheet, k);
        }
        let fields = matches!(mode, EditMode::Fields | EditMode::Script);
        let typing = text_field
            && matches!(
                k.code,
                KeyCode::Left
                    | KeyCode::Right
                    | KeyCode::Char(_)
                    | KeyCode::Backspace
                    | KeyCode::Delete
                    | KeyCode::Home
                    | KeyCode::End
            );
        match k.code {
            KeyCode::Esc => {
                self.finish_edit(&mut sheet);
                return Overlay::Workflows(Box::new(sheet));
            }
            KeyCode::Tab | KeyCode::BackTab => {
                let delta: isize = if k.code == KeyCode::BackTab { -1 } else { 1 };
                self.switch_field(&mut sheet, delta);
            }
            _ if typing => {
                if let WorkflowPage::Edit(edit) = &mut sheet.page {
                    edit.text.key(k);
                    edit.dirty = true;
                }
            }
            KeyCode::Char('j') | KeyCode::Down => self.step_cursor(&mut sheet, 1),
            KeyCode::Char('k') | KeyCode::Up => self.step_cursor(&mut sheet, -1),
            // Space steps through settings. A text field still takes a space; tab moves on.
            KeyCode::Char(' ') if !text_field => self.switch_field(&mut sheet, 1),
            KeyCode::Left | KeyCode::Right => {
                let delta: isize = if k.code == KeyCode::Left { -1 } else { 1 };
                self.cycle_setting(&mut sheet, delta);
            }
            KeyCode::Char('[') if !fields => self.shift_step(&mut sheet, -1),
            KeyCode::Char(']') if !fields => self.shift_step(&mut sheet, 1),
            KeyCode::Char('a') if !fields => self.begin_add(&mut sheet),
            KeyCode::Char('x') if !fields => self.remove_step(&mut sheet),
            KeyCode::Char('q') if !text_field => {
                self.finish_edit(&mut sheet);
                return Overlay::Workflows(Box::new(sheet));
            }
            _ => {}
        }
        Overlay::Workflows(Box::new(sheet))
    }

    pub(in crate::tui) fn workflow_add_key(&mut self, mut sheet: WorkflowSheet, k: KeyEvent) -> Overlay {
        if !matches!(sheet.page, WorkflowPage::Add(_)) {
            return Overlay::Workflows(Box::new(sheet));
        }
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => close_add(&mut sheet),
            KeyCode::Char('j') | KeyCode::Down => nudge_add(&mut sheet, 1),
            KeyCode::Char('k') | KeyCode::Up => nudge_add(&mut sheet, -1),
            KeyCode::Enter => self.confirm_add(&mut sheet),
            _ => {}
        }
        Overlay::Workflows(Box::new(sheet))
    }

    fn confirm_add(&mut self, sheet: &mut WorkflowSheet) {
        let (kind, has_condition, after) = {
            let WorkflowPage::Add(add) = &sheet.page else { return };
            let has = outline_of(&add.edit.graph).is_some_and(|outline| outline.condition.is_some());
            (ADD[add.cursor], has, current_id(&add.edit))
        };
        if kind == NodeKind::Condition && has_condition {
            sheet.error = Some("This workflow already has a condition".into());
            return;
        }
        let Some(id) = after else { return };
        let mut next = match &sheet.page {
            WorkflowPage::Add(add) => add.edit.graph.clone(),
            _ => return,
        };
        if let Err(error) = insert_step(&mut next, &id, kind) {
            sheet.error = Some(error);
            return;
        }
        if let Err(error) = self.checked(&next) {
            sheet.error = Some(error);
            return;
        }
        let placed = blank_id(&next, kind);
        if let WorkflowPage::Add(add) = &mut sheet.page {
            add.edit.graph = next;
            add.edit.dirty = true;
            place(&mut add.edit, &placed);
            self.persist_edit(&mut add.edit);
        }
        close_add(sheet);
    }

    fn begin_add(&mut self, sheet: &mut WorkflowSheet) {
        let committed = match &mut sheet.page {
            WorkflowPage::Edit(edit) => self.commit_edit(edit),
            _ => return,
        };
        if let Err(error) = committed {
            sheet.error = Some(error);
            return;
        }
        let WorkflowPage::Edit(edit) = std::mem::replace(&mut sheet.page, WorkflowPage::List) else { return };
        sheet.page = WorkflowPage::Add(Box::new(WorkflowAdd { edit: *edit, cursor: 0 }));
    }

    fn finish_edit(&mut self, sheet: &mut WorkflowSheet) {
        let committed = match &mut sheet.page {
            WorkflowPage::Edit(edit) => self.commit_edit(edit),
            _ => return,
        };
        if let Err(error) = committed {
            sheet.error = Some(error);
            return;
        }
        if let WorkflowPage::Edit(edit) = &mut sheet.page
            && (edit.dirty || edit.saving)
        {
            edit.leaving = true;
            self.persist_edit(edit);
            return;
        }
        let id = match &sheet.page {
            WorkflowPage::Edit(edit) => edit.id.clone(),
            _ => return,
        };
        sheet.page = WorkflowPage::Detail(Box::new(super::WorkflowDetail::pending(id)));
        self.open_workflow_detail(sheet);
    }

    fn step_cursor(&mut self, sheet: &mut WorkflowSheet, delta: isize) {
        let WorkflowPage::Edit(edit) = &mut sheet.page else { return };
        if let Err(error) = self.commit_edit(edit) {
            sheet.error = Some(error);
            return;
        }
        let n = step_rows(&edit.graph).len();
        if n == 0 {
            return;
        }
        let dirty = edit.dirty;
        edit.cursor = edit.cursor.saturating_add_signed(delta).min(n - 1);
        edit.field = 0;
        edit.load_text();
        if dirty {
            self.persist_edit(edit);
        }
    }

    fn switch_field(&mut self, sheet: &mut WorkflowSheet, delta: isize) {
        let WorkflowPage::Edit(edit) = &mut sheet.page else { return };
        if let Err(error) = self.commit_edit(edit) {
            sheet.error = Some(error);
            return;
        }
        let n = edit.node().map_or(0, |node| WorkflowEdit::settings(node).len());
        if n == 0 {
            return;
        }
        edit.field = edit.field.saturating_add_signed(delta).min(n - 1);
        edit.load_text();
        if edit.dirty {
            self.persist_edit(edit);
        }
    }

    fn cycle_setting(&mut self, sheet: &mut WorkflowSheet, delta: isize) {
        let WorkflowPage::Edit(edit) = &mut sheet.page else { return };
        let Some(id) = current_id(edit) else { return };
        let setting = edit.setting();
        if setting == Some(Setting::Source) {
            let mut next = edit.graph.clone();
            if !cycle_source(&mut next, &id, delta) {
                return;
            }
            if let Err(error) = self.checked(&next) {
                sheet.error = Some(error);
                return;
            }
            edit.graph = next;
            edit.dirty = true;
            sheet.error = None;
            self.persist_edit(edit);
            return;
        }
        let field = edit.field;
        let bots = self.bot_ids();
        let snapshot = edit.graph.clone();
        let changed = {
            let Some(node) = edit.graph.nodes.iter_mut().find(|n| n.id == id) else { return };
            match setting {
                Some(Setting::Trigger) => cycle_trigger(node, delta),
                Some(Setting::Schedule) => cycle_style(node, delta),
                Some(Setting::Op) => cycle_str(node, "op", &OPS, delta),
                Some(Setting::Target) => cycle_str(node, "target", &["notify", "chat"], delta),
                Some(Setting::Bot) => cycle_bot(node, &bots, delta),
                _ => false,
            }
        };
        if !changed {
            return;
        }
        if let Err(error) = self.checked(&edit.graph) {
            edit.graph = snapshot;
            sheet.error = Some(error);
            return;
        }
        let count = edit.node().map_or(1, |node| WorkflowEdit::settings(node).len());
        edit.field = field.min(count.saturating_sub(1));
        edit.dirty = true;
        edit.load_text();
        sheet.error = None;
        self.persist_edit(edit);
    }

    fn shift_step(&mut self, sheet: &mut WorkflowSheet, delta: isize) {
        let WorkflowPage::Edit(edit) = &mut sheet.page else { return };
        if let Err(error) = self.commit_edit(edit) {
            sheet.error = Some(error);
            return;
        }
        let Some(id) = current_id(edit) else { return };
        let mut next = edit.graph.clone();
        if !move_step(&mut next, &id, delta) {
            return;
        }
        if let Err(error) = self.checked(&next) {
            sheet.error = Some(error);
            return;
        }
        edit.graph = next;
        edit.dirty = true;
        place(edit, &id);
        self.persist_edit(edit);
    }

    fn remove_step(&mut self, sheet: &mut WorkflowSheet) {
        let WorkflowPage::Edit(edit) = &mut sheet.page else { return };
        let Some(id) = current_id(edit) else { return };
        let mut next = edit.graph.clone();
        if !delete_step(&mut next, &id) {
            return;
        }
        if let Err(error) = self.checked(&next) {
            sheet.error = Some(error);
            return;
        }
        edit.graph = next;
        edit.cursor = edit.cursor.min(step_rows(&edit.graph).len().saturating_sub(1));
        edit.field = 0;
        edit.dirty = true;
        edit.load_text();
        self.persist_edit(edit);
    }

    fn commit_edit(&self, edit: &mut WorkflowEdit) -> Result<(), String> {
        write_text(edit)?;
        self.checked(&edit.graph)?;
        Ok(())
    }

    fn checked(&self, graph: &Graph) -> Result<(), String> {
        let bots = &self.bots;
        validate(graph, &|id| match bots.get(id) {
            Some(bot) if bot.group => BotState::Group,
            Some(_) => BotState::Ok,
            None => BotState::Missing,
        })
        .map_err(|error| error.to_string())
    }

    pub(in crate::tui) fn persist_edit(&mut self, edit: &mut WorkflowEdit) {
        if edit.saving {
            edit.dirty = true;
            return;
        }
        let Ok(graph) = serde_json::to_value(&edit.graph) else { return };
        edit.saving = true;
        edit.dirty = false;
        self.sheet(
            "saveWorkflowGraph",
            json!({"id": edit.id, "baseRevision": edit.base, "graph": graph}),
            super::Reply::Workflow(super::WorkflowReply::Saved(edit.id.clone())),
        );
    }
}

fn read_only(mut sheet: WorkflowSheet, k: KeyEvent) -> Overlay {
    match k.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            let id = match &sheet.page {
                WorkflowPage::Edit(edit) => edit.id.clone(),
                _ => return Overlay::Workflows(Box::new(sheet)),
            };
            sheet.page = WorkflowPage::Detail(Box::new(super::WorkflowDetail::pending(id)));
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if let WorkflowPage::Edit(edit) = &mut sheet.page {
                let n = step_rows(&edit.graph).len();
                edit.cursor = (edit.cursor + 1).min(n.saturating_sub(1));
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if let WorkflowPage::Edit(edit) = &mut sheet.page {
                edit.cursor = edit.cursor.saturating_sub(1);
            }
        }
        _ => {}
    }
    Overlay::Workflows(Box::new(sheet))
}

fn nudge_add(sheet: &mut WorkflowSheet, delta: isize) {
    let WorkflowPage::Add(add) = &mut sheet.page else { return };
    if delta > 0 {
        add.cursor = (add.cursor + 1).min(ADD.len() - 1);
    } else {
        add.cursor = add.cursor.saturating_sub(1);
    }
}

fn close_add(sheet: &mut WorkflowSheet) {
    if let WorkflowPage::Add(add) = std::mem::replace(&mut sheet.page, WorkflowPage::List) {
        sheet.page = WorkflowPage::Edit(Box::new(add.edit));
    }
}

fn current_id(edit: &WorkflowEdit) -> Option<String> {
    step_rows(&edit.graph).get(edit.cursor).map(|row| row.id.clone())
}

fn place(edit: &mut WorkflowEdit, id: &str) {
    if let Some(index) = step_rows(&edit.graph).iter().position(|row| row.id == id) {
        edit.cursor = index;
    }
    edit.field = 0;
    edit.load_text();
}

/// The id just inserted: the last node, which `insert_step` appends.
fn blank_id(graph: &Graph, kind: NodeKind) -> String {
    graph.nodes.iter().rev().find(|n| n.kind == kind).map(|n| n.id.clone()).unwrap_or_default()
}

fn write_text(edit: &mut WorkflowEdit) -> Result<(), String> {
    let Some(id) = current_id(edit) else { return Ok(()) };
    let setting = edit.setting();
    let text = edit.text.text.clone();
    let Some(node) = edit.graph.nodes.iter_mut().find(|n| n.id == id) else { return Ok(()) };
    match setting {
        Some(Setting::Text(key)) => {
            node.fields.insert(key.into(), json!(text));
        }
        Some(Setting::Cron) => set_schedule(node, "custom", &text)?,
        _ => {}
    }
    Ok(())
}
