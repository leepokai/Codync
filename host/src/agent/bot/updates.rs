//! Agent → client: `session/update`s, permission requests and streamed text mapped onto entries.

use super::fast_mode::Listing;
use super::tool_entry::{activity, mark_bookkeeping, merge_subagent, merge_tool_content};
use super::{Actor, Reacted, Seg, SegKind};
use crate::agent::elicitation::Declined;
use crate::hub::BotStatus;
use crate::remote::push::{self, AlertKind};
use crate::store::{BotConfig, Entry, EntryKind, Lane};
use codync_agent::acp::{ELICITATION, Incoming, Permission, Request};
use codync_agent::events::{Event, Update};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

/// Who waits for an approval card's answer.
pub(super) enum Answer {
    /// The agent's `session/request_permission`: dropped unanswered, it tells the agent `cancelled`.
    Agent(Request),
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
                self.background_lost();
            }
            Some(Incoming::Update(update)) => {
                if !self.on_session_wide(&update)
                    && update.session.as_deref() == self.turn_session.as_deref()
                    && self.turn.is_some()
                {
                    self.on_update(update.event);
                }
            }
            Some(Incoming::Notification { .. }) => {}
            Some(Incoming::Request(request)) if self.turn.is_some() => match request.permission() {
                Some(permission) => self.on_permission(request, permission).await,
                // A question for the user, never a permission: the approval setting doesn't answer it.
                None if request.method() == ELICITATION => self.on_elicitation(request).await,
                None => self.answer_outside_turn(request).await,
            },
            Some(Incoming::Request(request)) => self.answer_outside_turn(request).await,
        }
    }

    /// Any session's update, turn or not, about the agent rather than a reply: an option or mode the agent changed
    /// itself, its commands, or work it runs in the background. Returns whether it was one.
    pub(super) fn on_session_wide(&mut self, update: &Update) -> bool {
        let sid = update.session.as_deref();
        match (&update.event, sid) {
            (Event::ConfigOptions(options), Some(sid)) => self.note_options(sid, options, Listing::Update),
            (Event::ModeChanged { mode }, Some(sid)) => self.note_current_mode(sid, mode.as_deref()),
            (Event::Commands(commands), _) => self.note_commands(commands),
            (Event::Task(task), sid) => self.on_task(task.clone(), sid),
            _ => return false,
        }
        true
    }

    /// A request no turn handles: a permission gets `answer_background`, a question is declined (no card can wait for
    /// it), anything else isn't supported (dropping it says so).
    pub(super) async fn answer_outside_turn(&self, request: Request) {
        if let Some(permission) = request.permission() {
            self.answer_background(request, &permission).await;
        } else if request.method() == ELICITATION {
            self.decline_elicitation(request, Declined::NoTurn).await;
        }
    }

    pub(super) fn on_update(&mut self, event: Event) {
        let turn = self.turn.unwrap_or(0);
        match event {
            Event::Text { text, notice } => {
                if let Some(error) =
                    codync_agent::pi::turn_error_text(crate::agent::APP, &self.cfg.backend, notice.as_deref(), &text)
                {
                    self.agent_error = Some(error.to_owned());
                    return;
                }
                self.unheard.clear();
                self.append(SegKind::Text, &text);
                self.set_activity("Writing a reply…");
            }
            Event::Thought { text } => {
                self.append(SegKind::Thought, &text);
                self.set_activity("Thinking…");
            }
            Event::ToolStarted(call) => {
                let call = *call;
                self.unheard.clear();
                self.close_seg();
                let tool_id = call.id;
                let mut data = json!({
                    "toolCallId": tool_id,
                    "title": call.title.as_deref().unwrap_or("Tool"),
                    "toolKind": call.kind.as_deref().unwrap_or("other"),
                    "status": call.status.as_deref().unwrap_or("pending"),
                    "output": "",
                    "diffs": [],
                    "locations": call.locations.unwrap_or_default(),
                });
                merge_tool_content(&mut data, call.content.as_deref());
                merge_subagent(&mut data, call.parent, call.subagent);
                mark_bookkeeping(&self.cfg, self.renamed(), &mut data);
                crate::chat::memory::turns::touched(&self.hub, &self.cfg, &data);
                let working = activity(&data, self.renamed()).to_owned();
                if let Some(e) = self.add(EntryKind::Tool, turn, data) {
                    self.tools.insert(tool_id, e.id);
                }
                self.set_activity(&working);
            }
            Event::ToolUpdated(call) => {
                let call = *call;
                let Some(entry_id) = self.tools.get(&call.id).cloned() else { return };
                let Some(mut e) = self.hub.store.entry(&entry_id) else { return };
                for (value, key) in [(call.title, "title"), (call.kind, "toolKind"), (call.status, "status")] {
                    if let Some(v) = value {
                        e.data[key] = v.into();
                    }
                }
                if let Some(locations) = call.locations {
                    e.data["locations"] = locations;
                }
                merge_tool_content(&mut e.data, call.content.as_deref());
                merge_subagent(&mut e.data, call.parent, call.subagent);
                mark_bookkeeping(&self.cfg, self.renamed(), &mut e.data);
                crate::chat::memory::turns::touched(&self.hub, &self.cfg, &e.data);
                let working = (e.data["status"] == "in_progress").then(|| activity(&e.data, self.renamed()).to_owned());
                self.hub.set_entry(&entry_id, &e.data);
                if let Some(working) = working {
                    self.set_activity(&working);
                }
            }
            Event::Compaction(compaction) => {
                if let Some(conn) = self.conn.as_mut() {
                    conn.reports_compaction = true;
                }
                // Claude's adapter carries the summary on one of its completed updates.
                if compaction.done {
                    self.compacted(&compaction.id, compaction.summary);
                }
            }
            Event::Usage(usage) => {
                if let Some(info) = &usage.claude_rate_limit {
                    let profile = crate::agent::login_profiles::of_bot(&self.hub.store, &self.cfg.backend, &self.cfg);
                    crate::usage::ingest_claude_rate_limit(&self.hub, profile, info);
                }
                if let Some(used) = usage.used {
                    self.track_usage(used, usage.size.unwrap_or(0));
                }
            }
            Event::Plan { entries } => {
                let data = json!({"entries": entries, "author": self.cfg.id});
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

    pub(super) async fn on_permission(&mut self, request: Request, permission: Permission) {
        if self.hub.ceilings.approves_by_itself(&self.cfg)
            && let Some(option) = permission.allow()
        {
            let _ = request.choose(Some(option)).await;
            return;
        }
        let tool = &permission.tool;
        let title = tool
            .title
            .clone()
            .filter(|t| !t.is_empty())
            .or_else(|| {
                let eid = self.tools.get(&tool.id)?;
                self.hub.store.entry(eid)?.data["title"].as_str().map(str::to_owned)
            })
            .unwrap_or_else(|| "Use a tool".into());
        let mut detail = json!({"output": "", "diffs": []});
        merge_tool_content(&mut detail, tool.content.as_deref());
        let data = json!({
            "title": title,
            // The call it asks for: once allowed, its tool entry holds what it returned.
            "toolCallId": tool.id,
            "toolKind": tool.kind.as_deref().unwrap_or("other"),
            "command": permission.command,
            "detail": detail["output"],
            "diffs": detail["diffs"],
            "cwd": self.cfg.cwd,
            "options": permission.options.iter().map(|o| json!({"optionId": o.id, "name": o.name, "kind": o.kind})).collect::<Vec<_>>(),
            "status": "pending",
            "selected": Value::Null,
        });
        self.show_card(&title, data, Answer::Agent(request));
    }

    /// A permission request from work the agent goes on with after its turn ended (a background subagent): no turn
    /// shows a card for it, so it gets what a turn would answer without asking, where the bot approves by itself
    /// (`Ceilings::approves_by_itself`), for one of this bot's own sessions, unless the user stopped its last turn.
    /// Anything else is declined.
    pub(super) async fn answer_background(&self, request: Request, permission: &Permission) {
        let Some(conn) = &self.conn else { return };
        let own = permission.session.as_deref().is_some_and(|sid| conn.loaded.contains_key(sid));
        let pick = (self.hub.ceilings.approves_by_itself(&self.cfg) && own && !self.stop_requested)
            .then(|| permission.allow())
            .flatten();
        tracing::info!(bot = %self.id(), approved = pick.is_some(), title = permission.tool.title.as_deref(), "permission asked outside a turn");
        let _ = request.choose(pick).await;
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

    /// An entry the user must see or act on, in the running turn: in its lane, or, from a thread, where the user is,
    /// its bot's chat, named by the task it works on.
    pub(super) fn add_for_user(&self, kind: EntryKind, mut data: Value) -> Option<Entry> {
        let Some(mark) = crate::chat::work::mark(&self.hub.store, &self.cfg) else {
            return self.add(kind, self.turn.unwrap_or(0), data);
        };
        mark.apply(&mut data);
        self.hub.add_entry(&Lane::main(&mark.bot), kind, self.hub.store.max_turn(&mark.bot), &data)
    }

    /// The chat `entry` is in, for an alert that opens it: the group, for a room turn; its bot's chat, for a thread.
    pub(super) fn chat_of(&self, entry: &Entry) -> BotConfig {
        self.hub.store.bot(&entry.bot_id).ok().flatten().map_or_else(|| self.cfg.clone(), |row| row.config)
    }

    /// Posts an approval card where the user is and tells them it needs them.
    fn show_card(&mut self, title: &str, data: Value, answer: Answer) {
        self.close_seg();
        let card = self.add_for_user(EntryKind::Permission, data);
        let target = card.as_ref().map_or_else(|| self.cfg.clone(), |card| self.chat_of(card));
        if let Some(e) = card {
            self.perms.insert(e.id, answer);
        }
        self.hub.set_runtime(&self.cfg.id, |r| {
            r.status = BotStatus::NeedsInput;
            r.activity = format!("Needs approval: {title}");
        });
        push::notify(
            &self.hub,
            &target,
            (!self.cfg.is_thread()).then_some(self.cfg.id.as_str()),
            &format!("{} needs you", self.cfg.name),
            title,
            AlertKind::NeedsInput,
            None,
        );
    }

    /// A Codync card whose tool stopped waiting (it timed out, or its call was dropped) expires, buttons and all.
    pub(super) async fn expire_abandoned(&mut self) {
        self.expire_abandoned_decisions();
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
            Answer::Agent(request) => {
                let _ = request.choose(option_id.as_deref()).await;
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
        if self.perms.is_empty() && self.asked.is_empty() && self.turn.is_some() {
            self.hub.set_runtime(&self.cfg.id, |r| {
                r.status = BotStatus::Working;
                r.activity = "Continuing…".into();
            });
        }
    }

    /// A `send_message`: the user's next bubble, in the turn's lane, right away. Only in the
    /// bot's own chat; a room, a bot request or a routine reads the turn's reply. `quote`: the message it answers.
    pub(super) fn send_to_user(&mut self, text: String, quote: Option<Value>) -> anyhow::Result<()> {
        let data = crate::chat::replies::with_quote(json!({"text": text, "final": true}), quote);
        let Some(turn) = self.turn else { return self.send_between_turns(&text, data) };
        // A background result can't be told from this turn's own send (no session in the call): it may try again later.
        if self.in_request() {
            anyhow::bail!(
                "send_message isn't available in a bot request: your final reply goes to the requesting bot, so put your answer there {LATER}"
            );
        }
        if self.active_group.is_some() || self.in_routine() || self.active_workflow.is_some() {
            anyhow::bail!("send_message isn't available in this turn: write your answer as your reply {LATER}");
        }
        self.close_seg();
        let saved = self.add(EntryKind::Agent, turn, data);
        self.record_sent(saved, text)?;
        self.set_activity("Working…");
        Ok(())
    }

    /// Saves a message the agent sent the user this turn (`send_message`, a voice note) and counts it as the turn's
    /// reply, so what the agent writes as its reply stays in the trace.
    pub(super) fn record_sent(&mut self, saved: Option<Entry>, text: String) -> anyhow::Result<()> {
        let saved = saved.ok_or_else(|| anyhow::anyhow!("couldn't save the message"))?;
        self.record_completion_reply(&saved);
        self.sent.push(text);
        self.last_text = None;
        Ok(())
    }

    /// A `send_message` from work the agent goes on with after its turn ended (a background subagent's result): a
    /// message in the bot's own chat, with the alert a finished turn sends, unless the user stopped its last turn.
    fn send_between_turns(&self, text: &str, mut data: Value) -> anyhow::Result<()> {
        anyhow::ensure!(self.conn.is_some(), "no turn is running");
        anyhow::ensure!(!self.stop_requested, "the user stopped you: send nothing until they write again");
        data["author"] = self.cfg.id.as_str().into();
        self.add_between_turns(EntryKind::Agent, &data).ok_or_else(|| anyhow::anyhow!("couldn't save the message"))?;
        push::notify(&self.hub, &self.cfg, None, &self.cfg.name, text, AlertKind::Done, None);
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
                self.last_text = None;
            }
            anyhow::bail!(
                "you reacted in this chat a moment ago and reactions stay rare: no reaction this time. If the message \
                 needs no words, end your turn by replying exactly \"(pass)\"; don't tell the user about this."
            );
        }
        self.hub.bot_react(&target, &self.cfg.id, emoji)?;
        if in_chat {
            self.reacted = Reacted::With(emoji.trim().to_owned());
            self.last_text = None;
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
        self.end_seg(true);
    }

    /// Closes the open segment. `mid_turn`: more of the turn may follow, so a message that answers the bot's question
    /// isn't the turn's report; at the turn's end (`close_attempt`) it stands as that too, the turn's last text.
    pub(super) fn end_seg(&mut self, mid_turn: bool) {
        self.flush(true);
        let text = match std::mem::replace(&mut self.seg, Seg::None) {
            Seg::Open { kind: SegKind::Text, entry_id, buf, .. } if !buf.trim().is_empty() => Some((entry_id, buf)),
            _ => None,
        };
        // A thread's message that answers its bot's question (`ask_task`) goes to the bot, not into its report.
        if let Some((entry_id, text)) = text
            && crate::chat::work::ask::answered(&self.hub, &self.cfg, &text)
            && mid_turn
            && self.last_text.as_deref() == Some(entry_id.as_str())
        {
            self.last_text = None;
        }
    }
}

/// Told with a `send_message` refused in a room, request or routine turn, in case it came from earlier work.
const LATER: &str = "(a result of earlier background work: send it again once this turn ends)";
