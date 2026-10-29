//! Bounded GPT-Live Responses protocol state. No I/O or application side effects.

use super::Speaker;
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

const MAX_EVENTS: usize = 32_768;
const MAX_TOOLS: usize = 1_024;
const MAX_RESPONSES: usize = 128;
const MAX_TOOLS_PER_RESPONSE: usize = 16;
const CONTEXT_BYTES: usize = 450;

#[derive(Default)]
pub(super) struct Protocol {
    seen: HashSet<String>,
    tools: HashSet<String>,
    finished: HashSet<String>,
    responses: HashMap<String, Batch>,
    pub closed: bool,
    continuation_pending: bool,
}

struct Batch {
    id: String,
    tools: Vec<Tool>,
}

pub(super) struct Tool {
    pub id: String,
    pub name: String,
    pub arguments: std::result::Result<Value, String>,
}

pub(super) enum Action {
    None,
    Transcript { speaker: Speaker, text: String, start_ms: i64, end_ms: i64 },
    Tools { calls: Vec<Tool>, continue_backend: bool },
}

impl Protocol {
    pub fn receive(&mut self, event: &Value) -> Result<Action> {
        let kind = string(event, "type")?;
        if !matches!(
            kind,
            "session.input_transcript.delta"
                | "session.output_transcript.delta"
                | "response.event"
                | "session.closed"
                | "error"
        ) {
            return Ok(Action::None);
        }
        let id = string(event, "event_id")?;
        if self.seen.contains(id) {
            return Ok(Action::None);
        }
        if self.seen.len() >= MAX_EVENTS {
            bail!("This Live session reached its event limit. Reconnect the call.");
        }
        self.seen.insert(id.to_owned());
        match kind {
            "session.input_transcript.delta" | "session.output_transcript.delta" => {
                let text = event["delta"].as_str().ok_or_else(|| anyhow!("Missing Live transcript text."))?;
                if text.is_empty() {
                    return Ok(Action::None);
                }
                if text.len() > 16 * 1024 {
                    bail!("Live transcript fragment is too large.");
                }
                let start_ms = milliseconds(event, "start_ms")?;
                let end_ms = milliseconds(event, "end_ms")?;
                if end_ms < start_ms {
                    bail!("Invalid Live transcript time range.");
                }
                Ok(Action::Transcript {
                    speaker: if kind == "session.input_transcript.delta" { Speaker::User } else { Speaker::Voice },
                    text: text.to_owned(),
                    start_ms,
                    end_ms,
                })
            }
            "response.event" => self.response(event["delegation_id"].as_str().unwrap_or_default(), &event["event"]),
            "session.closed" => {
                self.closed = true;
                self.responses.clear();
                Ok(Action::None)
            }
            "error" => {
                // The client also surfaces the provider error; do not keep tool actions queued behind it.
                self.responses.clear();
                self.closed = true;
                Ok(Action::None)
            }
            _ => Ok(Action::None),
        }
    }

