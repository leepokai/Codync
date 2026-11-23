//! The phone's step list. A graph it can express is edited as columns; any other shape
//! keeps its lines and only its settings change.

#![allow(clippy::map_unwrap_or, clippy::items_after_statements, clippy::cast_precision_loss, clippy::manual_find)]

use super::graph::loop_edges;
use super::model::{Branch, Edge, Graph, Node, NodeKind, Point, PortType};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outline {
    pub trigger: String,
    pub main: Vec<String>,
    pub condition: Option<String>,
    pub yes: Vec<String>,
    pub no: Vec<String>,
    pub after: Vec<String>,
    pub loose: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditMode {
    Outline,
    Fields,
    Script,
    ReadOnly,
}

pub fn edit_mode(graph: &Graph, source: &str) -> EditMode {
    if source == "script" {
        EditMode::Script
    } else if outline_of(graph).is_some() {
        EditMode::Outline
    } else {
        EditMode::Fields
    }
}

struct Core {
    main: Vec<String>,
    condition: Option<String>,
    yes: Vec<String>,
    no: Vec<String>,
    after: Vec<String>,
}

/// The outline of `graph`, or nothing when the phone must leave the lines alone.
pub fn outline_of(graph: &Graph) -> Option<Outline> {
    let steps: Vec<&Node> = graph.nodes.iter().filter(|n| n.kind.is_step()).collect();
    let triggers: Vec<_> = steps.iter().filter(|n| n.kind == NodeKind::Trigger).collect();
    if triggers.len() != 1 || steps.iter().filter(|n| n.kind == NodeKind::Condition).count() > 1 {
        return None;
    }
    let trigger = triggers[0].id.clone();
    let kinds: HashMap<&str, NodeKind> = steps.iter().map(|n| (n.id.as_str(), n.kind)).collect();
    let nodes: HashMap<&str, &Node> = steps.iter().map(|n| (n.id.as_str(), *n)).collect();
    if graph
        .edges
        .iter()
        .any(|e| e.port != PortType::Text || !kinds.contains_key(e.from.as_str()) || !kinds.contains_key(e.to.as_str()))
    {
        return None;
    }
    if !loop_edges(graph).is_empty() {
        return None;
    }
    let mut reached = HashSet::new();
    reached.insert(trigger.as_str());
    let mut queue = vec![trigger.as_str()];
    while let Some(id) = queue.pop() {
        for edge in &graph.edges {
            if edge.from == id && reached.insert(edge.to.as_str()) {
                queue.push(edge.to.as_str());
            }
        }
    }
    let mut prompts: HashMap<&str, &str> = HashMap::new();
    for node in &steps {
        if node.kind != NodeKind::Prompt {
            continue;
        }
        let targets: Vec<&str> = graph.edges.iter().filter(|e| e.from == node.id).map(|e| e.to.as_str()).collect();
        if targets.is_empty() || !targets.iter().any(|id| reached.contains(id)) {
            continue;
        }
        if targets.len() != 1 || !targets.iter().all(|id| reached.contains(id)) {
            return None;
        }
        prompts.insert(node.id.as_str(), targets[0]);
    }
    let loose: Vec<String> = steps
        .iter()
        .map(|n| n.id.clone())
        .filter(|id| !reached.contains(id.as_str()) && !prompts.contains_key(id.as_str()))
        .collect();
    let loose_set: HashSet<&str> = loose.iter().map(String::as_str).collect();
    if graph.edges.iter().any(|e| loose_set.contains(e.from.as_str()) && !loose_set.contains(e.to.as_str())) {
        return None;
    }
    let core: HashSet<&str> = reached.iter().copied().filter(|id| kinds.get(id) != Some(&NodeKind::Output)).collect();
    let chain = core_chain(trigger.as_str(), &core, &kinds, &graph.edges)?;
    let order = |left: &str, right: &str| {
        let a = nodes.get(left).map(|n| (n.at.x, n.at.y)).unwrap_or((0.0, 0.0));
        let b = nodes.get(right).map(|n| (n.at.x, n.at.y)).unwrap_or((0.0, 0.0));
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    };
    let mut placements: HashMap<String, Vec<&str>> = HashMap::new();
    for node in &steps {
        if node.kind != NodeKind::Output || !reached.contains(node.id.as_str()) {
            continue;
        }
        let feeds: Vec<&Edge> =
            graph.edges.iter().filter(|e| e.to == node.id && core.contains(e.from.as_str())).collect();
        match feeds.as_slice() {
            [first] if first.from == chain.condition.as_deref().unwrap_or("") => {
                let branch = first.branch?;
                placements.entry(format!("start:{}", branch.as_str())).or_default().push(node.id.as_str());
            }
            [first] => placements.entry(format!("after:{}", first.from)).or_default().push(node.id.as_str()),
            [first, second] if chain.condition.is_some() => {
                let ends = [lane_end(&chain, Branch::Yes), lane_end(&chain, Branch::No)];
                let matches = |edge: &Edge| ends.iter().any(|end| end.0 == edge.from && end.1 == edge.branch);
                if !matches(first) || !matches(second) || (first.from == second.from && first.branch == second.branch) {
                    return None;
                }
                placements.entry("start:after".into()).or_default().push(node.id.as_str());
            }
            _ => return None,
        }
    }
    let with_outputs = |name: &str, items: &[String]| {
        let mut result = placements.get(&format!("start:{name}")).cloned().unwrap_or_default();
        result.sort_by(|a, b| order(a, b));
        if name == "main" {
            let mut extra = placements.get(&format!("after:{trigger}")).cloned().unwrap_or_default();
            extra.sort_by(|a, b| order(a, b));
            result.extend(extra);
        }
        for id in items {
            result.push(id.as_str());
            let mut extra = placements.get(&format!("after:{id}")).cloned().unwrap_or_default();
            extra.sort_by(|a, b| order(a, b));
            result.extend(extra);
        }
        result.into_iter().map(str::to_owned).collect::<Vec<_>>()
    };
    let mut outline = Outline {
        trigger: trigger.clone(),
        main: with_outputs("main", &chain.main),
        condition: chain.condition.clone(),
        yes: with_outputs("yes", &chain.yes),
        no: with_outputs("no", &chain.no),
        after: with_outputs("after", &chain.after),
        loose,
    };
    let mut prompt_pairs: Vec<_> = prompts.into_iter().collect();
    prompt_pairs.sort_by(|a, b| order(a.0, b.0));
    for (prompt, target) in prompt_pairs {
        if Some(target) == chain.condition.as_deref() {
            outline.main.push(prompt.to_owned());
            continue;
        }
        let list = list_mut(&mut outline, target)?;
        let index = list.iter().position(|id| id == target)?;
        list.insert(index, prompt.to_owned());
    }
    Some(outline)
}

fn lane_end(chain: &Core, branch: Branch) -> (String, Option<Branch>) {
    let list = if branch == Branch::Yes { &chain.yes } else { &chain.no };
    list.last()
        .map(|id| (id.clone(), None))
        .unwrap_or_else(|| (chain.condition.clone().unwrap_or_default(), Some(branch)))
}

fn list_mut<'a>(outline: &'a mut Outline, id: &str) -> Option<&'a mut Vec<String>> {
    for list in [&mut outline.main, &mut outline.yes, &mut outline.no, &mut outline.after] {
        if list.iter().any(|item| item == id) {
            return Some(list);
        }
    }
    None
}

