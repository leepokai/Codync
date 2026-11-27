//! Step settings and outline changes. Edges come from `graph_from_outline`.

use serde_json::{Map, json};

use super::super::app::App;
use super::WorkflowEdit;
use super::workflow_edit::{ADD, Setting};
use crate::routines::{ScheduleDraft, Trigger, local_zone, schedule_preview};
use crate::workflows::{
    Graph, Node, NodeKind, Outline, Point, SourceTarget, TriggerKind, field_str, graph_from_outline, new_id,
    outline_of, reading_order, source_choices, source_target, with_source,
};

const STYLES: [&str; 5] = ["hourly", "daily", "weekdays", "weekly", "custom"];
const TRIGGERS: [TriggerKind; 3] = [TriggerKind::Manual, TriggerKind::Schedule, TriggerKind::Webhook];

pub(super) fn cycle_trigger(node: &mut Node, delta: isize) -> bool {
    let current = field_str(&node.fields, "trigger").and_then(TriggerKind::parse).unwrap_or(TriggerKind::Manual);
    let index = TRIGGERS.iter().position(|k| *k == current).unwrap_or(0);
    let next = TRIGGERS[cycle(index, delta, TRIGGERS.len())];
    if next == current {
        return false;
    }
    match next {
        TriggerKind::Schedule => set_schedule(node, "daily", "").is_ok(),
        TriggerKind::Manual | TriggerKind::Webhook => {
            node.schedule = None;
            node.fields.insert("trigger".into(), json!(next.as_str()));
            node.fields.remove("timezone");
            true
        }
    }
}

pub(super) fn cycle_style(node: &mut Node, delta: isize) -> bool {
    let current = style_of(node);
    let index = STYLES.iter().position(|s| *s == current).unwrap_or(0);
    let next = STYLES[cycle(index, delta, STYLES.len())];
    let expression = match &node.schedule {
        Some(Trigger::Cron { expression, .. }) => expression.clone(),
        _ => String::new(),
    };
    set_schedule(node, next, &expression).is_ok()
}

pub(super) fn cycle_str(node: &mut Node, key: &str, values: &[&str], delta: isize) -> bool {
    let current = field_str(&node.fields, key).unwrap_or(values[0]);
    let index = values.iter().position(|v| *v == current).unwrap_or(0);
    let next = values[cycle(index, delta, values.len())];
    node.fields.insert(key.into(), json!(next));
    true
}

pub(super) fn cycle_bot(node: &mut Node, bots: &[String], delta: isize) -> bool {
    if bots.is_empty() {
        node.fields.remove("botId");
        return false;
    }
    let current = field_str(&node.fields, "botId").unwrap_or("");
    let index = bots.iter().position(|id| id == current).unwrap_or(0);
    let next = &bots[cycle(index, delta, bots.len())];
    if next.is_empty() {
        node.fields.remove("botId");
    } else {
        node.fields.insert("botId".into(), json!(next));
    }
    true
}

fn cycle(index: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let len = i64::try_from(len).unwrap_or(1);
    let index = i64::try_from(index).unwrap_or(0);
    let delta = i64::from(delta > 0) - i64::from(delta < 0);
    usize::try_from((index + delta).rem_euclid(len)).unwrap_or(0)
}

pub(super) fn set_schedule(node: &mut Node, style: &str, expression: &str) -> Result<(), String> {
    let zone = local_zone();
    let expression = if style == "custom" { expression.to_owned() } else { "0 9 * * *".into() };
    let draft =
        ScheduleDraft { calendar_style: style.to_owned(), zone: zone.clone(), expression, ..ScheduleDraft::default() };
    let triggers = draft.triggers().map_err(|error| error.to_string())?;
    let Some(Trigger::Cron { expression, time_zone }) = triggers.into_iter().next() else {
        return Err("Pick when this workflow runs".into());
    };
    let cron = Trigger::Cron { expression, time_zone };
    cron.validate(crate::store::now_ms()).map_err(|error| error.to_string())?;
    node.schedule = Some(cron);
    node.fields.insert("trigger".into(), json!("schedule"));
    node.fields.insert("timezone".into(), json!(zone));
    Ok(())
}

