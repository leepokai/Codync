//! What an agent said about a tool call before, put back where a later message leaves it out. In ACP a field a tool
//! call's update or a permission request's `toolCall` leaves out is unchanged, and agents do leave them out:
//! claude-agent-acp in the form that reports background work
//! ([`background_tasks_meta`](super::claude::background_tasks_meta)) sends a tool call's `_meta.claudeCode` keys only
//! when they change, and a permission request's `toolCall` with only its id, title and input. Filled in here, every
//! message says it whole, so the events need no state.

use serde_json::{Map, Value};
use std::collections::{HashMap, VecDeque};

/// How many tool calls are remembered, the oldest forgotten first.
const KEPT: usize = 256;
/// The `_meta.claudeCode` keys that say whose a call is, which every update of it is read for.
const IDENTITY: [&str; 2] = ["toolName", "parentToolUseId"];

/// What the tool calls of one agent said so far.
#[derive(Default)]
pub(super) struct Calls {
    known: HashMap<String, Known>,
    order: VecDeque<String>,
}

/// What one tool call said: its identity keys, its kind and its last content that showed a file change.
// ponytail: keeps the last diff of each of the last 256 calls, for an approval card; drop it once the adapter sends
// the permission request's own content.
#[derive(Default)]
struct Known {
    identity: Map<String, Value>,
    kind: Option<Value>,
    diff: Option<Value>,
}

impl Calls {
    /// A `session/update`'s `params`: a tool call's update gets back the identity keys it left out.
    pub(super) fn update(&mut self, params: &mut Value) {
        let u = &mut params["update"];
        let started = u["sessionUpdate"] == "tool_call";
        if !started && u["sessionUpdate"] != "tool_call_update" {
            return;
        }
        let Some(id) = u["toolCallId"].as_str().map(str::to_owned) else { return };
        if started {
            self.known.remove(&id);
        }
        let known = self.remember(id);
        if let Some(kind) = u.get("kind").filter(|k| !k.is_null()) {
            known.kind = Some(kind.clone());
        }
        if u["content"].as_array().is_some_and(|items| items.iter().any(|i| i["type"] == "diff")) {
            known.diff = Some(u["content"].clone());
        }
        let claude = &u["_meta"]["claudeCode"];
        for key in IDENTITY {
            if let Some(value) = claude.get(key) {
                known.identity.insert(key.to_owned(), value.clone());
            }
        }
        if known.identity.is_empty() {
            return;
        }
        let identity = known.identity.clone();
        if !u["_meta"].is_object() {
            u["_meta"] = Value::Object(Map::new());
        }
        if !u["_meta"]["claudeCode"].is_object() {
            u["_meta"]["claudeCode"] = Value::Object(Map::new());
        }
        if let Some(claude) = u["_meta"]["claudeCode"].as_object_mut() {
            for (key, value) in identity {
                claude.entry(key).or_insert(value);
            }
        }
    }

    /// A `session/request_permission`'s `params`: its `toolCall` gets back the kind and the file change it left out.
    pub(super) fn permission(&self, params: &mut Value) {
        let tool = &mut params["toolCall"];
        let Some(known) = tool["toolCallId"].as_str().and_then(|id| self.known.get(id)) else { return };
        let Some(tool) = tool.as_object_mut() else { return };
        for (key, value) in [("kind", &known.kind), ("content", &known.diff)] {
            if let Some(value) = value
                && tool.get(key).is_none_or(Value::is_null)
            {
                tool.insert(key.to_owned(), value.clone());
            }
        }
    }

