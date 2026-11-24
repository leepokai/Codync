//! Which steps can start, given the ones already recorded for this run.

use super::limits::{MAX_INPUT, MAX_PASSES};
#[cfg(test)]
use super::model::NodeKind;
use super::model::{Branch, Graph, Node, StepStatus};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct Recorded {
    #[allow(dead_code)]
    pub id: String,
    pub node_id: String,
    pub status: StepStatus,
    pub output: String,
    pub branch: Option<Branch>,
    pub ended_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Start { node_id: String, input: String, truncated: bool, passes: i64 },
    Skip { node_id: String },
    Fail { node_id: String, error: &'static str },
}

/// The steps that can move now. Loop edges don't count as inputs or as something to wait for.
pub fn next_actions(
    graph: &Graph,
    steps: &[Recorded],
    scope: &HashSet<String>,
    loops: &HashSet<String>,
) -> Vec<Action> {
    let nodes: Vec<&Node> = graph.nodes.iter().filter(|n| n.kind.is_step() && scope.contains(&n.id)).collect();
    let mut latest: HashMap<&str, &Recorded> = HashMap::new();
    let mut attempts: HashMap<&str, i64> = HashMap::new();
    for step in steps {
        *attempts.entry(step.node_id.as_str()).or_insert(0) += 1;
        // Rows are in attempt order. An open attempt ends at 0 and still replaces the older one.
        latest.insert(step.node_id.as_str(), step);
    }
    let incoming = |id: &str, looped: bool| {
        graph.edges.iter().filter(move |e| e.to == id && loops.contains(&e.id) == looped).collect::<Vec<_>>()
    };
    let waiting = |id: &str| latest.get(id).is_some_and(|s| s.status.is_open());
    let due = |id: &str| -> bool {
        if waiting(id) {
            return false;
        }
        let Some(had) = latest.get(id) else { return true };
        if had.status.is_open() {
            return false;
        }
        incoming(id, false).into_iter().any(|edge| newer(latest.get(edge.from.as_str()).copied(), had))
            || incoming(id, true).into_iter().any(|edge| newer(latest.get(edge.from.as_str()).copied(), had))
    };
    let order = topo(graph, &nodes, loops);
    let mut actions = Vec::new();
    for id in &order {
        if !due(id) {
            continue;
        }
        let blocked = incoming(id, false).iter().any(|edge| due(&edge.from) || waiting(&edge.from));
        if blocked {
            continue;
        }
        let ins = incoming(id, false);
        let gated: HashSet<&str> =
            ins.iter().filter(|edge| edge.branch.is_some()).map(|edge| edge.from.as_str()).collect();
        let order_unmet = ins.iter().any(|edge| {
            edge.id.ends_with(":order")
                && latest.get(edge.from.as_str()).is_none_or(|source| source.status != StepStatus::Done)
        });
        if order_unmet {
            actions.push(Action::Skip { node_id: (*id).to_owned() });
            continue;
        }
        let mut parts = Vec::new();
        let mut text_edges = 0;
        for edge in &ins {
            if edge.id.ends_with(":order") {
                continue;
            }
            text_edges += 1;
            let Some(source) = latest.get(edge.from.as_str()) else { continue };
            if source.status != StepStatus::Done {
                continue;
            }
            // A conditional line is the gate. A plain line from that same step cannot start the other branch.
            if gated.contains(edge.from.as_str()) {
                if edge.branch != source.branch {
                    continue;
                }
            } else if edge.branch.is_some() && edge.branch != source.branch {
                continue;
            }
            parts.push((edge_pos(graph, &edge.id), source.output.clone()));
        }
        parts.sort_by(|a, b| a.0.cmp(&b.0));
        if text_edges > 0 && parts.is_empty() {
            actions.push(Action::Skip { node_id: (*id).to_owned() });
            continue;
        }
        let passes = attempts.get(*id).copied().unwrap_or(0) + 1;
        if passes > MAX_PASSES {
            actions.push(Action::Fail { node_id: (*id).to_owned(), error: "This step repeated too many times" });
            continue;
        }
        let (input, truncated) = join(parts.into_iter().map(|(_, text)| text).collect());
        actions.push(Action::Start { node_id: (*id).to_owned(), input, truncated, passes });
    }
    actions
}

fn newer(source: Option<&Recorded>, had: &Recorded) -> bool {
    source.is_some_and(|source| source.status == StepStatus::Done && source.ended_at > had.ended_at)
}

fn join(parts: Vec<String>) -> (String, bool) {
    let text = if parts.len() <= 1 {
        parts.into_iter().next().unwrap_or_default()
    } else {
        parts.iter().enumerate().map(|(i, part)| format!("Input {}:\n{part}", i + 1)).collect::<Vec<_>>().join("\n\n")
    };
    if text.chars().count() > MAX_INPUT { (text.chars().take(MAX_INPUT).collect(), true) } else { (text, false) }
}

fn node_pos(graph: &Graph, id: &str) -> (i64, String) {
    let index = graph.nodes.iter().position(|n| n.id == id).unwrap_or(0);
    (i64::try_from(index).unwrap_or(i64::MAX), id.to_owned())
}

fn edge_pos(graph: &Graph, id: &str) -> (i64, String) {
    let index = graph.edges.iter().position(|e| e.id == id).unwrap_or(0);
    (i64::try_from(index).unwrap_or(i64::MAX), id.to_owned())
}

fn topo<'a>(graph: &'a Graph, nodes: &[&'a Node], loops: &HashSet<String>) -> Vec<&'a str> {
    let ids: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let mut indegree: HashMap<&str, usize> = ids.iter().map(|id| (*id, 0)).collect();
    let mut outs: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in &graph.edges {
        if loops.contains(&edge.id) || !ids.contains(edge.from.as_str()) || !ids.contains(edge.to.as_str()) {
            continue;
        }
        *indegree.entry(edge.to.as_str()).or_insert(0) += 1;
        outs.entry(edge.from.as_str()).or_default().push(edge.to.as_str());
    }
    let mut ready: Vec<&str> = indegree.iter().filter(|(_, n)| **n == 0).map(|(id, _)| *id).collect();
    ready.sort_by_key(|id| node_pos(graph, id));
    let mut order = Vec::new();
    while let Some(id) = ready.first().copied() {
        ready.remove(0);
        order.push(id);
        for next in outs.get(id).into_iter().flatten() {
            if let Some(left) = indegree.get_mut(next) {
                *left = left.saturating_sub(1);
                if *left == 0 {
                    ready.push(next);
                    ready.sort_by_key(|id| node_pos(graph, id));
                }
            }
        }
    }
    for node in nodes {
        if !order.contains(&node.id.as_str()) {
            order.push(node.id.as_str());
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::model::{Edge, Point, PortType};
    use serde_json::{Map, json};

    fn node(id: &str, kind: NodeKind) -> Node {
        let mut fields = Map::new();
        fields.insert("trigger".into(), json!("manual"));
        fields.insert("op".into(), json!("contains"));
        fields.insert("value".into(), json!("go"));
        fields.insert("target".into(), json!("notify"));
        Node { id: id.into(), kind, at: Point { x: 0.0, y: 0.0 }, size: None, locked: false, fields, schedule: None }
    }

    fn edge(id: &str, from: &str, to: &str, branch: Option<Branch>) -> Edge {
        Edge { id: id.into(), from: from.into(), to: to.into(), port: PortType::Text, branch }
    }

    #[test]
    fn two_inputs_are_numbered_and_a_no_branch_skips() {
        let graph = Graph {
            nodes: vec![
                node("t", NodeKind::Trigger),
                node("c", NodeKind::Condition),
                node("y", NodeKind::Output),
                node("n", NodeKind::Output),
            ],
            edges: vec![
                edge("t>c:text", "t", "c", None),
                edge("c.yes>y:text", "c", "y", Some(Branch::Yes)),
                edge("c.no>n:text", "c", "n", Some(Branch::No)),
            ],
            strokes: vec![],
        };
        let scope: HashSet<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();
        let start = next_actions(&graph, &[], &scope, &HashSet::new());
        assert_eq!(
            start,
            vec![Action::Start { node_id: "t".into(), input: String::new(), truncated: false, passes: 1 }]
        );
        let trigger = Recorded {
            id: "s1".into(),
            node_id: "t".into(),
            status: StepStatus::Done,
            output: "go".into(),
            branch: None,
            ended_at: 1,
        };
        let cond = Recorded {
            id: "s2".into(),
            node_id: "c".into(),
            status: StepStatus::Done,
            output: "go".into(),
            branch: Some(Branch::No),
            ended_at: 2,
        };
        let next = next_actions(&graph, &[trigger, cond], &scope, &HashSet::new());
        assert!(next.iter().any(|a| matches!(a, Action::Skip { node_id } if node_id == "y")));
        assert!(next.iter().any(|a| matches!(a, Action::Start { node_id, .. } if node_id == "n")));
    }

    #[test]
    fn the_twenty_first_pass_fails() {
        let graph = Graph {
            nodes: vec![node("a", NodeKind::Agent), node("b", NodeKind::Agent)],
            edges: vec![edge("b>a:text", "b", "a", None)],
            strokes: vec![],
        };
        let scope = HashSet::from(["a".into(), "b".into()]);
        let mut steps: Vec<_> = (0..20)
            .map(|i| Recorded {
                id: format!("s{i}"),
                node_id: "a".into(),
                status: StepStatus::Done,
                output: String::new(),
                branch: None,
                ended_at: i,
            })
            .collect();
        steps.push(Recorded {
            id: "b1".into(),
            node_id: "b".into(),
            status: StepStatus::Done,
            output: "again".into(),
            branch: None,
            ended_at: 100,
        });
        let loops = HashSet::from(["b>a:text".into()]);
        let next = next_actions(&graph, &steps, &scope, &loops);
        assert!(matches!(next.first(), Some(Action::Fail { error: "This step repeated too many times", .. })));
    }

    #[test]
    fn an_open_attempt_replaces_an_older_done_one() {
        let graph = Graph { nodes: vec![node("a", NodeKind::Agent)], edges: vec![], strokes: vec![] };
        let scope = HashSet::from(["a".into()]);
        let steps = vec![
            Recorded {
                id: "done".into(),
                node_id: "a".into(),
                status: StepStatus::Done,
                output: "first".into(),
                branch: None,
                ended_at: 50,
            },
            Recorded {
                id: "open".into(),
                node_id: "a".into(),
                status: StepStatus::Running,
                output: String::new(),
                branch: None,
                ended_at: 0,
            },
        ];
        let next = next_actions(&graph, &steps, &scope, &HashSet::new());
        assert!(next.is_empty(), "the open attempt is still the latest one");
    }

    #[test]
    fn a_plain_line_does_not_bypass_a_rejected_branch() {
        let graph = Graph {
            nodes: vec![node("c", NodeKind::Condition), node("o", NodeKind::Output)],
            edges: vec![edge("c>o:text", "c", "o", None), edge("c.yes>o:text", "c", "o", Some(Branch::Yes))],
            strokes: vec![],
        };
        let scope = HashSet::from(["c".into(), "o".into()]);
        let steps = vec![Recorded {
            id: "s".into(),
            node_id: "c".into(),
            status: StepStatus::Done,
            output: "no".into(),
            branch: Some(Branch::No),
            ended_at: 1,
        }];
        let next = next_actions(&graph, &steps, &scope, &HashSet::new());
        assert!(next.iter().any(|action| matches!(action, Action::Skip { node_id } if node_id == "o")));
        assert!(!next.iter().any(|action| matches!(action, Action::Start { node_id, .. } if node_id == "o")));
    }
}