pub(super) fn style_of(node: &Node) -> String {
    let Some(schedule) = &node.schedule else { return "daily".into() };
    let zone = field_str(&node.fields, "timezone").unwrap_or("UTC");
    schedule_preview(&json!({"triggers": [schedule], "timeZone": zone}), crate::store::now_ms())
        .ok()
        .and_then(|v| v["draft"]["calendarStyle"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "custom".into())
}

impl App {
    pub(super) fn bot_ids(&self) -> Vec<String> {
        let mut ids = vec![String::new()];
        let mut bots: Vec<_> = self.bots.values().filter(|b| !b.group && !b.hidden).map(|b| b.id.clone()).collect();
        bots.sort();
        ids.extend(bots);
        ids
    }
}

pub(super) fn move_step(graph: &mut Graph, id: &str, delta: isize) -> bool {
    let Some(mut outline) = outline_of(graph) else { return false };
    let Some(index) = column_of(&outline, id) else { return false };
    let list = column_mut(&mut outline, index);
    let Some(at) = list.iter().position(|item| item == id) else { return false };
    let next = at.saturating_add_signed(delta);
    if next >= list.len() || next == at {
        return false;
    }
    list.swap(at, next);
    *graph = graph_from_outline(graph, &outline);
    true
}

pub(super) fn delete_step(graph: &mut Graph, id: &str) -> bool {
    if graph.nodes.iter().any(|n| n.id == id && n.locked) {
        return false;
    }
    let Some(mut outline) = outline_of(graph) else { return false };
    if outline.trigger == id {
        return false;
    }
    if outline.condition.as_deref() == Some(id) {
        outline.condition = None;
        let yes = std::mem::take(&mut outline.yes);
        let no = std::mem::take(&mut outline.no);
        let after = std::mem::take(&mut outline.after);
        outline.main.extend(yes);
        outline.main.extend(no);
        outline.main.extend(after);
    } else if let Some(index) = column_of(&outline, id) {
        column_mut(&mut outline, index).retain(|item| item != id);
    } else {
        return false;
    }
    graph.nodes.retain(|n| n.id != id);
    *graph = graph_from_outline(graph, &outline);
    true
}

pub(super) fn insert_step(graph: &mut Graph, after: &str, kind: NodeKind) -> Result<(), String> {
    let Some(mut outline) = outline_of(graph) else {
        return Err("This workflow was arranged on a computer. You can edit each step's settings.".into());
    };
    if kind == NodeKind::Condition && outline.condition.is_some() {
        return Err("This workflow already has a condition".into());
    }
    let node = blank(kind);
    let id = node.id.clone();
    graph.nodes.push(node);
    if kind == NodeKind::Condition {
        place_condition(&mut outline, after);
        outline.condition = Some(id);
    } else if !place_after(&mut outline, after, id) {
        graph.nodes.pop();
        return Err("This workflow was arranged on a computer. You can edit each step's settings.".into());
    }
    *graph = graph_from_outline(graph, &outline);
    Ok(())
}

fn place_condition(outline: &mut Outline, after: &str) {
    if outline.trigger == after {
        outline.yes.append(&mut outline.main);
        return;
    }
    if let Some(index) = outline.main.iter().position(|id| id == after) {
        outline.yes = outline.main.split_off(index + 1);
    }
}

fn place_after(outline: &mut Outline, after: &str, id: String) -> bool {
    if outline.trigger == after {
        outline.main.insert(0, id);
        return true;
    }
    if outline.condition.as_deref() == Some(after) {
        outline.yes.insert(0, id);
        return true;
    }
    let Some(index) = column_of(outline, after) else { return false };
    let list = column_mut(outline, index);
    let Some(at) = list.iter().position(|item| item == after) else { return false };
    list.insert(at + 1, id);
    true
}

fn column_of(outline: &Outline, id: &str) -> Option<usize> {
    [&outline.main, &outline.yes, &outline.no, &outline.after, &outline.loose]
        .iter()
        .position(|col| col.iter().any(|item| item == id))
}

fn column_mut(outline: &mut Outline, index: usize) -> &mut Vec<String> {
    match index {
        0 => &mut outline.main,
        1 => &mut outline.yes,
        2 => &mut outline.no,
        3 => &mut outline.after,
        _ => &mut outline.loose,
    }
}

fn blank(kind: NodeKind) -> Node {
    let mut fields = Map::new();
    match kind {
        NodeKind::Trigger => {
            fields.insert("trigger".into(), json!("manual"));
        }
        NodeKind::Prompt => {
            fields.insert("text".into(), json!(""));
        }
        NodeKind::Agent => {
            fields.insert("task".into(), json!(""));
        }
        NodeKind::Condition => {
            fields.insert("op".into(), json!("contains"));
            fields.insert("value".into(), json!(""));
        }
        NodeKind::Approval => {
            fields.insert("question".into(), json!(""));
        }
        NodeKind::Output => {
            fields.insert("target".into(), json!("notify"));
        }
        _ => {}
    }
    Node { id: new_id(), kind, at: Point { x: 80.0, y: 80.0 }, size: None, locked: false, fields, schedule: None }
}

pub(in crate::tui) fn field_rows(app: &App, edit: &WorkflowEdit) -> Vec<(String, String)> {
    let Some(node) = edit.node() else { return Vec::new() };
    WorkflowEdit::settings(node).iter().map(|setting| field_row(app, &edit.graph, node, *setting)).collect()
}

fn field_row(app: &App, graph: &Graph, node: &Node, setting: Setting) -> (String, String) {
    let show = |key: &str| field_str(&node.fields, key).unwrap_or("").to_owned();
    match setting {
        Setting::Trigger => ("Start".into(), show("trigger")),
        Setting::Schedule => ("When".into(), style_name(&style_of(node))),
        Setting::Cron => ("Cron".into(), show_cron(node)),
        Setting::Text(key) => (text_label(key).into(), show(key)),
        Setting::Bot => ("Bot".into(), bot_label(app, field_str(&node.fields, "botId"))),
        Setting::Op => ("Match".into(), show("op")),
        Setting::Target => ("Send".into(), show("target")),
        Setting::Source => ("Source".into(), source_label(graph, node)),
    }
}

pub(super) fn cycle_source(graph: &mut Graph, id: &str, delta: isize) -> bool {
    let options = source_choices(graph, id);
    if options.len() < 2 {
        return false;
    }
    let current = {
        let kinds = graph.nodes.iter().map(|node| (node.id.as_str(), node.kind)).collect();
        outline_of(graph).and_then(|outline| source_target(&outline, &kinds, id))
    };
    let index = current.as_ref().and_then(|target| options.iter().position(|item| item == target)).unwrap_or(0);
    let next = &options[cycle(index, delta, options.len())];
    if current.as_ref() == Some(next) {
        return false;
    }
    let Some(updated) = with_source(graph, id, next) else { return false };
    *graph = updated;
    true
}

fn source_label(graph: &Graph, node: &Node) -> String {
    let Some(outline) = outline_of(graph) else { return String::new() };
    let kinds = graph.nodes.iter().map(|item| (item.id.as_str(), item.kind)).collect();
    let Some(target) = source_target(&outline, &kinds, &node.id) else { return String::new() };
    match target {
        SourceTarget::Yes => branch_label(graph, outline.condition.as_deref(), "Yes"),
        SourceTarget::No => branch_label(graph, outline.condition.as_deref(), "No"),
        SourceTarget::Both => "Both branches".into(),
        SourceTarget::Node(id) if id == outline.trigger => field_str(
            &graph.nodes.iter().find(|item| item.id == id).map(|item| &item.fields).cloned().unwrap_or_default(),
            "trigger",
        )
        .unwrap_or("manual")
        .to_owned(),
        SourceTarget::Node(id) => match (step_number(graph, &id), graph.nodes.iter().find(|item| item.id == id)) {
            (Some(number), Some(item)) => format!("{number} · {}", kind_name(item.kind)),
            (_, Some(item)) => kind_name(item.kind).to_owned(),
            _ => String::new(),
        },
    }
}

fn branch_label(graph: &Graph, id: Option<&str>, name: &str) -> String {
    match id.and_then(|id| step_number(graph, id)) {
        Some(number) => format!("{number} · {name}"),
        None => name.to_owned(),
    }
}

fn step_number(graph: &Graph, id: &str) -> Option<usize> {
    let mut number = 0;
    for step in reading_order(graph) {
        let kind = graph.nodes.iter().find(|node| node.id == step)?.kind;
        if kind == NodeKind::Trigger || kind.is_annotation() {
            continue;
        }
        number += 1;
        if step == id {
            return Some(number);
        }
    }
    None
}

fn text_label(key: &str) -> &'static str {
    match key {
        "task" => "Task",
        "question" => "Question",
        "value" => "Value",
        _ => "Text",
    }
}

