//! Whether a graph may be saved: ports, branches, fields and the size caps.

#![allow(
    clippy::items_after_statements,
    clippy::unnecessary_get_then_check,
    clippy::needless_borrow,
    clippy::needless_pass_by_value,
    clippy::map_unwrap_or
)]

use super::limits::{
    ALIGNS, MAX_COORD, MAX_EDGES, MAX_FIELD_CHARS, MAX_FIELD_KEYS, MAX_GRAPH_BYTES, MAX_KEY_LEN, MAX_NODES, MAX_SIZE,
    MAX_STROKE_POINTS, MAX_STROKES, NOTE_TONES, SHAPES,
};
use super::model::{Branch, Graph, Node, NodeKind, PortType, TriggerKind, field_str, ports};
use crate::routines::Trigger;
use anyhow::{Result, bail};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Why a graph was refused. `message` is the sentence a client shows.
#[derive(Debug)]
pub struct Problem {
    pub kind: &'static str,
    pub message: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = self.kind;
        f.write_str(&self.message)
    }
}

impl std::error::Error for Problem {}

fn problem(kind: &'static str, message: impl Into<String>) -> Problem {
    Problem { kind, message: message.into() }
}

/// A bot named by an assistant or a chat output.
#[derive(Clone, Copy)]
pub enum BotState {
    Ok,
    Missing,
    Group,
}

/// `bots` says whether an id is a live bot on this computer.
pub fn validate(graph: &Graph, bots: &impl Fn(&str) -> BotState) -> Result<()> {
    let bytes = serde_json::to_vec(graph).map_err(|_| problem("limit", "This workflow is too large to save"))?;
    if bytes.len() > MAX_GRAPH_BYTES {
        bail!(problem("limit", "This workflow is too large to save"));
    }
    if graph.nodes.len() > MAX_NODES {
        bail!(problem("limit", "This workflow has too many steps"));
    }
    if graph.edges.len() > MAX_EDGES {
        bail!(problem("limit", "This workflow has too many lines"));
    }
    if graph.strokes.len() > MAX_STROKES {
        bail!(problem("limit", "This workflow has too many drawings"));
    }
    let mut ids = HashSet::new();
    for node in &graph.nodes {
        if !ids.insert(node.id.clone()) {
            bail!(problem("duplicate", "Two steps share an id"));
        }
        check_point(node.at.x, node.at.y)?;
        if let Some(size) = node.size {
            check_size(size.width)?;
            check_size(size.height)?;
        }
        check_fields(node, bots)?;
    }
    let by_id: HashMap<&str, &Node> = graph.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut seen = HashSet::new();
    let mut condition_branch: HashMap<(&str, Branch), usize> = HashMap::new();
    for edge in &graph.edges {
        if !seen.insert(edge.id.clone()) {
            bail!(problem("duplicate", "Two lines share an id"));
        }
        if edge.from == edge.to {
            bail!(problem("port", "A step can't connect to itself"));
        }
        let (Some(from), Some(to)) = (by_id.get(edge.from.as_str()), by_id.get(edge.to.as_str())) else {
            bail!(problem("port", "A line points at a step that isn't in this workflow"));
        };
        if from.kind.is_annotation() || to.kind.is_annotation() {
            bail!(problem("annotation", "A line can't connect a note"));
        }
        if edge.port != PortType::Text {
            bail!(problem("port", "That line doesn't fit this step"));
        }
        let (_, outs) = ports(from.kind);
        let (ins, _) = ports(to.kind);
        if outs.is_empty() || ins.is_empty() {
            bail!(problem("port", "That line doesn't fit this step"));
        }
        match (from.kind, edge.branch) {
            (NodeKind::Condition, None) => bail!(problem("branch", "A condition line needs a yes or no")),
            (NodeKind::Condition, Some(branch)) => {
                let count = condition_branch.entry((from.id.as_str(), branch)).or_insert(0);
                *count += 1;
                if *count > 1 {
                    bail!(problem(
                        "condition-branch",
                        format!("This condition already has a {} line", branch.as_str())
                    ));
                }
            }
            (_, Some(_)) => bail!(problem("branch", "Only a condition line has a yes or no")),
            _ => {}
        }
        if !seen_pair(&graph, edge) {
            bail!(problem("duplicate", "Those steps are already connected"));
        }
    }
    for stroke in &graph.strokes {
        if stroke.id.is_empty() || stroke.points.len() > MAX_STROKE_POINTS {
            bail!(problem("limit", "A drawing has too many points"));
        }
        if stroke.brush != "pen" && stroke.brush != "marker" {
            bail!(problem("field", "A drawing's brush is pen or marker"));
        }
        check_size(stroke.width)?;
        for point in &stroke.points {
            check_point(point.x, point.y)?;
        }
    }
    Ok(())
}

