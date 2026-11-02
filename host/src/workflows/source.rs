//! Changing which step a step reads. An outline stays an outline.

use super::model::{Branch, Edge, Graph, NodeKind, PortType};
use super::outline::{Outline, graph_from_outline, outline_of};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceTarget {
    Node(String),
    Yes,
    No,
    Both,
}

/// The step this one reads. A prompt has none.
pub fn source_target(outline: &Outline, kinds: &HashMap<&str, NodeKind>, step: &str) -> Option<SourceTarget> {
    let mut found = None;
    let feeding = walk(&outline.main, SourceTarget::Node(outline.trigger.clone()), step, kinds, &mut found);
    if let Some(condition) = &outline.condition {
        if condition == step {
            found = Some(feeding);
        }
        walk(&outline.yes, SourceTarget::Yes, step, kinds, &mut found);
        walk(&outline.no, SourceTarget::No, step, kinds, &mut found);
        walk(&outline.after, SourceTarget::Both, step, kinds, &mut found);
    }
    found
}

fn walk(
    items: &[String],
    start: SourceTarget,
    step: &str,
    kinds: &HashMap<&str, NodeKind>,
    found: &mut Option<SourceTarget>,
) -> SourceTarget {
    let mut source = start;
    for item in items {
        let kind = kinds.get(item.as_str()).copied();
        if kind == Some(NodeKind::Prompt) {
            continue;
        }
        if item == step {
            *found = Some(source.clone());
        }
        if kind != Some(NodeKind::Output) {
            source = SourceTarget::Node(item.clone());
        }
    }
    source
}

/// Moves `step` so it reads from `target`.
pub fn retarget(outline: &Outline, step: &str, target: &SourceTarget) -> Option<Outline> {
    if step == outline.trigger {
        return None;
    }
    if let SourceTarget::Node(source) = target
        && source == step
    {
        return None;
    }
    if outline.condition.as_deref() == Some(step) {
        return retarget_condition(outline, target);
    }
    let mut next = detach(outline, step);
    match target {
        SourceTarget::Both => {
            next.condition.as_ref()?;
            next.after.insert(0, step.to_owned());
        }
        SourceTarget::Yes => {
            next.condition.as_ref()?;
            next.yes.insert(0, step.to_owned());
        }
        SourceTarget::No => {
            next.condition.as_ref()?;
            next.no.insert(0, step.to_owned());
        }
        SourceTarget::Node(source) if source == &next.trigger => next.main.insert(0, step.to_owned()),
        SourceTarget::Node(source) => place(&mut next, step, source)?,
    }
    Some(next)
}

/// Choices the step editor can offer. Prompts and the trigger have none.
pub fn source_choices(graph: &Graph, step: &str) -> Vec<SourceTarget> {
    let Some(node) = graph.nodes.iter().find(|node| node.id == step) else { return Vec::new() };
    if matches!(node.kind, NodeKind::Trigger | NodeKind::Prompt) {
        return Vec::new();
    }
    let Some(outline) = outline_of(graph) else { return Vec::new() };
    let mut options = vec![SourceTarget::Node(outline.trigger.clone())];
    let pool: Vec<String> = if node.kind == NodeKind::Condition {
        outline.main.clone()
    } else {
        outline
            .main
            .iter()
            .chain(outline.yes.iter())
            .chain(outline.no.iter())
            .chain(outline.after.iter())
            .cloned()
            .collect()
    };
    for id in pool {
        if id == step {
            continue;
        }
        let Some(kind) = graph.nodes.iter().find(|node| node.id == id).map(|node| node.kind) else { continue };
        if matches!(kind, NodeKind::Output | NodeKind::Prompt | NodeKind::Trigger) {
            continue;
        }
        options.push(SourceTarget::Node(id));
    }
    if node.kind != NodeKind::Condition && outline.condition.is_some() {
        options.push(SourceTarget::Yes);
        options.push(SourceTarget::No);
        options.push(SourceTarget::Both);
    }
    options
}