fn style_name(style: &str) -> String {
    match style {
        "hourly" => "Hourly",
        "weekdays" => "Weekdays",
        "weekly" => "Weekly",
        "custom" => "Custom",
        _ => "Daily",
    }
    .into()
}

fn show_cron(node: &Node) -> String {
    match &node.schedule {
        Some(Trigger::Cron { expression, .. }) => expression.clone(),
        _ => String::new(),
    }
}

fn bot_label(app: &App, id: Option<&str>) -> String {
    match id.and_then(|id| app.bots.get(id)) {
        Some(bot) => bot.name.clone(),
        None => "Choose a bot".into(),
    }
}

pub(in crate::tui) fn kind_name(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Agent => "Agent",
        NodeKind::Condition => "Condition",
        NodeKind::Approval => "Approval",
        NodeKind::Prompt => "Prompt",
        NodeKind::Output => "Output",
        NodeKind::Trigger => "Manual",
        _ => "Note",
    }
}

pub(in crate::tui) fn add_kinds() -> &'static [NodeKind] {
    &ADD
}

/// A prompt with no outgoing line is the one the outline calls out.
pub(in crate::tui) fn feeds(graph: &Graph, id: &str) -> bool {
    graph.edges.iter().any(|edge| edge.from == id)
}

#[cfg(test)]
mod tests {
    use super::super::super::app::{Editor, Overlay};
    use super::super::workflow_edit::step_rows;
    use super::super::{WorkflowPage, WorkflowSheet};
    use super::*;
    use crate::tui::app::test_app;
    use crate::workflows::{EditMode, default_graph, edit_mode};
    use crossterm::event::{KeyCode, KeyEvent};