fn seen_pair(graph: &Graph, edge: &super::model::Edge) -> bool {
    graph
        .edges
        .iter()
        .filter(|other| other.from == edge.from && other.to == edge.to && other.branch == edge.branch)
        .count()
        == 1
}

fn check_point(x: f64, y: f64) -> Result<()> {
    if !x.is_finite() || !y.is_finite() || x.abs() > MAX_COORD || y.abs() > MAX_COORD {
        bail!(problem("limit", "A step is too far from the canvas"));
    }
    Ok(())
}

fn check_size(value: f64) -> Result<()> {
    if !value.is_finite() || value < 0.0 || value > MAX_SIZE {
        bail!(problem("limit", "A step is too large"));
    }
    Ok(())
}

fn check_fields(node: &Node, bots: &impl Fn(&str) -> BotState) -> Result<()> {
    if node.fields.len() > MAX_FIELD_KEYS {
        bail!(problem("limit", "A step has too many settings"));
    }
    for (key, value) in &node.fields {
        if key.len() > MAX_KEY_LEN {
            bail!(problem("field", "unknown field"));
        }
        if value.as_str().is_some_and(|s| s.chars().count() > MAX_FIELD_CHARS) {
            bail!(problem("limit", "A step's text is too long"));
        }
    }
    let allowed: &[&str] = match node.kind {
        NodeKind::Trigger => &["trigger", "timezone"],
        NodeKind::Prompt => &["text"],
        NodeKind::Agent => &["botId", "task"],
        NodeKind::Condition => &["op", "value"],
        NodeKind::Approval => &["question"],
        NodeKind::Output => &["target", "botId"],
        NodeKind::Text => &["text", "font", "align", "bold"],
        NodeKind::Note => &["text", "tone"],
        NodeKind::Shape => &["shape", "text", "fill"],
    };
    for key in node.fields.keys() {
        if !allowed.contains(&key.as_str()) {
            bail!(problem("field", "unknown field"));
        }
    }
    match node.kind {
        NodeKind::Trigger => check_trigger(node)?,
        NodeKind::Agent => check_bot(field_str(&node.fields, "botId"), bots, false)?,
        NodeKind::Condition => check_condition(node)?,
        NodeKind::Output => check_output(node, bots)?,
        NodeKind::Text => check_text(node)?,
        NodeKind::Note => check_tone(field_str(&node.fields, "tone"))?,
        NodeKind::Shape => {
            let shape = field_str(&node.fields, "shape").unwrap_or("");
            if !SHAPES.contains(&shape) {
                bail!(problem("field", "Pick a shape"));
            }
            check_tone(field_str(&node.fields, "fill"))?;
        }
        NodeKind::Prompt | NodeKind::Approval => {}
    }
    Ok(())
}

fn check_trigger(node: &Node) -> Result<()> {
    let kind = field_str(&node.fields, "trigger").and_then(TriggerKind::parse);
    let Some(kind) = kind else { bail!(problem("field", "Pick how this workflow starts")) };
    match kind {
        TriggerKind::Manual | TriggerKind::Webhook => {
            if node.schedule.is_some() {
                bail!(problem("schedule", "Only a schedule trigger has a time"));
            }
        }
        TriggerKind::Schedule => {
            let Some(Trigger::Cron { expression, time_zone }) = &node.schedule else {
                bail!(problem("schedule", "Pick when this workflow runs"));
            };
            let zone = field_str(&node.fields, "timezone").unwrap_or("");
            if zone != time_zone {
                bail!(problem("schedule", "The time zone doesn't match the schedule"));
            }
            let cron = Trigger::Cron { expression: expression.clone(), time_zone: time_zone.clone() };
            cron.validate(crate::store::now_ms()).map_err(|_| problem("schedule", "That schedule isn't valid"))?;
        }
    }
    Ok(())
}

fn check_bot(id: Option<&str>, bots: &impl Fn(&str) -> BotState, required: bool) -> Result<()> {
    match id.map(str::trim).filter(|s| !s.is_empty()) {
        None if required => bail!(problem("field", "Choose a bot for this step")),
        None => Ok(()),
        Some(id) => match bots(id) {
            BotState::Ok => Ok(()),
            BotState::Missing => bail!(problem("field", "Choose a bot for this step")),
            BotState::Group => bail!(problem("field", "A group can't run this step")),
        },
    }
}

fn check_condition(node: &Node) -> Result<()> {
    let op = field_str(&node.fields, "op").unwrap_or("");
    match op {
        "contains" | "equals" | "empty" => Ok(()),
        "regex" => {
            let pattern = field_str(&node.fields, "value").unwrap_or("");
            regex::Regex::new(pattern).map_err(|_| problem("field", "That pattern isn't valid"))?;
            Ok(())
        }
        _ => bail!(problem("field", "Pick how this condition matches")),
    }
}

