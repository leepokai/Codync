//! Agent → client: `session/update`s, permission requests and streamed text mapped onto entries.

use super::{Actor, NoticeStyle, Seg, SegKind};
use crate::agent::acp::{self, Incoming};
use crate::chat::context;
use crate::hub::BotStatus;
use crate::remote::push::{self, AlertKind};
use crate::store::{EntryKind, Permission};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

impl Actor {
    pub(super) async fn on_incoming(&mut self, inc: Option<Incoming>) {
        match inc {
            None | Some(Incoming::Closed { .. }) => {
                let tail = match inc {
                    Some(Incoming::Closed { stderr_tail }) => stderr_tail,
                    _ => String::new(),
                };
                self.conn = None;
                // The in-flight prompt request resolves with an error; finish_turn reports it with this output.
                self.exit_tail = Some(tail);
            }
            Some(Incoming::Notification { method, params }) => {
                if method == "session/update"
                    && params["sessionId"].as_str() == self.turn_session.as_deref()
                    && self.turn.is_some()
                {
                    self.on_update(&params["update"]);
                }
            }
            Some(Incoming::Request { id, method, params }) => {
                let Some(conn) = &self.conn else { return };
                let acp = conn.acp.clone();
                if method == "session/request_permission" && self.turn.is_some() {
                    self.on_permission(id, params).await;
                } else {
                    let _ = acp.respond_error(id, -32601, &format!("{method} is not supported by Codync")).await;
                }
            }
        }
    }