    fn agents() -> (Graph, String, String) {
        let mut graph = default_graph();
        let trigger = graph.nodes[0].id.clone();
        let first = blank(NodeKind::Agent);
        let second = blank(NodeKind::Agent);
        let (a, b) = (first.id.clone(), second.id.clone());
        graph.nodes.push(first);
        graph.nodes.push(second);
        let outline = Outline {
            trigger,
            main: vec![a.clone(), b.clone()],
            condition: None,
            yes: Vec::new(),
            no: Vec::new(),
            after: Vec::new(),
            loose: Vec::new(),
        };
        (graph_from_outline(&graph, &outline), a, b)
    }

    fn edit_of(graph: Graph, cursor: usize) -> WorkflowEdit {
        let mut edit = WorkflowEdit {
            id: "w".into(),
            title: "Workflow".into(),
            base: 1,
            graph,
            source: "graph".into(),
            script: String::new(),
            phases: Vec::new(),
            cursor,
            field: 0,
            text: Editor::default(),
            saving: false,
            dirty: false,
            leaving: false,
        };
        edit.load_text();
        edit
    }

    fn press(edit: WorkflowEdit, key: KeyCode) -> WorkflowSheet {
        let mut app = test_app();
        let sheet = WorkflowSheet {
            list: super::super::workflows::WorkflowList { items: Some(Vec::new()), cursor: 0 },
            page: WorkflowPage::Edit(Box::new(edit)),
            notice: None,
            error: None,
            versions: None,
            version_cursor: 0,
            lines: false,
        };
        app.overlays.push(Overlay::Workflows(Box::new(sheet)));
        app.on_key(KeyEvent::from(key));
        match app.overlays.pop() {
            Some(Overlay::Workflows(sheet)) => *sheet,
            _ => panic!("the workflow sheet stays open"),
        }
    }

