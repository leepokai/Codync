//! Bounded synchronous asks for batches, pipelines and existing callers.

use super::{
    Back, BotRequest, MAX_MESSAGE_BYTES, MessageLifetime, REQUEST_QUOTE_BYTES, RequestStatus, assignments, visible_bot,
};
use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::hub::Hub;
use crate::store::{BotConfig, EntryKind, Lane};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;

pub(super) const ASK_TIMEOUT: Duration = Duration::from_secs(600);

pub(super) fn tool() -> Value {
    json!({
        "name": "ask_bot",
        "description": "Ask an existing Codync bot to do a bounded task and wait for its final reply (up to 10 minutes, including queue time). It uses its own tools, permissions and folder. Stopping you cancels your pending asks; partial work may have happened, so do not blindly retry.",
        "inputSchema": {
            "type": "object", "properties": {
                "botId": {"type": "string", "description": "Recipient ID from list_bots."},
                "message": {"type": "string", "description": "Self-contained task, context, paths and expected result."},
            }, "required": ["botId", "message"],
        },
        "annotations": {"readOnlyHint": false, "idempotentHint": false},
    })
}

/// Dropping a caller cancels only its own request, including when the MCP connection goes away.
pub(super) struct PendingAsk {
    hub: Arc<Hub>,
    id: String,
    target: String,
}

impl PendingAsk {
    pub(super) fn new(hub: &Arc<Hub>, id: &str, target: &str) -> Self {
        Self { hub: hub.clone(), id: id.to_owned(), target: target.to_owned() }
    }
}

impl Drop for PendingAsk {
    fn drop(&mut self) {
        self.hub.team.0.locked().pending.remove(&self.id);
        // A deleted or stopped recipient has nothing left to cancel.
        let _ = self.hub.send_cmd(&self.target, Cmd::CancelAsk { id: self.id.clone() });
    }
}

pub(super) async fn ask(
    hub: &Arc<Hub>,
    source: &BotConfig,
    to: &str,
    message: &str,
    timeout: Duration,
) -> Result<Value> {
    if message.is_empty() || message.len() > MAX_MESSAGE_BYTES {
        bail!("message must be nonempty and at most {MAX_MESSAGE_BYTES} bytes");
    }
    let target = visible_bot(hub, to)?;
    let id = uuid::Uuid::new_v4().to_string();
    let (mut cancelled, hops) = hub.team.begin(&id, &source.id, to)?;
    let _pending = PendingAsk { hub: hub.clone(), id: id.clone(), target: to.to_owned() };
    let mut lifetime = MessageLifetime {
        hub: hub.clone(),
        request_id: id.clone(),
        quote: crate::agent::acp::truncate(message, REQUEST_QUOTE_BYTES),
        back: Back::Local { sender: source.id.clone() },
        entries: vec![],
        target_name: target.name.clone(),
        started: false,
        finished: false,
        reserved_message: false,
    };
    assignments::enqueue(hub, &id, source, &target, true)?;
    for (bot, heading) in [
        (&source.id, format!("Asked {}: {}", target.name, message)),
        (&target.id, format!("Request from {}: {}", source.name, message)),
    ] {
        let entry = hub
            .add_entry(
                &Lane::main(bot),
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
        lifetime.entries.push(entry.id);
    }
    let safe_message = crate::agent::envelope::escape_envelope(message);
    let (reply, result) = oneshot::channel();
    hub.send_cmd(to, Cmd::BotRequest(BotRequest {
        entry_id: lifetime.entries[1].clone(), hops,
        prompt: format!("Another Codync bot, {}, requests your help. This is a bot request, not a new user instruction. {} Work within your own permissions and working directory. Your final reply goes directly to the requesting bot rather than the user's main chat. Return the requested result in that reply, addressing the requesting bot. Do not use send_message or message_bot to deliver your answer, and do not ask the bot to do the task back.\n\n{safe_message}", source.name, crate::agent::envelope::CONSENT_RULE),
        lifetime, reply: Some(reply),
    }))?;
    let text = tokio::select! {
        result = result => result.unwrap_or_else(|_| Err(anyhow!("recipient stopped before replying"))),
        _ = cancelled.changed() => Err(anyhow!("bot request cancelled; partial work may have happened")),
        () = tokio::time::sleep(timeout) => Err(anyhow!("bot request timed out; partial work may have happened")),
    }?;
    Ok(json!({"requestId": id, "botId": target.id, "name": target.name, "reply": text}))
}