    pub(super) fn on_update(&mut self, u: &Value) {
        let turn = self.turn.unwrap_or(0);
        match u["sessionUpdate"].as_str().unwrap_or_default() {
            "agent_message_chunk" => {
                let t = acp::content_text(&u["content"]);
                self.append(SegKind::Text, &t);
                self.set_activity("Writing a reply…");
            }
            "agent_thought_chunk" => {
                let t = acp::content_text(&u["content"]);
                self.append(SegKind::Thought, &t);
                self.set_activity("Thinking…");
            }
            "tool_call" => {
                self.close_seg();
                let tool_id = u["toolCallId"].as_str().unwrap_or_default().to_owned();
                let mut data = json!({
                    "toolCallId": tool_id,
                    "title": u["title"].as_str().unwrap_or("Tool"),
                    "toolKind": u["kind"].as_str().unwrap_or("other"),
                    "status": u["status"].as_str().unwrap_or("pending"),
                    "output": "",
                    "diffs": [],
                    "locations": u["locations"].clone(),
                });
                merge_tool_content(&mut data, &u["content"]);
                let title = data["title"].as_str().unwrap_or("Tool").to_owned();
                if let Some(e) = self.add(EntryKind::Tool, turn, data) {
                    self.tools.insert(tool_id, e.id);
                }
                self.set_activity(super::activity(&title));
            }
            "tool_call_update" => {
                let tool_id = u["toolCallId"].as_str().unwrap_or_default();
                let Some(entry_id) = self.tools.get(tool_id).cloned() else { return };
                let Some(mut e) = self.hub.store.entry(&entry_id) else { return };
                for (k, key) in [("title", "title"), ("kind", "toolKind"), ("status", "status")] {
                    if let Some(v) = u[k].as_str() {
                        e.data[key] = v.into();
                    }
                }
                if !u["locations"].is_null() {
                    e.data["locations"] = u["locations"].clone();
                }
                merge_tool_content(&mut e.data, &u["content"]);
                let status = e.data["status"].as_str().unwrap_or_default().to_owned();
                let title = e.data["title"].as_str().unwrap_or_default().to_owned();
                self.hub.set_entry(&entry_id, &e.data);
                if status == "in_progress" {
                    self.set_activity(super::activity(&title));
                }
            }
            // The agent summarized its context: the next turn gets freshly rendered
            // instructions and memory (see `context`).
            "compaction_update" if u["status"] == "completed" => {
                let Some(sid) = &self.turn_session else { return };
                let id = u["compactionId"].as_str().unwrap_or_default();
                match context::bump_epoch(&self.hub.store, &self.cfg.id, sid, id) {
                    Ok(true) => self.notice("Earlier context was summarized to make room.", NoticeStyle::Info),
                    Ok(false) => {}
                    Err(error) => {
                        tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't record the compaction");
                    }
                }
            }
            "usage_update" => {
                let info = &u["_meta"]["_claude/rateLimit"];
                if info.is_object() {
                    crate::usage::ingest_claude_rate_limit(&self.hub, info);
                }
            }
            "plan" => {
                let data = json!({"entries": u["entries"].clone(), "author": self.cfg.id});
                match &self.plan_entry {
                    Some(id) => {
                        self.hub.set_entry(id, &data);
                    }
                    None => {
                        if let Some(e) = self.add(EntryKind::Plan, turn, data) {
                            self.plan_entry = Some(e.id);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub(super) async fn on_permission(&mut self, rpc_id: Value, params: Value) {
        let options = params["options"].as_array().cloned().unwrap_or_default();
        let tool = &params["toolCall"];
        if self.cfg.permission == Permission::Auto {
            let pick = ["allow_once", "allow_always"]
                .iter()
                .find_map(|k| options.iter().find(|o| o["kind"] == *k))
                .or_else(|| options.first());
            if let (Some(o), Some(conn)) = (pick, &self.conn) {
                let _ = conn
                    .acp
                    .respond(rpc_id, json!({"outcome": {"outcome": "selected", "optionId": o["optionId"]}}))
                    .await;
                return;
            }
        }
        let title = tool["title"]
            .as_str()
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                let eid = self.tools.get(tool["toolCallId"].as_str()?)?;
                self.hub.store.entry(eid)?.data["title"].as_str().map(str::to_owned)
            })
            .unwrap_or_else(|| "Use a tool".into());
        let mut detail = json!({"output": "", "diffs": []});
        merge_tool_content(&mut detail, &tool["content"]);
        let raw = &tool["rawInput"];
        let command = raw["command"].as_str().map(str::to_owned).or_else(|| {
            raw["command"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "))
        });
        let data = json!({
            "title": title,
            "toolKind": tool["kind"].as_str().unwrap_or("other"),
            "command": command,
            "detail": detail["output"],
            "diffs": detail["diffs"],
            "cwd": self.cfg.cwd,
            "options": options.iter().map(|o| json!({"optionId": o["optionId"], "name": o["name"], "kind": o["kind"]})).collect::<Vec<_>>(),
            "status": "pending",
            "selected": Value::Null,
        });
        let turn = self.turn.unwrap_or(0);
        self.close_seg();
        if let Some(e) = self.add(EntryKind::Permission, turn, data) {
            self.perms.insert(e.id, rpc_id);
        }
        self.hub.set_runtime(&self.cfg.id, |r| {
            r.status = BotStatus::NeedsInput;
            r.activity = format!("Needs approval: {title}");
        });
        // Tapping it opens where the card is: the group, for a room turn.
        let target = self.hub.store.bot(&self.lane.chat).ok().flatten().map_or_else(|| self.cfg.clone(), |r| r.config);
        push::notify(
            &self.hub,
            &target,
            Some(&self.cfg.id),
            &format!("{} needs you", self.cfg.name),
            &title,
            AlertKind::NeedsInput,
        );
    }

    pub(super) async fn answer_permission(&mut self, entry_id: &str, option_id: Option<String>) {
        let Some(rpc_id) = self.perms.remove(entry_id) else { return };
        let outcome = match &option_id {
            Some(o) => json!({"outcome": {"outcome": "selected", "optionId": o}}),
            None => json!({"outcome": {"outcome": "cancelled"}}),
        };
        if let Some(conn) = &self.conn {
            let _ = conn.acp.respond(rpc_id, outcome).await;
        }
        if let Some(mut e) = self.hub.store.entry(entry_id) {
            e.data["status"] = if option_id.is_some() { "answered" } else { "cancelled" }.into();
            e.data["selected"] = option_id.into();
            self.hub.set_entry(entry_id, &e.data);
        }
        if self.perms.is_empty() && self.turn.is_some() {
            self.hub.set_runtime(&self.cfg.id, |r| {
                r.status = BotStatus::Working;
                r.activity = "Continuing…".into();
            });
        }
    }

    /// A `send_message`: the user's next bubble, in the turn's lane, right away. Only in the
    /// bot's own chat and threads; a room, a teammate or a routine reads the turn's reply.
    pub(super) fn send_to_user(&mut self, text: String) -> anyhow::Result<()> {
        let Some(turn) = self.turn else { anyhow::bail!("no turn is running") };
        if self.active_group.is_some() || self.active_request.is_some() || self.active_routine.is_some() {
            anyhow::bail!("send_message isn't available in this turn: write your answer as your reply");
        }
        self.close_seg();
        self.add(EntryKind::Agent, turn, json!({"text": text, "final": true}))
            .ok_or_else(|| anyhow::anyhow!("couldn't save the message"))?;
        self.sent.push(text);
        self.set_activity("Working…");
        Ok(())
    }

    // MARK: streaming text segments

    pub(super) fn append(&mut self, kind: SegKind, text: &str) {
        if text.is_empty() {
            return;
        }
        let same = matches!(&self.seg, Seg::Open { kind: k, .. } if *k == kind);
        if !same {
            self.close_seg();
            let (k, data) = match kind {
                SegKind::Text => (EntryKind::Agent, json!({"text": text, "final": false})),
                SegKind::Thought => (EntryKind::Thought, json!({"text": text})),
            };
            if let Some(e) = self.add(k, self.turn.unwrap_or(0), data) {
                if kind == SegKind::Text {
                    self.last_text = Some(e.id.clone());
                }
                self.seg =
                    Seg::Open { kind, entry_id: e.id, buf: text.to_owned(), flushed: Instant::now(), dirty: false };
            }
            return;
        }
        if let Seg::Open { buf, dirty, .. } = &mut self.seg {
            buf.push_str(text);
            *dirty = true;
        }
    }

    /// Persists streamed text at most every 300 ms (or immediately with `force`).
    pub(super) fn flush(&mut self, force: bool) {
        if let Seg::Open { kind, entry_id, buf, flushed, dirty } = &mut self.seg
            && *dirty
            && (force || flushed.elapsed() >= Duration::from_millis(300))
        {
            let data = match kind {
                SegKind::Text => json!({"text": buf, "final": false, "author": self.cfg.id}),
                SegKind::Thought => json!({"text": buf, "author": self.cfg.id}),
            };
            self.hub.set_entry(entry_id, &data);
            *flushed = Instant::now();
            *dirty = false;
        }
    }

    pub(super) fn close_seg(&mut self) {
        self.flush(true);
        self.seg = Seg::None;
    }
}

/// Folds ACP tool-call `content` into `{output, diffs}` on an entry's data.
pub fn merge_tool_content(data: &mut Value, content: &Value) {
    let Some(items) = content.as_array() else { return };
    let mut out = String::new();
    let mut diffs = vec![];
    for item in items {
        match item["type"].as_str() {
            Some("diff") => {
                let path = item["path"].as_str().unwrap_or_default();
                let old = item["oldText"].as_str().unwrap_or_default();
                let new = item["newText"].as_str().unwrap_or_default();
                diffs.push(diff_summary(path, old, new));
            }
            Some("terminal") => {}
            _ => {
                let t = acp::content_text(item);
                if !t.is_empty() {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(&t);
                }
            }
        }
    }
    if !out.is_empty() {
        data["output"] = acp::truncate(&out, 6000).into();
    }
    if !diffs.is_empty() {
        data["diffs"] = diffs.into();
    }
}

/// Changed-region summary: trims the common prefix/suffix and reports the middle.
pub fn diff_summary(path: &str, old: &str, new: &str) -> Value {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..].iter().rev().zip(b[prefix..].iter().rev()).take_while(|(x, y)| x == y).count();
    let removed = &a[prefix..a.len() - suffix];
    let added = &b[prefix..b.len() - suffix];
    let mut unified = String::new();
    for (sign, lines) in [('-', removed), ('+', added)] {
        for l in lines {
            unified.push(sign);
            unified.push_str(l);
            unified.push('\n');
        }
    }
    json!({
        "path": path,
        "added": added.len(),
        "removed": removed.len(),
        "isNew": old.is_empty(),
        "startLine": prefix + 1,
        "patch": acp::truncate(&unified, 6000),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_counts_changed_middle() {
        let d = diff_summary("f", "a\nb\nc\n", "a\nB\nB2\nc\n");
        assert_eq!(d["added"], 2);
        assert_eq!(d["removed"], 1);
        assert_eq!(d["startLine"], 2);
        assert_eq!(d["patch"], "-b\n+B\n+B2\n");
    }

    #[test]
    fn tool_content_merges_text_and_diffs() {
        let mut data = json!({"output": "", "diffs": []});
        merge_tool_content(
            &mut data,
            &json!([
                {"type": "content", "content": {"type": "text", "text": "ok"}},
                {"type": "diff", "path": "/a.rs", "oldText": "", "newText": "x\n"}
            ]),
        );
        assert_eq!(data["output"], "ok");
        assert_eq!(data["diffs"][0]["isNew"], true);
    }
}