fn check_output(node: &Node, bots: &impl Fn(&str) -> BotState) -> Result<()> {
    match field_str(&node.fields, "target") {
        Some("notify") => Ok(()),
        Some("chat") => check_bot(field_str(&node.fields, "botId"), bots, true),
        _ => bail!(problem("field", "Pick where this step sends its result")),
    }
}

fn check_text(node: &Node) -> Result<()> {
    let font = node.fields.get("font").and_then(Value::as_f64).unwrap_or(16.0);
    if !(8.0..=96.0).contains(&font) {
        bail!(problem("field", "Text size is between 8 and 96"));
    }
    let align = field_str(&node.fields, "align").unwrap_or("leading");
    if !ALIGNS.contains(&align) {
        bail!(problem("field", "Pick how the text lines up"));
    }
    if node.fields.get("bold").is_some_and(|v| !v.is_boolean()) {
        bail!(problem("field", "Bold is on or off"));
    }
    Ok(())
}

fn check_tone(tone: Option<&str>) -> Result<()> {
    if tone.is_some_and(|tone| !NOTE_TONES.contains(&tone)) {
        bail!(problem("field", "Pick a color"));
    }
    Ok(())
}

/// Edges that return to a step still on the walk. Those don't wait on the upstream step.
pub fn loop_edges(graph: &Graph) -> HashSet<String> {
    let mut outs: HashMap<&str, Vec<&super::model::Edge>> = HashMap::new();
    let mut incoming: HashSet<&str> = HashSet::new();
    for edge in &graph.edges {
        outs.entry(edge.from.as_str()).or_default().push(edge);
        incoming.insert(edge.to.as_str());
    }
    let mut back = HashSet::new();
    let mut state: HashMap<&str, u8> = HashMap::new();
    fn walk<'a>(
        id: &'a str,
        outs: &HashMap<&str, Vec<&'a super::model::Edge>>,
        state: &mut HashMap<&'a str, u8>,
        back: &mut HashSet<String>,
    ) {
        state.insert(id, 1);
        for edge in outs.get(id).into_iter().flatten() {
            match state.get(edge.to.as_str()).copied().unwrap_or(0) {
                1 => {
                    back.insert(edge.id.clone());
                }
                0 => walk(&edge.to, outs, state, back),
                _ => {}
            }
        }
        state.insert(id, 2);
    }
    let mut starts: Vec<&str> = graph.nodes.iter().map(|n| n.id.as_str()).filter(|id| !incoming.contains(id)).collect();
    starts.extend(graph.nodes.iter().map(|n| n.id.as_str()).filter(|id| state.get(id).is_none()));
    for id in starts {
        if state.get(id).is_none() {
            walk(id, &outs, &mut state, &mut back);
        }
    }
    back
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routines::Trigger;
    use serde_json::{Map, json};

    fn node(id: &str, kind: NodeKind, fields: Map<String, Value>) -> Node {
        Node {
            id: id.into(),
            kind,
            at: super::super::model::Point { x: 0.0, y: 0.0 },
            size: None,
            locked: false,
            fields,
            schedule: None,
        }
    }

    fn fields(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap_or_default()
    }

    fn bots(id: &str) -> BotState {
        if id == "bot" {
            BotState::Ok
        } else if id == "group" {
            BotState::Group
        } else {
            BotState::Missing
        }
    }

    #[test]
    fn a_condition_needs_one_branch_each() {
        let mut yes = node("c", NodeKind::Condition, fields(json!({"op": "empty"})));
        let graph = Graph {
            nodes: vec![
                node("t", NodeKind::Trigger, fields(json!({"trigger": "manual"}))),
                yes.clone(),
                node("o", NodeKind::Output, fields(json!({"target": "notify"}))),
            ],
            edges: vec![super::super::model::Edge {
                id: "a".into(),
                from: "c".into(),
                to: "o".into(),
                port: PortType::Text,
                branch: None,
            }],
            strokes: vec![],
        };
        let err = validate(&graph, &bots).unwrap_err();
        assert!(err.to_string().contains("yes or no"), "{err}");
        yes.fields = fields(json!({"op": "regex", "value": "("}));
        let mut graph = graph;
        graph.nodes[1] = yes;
        graph.edges.clear();
        assert!(validate(&graph, &bots).unwrap_err().to_string().contains("pattern"));
    }

    #[test]
    fn a_schedule_is_a_cron() {
        let mut trigger = node("t", NodeKind::Trigger, fields(json!({"trigger": "schedule", "timezone": "UTC"})));
        trigger.schedule = Some(Trigger::Cron { expression: "0 9 * * *".into(), time_zone: "UTC".into() });
        let graph = Graph { nodes: vec![trigger], edges: vec![], strokes: vec![] };
        validate(&graph, &bots).unwrap();
    }
}
