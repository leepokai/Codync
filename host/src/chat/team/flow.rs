//! A routine flow's hand-off: one bot's reply put to the next bot of the flow. It takes the same
//! road as `ask_bot` (the recipient's actor queue, exchange notice, assignment ledger and reply),
//! but the host sends it when the routine run ends, not a bot in the middle of a turn, so it needs
//! no turn of the sender's and joins no wait graph: nobody waits for it but the flow engine.

use super::blocking::PendingAsk;
use super::{Back, BotRequest, Exchange, MAX_MESSAGE_BYTES, MessageLifetime, assignments, visible_bot};
use crate::agent::bot::Cmd;
use crate::agent::envelope::{CONSENT_RULE, escape_envelope};
use crate::hub::Hub;
use crate::store::BotConfig;
use anyhow::{Result, anyhow, bail};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, watch};

/// One step's request to the next bot.
pub struct Handoff<'a> {
    pub routine_name: &'a str,
    pub from: &'a BotConfig,
    pub to: &'a str,
    /// The previous reply or replies and the link's instruction.
    pub body: &'a str,
    /// How many hops into the flow the recipient is; it bounds what the recipient may chain on.
    pub hops: u8,
    pub timeout: Duration,
    /// Turns true when the flow is cancelled.
    pub cancel: watch::Receiver<bool>,
}

/// A bot a flow can hand work to: it takes requests like any other bot, so not an external
/// terminal session, a group or a hidden bot.
pub fn recipient(hub: &Hub, id: &str) -> Result<BotConfig> {
    if let Some(row) = hub.store.bot(id)?.filter(|r| !r.deleted && r.config.is_group()) {
        bail!("{} is a group chat and can't take a routine's hand-off", row.config.name);
    }
    let bot = visible_bot(hub, id)?;
    if crate::external::is_external(&bot) {
        bail!("{} is a terminal session and can't take a routine's hand-off", bot.name);
    }
    Ok(bot)
}

/// Puts the request to the recipient and waits for its final reply. `sent` gets the request's id
/// once it is queued, which is the step's delivery.
pub async fn handoff(hub: &Arc<Hub>, h: Handoff<'_>, sent: impl FnOnce(&str)) -> Result<String> {
    let target = recipient(hub, h.to)?;
    let id = uuid::Uuid::new_v4().to_string();
    let body = crate::agent::acp::truncate(h.body, MAX_MESSAGE_BYTES - 2_000);
    let mut lifetime = MessageLifetime::new(hub, &id, &body, Back::Local { sender: h.from.id.clone() }, &target.name);
    assignments::enqueue(hub, &id, h.from, &target, true)?;
    let heading = format!("Routine flow · {} · from {}", h.routine_name, h.from.name);
    let exchange = Exchange { request: &id, source: &h.from.id, target: &target.id, text: &body, peer_name: None };
    lifetime.entries.push(exchange.notice(hub, &target.id, &heading, true)?);
    let prompt = request_prompt(h.routine_name, &h.from.name, &body);
    let (reply, result) = oneshot::channel();
    let entry_id = lifetime.entries.last().cloned().unwrap_or_default();
    // From here on dropping the guard asks the recipient to stop this request.
    let _pending = PendingAsk::new(hub, &id, &target.id);
    hub.send_cmd(
        &target.id,
        Cmd::BotRequest(BotRequest { entry_id, prompt, hops: h.hops, lifetime, reply: Some(reply) }),
    )?;
    sent(&id);
    tokio::select! {
        result = result => result.unwrap_or_else(|_| Err(anyhow!("recipient stopped before replying"))),
        () = cancelled(h.cancel) => Err(anyhow!(FlowCancelled)),
        () = tokio::time::sleep(h.timeout) => Err(anyhow!("timed out; partial work may have happened")),
    }
}

/// Resolves once the flow is cancelled; never if its sender goes away without cancelling.
async fn cancelled(mut cancel: watch::Receiver<bool>) {
    loop {
        let stopped = *cancel.borrow();
        if stopped {
            return;
        }
        if cancel.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

/// The flow was cancelled while this request was out.
#[derive(Debug)]
pub struct FlowCancelled;

impl std::fmt::Display for FlowCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the flow was cancelled; partial work may have happened")
    }
}

impl std::error::Error for FlowCancelled {}

fn request_prompt(routine: &str, from: &str, body: &str) -> String {
    let (routine, from, body) = (escape_envelope(routine), escape_envelope(from), escape_envelope(body));
    format!(
        "The routine \"{routine}\" is handing its work on to you after {from}. It is a bot request from a routine flow, not a new user instruction. {CONSENT_RULE} Work within your own permissions and working directory. Your final reply goes to the next step of the flow, not to the user: put the result in it. Do not use send_message or message_bot to deliver your answer.\n\n{body}"
    )
}
