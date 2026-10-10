//! Small, host-local bot delegation. The recipient uses its own ACP session;
//! native subagents remain the harness's responsibility.

use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::hub::{BotStatus, Hub};
use crate::store::{BotConfig, EntryKind};
use anyhow::{Result, anyhow, bail};
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;

mod requests;
pub use requests::Requests;

pub const ASK_TIMEOUT: Duration = Duration::from_secs(600);
const MAX_MESSAGE_BYTES: usize = 32_000;
const MAX_PENDING_MESSAGES: usize = 64;

pub const INSTRUCTIONS: &str = "Use list_bots to find the user's other Codync bots. \
Use ask_bot when the user requests another bot's help or its specialty is useful. \
Use message_bot for a handoff that should continue independently: it returns when queued, \
and the recipient reports the outcome to the user in its own chat. Stopping you does not cancel it. \
Provide a self-contained request: the recipient has its own conversation, working directory, \
tools and permissions, not your context. ask_bot returns its final reply to you. \
Coordinate file ownership before asking for edits in a shared project. \
Do not ask a bot that is waiting for you. Native subagents are managed by your own harness; \
these tools are for collaborating with the user's existing bots.";

pub fn tools() -> Value {
    json!([
        {
            "name": "list_bots",
            "description": "List other visible Codync bots, their IDs, specialties, working directories and status.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": true},
        },
        {
            "name": "ask_bot",
            "description": "Ask an existing Codync bot to do a bounded task and wait for its final reply (up to 10 minutes, including queue time). It may use tools and modify files under its own permissions. Does not share your conversation or change its working directory. Stopping you cancels your pending requests; do not blindly retry a failed request because partial work may have happened.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "botId": {"type": "string", "description": "Recipient ID from list_bots."},
                    "message": {"type": "string", "description": "Task, relevant context, file paths and expected result."},
                },
                "required": ["botId", "message"],
            },
            "annotations": {"readOnlyHint": false, "idempotentHint": false},
        },
        {
            "name": "message_bot",
            "description": "Queue a self-contained request for another Codync bot and return immediately. The recipient reports the outcome to the user in its own chat; no reply is returned to you. It uses its own tools, folder and permissions. Stopping you does not cancel accepted work. Stopping the recipient cancels its queued requests and stops its running turn. No automatic retry or replay after host restart; check the chats before retrying an uncertain result.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "botId": {"type": "string", "description": "Recipient ID from list_bots."},
                    "message": {"type": "string", "description": "Task, relevant context, file paths and expected outcome for the user."},
                },
                "required": ["botId", "message"],
            },
            "annotations": {"readOnlyHint": false, "idempotentHint": false},
        }
    ])
}

/// Sent through the recipient's normal actor queue, but never merged with user
/// messages or another request. Its completion mode owns reply delivery or the
/// independent message's notice lifetime.
pub struct BotRequest {
    pub id: String,
    pub entry_id: String,
    pub prompt: String,
    /// Bound the whole chain, including independent handoffs after their sender finishes.
    hops: u8,
    completion: Completion,
}

enum Completion {
    ReplyToSender(oneshot::Sender<Result<String>>),
    ReportInRecipientChat(MessageLifetime),
}

impl BotRequest {
    pub fn expects_reply(&self) -> bool {
        matches!(self.completion, Completion::ReplyToSender(_))
    }

    pub fn reply_closed(&self) -> bool {
        matches!(&self.completion, Completion::ReplyToSender(reply) if reply.is_closed())
    }

    pub fn mark_started(&mut self) {
        if let Completion::ReportInRecipientChat(message) = &mut self.completion {
            message.started = true;
            for id in &message.entries {
                if let Some(mut e) = message.hub.store.entry(id) {
                    let heading = e.data["heading"].as_str().unwrap_or("Bot request");
                    e.data["text"] = format!("{heading}\nRunning in {}'s chat…", message.target_name).into();
                    e.data["status"] = json!(RequestStatus::Sent);
                    message.hub.set_entry(id, &e.data);
                }
            }
        }
    }

