//! The script face of a workflow: generate one from the canvas, or accept one a bot wrote.

#![allow(clippy::too_many_lines, clippy::map_unwrap_or, clippy::needless_pass_by_value)]

#[path = "script_parse.rs"]
mod parse;

use super::graph::BotState;
use super::limits::MAX_TITLE;
use super::model::{Branch, Edge, Graph, Node, NodeKind, Point, PortType, field_str};
use super::outline::outline_of;
use anyhow::{Result, bail};
use parse::Kind;
use serde_json::{Map, Value, json};
use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;

pub use parse::{Meta, phases_of};

pub struct Applied {
    pub source: &'static str,
    pub script: String,
    pub graph: Graph,
    pub title: Option<String>,
}

struct Draft<'a> {
    graph: &'a Graph,
    params: &'a mut BTreeSet<&'static str>,
    vars: HashMap<String, String>,
    lines: &'a mut Vec<String>,
    seq: &'a mut u32,
    last: Option<String>,
}

pub fn has_body(graph: &Graph) -> bool {
    graph.nodes.iter().any(|n| {
        matches!(
            n.kind,
            NodeKind::Prompt | NodeKind::Agent | NodeKind::Condition | NodeKind::Approval | NodeKind::Output
        )
    })
}

/// A canvas save rewrites the script people and bots read. The walker still runs the graph.
pub fn generate(title: &str, graph: &Graph) -> String {
    let mut params = BTreeSet::new();
    let mut lines = Vec::new();
    let mut seq = 0u32;
    let mut draft =
        Draft { graph, params: &mut params, vars: HashMap::new(), lines: &mut lines, seq: &mut seq, last: None };
    if let Some(outline) = outline_of(graph) {
        emit_list(&outline.main, &mut draft);
        if let Some(condition) = &outline.condition {
            emit_condition(condition, &outline.yes, &outline.no, &mut draft);
        }
        emit_list(&outline.after, &mut draft);
        emit_list(&outline.loose, &mut draft);
    } else {
        let ids = super::outline::reading_order(graph);
        emit_list(&ids, &mut draft);
    }
    let returned = draft.last.unwrap_or_else(|| {
        params.insert("args");
        "args.input".into()
    });
    let head = if params.is_empty() {
        "{}".into()
    } else {
        let ordered = ["agent", "parallel", "pipeline", "phase", "log", "args", "approval", "output"];
        let names: Vec<&str> = ordered.iter().copied().filter(|name| params.contains(name)).collect();
        format!("{{ {} }}", names.join(", "))
    };
    let mut body = String::new();
    for line in lines {
        body.push_str("  ");
        body.push_str(&line);
        body.push('\n');
    }
    body.push_str("  return ");
    body.push_str(&returned);
    body.push('\n');
    format!(
        "export const meta = {{\n  name: {},\n  description: \"\"\n}}\n\nexport default async function ({head}) {{\n{body}}}\n",
        js_str(&title.chars().take(MAX_TITLE).collect::<String>())
    )
}

pub fn script_title(script: &str) -> Option<String> {
    parse::meta_name(script)
}

pub fn agent_ids(script: &str) -> Result<Vec<String>> {
    let agents = parse::check(script, &|_| BotState::Ok)?.agents;
    if agents.iter().any(String::is_empty) {
        bail!("this workflow can only call you: name this bot in every agent() call");
    }
    Ok(agents)
}

pub fn prepare(
    script: &str,
    previous: &Graph,
    previous_script: &str,
    bots: &impl Fn(&str) -> BotState,
) -> Result<Applied> {
    let checked = parse::check(script, bots)?;
    let title = title_change(&checked.meta, previous_script);
    let graph = match &checked.kind {
        Kind::Static(body) => parse::assemble(previous, body),
        Kind::Dynamic => triggers_only(previous),
    };
    Ok(Applied {
        source: if matches!(checked.kind, Kind::Static(_)) { "graph" } else { "script" },
        script: script.to_owned(),
        graph,
        title,
    })
}

fn title_change(meta: &Meta, previous_script: &str) -> Option<String> {
    if previous_script.is_empty() {
        return Some(meta.name.clone());
    }
    match parse::meta_name(previous_script) {
        Some(old) if old != meta.name => Some(meta.name.clone()),
        _ => None,
    }
}

fn triggers_only(previous: &Graph) -> Graph {
    Graph {
        nodes: previous
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Trigger || n.kind.is_annotation())
            .cloned()
            .collect(),
        edges: Vec::new(),
        strokes: previous.strokes.clone(),
    }
}

fn emit_list(ids: &[String], draft: &mut Draft<'_>) {
    for id in ids {
        let Some(node) = draft.graph.nodes.iter().find(|n| n.id == *id) else { continue };
        if node.kind == NodeKind::Trigger || node.kind == NodeKind::Condition || node.kind.is_annotation() {
            continue;
        }
        emit_node(node, draft);
    }
}