fn core_chain(trigger: &str, core: &HashSet<&str>, kinds: &HashMap<&str, NodeKind>, edges: &[Edge]) -> Option<Core> {
    let mut out: HashMap<&str, Vec<&Edge>> = HashMap::new();
    let mut incoming: HashMap<&str, usize> = HashMap::new();
    for edge in edges {
        if !core.contains(edge.from.as_str()) || !core.contains(edge.to.as_str()) {
            continue;
        }
        out.entry(edge.from.as_str()).or_default().push(edge);
        *incoming.entry(edge.to.as_str()).or_insert(0) += 1;
    }
    for id in core {
        let ways = out.get(id).map(Vec::as_slice).unwrap_or(&[]);
        if kinds.get(id) == Some(&NodeKind::Condition) {
            if ways.iter().any(|e| e.branch.is_none())
                || ways.iter().filter(|e| e.branch == Some(Branch::Yes)).count() > 1
                || ways.iter().filter(|e| e.branch == Some(Branch::No)).count() > 1
            {
                return None;
            }
        } else if ways.len() > 1 {
            return None;
        }
    }
    if incoming.contains_key(trigger) {
        return None;
    }
    let mut seen = HashSet::new();
    seen.insert(trigger);
    let next = |id: &str| out.get(id).and_then(|ways| ways.first()).map(|e| e.to.as_str());
    let start = run(next(trigger), true, &mut seen, &incoming, kinds, &next)?;
    if start.rest.is_some() {
        return None;
    }
    let Some(condition) = start.condition else {
        return (seen.len() == core.len()).then_some(Core {
            main: start.list,
            condition: None,
            yes: Vec::new(),
            no: Vec::new(),
            after: Vec::new(),
        });
    };
    let branch = |name: Branch| {
        out.get(condition.as_str()).and_then(|ways| ways.iter().find(|e| e.branch == Some(name))).map(|e| e.to.as_str())
    };
    let yes = run(branch(Branch::Yes), false, &mut seen, &incoming, kinds, &next)?;
    let no = run(branch(Branch::No), false, &mut seen, &incoming, kinds, &next)?;
    if yes.rest != no.rest {
        return None;
    }
    let mut after = Vec::new();
    if let Some(join) = yes.rest {
        if incoming.get(join) != Some(&2) || kinds.get(join) == Some(&NodeKind::Condition) || seen.contains(join) {
            return None;
        }
        seen.insert(join);
        after.push(join.to_owned());
        let rest = run(next(join), false, &mut seen, &incoming, kinds, &next)?;
        if rest.rest.is_some() {
            return None;
        }
        after.extend(rest.list);
    }
    (seen.len() == core.len()).then_some(Core {
        main: start.list,
        condition: Some(condition),
        yes: yes.list,
        no: no.list,
        after,
    })
}