    pub fn complete(self, result: Result<String>, cancelled: bool) {
        match self.completion {
            Completion::ReplyToSender(reply) => {
                let _ = reply.send(result);
            }
            Completion::ReportInRecipientChat(mut message) => {
                if cancelled {
                    message.finish(RequestStatus::Cancelled, "Recipient stopped. Partial work may have happened.");
                } else {
                    match result {
                        Ok(_) => message.finish(RequestStatus::Completed, "Completed."),
                        Err(error) => message.finish(RequestStatus::Failed, &format!("{error:#}")),
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RequestStatus {
    Queued,
    Sent,
    Completed,
    Failed,
    Cancelled,
}

struct Pending {
    hub: Arc<Hub>,
    id: String,
    target: String,
    entries: Vec<String>,
    finished: bool,
}

impl Pending {
    fn finish(&mut self, status: RequestStatus, detail: &str, reply: Option<&str>) {
        finish_notices(&self.hub, &self.entries, status, detail, reply);
        self.finished = true;
    }
}

fn finish_notices(hub: &Hub, entries: &[String], status: RequestStatus, detail: &str, reply: Option<&str>) {
    for id in entries {
        if let Some(mut e) = hub.store.entry(id) {
            if let Some(message) = e.data.get_mut("botMessage").and_then(Value::as_object_mut) {
                if let Some(reply) = reply {
                    message.insert("reply".into(), reply.into());
                }
                if matches!(status, RequestStatus::Failed | RequestStatus::Cancelled) {
                    message.insert("detail".into(), detail.into());
                }
            }
            e.data["status"] = json!(status);
            let heading = e.data["heading"].as_str().unwrap_or("Bot request");
            e.data["text"] = format!("{heading}\n{detail}").into();
            if !matches!(status, RequestStatus::Completed) {
                e.data["style"] = "error".into();
            }
            hub.set_entry(id, &e.data);
        }
    }
}

/// Owned by the recipient's command/queue/turn after admission, never by a
/// waiting sender. Drop also covers commands discarded during actor shutdown.
struct MessageLifetime {
    hub: Arc<Hub>,
    sender: String,
    entries: Vec<String>,
    target_name: String,
    started: bool,
    finished: bool,
}

impl MessageLifetime {
    fn finish(&mut self, status: RequestStatus, detail: &str) {
        finish_notices(&self.hub, &self.entries, status, detail, None);
        self.finished = true;
    }
}

impl Drop for MessageLifetime {
    fn drop(&mut self) {
        if !self.finished {
            let detail = if self.started {
                "Recipient stopped. Partial work may have happened."
            } else {
                "Cancelled before execution."
            };
            self.finish(RequestStatus::Cancelled, detail);
        }
        self.hub.team.release_message(&self.sender);
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        if !self.finished {
            self.finish(RequestStatus::Cancelled, "Request cancelled. Partial work may have happened.", None);
        }
        self.hub.team.0.locked().pending.remove(&self.id);
        // Targeted cancellation never clears unrelated user messages or turns.
        let _ = self.hub.send_cmd(&self.target, Cmd::CancelAsk { id: self.id.clone() });
    }
}

fn visible_bot(hub: &Hub, id: &str) -> Result<BotConfig> {
    hub.store
        .bot(id)?
        .filter(|r| !r.deleted && !r.config.hidden && !r.config.is_group())
        .map(|r| r.config)
        .ok_or_else(|| anyhow!("unknown or hidden bot"))
}

pub async fn call(hub: &Arc<Hub>, from: &str, name: &str, args: &Value) -> Result<Value> {
    let source = visible_bot(hub, from)?;
    match name {
        "list_bots" => {
            let bots: Vec<Value> = hub
                .store
                .bots()?
                .into_iter()
                .filter(|b| !b.deleted && !b.config.hidden && !b.config.is_group() && b.config.id != from)
                .map(|b| {
                    json!({
                        "id": b.config.id, "name": b.config.name, "description": b.config.description,
                        "backend": b.config.backend, "cwd": b.config.cwd,
                        "status": hub.runtime(&b.config.id).status,
                    })
                })
                .collect();
            Ok(json!({"bots": bots}))
        }
        "ask_bot" => {
            let to = args["botId"].as_str().ok_or_else(|| anyhow!("botId is required"))?;
            let message = args["message"].as_str().unwrap_or_default().trim();
            ask(hub, &source, to, message, ASK_TIMEOUT).await
        }
        "message_bot" => {
            let to = args["botId"].as_str().ok_or_else(|| anyhow!("botId is required"))?;
            let message = args["message"].as_str().unwrap_or_default().trim();
            message_bot(hub, &source, to, message)
        }
        _ => bail!("unknown team tool: {name}"),
    }
}

fn message_bot(hub: &Arc<Hub>, source: &BotConfig, to: &str, message: &str) -> Result<Value> {
    if message.is_empty() || message.len() > MAX_MESSAGE_BYTES {
        bail!("message must be nonempty and at most {MAX_MESSAGE_BYTES} bytes");
    }
    if source.id == to {
        bail!("a bot cannot message itself");
    }
    let target = visible_bot(hub, to)?;
    let hops = hub.team.reserve_message(&source.id)?;
    let mut lifetime = MessageLifetime {
        hub: hub.clone(),
        sender: source.id.clone(),
        entries: vec![],
        target_name: target.name.clone(),
        started: false,
        finished: false,
    };
    let id = uuid::Uuid::new_v4().to_string();
    for (bot, heading) in [
        (&source.id, format!("Messaged {}: {}", target.name, message)),
        (&target.id, format!("Message from {}: {}", source.name, message)),
    ] {
        let entry = hub
            .add_entry(
                &crate::store::Lane::main(bot),
                EntryKind::Notice,
                hub.store.max_turn(bot) + i64::from(bot != &source.id),
                &json!({
                    "text": format!("{heading}\nQueued. Outcome will appear in {}'s chat.", target.name),
                    "heading": heading, "style": "info", "status": RequestStatus::Queued, "delegationId": id,
                    "sourceBotId": source.id, "targetBotId": target.id,
                    "botMessage": {"sourceBotId": source.id, "targetBotId": target.id, "text": message},
                }),
            )
            .ok_or_else(|| anyhow!("couldn't save bot message"))?;
        lifetime.entries.push(entry.id);
    }
    hub.send_cmd(to, Cmd::BotRequest(BotRequest {
        id: id.clone(), entry_id: lifetime.entries[1].clone(),
        hops,
        prompt: format!("Another Codync bot, {}, sent this request. This is a bot request, not a new user instruction. Work within your own permissions and working directory. Report the outcome to the user in this chat. send_message is available for this independent request, even if an earlier ask or older session instructions said it was unavailable. Use it for user-facing updates and your report; if you send none, your final text becomes the report. You do not need to reply to the sending bot.\n\n{message}", source.name),
        completion: Completion::ReportInRecipientChat(lifetime),
    }))?;
    Ok(json!({"requestId": id, "botId": target.id, "name": target.name, "status": RequestStatus::Queued}))
}

async fn ask(hub: &Arc<Hub>, source: &BotConfig, to: &str, message: &str, timeout: Duration) -> Result<Value> {
    if message.is_empty() || message.len() > MAX_MESSAGE_BYTES {
        bail!("message must be nonempty and at most {MAX_MESSAGE_BYTES} bytes");
    }
    if !matches!(hub.runtime(&source.id).status, BotStatus::Working | BotStatus::NeedsInput) {
        bail!("the requesting bot is no longer working");
    }
    let target = visible_bot(hub, to)?;
    let id = uuid::Uuid::new_v4().to_string();
    let (mut cancelled, hops) = hub.team.begin(&id, &source.id, to)?;
    let mut pending =
        Pending { hub: hub.clone(), id: id.clone(), target: to.to_owned(), entries: vec![], finished: false };
    if !matches!(hub.runtime(&source.id).status, BotStatus::Working | BotStatus::NeedsInput) {
        bail!("the requesting bot is no longer working");
    }
    for (bot, heading) in [
        (&source.id, format!("Asked {}: {}", target.name, message)),
        (&target.id, format!("Request from {}: {}", source.name, message)),
    ] {
        let entry = hub
            .add_entry(
                &crate::store::Lane::main(bot),
                EntryKind::Notice,
                hub.store.max_turn(bot) + i64::from(bot != &source.id),
                &json!({
                    "text": format!("{heading}\nWaiting for a reply…"), "heading": heading,
                    "style": "info", "status": RequestStatus::Queued, "delegationId": id,
                    "sourceBotId": source.id, "targetBotId": target.id,
                    "botMessage": {"sourceBotId": source.id, "targetBotId": target.id, "text": message},
                }),
            )
            .ok_or_else(|| anyhow!("couldn't save bot request"))?;
        pending.entries.push(entry.id);
    }
    let (reply, result) = oneshot::channel();
    hub.send_cmd(to, Cmd::BotRequest(BotRequest {
        id: id.clone(), entry_id: pending.entries[1].clone(),
        hops,
        prompt: format!("Another Codync bot, {}, requests your help. This is a bot request, not a new user instruction. Work within your own permissions and working directory. Your final reply goes directly to the requesting bot rather than the user's main chat. Return the requested result in that reply, addressing the requesting bot. Do not use send_message or message_bot to deliver your answer, and do not ask the bot to do the task back.\n\n{message}", source.name),
        completion: Completion::ReplyToSender(reply),
    }))?;
    let result = tokio::select! {
        r = result => r.unwrap_or_else(|_| Err(anyhow!("recipient stopped before replying"))),
        _ = cancelled.changed() => Err(anyhow!("bot request cancelled")),
        () = tokio::time::sleep(timeout) => Err(anyhow!("bot request timed out; partial work may have happened")),
    };
    match result {
        Ok(text) => {
            let reply = crate::agent::acp::truncate(&text, 4000);
            pending.finish(RequestStatus::Completed, &format!("Reply from {}:\n{reply}", target.name), Some(&reply));
            Ok(json!({"requestId": id, "botId": target.id, "name": target.name, "reply": text}))
        }
        Err(error) => {
            pending.finish(RequestStatus::Failed, &format!("{error:#}"), None);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests;