fn emit_condition(id: &str, yes: &[String], no: &[String], draft: &mut Draft<'_>) {
    let Some(node) = draft.graph.nodes.iter().find(|n| n.id == id) else { return };
    let probe = draft.last.clone().unwrap_or_else(|| {
        draft.params.insert("args");
        "args.input".into()
    });
    let op = field_str(&node.fields, "op").unwrap_or("contains");
    let value = field_str(&node.fields, "value").unwrap_or("");
    let test = match op {
        "equals" => format!("{probe} === {}", js_str(value)),
        "empty" => format!("{probe}.trim() === \"\""),
        "regex" => format!("match({probe}, {})", js_str(value)),
        _ => format!("{probe}.includes({})", js_str(value)),
    };
    let result = fresh(draft.seq);
    draft.lines.push(format!("// @node {id}"));
    draft.lines.push(format!("let {result};"));
    draft.lines.push(format!("if ({test}) {{"));
    let saved_vars = draft.vars.clone();
    let saved_last = draft.last.clone();
    emit_list(yes, draft);
    if let Some(value) = draft.last.clone() {
        draft.lines.push(format!("{result} = {value};"));
    }
    draft.lines.push("} else {".into());
    draft.vars = saved_vars;
    draft.last = saved_last;
    emit_list(no, draft);
    if let Some(value) = draft.last.clone() {
        draft.lines.push(format!("{result} = {value};"));
    }
    draft.lines.push("}".into());
    draft.last = Some(result);
}

fn emit_node(node: &Node, draft: &mut Draft<'_>) {
    draft.lines.push(format!("// @node {}", node.id));
    match node.kind {
        NodeKind::Prompt => {
            let var = fresh(draft.seq);
            let text = field_str(&node.fields, "text").unwrap_or("");
            draft.lines.push(format!("const {var} = {}", js_str(text)));
            draft.vars.insert(node.id.clone(), var.clone());
            draft.last = Some(var);
        }
        NodeKind::Agent => {
            draft.params.insert("agent");
            let var = fresh(draft.seq);
            let bot = field_str(&node.fields, "botId").unwrap_or("");
            let task = field_str(&node.fields, "task").unwrap_or("");
            let label = label_of(task);
            let text = text_expr(node, draft.graph, &draft.vars, draft.params, task);
            draft.lines.push(format!(
                "const {var} = await agent({}, {text}, {{ label: {} }})",
                js_str(bot),
                js_str(&label)
            ));
            draft.vars.insert(node.id.clone(), var.clone());
            draft.last = Some(var);
        }
        NodeKind::Approval => {
            draft.params.insert("approval");
            let question = field_str(&node.fields, "question").unwrap_or("");
            let question = if question.is_empty() { "Approve this step?" } else { question };
            draft.lines.push(format!("await approval({})", js_str(question)));
        }
        NodeKind::Output => {
            draft.params.insert("output");
            let target = field_str(&node.fields, "target").unwrap_or("notify");
            let text = text_expr(node, draft.graph, &draft.vars, draft.params, "");
            if target == "chat" {
                let bot = field_str(&node.fields, "botId").unwrap_or("");
                draft.lines.push(format!("await output({{ target: \"chat\", botId: {}, text: {text} }})", js_str(bot)));
            } else {
                draft.lines.push(format!("await output({{ target: \"notify\", text: {text} }})"));
            }
        }
        _ => {}
    }
}

fn text_expr(
    node: &Node,
    graph: &Graph,
    vars: &HashMap<String, String>,
    params: &mut BTreeSet<&'static str>,
    task: &str,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut from_trigger = false;
    for edge in &graph.edges {
        if edge.to != node.id {
            continue;
        }
        let Some(source) = graph.nodes.iter().find(|n| n.id == edge.from) else { continue };
        if source.kind == NodeKind::Trigger {
            from_trigger = true;
        } else if let Some(var) = vars.get(&source.id) {
            parts.push(var.clone());
        }
    }
    if from_trigger {
        params.insert("args");
    }
    if parts.is_empty() {
        return if task.is_empty() {
            if from_trigger { "args.input".into() } else { js_str("") }
        } else if from_trigger {
            format!("{} + args.input", js_str(&format!("{task}\n\n")))
        } else {
            js_str(task)
        };
    }
    if parts.len() == 1 && !from_trigger {
        let name = &parts[0];
        return if task.is_empty() { name.clone() } else { format!("{} + {name}", js_str(&format!("{task}\n\n"))) };
    }
    let mut all = Vec::new();
    if from_trigger {
        all.push("args.input".into());
    }
    all.extend(parts);
    if !task.is_empty() && all.len() == 1 {
        return format!("{} + {}", js_str(&format!("{task}\n\n")), all[0]);
    }
    let mut expr = String::new();
    for (index, part) in all.iter().enumerate() {
        if index > 0 {
            expr.push_str(" + ");
        }
        expr.push_str(&js_str(&format!("Input {}:\n", index + 1)));
        expr.push_str(" + ");
        expr.push_str(part);
        if index + 1 != all.len() {
            expr.push_str(" + ");
            expr.push_str(&js_str("\n\n"));
        }
    }
    expr
}