    fn response(&mut self, delegation: &str, event: &Value) -> Result<Action> {
        match event["type"].as_str().unwrap_or_default() {
            "response.created" => {
                let id = string(&event["response"], "id")?;
                if self.finished.contains(id) || self.responses.values().any(|b| b.id == id) {
                    return Ok(Action::None);
                }
                if self.responses.contains_key(delegation) {
                    bail!("Live started a response before its previous response finished.");
                }
                if self.responses.len() >= MAX_RESPONSES || self.finished.len() >= MAX_EVENTS {
                    bail!("Too many Live responses. Reconnect the call.");
                }
                self.responses.insert(delegation.to_owned(), Batch { id: id.to_owned(), tools: vec![] });
            }
            "response.output_item.done" if event["item"]["type"] == "function_call" => {
                let item = &event["item"];
                let id = string(item, "call_id")?;
                if self.tools.contains(id) {
                    return Ok(Action::None);
                }
                if self.tools.len() >= MAX_TOOLS {
                    bail!("This Live session reached its tool limit. Reconnect the call.");
                }
                let batch = self
                    .responses
                    .get_mut(delegation)
                    .ok_or_else(|| anyhow!("Live sent a tool outside a response."))?;
                if batch.tools.len() >= MAX_TOOLS_PER_RESPONSE {
                    bail!("Too many Live tools in one response.");
                }
                let name = string(item, "name")?.to_owned();
                let arguments = item["arguments"]
                    .as_str()
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    .filter(Value::is_object)
                    .ok_or_else(|| "The tool arguments were not a JSON object.".to_owned());
                self.tools.insert(id.to_owned());
                batch.tools.push(Tool { id: id.to_owned(), name, arguments });
            }
            "response.completed" | "response.failed" | "response.incomplete" | "response.cancelled" => {
                let id = string(&event["response"], "id")?;
                if self.finished.contains(id) {
                    return Ok(Action::None);
                }
                let Some(key) = self.responses.iter().find_map(|(key, b)| (b.id == id).then(|| key.clone())) else {
                    return Ok(Action::None);
                };
                if !delegation.is_empty() && key != delegation {
                    bail!("Live response belongs to another delegation.");
                }
                let batch = self.responses.remove(&key).ok_or_else(|| anyhow!("Live response disappeared."))?;
                self.finished.insert(id.to_owned());
                if event["type"] != "response.completed" || event["response"]["status"] != "completed" {
                    let stranded_results = self.continuation_pending;
                    self.continuation_pending = false;
                    // A cancelled batch with no completed sibling results has performed no work here.
                    // Otherwise stop visibly: retrying or silently dropping already-returned outputs is unsafe.
                    if event["response"]["status"] == "cancelled" && !stranded_results {
                        return Ok(Action::None);
                    }
                    self.closed = true;
                    self.responses.clear();
                    bail!(
                        "GPT-Live's backend response did not complete. The call stopped; check the bot's chat before repeating a task."
                    );
                }
                self.continuation_pending |= !batch.tools.is_empty();
                let continue_backend = self.responses.is_empty() && self.continuation_pending;
                if continue_backend {
                    self.continuation_pending = false;
                }
                if !batch.tools.is_empty() || continue_backend {
                    return Ok(Action::Tools { calls: batch.tools, continue_backend });
                }
            }
            _ => {}
        }
        Ok(Action::None)
    }
}

fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value[field].as_str().filter(|s| !s.is_empty() && s.len() <= 256).ok_or_else(|| anyhow!("Invalid Live `{field}`."))
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "Call timestamps have millisecond precision; sub-millisecond fractions are rounded down after bounds checks"
)]
fn milliseconds(value: &Value, field: &str) -> Result<i64> {
    value[field]
        .as_f64()
        .filter(|v| v.is_finite() && *v >= 0.0 && *v <= 24.0 * 60.0 * 60.0 * 1000.0)
        .map(|v| v as i64)
        .ok_or_else(|| anyhow!("Invalid Live `{field}`."))
}

/// UTF-8 bytes conservatively bound tokens for arbitrary scripts, including emoji and CJK.
pub(super) fn updates(kind: &str, text: &str) -> Result<Vec<Value>> {
    let (event, prefix) = match kind {
        "reply" => ("session.commentary.append", "[Bot replied] "),
        "notice" => ("session.commentary.append", "[Notice] "),
        "progress" => ("session.thinking.append", "[Progress] "),
        _ => bail!("Unknown Live update kind."),
    };
    let mut rest = text.trim();
    let mut events = Vec::new();
    while !rest.is_empty() {
        let mut end = rest.len().min(CONTEXT_BYTES - prefix.len());
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        let (piece, tail) = rest.split_at(end);
        events.push(json!({"type": event, "event_id": uuid::Uuid::new_v4().to_string(),
            "delegation_id": null, "content": format!("{prefix}{piece}")}));
        rest = tail;
    }
    Ok(events)
}