struct Run<'a> {
    list: Vec<String>,
    condition: Option<String>,
    rest: Option<&'a str>,
}

fn run<'a>(
    first: Option<&'a str>,
    stop_at_condition: bool,
    seen: &mut HashSet<&'a str>,
    incoming: &HashMap<&str, usize>,
    kinds: &HashMap<&str, NodeKind>,
    next: &dyn Fn(&str) -> Option<&'a str>,
) -> Option<Run<'a>> {
    let mut list = Vec::new();
    let mut current = first;
    while let Some(id) = current.filter(|id| incoming.get(id).copied().unwrap_or(0) == 1) {
        if !seen.insert(id) {
            return None;
        }
        if kinds.get(id) == Some(&NodeKind::Condition) {
            if !stop_at_condition {
                return None;
            }
            return Some(Run { list, condition: Some(id.to_owned()), rest: None });
        }
        list.push(id.to_owned());
        current = next(id);
    }
    Some(Run { list, condition: None, rest: current })
}

/// Rewrites the lines of an outline and lays the steps out in columns. Notes and loose lines stay.
pub fn graph_from_outline(graph: &Graph, outline: &Outline) -> Graph {
    let kinds: HashMap<&str, NodeKind> = graph.nodes.iter().map(|n| (n.id.as_str(), n.kind)).collect();
    let mut edges = Vec::new();
    let link = |edges: &mut Vec<Edge>, from: &str, to: &str, branch: Option<Branch>| {
        let id = match branch {
            Some(branch) => format!("{from}.{}>{to}:text", branch.as_str()),
            None => format!("{from}>{to}:text"),
        };
        edges.push(Edge { id, from: from.to_owned(), to: to.to_owned(), port: PortType::Text, branch });
    };
    #[derive(Clone)]
    struct Source {
        from: String,
        branch: Option<Branch>,
    }
    let walk = |edges: &mut Vec<Edge>, items: &[String], start: Vec<Source>| {
        let mut sources = start;
        let mut prompts = Vec::new();
        for id in items {
            if kinds.get(id.as_str()) == Some(&NodeKind::Prompt) {
                prompts.push(id.clone());
                continue;
            }
            for source in &sources {
                link(edges, &source.from, id, source.branch);
            }
            for prompt in &prompts {
                link(edges, prompt, id, None);
            }
            prompts.clear();
            if kinds.get(id.as_str()) != Some(&NodeKind::Output) {
                sources = vec![Source { from: id.clone(), branch: None }];
            }
        }
        (sources, prompts)
    };
    let (mut sources, mut prompts) =
        walk(&mut edges, &outline.main, vec![Source { from: outline.trigger.clone(), branch: None }]);
    if let Some(condition) = &outline.condition {
        for source in &sources {
            link(&mut edges, &source.from, condition, source.branch);
        }
        for prompt in &prompts {
            link(&mut edges, prompt, condition, None);
        }
        let (yes, _) =
            walk(&mut edges, &outline.yes, vec![Source { from: condition.clone(), branch: Some(Branch::Yes) }]);
        let (no, _) = walk(&mut edges, &outline.no, vec![Source { from: condition.clone(), branch: Some(Branch::No) }]);
        if !outline.after.is_empty() {
            sources = yes.into_iter().chain(no).collect();
            prompts.clear();
            walk(&mut edges, &outline.after, sources);
        }
    }
    let loose: HashSet<&str> = outline.loose.iter().map(String::as_str).collect();
    edges.extend(
        graph.edges.iter().filter(|e| loose.contains(e.from.as_str()) && loose.contains(e.to.as_str())).cloned(),
    );
    let mut next = graph.clone();
    next.edges = edges;
    place(&mut next, outline);
    next
}