/// Applies a new source. An outline stays an outline.
pub fn with_source(graph: &Graph, step: &str, target: &SourceTarget) -> Option<Graph> {
    let kinds: HashMap<&str, NodeKind> = graph.nodes.iter().map(|node| (node.id.as_str(), node.kind)).collect();
    if let Some(outline) = outline_of(graph) {
        if source_target(&outline, &kinds, step).as_ref() == Some(target) {
            return None;
        }
        return Some(graph_from_outline(graph, &retarget(&outline, step, target)?));
    }
    rewired(graph, step, target)
}

fn retarget_condition(outline: &Outline, target: &SourceTarget) -> Option<Outline> {
    let SourceTarget::Node(source) = target else { return None };
    let mut next = outline.clone();
    if source == &outline.trigger {
        let main = std::mem::take(&mut next.main);
        next.after.splice(0..0, main);
        return Some(next);
    }
    let index = next.main.iter().position(|id| id == source)?;
    let rest: Vec<String> = next.main.split_off(index + 1);
    next.after.splice(0..0, rest);
    Some(next)
}

fn detach(outline: &Outline, step: &str) -> Outline {
    let mut next = outline.clone();
    next.main.retain(|id| id != step);
    next.yes.retain(|id| id != step);
    next.no.retain(|id| id != step);
    next.after.retain(|id| id != step);
    next.loose.retain(|id| id != step);
    next
}

fn place(outline: &mut Outline, step: &str, after: &str) -> Option<()> {
    for column in [&mut outline.main, &mut outline.yes, &mut outline.no, &mut outline.after] {
        if let Some(index) = column.iter().position(|id| id == after) {
            column.insert(index + 1, step.to_owned());
            return Some(());
        }
    }
    None
}

fn rewired(graph: &Graph, step: &str, target: &SourceTarget) -> Option<Graph> {
    let node = graph.nodes.iter().find(|node| node.id == step)?;
    if node.kind == NodeKind::Trigger {
        return None;
    }
    let mut next = graph.clone();
    next.edges.retain(|edge| edge.to != step);
    let mut link = |from: &str, branch: Option<Branch>| {
        let id = match branch {
            Some(branch) => format!("{from}.{}>{step}:text", branch.as_str()),
            None => format!("{from}>{step}:text"),
        };
        next.edges.push(Edge { id, from: from.to_owned(), to: step.to_owned(), port: PortType::Text, branch });
    };
    match target {
        SourceTarget::Node(source) => {
            if source == step || graph.nodes.iter().all(|node| node.id != *source) {
                return None;
            }
            link(source, None);
        }
        SourceTarget::Yes | SourceTarget::No | SourceTarget::Both => {
            let condition = graph.nodes.iter().find(|node| node.kind == NodeKind::Condition)?;
            if matches!(target, SourceTarget::Both | SourceTarget::Yes) {
                link(&condition.id, Some(Branch::Yes));
            }
            if matches!(target, SourceTarget::Both | SourceTarget::No) {
                link(&condition.id, Some(Branch::No));
            }
        }
    }
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::model::{Node, Point};
    use serde_json::{Map, json};

    fn node(id: &str, kind: NodeKind) -> Node {
        let mut fields = Map::new();
        if kind == NodeKind::Trigger {
            fields.insert("trigger".into(), json!("manual"));
        }
        if kind == NodeKind::Output {
            fields.insert("target".into(), json!("notify"));
        }
        Node { id: id.into(), kind, at: Point { x: 0.0, y: 0.0 }, size: None, locked: false, fields, schedule: None }
    }

    fn edge(from: &str, to: &str) -> Edge {
        Edge { id: format!("{from}>{to}:text"), from: from.into(), to: to.into(), port: PortType::Text, branch: None }
    }

    #[test]
    fn changing_a_source_moves_the_step() {
        let graph = Graph {
            nodes: vec![
                node("t", NodeKind::Trigger),
                node("a", NodeKind::Agent),
                node("b", NodeKind::Agent),
                node("o", NodeKind::Output),
            ],
            edges: vec![edge("t", "a"), edge("a", "b"), edge("b", "o")],
            strokes: Vec::new(),
        };
        let moved = with_source(&graph, "o", &SourceTarget::Node("t".into())).expect("output can read the trigger");
        assert_eq!(
            moved.edges.iter().filter(|edge| edge.to == "o").map(|edge| edge.from.as_str()).collect::<Vec<_>>(),
            ["t"]
        );
        assert!(outline_of(&moved).is_some());
    }
}
