//! Agent → client: `session/update`s, permission requests and streamed text mapped onto entries.

use super::fast_mode::Listing;
use super::tool_entry::{activity, mark_bookkeeping, merge_tool_content};
use super::{Actor, Reacted, Seg, SegKind};
use crate::agent::acp::{self, Incoming};
use crate::hub::BotStatus;
use crate::remote::push::{self, AlertKind};
use crate::store::{EntryKind, Permission};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

/// Who waits for an approval card's answer.
pub(super) enum Answer {
    /// The agent's `session/request_permission`, by its JSON-RPC id.
    Agent(Value),
    /// A Codync tool (`Cmd::Approve`): true when the user allowed it.
    Codync(oneshot::Sender<bool>),
}

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
                // Any session's, turn or not: the agent changed an option itself.
                if method == "session/update"
                    && params["update"]["sessionUpdate"] == "config_option_update"
                    && let Some(sid) = params["sessionId"].as_str()
                {
                    self.note_options(sid, &params["update"], Listing::Update);
                } else if method == "session/update"
                    && params["update"]["sessionUpdate"] == "current_mode_update"
                    && let Some(sid) = params["sessionId"].as_str()
                {
                    self.note_current_mode(sid, params["update"]["currentModeId"].as_str());
                } else if method == "session/update"
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
                mark_bookkeeping(&self.cfg, self.renamed(), &mut data);
                crate::chat::memory::turns::touched(&self.hub, &self.cfg, &data);
                let working = activity(&data, self.renamed()).to_owned();
                if let Some(e) = self.add(EntryKind::Tool, turn, data) {
                    self.tools.insert(tool_id, e.id);
                }
                self.set_activity(&working);
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
                mark_bookkeeping(&self.cfg, self.renamed(), &mut e.data);
                crate::chat::memory::turns::touched(&self.hub, &self.cfg, &e.data);
                let working = (e.data["status"] == "in_progress").then(|| activity(&e.data, self.renamed()).to_owned());
                self.hub.set_entry(&entry_id, &e.data);
                if let Some(working) = working {
                    self.set_activity(&working);
                }
            }
            "compaction_update" => {
                if let Some(conn) = self.conn.as_mut() {
                    conn.reports_compaction = true;
                }
                // Claude's adapter carries the summary on one of its completed updates.
                if u["status"] == "completed" {
                    let summary = acp::content_text(&u["summary"]);
                    self.compacted(
                        u["compactionId"].as_str().unwrap_or_default(),
                        (!summary.trim().is_empty()).then_some(summary),
                    );
                }
            }
            "usage_update" => {
                let info = &u["_meta"]["_claude/rateLimit"];
                if info.is_object() {
                    crate::usage::ingest_claude_rate_limit(&self.hub, info);
                }
                if let Some(used) = u["used"].as_u64() {
                    self.track_usage(used, u["size"].as_u64().unwrap_or(0));
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
        self.show_card(&title, data, Answer::Agent(rpc_id));
    }

    /// `Cmd::Approve`: Allow or Deny on a card, with what the tool will do in its details.
    pub(super) fn ask_approval(&mut self, title: &str, detail: &str, reply: oneshot::Sender<bool>) {
        if self.turn.is_none() {
            return; // dropping `reply` refuses: no turn waits on it
        }
        let data = json!({
            "title": title, "toolKind": "other", "command": Value::Null, "detail": detail, "diffs": [],
            "options": [{"optionId": "allow", "name": "Allow", "kind": "allow_once"},
                        {"optionId": "deny", "name": "Deny", "kind": "reject_once"}],
            "status": "pending", "selected": Value::Null,
        });
        self.show_card(title, data, Answer::Codync(reply));
    }

    /// Posts an approval card in the turn's lane and tells the user it needs them.
    fn show_card(&mut self, title: &str, data: Value, answer: Answer) {
        let turn = self.turn.unwrap_or(0);
        self.close_seg();
        if let Some(e) = self.add(EntryKind::Permission, turn, data) {
            self.perms.insert(e.id, answer);
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
            title,
            AlertKind::NeedsInput,
        );
    }

    /// A Codync card whose tool stopped waiting (it timed out, or its call was dropped) expires, buttons and all.
    pub(super) async fn expire_abandoned(&mut self) {
        let gone: Vec<String> = self
            .perms
            .iter()
            .filter(|(_, answer)| matches!(answer, Answer::Codync(reply) if reply.is_closed()))
            .map(|(id, _)| id.clone())
            .collect();
        for id in gone {
            self.answer_permission(&id, None).await;
        }
    }

    pub(super) async fn answer_permission(&mut self, entry_id: &str, option_id: Option<String>) {
        let Some(answer) = self.perms.remove(entry_id) else { return };
        let mut status = if option_id.is_some() { "answered" } else { "cancelled" };
        match answer {
            Answer::Agent(rpc_id) => {
                let outcome = match &option_id {
                    Some(o) => json!({"outcome": {"outcome": "selected", "optionId": o}}),
                    None => json!({"outcome": {"outcome": "cancelled"}}),
                };
                if let Some(conn) = &self.conn {
                    let _ = conn.acp.respond(rpc_id, outcome).await;
                }
            }
            Answer::Codync(reply) => {
                // The tool stopped waiting (see `expire_abandoned`): nothing acts on the answer.
                if reply.send(option_id.as_deref() == Some("allow")).is_err() {
                    status = "expired";
                }
            }
        }
        if let Some(mut e) = self.hub.store.entry(entry_id) {
            e.data["status"] = status.into();
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
    /// bot's own chat; a room, a bot request or a routine reads the turn's reply.
    pub(super) fn send_to_user(&mut self, text: String) -> anyhow::Result<()> {
        let Some(turn) = self.turn else { anyhow::bail!("no turn is running") };
        if self.active_request.is_some() {
            anyhow::bail!(
                "send_message isn't available in a bot request: your final reply goes to the requesting bot, so put your answer there"
            );
        }
        if self.active_group.is_some() || self.active_routine.is_some() {
            anyhow::bail!("send_message isn't available in this turn: write your answer as your reply");
        }
        self.close_seg();
        self.add(EntryKind::Agent, turn, json!({"text": text, "final": true}))
            .ok_or_else(|| anyhow::anyhow!("couldn't save the message"))?;
        self.sent.push(text);
        self.set_activity("Working…");
        Ok(())
    }

    /// `react`: the emoji goes on the user's message this turn answers, or in a room on a message the bot was shown.
    pub(super) fn react(&mut self, emoji: &str, to: Option<&str>) -> anyhow::Result<()> {
        if self.turn.is_none() {
            anyhow::bail!("no turn is running");
        }
        let target = match (&self.active_group, self.turn_entries.last()) {
            (Some(g), _) => crate::chat::reactions::in_room(&self.hub, &g.lane, &self.cfg.id, to, g.shown)?,
            (None, Some(id)) => id.clone(),
            (None, None) => anyhow::bail!("react works only on a message you're answering in a chat or a group chat"),
        };
        let in_chat = self.active_group.is_none();
        if crate::chat::reactions::reacted_lately(&self.hub, &self.lane, &self.cfg.id, &target)? {
            if in_chat {
                self.reacted = Reacted::Refused;
            }
            anyhow::bail!(
                "you reacted in this chat a moment ago and reactions stay rare: no reaction this time. If the message \
                 needs no words, end your turn by replying exactly \"(pass)\"; don't tell the user about this."
            );
        }
        self.hub.bot_react(&target, &self.cfg.id, emoji)?;
        if in_chat {
            self.reacted = Reacted::With(emoji.trim().to_owned());
        }
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