fn place(graph: &mut Graph, outline: &Outline) {
    let mut col = 0_i64;
    let set = |graph: &mut Graph, id: &str, column: i64, y: f64| {
        if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == id) {
            node.at = Point { x: 80.0 + column as f64 * 400.0, y };
        }
    };
    let place_list = |graph: &mut Graph, items: &[String], mut column: i64, y: f64| {
        for id in items {
            column += 1;
            set(graph, id, column, y);
        }
        column
    };
    set(graph, &outline.trigger, 0, 80.0);
    col = place_list(graph, &outline.main, col, 80.0);
    let Some(condition) = &outline.condition else { return };
    col += 1;
    set(graph, condition, col, 80.0);
    let yes = place_list(graph, &outline.yes, col, 80.0);
    let no = place_list(graph, &outline.no, col, 300.0);
    place_list(graph, &outline.after, yes.max(no), 80.0);
}

/// Top to bottom on a detail page: a layer's steps go left to right, then top to bottom.
pub fn reading_order(graph: &Graph) -> Vec<String> {
    let back = loop_edges(graph);
    let steps: Vec<&Node> = graph.nodes.iter().filter(|n| n.kind.is_step()).collect();
    let mut indegree: HashMap<&str, usize> = steps.iter().map(|n| (n.id.as_str(), 0)).collect();
    let mut outs: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in &graph.edges {
        if back.contains(&edge.id) {
            continue;
        }
        if indegree.contains_key(edge.to.as_str()) && indegree.contains_key(edge.from.as_str()) {
            *indegree.entry(edge.to.as_str()).or_insert(0) += 1;
            outs.entry(edge.from.as_str()).or_default().push(edge.to.as_str());
        }
    }
    let mut ready: Vec<&str> = indegree.iter().filter(|(_, n)| **n == 0).map(|(id, _)| *id).collect();
    let pos = |id: &str| steps.iter().find(|n| n.id == id).map(|n| (n.at.x, n.at.y)).unwrap_or((0.0, 0.0));
    ready.sort_by(|a, b| {
        pos(a)
            .0
            .partial_cmp(&pos(b).0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(pos(a).1.partial_cmp(&pos(b).1).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.cmp(b))
    });
    let mut order = Vec::new();
    while let Some(id) = ready.first().copied() {
        ready.remove(0);
        order.push(id.to_owned());
        for next in outs.get(id).into_iter().flatten() {
            if let Some(left) = indegree.get_mut(next) {
                *left = left.saturating_sub(1);
                if *left == 0 {
                    ready.push(next);
                }
            }
        }
        ready.sort_by(|a, b| {
            pos(a)
                .0
                .partial_cmp(&pos(b).0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(pos(a).1.partial_cmp(&pos(b).1).unwrap_or(std::cmp::Ordering::Equal))
                .then(a.cmp(b))
        });
    }
    for node in steps {
        if !order.iter().any(|id| id == &node.id) {
            order.push(node.id.clone());
        }
    }
    order
}

/// Rows for the phone's line view. A step sits one row below the deepest numbered step that feeds it.
/// Triggers and notes are left out. A cycle stops instead of looping.
pub fn step_ranks(graph: &Graph) -> Vec<Vec<String>> {
    let numbered: Vec<String> = reading_order(graph)
        .into_iter()
        .filter(|id| {
            graph
                .nodes
                .iter()
                .find(|node| node.id == *id)
                .is_some_and(|node| node.kind != NodeKind::Trigger && !node.kind.is_annotation())
        })
        .collect();
    let set: HashSet<&str> = numbered.iter().map(String::as_str).collect();
    let mut memo = HashMap::<String, usize>::new();
    let mut stack = HashSet::<String>::new();
    let mut rows = Vec::<Vec<String>>::new();
    for id in &numbered {
        let level = step_rank(graph, id, &set, &mut memo, &mut stack);
        if rows.len() <= level {
            rows.resize(level + 1, Vec::new());
        }
        rows[level].push(id.clone());
    }
    rows.retain(|row| !row.is_empty());
    rows
}

fn step_rank(
    graph: &Graph,
    id: &str,
    set: &HashSet<&str>,
    memo: &mut HashMap<String, usize>,
    stack: &mut HashSet<String>,
) -> usize {
    if let Some(known) = memo.get(id) {
        return *known;
    }
    if !stack.insert(id.to_owned()) {
        return 0;
    }
    let mut best = None;
    for edge in &graph.edges {
        if edge.to != id || !set.contains(edge.from.as_str()) {
            continue;
        }
        let below = step_rank(graph, &edge.from, set, memo, stack).saturating_add(1);
        best = Some(best.map_or(below, |so_far: usize| so_far.max(below)));
    }
    stack.remove(id);
    let value = best.unwrap_or(0);
    memo.insert(id.to_owned(), value);
    value
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
        if kind == NodeKind::Condition {
            fields.insert("op".into(), json!("empty"));
        }
        Node { id: id.into(), kind, at: Point { x: 0.0, y: 0.0 }, size: None, locked: false, fields, schedule: None }
    }

    #[test]
    fn a_straight_line_round_trips() {
        let graph = Graph {
            nodes: vec![node("t", NodeKind::Trigger), node("a", NodeKind::Agent), node("o", NodeKind::Output)],
            edges: vec![
                Edge { id: "t>a:text".into(), from: "t".into(), to: "a".into(), port: PortType::Text, branch: None },
                Edge { id: "a>o:text".into(), from: "a".into(), to: "o".into(), port: PortType::Text, branch: None },
            ],
            strokes: vec![],
        };
        let outline = outline_of(&graph).expect("outline");
        assert_eq!(outline.main, ["a", "o"]);
        let again = graph_from_outline(&graph, &outline);
        assert_eq!(outline_of(&again).expect("round trip").main, outline.main);
        assert_eq!(edit_mode(&graph, "graph"), EditMode::Outline);
    }

    #[test]
    fn step_ranks_follow_the_deepest_parent() {
        let mut prompt = node("p", NodeKind::Prompt);
        prompt.at.x = 0.0;
        let mut agent_a = node("a", NodeKind::Agent);
        agent_a.at.x = 80.0;
        let mut agent_b = node("b", NodeKind::Agent);
        agent_b.at.x = 200.0;
        let mut output = node("o", NodeKind::Output);
        output.at.x = 140.0;
        let graph = Graph {
            nodes: vec![node("t", NodeKind::Trigger), prompt, agent_a, agent_b, output],
            edges: vec![
                Edge { id: "t>p:text".into(), from: "t".into(), to: "p".into(), port: PortType::Text, branch: None },
                Edge { id: "p>a:text".into(), from: "p".into(), to: "a".into(), port: PortType::Text, branch: None },
                Edge { id: "p>b:text".into(), from: "p".into(), to: "b".into(), port: PortType::Text, branch: None },
                Edge { id: "a>o:text".into(), from: "a".into(), to: "o".into(), port: PortType::Text, branch: None },
                Edge { id: "b>o:text".into(), from: "b".into(), to: "o".into(), port: PortType::Text, branch: None },
            ],
            strokes: vec![],
        };
        assert_eq!(step_ranks(&graph), vec![vec!["p".to_owned()], vec!["a".into(), "b".into()], vec!["o".into()]]);
    }

    #[test]
    fn two_conditions_are_fields_only() {
        let graph = Graph {
            nodes: vec![node("t", NodeKind::Trigger), node("c1", NodeKind::Condition), node("c2", NodeKind::Condition)],
            edges: vec![],
            strokes: vec![],
        };
        assert!(outline_of(&graph).is_none());
        assert_eq!(edit_mode(&graph, "graph"), EditMode::Fields);
    }
}