    #[tokio::test]
    async fn bracket_moves_a_step_up_the_outline() {
        let (graph, a, b) = agents();
        let sheet = press(edit_of(graph, 2), KeyCode::Char('['));
        let WorkflowPage::Edit(edit) = sheet.page else { panic!("still editing") };
        let Some(outline) = outline_of(&edit.graph) else { panic!("the graph stays an outline") };
        assert_eq!(outline.main, vec![b, a]);
    }

    #[tokio::test]
    async fn adding_an_assistant_extends_the_outline() {
        let (graph, _, b) = agents();
        let mut app = test_app();
        let sheet = WorkflowSheet {
            list: super::super::workflows::WorkflowList { items: Some(Vec::new()), cursor: 0 },
            page: WorkflowPage::Edit(Box::new(edit_of(graph, 2))),
            notice: None,
            error: None,
            versions: None,
            version_cursor: 0,
            lines: false,
        };
        app.overlays.push(Overlay::Workflows(Box::new(sheet)));
        app.on_key(KeyEvent::from(KeyCode::Char('a')));
        app.on_key(KeyEvent::from(KeyCode::Enter));
        let Some(Overlay::Workflows(sheet)) = app.overlays.pop() else { panic!("sheet") };
        let WorkflowPage::Edit(edit) = sheet.page else { panic!("back to the editor") };
        let Some(outline) = outline_of(&edit.graph) else { panic!("outline") };
        assert_eq!(outline.main.len(), 3);
        assert_eq!(outline.main[1], b);
        assert_eq!(edit.graph.nodes.iter().filter(|n| n.kind == NodeKind::Agent).count(), 3);
    }

    #[tokio::test]
    async fn fields_mode_ignores_reorder_and_add() {
        let mut graph = default_graph();
        graph.nodes.push(blank(NodeKind::Condition));
        graph.nodes.push(blank(NodeKind::Condition));
        assert_eq!(edit_mode(&graph, "graph"), EditMode::Fields);
        let before = graph.clone();
        let sheet = press(edit_of(graph, 0), KeyCode::Char('['));
        let WorkflowPage::Edit(edit) = &sheet.page else { panic!("editing") };
        assert_eq!(edit.graph, before);
        let again = press(edit_of(before.clone(), 0), KeyCode::Char('a'));
        let WorkflowPage::Edit(edit) = again.page else { panic!("add is not an outline action") };
        assert_eq!(edit.graph, before);
    }

    #[tokio::test]
    async fn a_second_condition_is_refused() {
        let (mut graph, a, _) = agents();
        let condition = blank(NodeKind::Condition);
        let cid = condition.id.clone();
        graph.nodes.push(condition);
        let Some(mut outline) = outline_of(&graph) else { panic!("one column") };
        outline.condition = Some(cid);
        graph = graph_from_outline(&graph, &outline);
        let mut app = test_app();
        let cursor = step_rows(&graph).iter().position(|row| row.id == a).unwrap_or(0);
        let sheet = WorkflowSheet {
            list: super::super::workflows::WorkflowList { items: Some(Vec::new()), cursor: 0 },
            page: WorkflowPage::Edit(Box::new(edit_of(graph, cursor))),
            notice: None,
            error: None,
            versions: None,
            version_cursor: 0,
            lines: false,
        };
        app.overlays.push(Overlay::Workflows(Box::new(sheet)));
        app.on_key(KeyEvent::from(KeyCode::Char('a')));
        app.on_key(KeyEvent::from(KeyCode::Char('j')));
        app.on_key(KeyEvent::from(KeyCode::Enter));
        let Some(Overlay::Workflows(sheet)) = app.overlays.last() else { panic!("sheet") };
        assert_eq!(sheet.error.as_deref(), Some("This workflow already has a condition"));
    }

    #[test]
    fn move_step_rewrites_edges_through_the_outline() {
        let (mut graph, a, b) = agents();
        assert!(move_step(&mut graph, &b, -1));
        let Some(outline) = outline_of(&graph) else { panic!("outline") };
        assert_eq!(outline.main, vec![b, a]);
        assert!(graph.edges.iter().any(|e| e.from != e.to));
    }
}