    fn remember(&mut self, id: String) -> &mut Known {
        if !self.known.contains_key(&id) {
            if self.order.len() == KEPT
                && let Some(oldest) = self.order.pop_front()
            {
                self.known.remove(&oldest);
            }
            self.order.push_back(id.clone());
        }
        self.known.entry(id).or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn update(u: &Value) -> Value {
        json!({"sessionId": "s", "update": u})
    }

    #[test]
    fn a_later_update_gets_back_whose_call_it_is() {
        let mut calls = Calls::default();
        let mut first = update(&json!({"sessionUpdate": "tool_call", "toolCallId": "b",
            "_meta": {"claudeCode": {"toolName": "Bash", "parentToolUseId": "a"}}}));
        calls.update(&mut first);
        let mut later = update(&json!({"sessionUpdate": "tool_call_update", "toolCallId": "b", "status": "completed"}));
        calls.update(&mut later);
        assert_eq!(later["update"]["_meta"]["claudeCode"], json!({"toolName": "Bash", "parentToolUseId": "a"}));
        // What an update says itself is kept as it is.
        let mut own = update(&json!({"sessionUpdate": "tool_call_update", "toolCallId": "b",
            "_meta": {"claudeCode": {"toolResponse": {"stdout": "x"}}}}));
        calls.update(&mut own);
        assert_eq!(own["update"]["_meta"]["claudeCode"]["toolResponse"], json!({"stdout": "x"}));
        assert_eq!(own["update"]["_meta"]["claudeCode"]["toolName"], "Bash");
    }

    #[test]
    fn a_permission_request_gets_back_the_kind_and_the_change_it_asks_about() {
        let mut calls = Calls::default();
        let diff = json!([{"type": "diff", "path": "/a", "oldText": null, "newText": "hi"}]);
        calls.update(&mut update(&json!({"sessionUpdate": "tool_call", "toolCallId": "w", "kind": "edit",
            "content": [], "_meta": {"claudeCode": {"toolName": "Write"}}})));
        calls.update(&mut update(&json!({"sessionUpdate": "tool_call_update", "toolCallId": "w", "content": diff})));
        let mut asked = json!({"toolCall": {"toolCallId": "w", "title": "Write /a", "rawInput": {}}});
        calls.permission(&mut asked);
        assert_eq!(asked["toolCall"]["kind"], "edit");
        assert_eq!(asked["toolCall"]["content"], diff);
        // A request that says them itself keeps them.
        let mut whole = json!({"toolCall": {"toolCallId": "w", "kind": "execute", "content": []}});
        calls.permission(&mut whole);
        assert_eq!(whole["toolCall"]["kind"], "execute");
        assert_eq!(whole["toolCall"]["content"], json!([]));
    }

    #[test]
    fn only_the_latest_calls_are_remembered() {
        let mut calls = Calls::default();
        for n in 0..=KEPT {
            calls.update(&mut update(&json!({"sessionUpdate": "tool_call", "toolCallId": n.to_string(),
                "_meta": {"claudeCode": {"toolName": "Bash"}}})));
        }
        assert_eq!(calls.known.len(), KEPT);
        assert!(!calls.known.contains_key("0"));
    }

    /// Each message of a capture of claude-agent-acp 0.113.0 reporting its background work (`tests/fixtures`), as
    /// the reader hands it on: a permission request as its `Permission`, an update as its event.
    fn read(capture: &str) -> Vec<Result<crate::events::Event, Box<crate::acp::Permission>>> {
        let mut calls = Calls::default();
        capture
            .lines()
            .map(|line| {
                let mut msg: Value = serde_json::from_str(line).expect("a capture line is JSON");
                if msg["method"] == super::super::PERMISSION {
                    calls.permission(&mut msg["params"]);
                    Err(Box::new(crate::acp::Permission::parse(&msg["params"])))
                } else {
                    calls.update(&mut msg["params"]);
                    Ok(crate::events::Update::parse(&msg["params"]).event)
                }
            })
            .collect()
    }

    #[test]
    fn a_subagents_calls_name_their_launch_on_every_update() {
        use crate::events::Event;
        let read = read(include_str!("../../tests/fixtures/claude-background/subagents.jsonl"));
        let calls: Vec<_> = read
            .iter()
            .filter_map(|e| match e {
                Ok(Event::ToolStarted(c) | Event::ToolUpdated(c)) => Some(c),
                _ => None,
            })
            .collect();
        // The commands the two subagents ran: every update of each names the Agent call that launched it.
        let launches: Vec<&str> = calls.iter().filter(|c| c.subagent.is_some()).map(|c| c.id.as_str()).collect();
        let commands: Vec<_> = calls.iter().filter(|c| c.parent.is_some()).collect();
        assert_eq!(commands.len(), 4, "a start and an end for each subagent's command");
        assert!(commands.iter().all(|c| launches.contains(&c.parent.as_deref().unwrap_or_default())));
        // What it was asked arrived after the start, without the tool's name: it still reads as a subagent's.
        let kinds: Vec<_> = calls.iter().filter_map(|c| c.subagent.as_ref()?.kind.clone()).collect();
        assert_eq!(kinds, ["general-purpose", "general-purpose"]);
    }

    #[test]
    fn a_permission_request_shows_the_change_an_earlier_update_named() {
        let read = read(include_str!("../../tests/fixtures/claude-background/write-permission.jsonl"));
        let Some(Err(asked)) = read.iter().find(|e| e.is_err()) else { panic!("the capture asks once") };
        assert_eq!(asked.tool.kind.as_deref(), Some("edit"));
        let content = asked.tool.content.as_deref().unwrap_or_default();
        assert!(matches!(content, [crate::events::ToolContent::Diff { new_text, .. }] if new_text == "hello\n"));
    }

    #[test]
    fn background_work_reads_as_tasks_from_start_to_end() {
        use crate::events::{Event, Task, TaskChange as C, TaskEnd, TaskKind as K};
        let read = read(include_str!("../../tests/fixtures/claude-background/background.jsonl"));
        let tasks: Vec<&Task> = read
            .iter()
            .filter_map(|e| match e {
                Ok(Event::Task(t)) => Some(t),
                _ => None,
            })
            .collect();
        let started: Vec<(K, &str)> = tasks
            .iter()
            .filter_map(|t| match &t.change {
                C::Started { kind, name, .. } => Some((*kind, name.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(
            started,
            [
                (K::Shell, "Sleep in the background"),
                (K::Workflow, "probe"),
                (K::Shell, "Sleep in the background"),
                (K::Turn, "Claude is working"),
                (K::Turn, "Claude is working"),
            ]
        );
        let workflow = |t: &&&Task| t.id == "wex5xfcjx";
        let activity: Vec<_> = tasks
            .iter()
            .filter(workflow)
            .filter_map(|t| match &t.change {
                C::Progress { activity } => activity.clone(),
                _ => None,
            })
            .collect();
        assert_eq!(activity, ["Count: two"]);
        assert!(tasks.iter().filter(workflow).any(|t| t.change == C::Ended(TaskEnd::Completed)));
        // The workflow's call started it: the launch says which.
        let tool_call = tasks.iter().find_map(|t| match &t.change {
            C::Started { kind: K::Workflow, tool_call, .. } => tool_call.clone(),
            _ => None,
        });
        assert_eq!(tool_call.as_deref(), Some("toolu_whJ2xWwISk0XDGmf"));
    }
}