fn fresh(seq: &mut u32) -> String {
    *seq += 1;
    format!("step{seq}")
}

fn label_of(task: &str) -> String {
    let line = task.lines().next().unwrap_or("").trim();
    let line = if line.is_empty() { "Agent" } else { line };
    line.chars().take(80).collect()
}

fn js_str(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => {
                let _ = write!(out, "\\u{:04x}", u32::from(ch));
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

pub fn node(id: String, kind: NodeKind, x: f64, y: f64, fields: Map<String, Value>) -> Node {
    Node { id, kind, at: Point { x, y }, size: None, locked: false, fields, schedule: None }
}

pub fn edge(from: &str, to: &str, branch: Option<Branch>) -> Edge {
    Edge { id: format!("{from}>{to}:text"), from: from.to_owned(), to: to.to_owned(), port: PortType::Text, branch }
}

/// The later step waits for the earlier one. The line carries no text.
pub fn order_edge(from: &str, to: &str) -> Edge {
    Edge {
        id: format!("{from}>{to}:order"),
        from: from.to_owned(),
        to: to.to_owned(),
        port: PortType::Text,
        branch: None,
    }
}

pub fn field_map(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs.iter().map(|(key, value)| ((*key).to_owned(), value.clone())).collect()
}

pub fn json_str(text: &str) -> Value {
    json!(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::model::Point;

    fn manual(id: &str) -> Node {
        node(id.into(), NodeKind::Trigger, 80.0, 80.0, field_map(&[("trigger", json!("manual"))]))
    }

    #[test]
    fn a_generated_chain_compiles_back_to_the_same_nodes() {
        let trigger = manual("t1");
        let agent = node(
            "a1".into(),
            NodeKind::Agent,
            480.0,
            80.0,
            field_map(&[("botId", json!("bot_abc")), ("task", json!("Review this."))]),
        );
        let output = node("o1".into(), NodeKind::Output, 880.0, 80.0, field_map(&[("target", json!("notify"))]));
        let graph = Graph {
            nodes: vec![trigger, agent, output],
            edges: vec![edge("t1", "a1", None), edge("a1", "o1", None)],
            strokes: Vec::new(),
        };
        let script = generate("Review", &graph);
        assert!(script.contains("// @node a1"), "{script}");
        assert!(!script.contains("|| \"\""), "{script}");
        let applied = prepare(&script, &graph, "", &|_| BotState::Ok).unwrap();
        assert_eq!(applied.source, "graph");
        let trigger = applied.graph.nodes.iter().find(|n| n.kind == NodeKind::Trigger).unwrap();
        assert_eq!(trigger.id, "t1");
        assert_eq!(trigger.at, Point { x: 80.0, y: 80.0 });
        let agent = applied.graph.nodes.iter().find(|n| n.id == "a1").unwrap();
        assert_eq!(field_str(&agent.fields, "task"), Some("Review this."));
        assert_eq!(field_str(&agent.fields, "botId"), Some("bot_abc"));
        assert!(applied.graph.edges.iter().any(|e| e.from == "t1" && e.to == "a1"));
        assert!(applied.graph.edges.iter().any(|e| e.from == "a1" && e.to == "o1"));
    }

    #[test]
    fn an_approval_waits_without_copying_its_text() {
        let script = "export const meta = { name: \"Ship\", description: \"\" }\nexport default async function ({ agent, approval, output }) {\n  const a = await agent(\"bot\", \"Review\", { label: \"Review\" })\n  await approval(\"Ship?\")\n  await output({ target: \"notify\", text: \"done\" })\n  return \"\"\n}\n";
        let graph = Graph { nodes: vec![manual("t")], edges: Vec::new(), strokes: Vec::new() };
        let applied = prepare(script, &graph, "", &|_| BotState::Ok).unwrap();
        let output = applied.graph.nodes.iter().find(|node| node.kind == NodeKind::Output).unwrap();
        let approval = applied.graph.nodes.iter().find(|node| node.kind == NodeKind::Approval).unwrap();
        assert_eq!(field_str(&output.fields, "text"), Some("done"));
        let into_output: Vec<_> = applied.graph.edges.iter().filter(|edge| edge.to == output.id).collect();
        assert!(into_output.iter().any(|edge| edge.from == approval.id && edge.id.ends_with(":order")));
        assert!(into_output.iter().all(|edge| edge.id.ends_with(":order") || edge.from != approval.id));
    }

    #[test]
    fn awaited_calls_keep_their_order() {
        let script = "export const meta = { name: \"N\", description: \"\" }\nexport default async function ({ agent, output }) {\n  const a = await agent(\"a\", \"first\");\n  const b = await agent(\"b\", \"second\");\n  await output({ target: \"chat\", botId: \"a\", text: a });\n  return \"\"\n}\n";
        let graph = Graph { nodes: vec![manual("t")], edges: Vec::new(), strokes: Vec::new() };
        let applied = prepare(script, &graph, "", &|_| BotState::Ok).unwrap();
        let agent = |bot: &str| {
            applied
                .graph
                .nodes
                .iter()
                .find(|node| node.kind == NodeKind::Agent && field_str(&node.fields, "botId") == Some(bot))
                .unwrap()
                .id
                .clone()
        };
        let first = agent("a");
        let second = agent("b");
        let output = applied.graph.nodes.iter().find(|node| node.kind == NodeKind::Output).unwrap();
        let edges = &applied.graph.edges;
        assert!(edges.iter().any(|edge| edge.from == first && edge.to == second && edge.id.ends_with(":order")));
        assert!(edges.iter().any(|edge| edge.from == second && edge.to == output.id && edge.id.ends_with(":order")));
        assert!(edges.iter().any(|edge| edge.from == first && edge.to == output.id && !edge.id.ends_with(":order")));
    }

    #[test]
    fn one_output_can_follow_another() {
        let script = "export const meta = { name: \"N\", description: \"\" }\nexport default async function ({ agent, output }) {\n  const a = await agent(\"a\", \"first\");\n  await output({ target: \"notify\", text: a });\n  await output({ target: \"notify\", text: a });\n  return \"\"\n}\n";
        let graph = Graph { nodes: vec![manual("t")], edges: Vec::new(), strokes: Vec::new() };
        let applied = prepare(script, &graph, "", &|_| BotState::Ok).unwrap();
        crate::workflows::graph::validate(&applied.graph, &|_| BotState::Ok).unwrap();
        let outputs: Vec<_> = applied.graph.nodes.iter().filter(|node| node.kind == NodeKind::Output).collect();
        assert_eq!(outputs.len(), 2);
        assert!(
            applied
                .graph
                .edges
                .iter()
                .any(|edge| { edge.from == outputs[0].id && edge.to == outputs[1].id && edge.id.ends_with(":order") })
        );
    }

    #[test]
    fn a_string_that_mentions_date_is_not_a_use_of_date() {
        let script = "export const meta = { name: \"Date note\", description: \"\" }\n\nexport default async function ({ output }) {\n  await output({ target: \"notify\", text: \"Date\" })\n  return \"\"\n}\n";
        let graph = Graph { nodes: vec![manual("t")], edges: Vec::new(), strokes: Vec::new() };
        assert!(prepare(script, &graph, "", &|_| BotState::Ok).is_ok());
    }

    #[test]
    fn date_and_workflow_are_rejected() {
        let graph = Graph { nodes: vec![manual("t")], edges: Vec::new(), strokes: Vec::new() };
        let date = "export const meta = { name: \"N\", description: \"\" }\nexport default async function ({}) {\n  return Date.now()\n}\n";
        let err = prepare(date, &graph, "", &|_| BotState::Ok).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err.contains("Date"), "{err}");
        let nested = "export const meta = { name: \"N\", description: \"\" }\nexport default async function ({}) {\n  return workflow()\n}\n";
        let err = prepare(nested, &graph, "", &|_| BotState::Ok).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err.contains("workflow"), "{err}");
    }

    #[test]
    fn eval_inside_a_for_loop_is_rejected() {
        let graph = Graph { nodes: vec![manual("t")], edges: Vec::new(), strokes: Vec::new() };
        let script = "export const meta = { name: \"N\", description: \"\" }\nexport default async function ({}) {\n  for (;;) { eval(\"1\") }\n}\n";
        let err = prepare(script, &graph, "", &|_| BotState::Ok).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err.contains("eval"), "{err}");
    }

    #[test]
    fn a_deeply_nested_script_is_rejected() {
        let graph = Graph { nodes: vec![manual("t")], edges: Vec::new(), strokes: Vec::new() };
        let inner = format!("{}1{}", "[".repeat(80), "]".repeat(80));
        let script = format!(
            "export const meta = {{ name: \"N\", description: \"\" }}\nexport default async function ({{}}) {{\n  return {inner}\n}}\n"
        );
        let err = prepare(&script, &graph, "", &|_| BotState::Ok).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err.contains("nested"), "{err}");
    }
}
