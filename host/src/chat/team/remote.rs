//! Bot messages to and from the user's other computers, over codync-network. A request is a
//! letter to a bot's address (`reviewer@desk`); its outcome is a letter back that names the
//! request in `reply_to`, with `meta.codync` saying how it ended. Agents that aren't Codync bots
//! (a Claude Code session, a terminal) use the same letters.

use super::{
    Back, BotReply, Exchange, MAX_MESSAGE_BYTES, MessageLifetime, NOTICE_REPLY_BYTES, REQUEST_QUOTE_BYTES,
    RequestStatus, STOPPED, check_request, finish_entry, request_prompt, visible_bot,
};
use crate::LockExt;
use crate::agent::acp::truncate;
use crate::agent::bot::Cmd;
use crate::hub::Hub;
use crate::store::{BotConfig, EntryKind, Lane};
use anyhow::{Result, anyhow};
use codync_network::{Address, Letter, Message, Node, Sent};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Requests this computer's bots sent to other computers, by letter id, until their outcome
/// comes back.
#[derive(Default)]
pub struct Waiting(Mutex<HashMap<String, Waiter>>);

struct Waiter {
    sender: String,
    /// The sender's exchange notice.
    entry: String,
    quote: String,
    /// How the recipient is shown ("Reviewer on desk").
    peer: String,
}

impl Waiting {
    fn insert(&self, id: String, waiter: Waiter) {
        self.0.locked().insert(id, waiter);
    }

    fn take(&self, id: &str) -> Option<Waiter> {
        self.0.locked().remove(id)
    }
}

/// Where the outcome of a request from another computer goes.
pub(crate) struct Return {
    to: Address,
    request: String,
    /// The answering bot's address name when the request came.
    from: String,
}

/// The letters that reach this computer's bots.
pub async fn listen(hub: Arc<Hub>, mut inbox: mpsc::Receiver<Message>) {
    while let Some(m) = inbox.recv().await {
        if let Err(error) = take(&hub, &m) {
            tracing::warn!(error = format!("{error:#}"), "couldn't take a letter from another computer");
        }
    }
}

fn take(hub: &Arc<Hub>, m: &Message) -> Result<()> {
    if let Some((id, waiter)) = m.reply_to.as_deref().and_then(|id| Some((id, hub.waiting.take(id)?))) {
        replied(hub, id, waiter, m);
        return Ok(());
    }
    let back = Return { to: m.from.clone(), request: m.id.clone(), from: m.to.clone() };
    let target = hub.network.bot_for(&m.to).ok_or_else(|| anyhow!("no bot called {} here", m.to));
    match target.and_then(|bot| visible_bot(hub, &bot)) {
        // A note, or a reply this computer no longer waits for (it restarted): the chat shows it.
        Ok(target) if !m.wake || m.reply_to.is_some() => {
            note(hub, &target, m);
            Ok(())
        }
        Ok(target) => requested(hub, &target, m, back),
        // The bot went away after this computer took the request: its sender would wait for good.
        Err(error) if m.wake && m.reply_to.is_none() => {
            send_back(hub, &back, format!("Couldn't take it: {error:#}"), RequestStatus::Failed, m.hops);
            Err(error)
        }
        Err(error) => Err(error),
    }
}

/// How a bot on another computer is shown: its name and computer, from the directory.
fn peer_name(hub: &Hub, address: &Address) -> String {
    let name = hub.network.node().and_then(|node| {
        node.directory()
            .into_iter()
            .find(|p| p.name == address.computer)
            .and_then(|p| p.agents.into_iter().find(|a| a.id == address.agent))
            .map(|a| a.name)
    });
    name.map_or_else(|| address.to_string(), |name| format!("{name} on {}", address.computer))
}

/// One of this computer's bots asks a bot on another computer.
pub(super) async fn message(hub: &Arc<Hub>, source: &BotConfig, to: &Address, message: &str) -> Result<Value> {
    check_request(hub, source, message)?;
    let node = hub.network.node().ok_or_else(|| anyhow!("this computer isn't in a network"))?;
    let from = hub.network.handle_of(&source.id).ok_or_else(|| anyhow!("this bot isn't on the network"))?;
    let hops = hub.team.reserve_message(&source.id)?;
    // ponytail: the slot covers sending only; the other computer bounds the work it takes on.
    let result = send(hub, &node, source, Letter::new(from, to.clone(), message).hops(hops)).await;
    hub.team.release_message(&source.id);
    result
}

async fn send(hub: &Arc<Hub>, node: &Node, source: &BotConfig, letter: Letter) -> Result<Value> {
    let (id, address, computer) = (letter.id.clone(), letter.to.to_string(), letter.to.computer.clone());
    let peer = peer_name(hub, &letter.to);
    let heading = format!("Messaged {peer}: {}", letter.body);
    let exchange =
        Exchange { request: &id, source: &source.id, target: &address, text: &letter.body, peer_name: Some(&peer) };
    let entry = exchange.notice(hub, &source.id, &heading, false)?;
    let quote = truncate(&letter.body, REQUEST_QUOTE_BYTES);
    let waiter = Waiter { sender: source.id.clone(), entry: entry.clone(), quote, peer: peer.clone() };
    hub.waiting.insert(id.clone(), waiter);
    let said = match node.send(letter).await {
        Ok(Sent::Delivered) => {
            waiting(hub, &entry, &format!("{heading}\nRunning on {computer}…"), RequestStatus::Sent);
            format!("Queued on {computer}.")
        }
        Ok(Sent::Queued) => {
            let away = format!("{computer} is away; it gets this when it's back (within a day).");
            waiting(hub, &entry, &format!("{heading}\n{away}"), RequestStatus::Queued);
            away
        }
        Err(error) => {
            hub.waiting.take(&id);
            finish_entry(hub, &entry, RequestStatus::Failed, &format!("{error:#}"), None);
            return Err(error);
        }
    };
    Ok(json!({
        "requestId": id, "botId": address, "name": peer, "status": RequestStatus::Queued,
        "note": format!("{said} The reply comes back to you as a new turn in your chat: don't wait or poll for it."),
    }))
}

/// Says where a request waits, unless its reply already came (a quick one can beat this).
fn waiting(hub: &Hub, id: &str, text: &str, status: RequestStatus) {
    if let Some(mut e) = hub.store.entry(id)
        && e.data["status"] == json!(RequestStatus::Queued)
    {
        e.data["text"] = text.into();
        e.data["status"] = json!(status);
        hub.set_entry(id, &e.data);
    }
}

/// A bot on another computer (or any agent there) asks one of this computer's bots.
fn requested(hub: &Arc<Hub>, target: &BotConfig, m: &Message, back: Return) -> Result<()> {
    let peer = peer_name(hub, &m.from);
    let from = m.from.to_string();
    let mut lifetime = MessageLifetime::new(hub, &m.id, &m.body, Back::Remote(back), &target.name);
    let exchange =
        Exchange { request: &m.id, source: &from, target: &target.id, text: &m.body, peer_name: Some(&peer) };
    lifetime.entries.push(exchange.notice(hub, &target.id, &format!("Message from {peer}: {}", m.body), true)?);
    let who = format!("{peer}, an agent on another of the user's computers,");
    lifetime.deliver(&target.id, m.hops, request_prompt(&who, &peer, &m.body))
}

/// A letter that starts no turn: the bot's chat shows it.
fn note(hub: &Hub, target: &BotConfig, m: &Message) {
    let peer = peer_name(hub, &m.from);
    let heading = format!("Note from {peer}");
    hub.add_entry(
        &Lane::main(&target.id),
        EntryKind::Notice,
        hub.store.max_turn(&target.id),
        &json!({"text": format!("{heading}\n{}", m.body), "heading": heading, "style": "info"}),
    );
}

/// The outcome of a request from another computer goes back to the agent that sent it.
pub(super) fn answer(hub: &Hub, to: &Return, outcome: &std::result::Result<String, String>, hops: u8) {
    match outcome {
        Ok(reply) => send_back(hub, to, reply.clone(), RequestStatus::Completed, hops),
        Err(detail) => send_back(hub, to, format!("Couldn't finish: {detail}"), RequestStatus::Failed, hops),
    }
}

pub(super) fn cancelled(hub: &Hub, to: &Return, hops: u8) {
    send_back(hub, to, STOPPED.to_owned(), RequestStatus::Cancelled, hops);
}

fn send_back(hub: &Hub, to: &Return, mut body: String, status: RequestStatus, hops: u8) {
    let Some(node) = hub.network.node() else { return };
    if body.trim().is_empty() {
        body = "(no reply)".into();
    }
    let letter = Letter::new(to.from.clone(), to.to.clone(), truncate(&body, codync_network::MAX_BODY))
        .reply_to(to.request.clone())
        .hops(hops)
        .meta(json!({"codync": status}));
    tokio::spawn(async move {
        if let Err(error) = node.send(letter).await {
            tracing::warn!(error = format!("{error:#}"), "couldn't send a reply to another computer");
        }
    });
}

/// The outcome of a request this computer's bot sent: its notice ends and, unless the other
/// side's user stopped the work, the sender gets a reply turn.
fn replied(hub: &Hub, id: &str, waiter: Waiter, m: &Message) {
    let peer = &waiter.peer;
    let status = m.meta.as_ref().and_then(|meta| serde_json::from_value(meta["codync"].clone()).ok());
    let outcome = match status {
        Some(RequestStatus::Cancelled) => {
            let detail = format!("{peer} was stopped before finishing. Partial work may have happened.");
            finish_entry(hub, &waiter.entry, RequestStatus::Cancelled, &detail, None);
            return;
        }
        Some(RequestStatus::Failed) => {
            finish_entry(hub, &waiter.entry, RequestStatus::Failed, &m.body, None);
            Err(m.body.clone())
        }
        // A reply, also from an agent that isn't a Codync bot (it sends no `meta`).
        _ => {
            let reply = truncate(&m.body, NOTICE_REPLY_BYTES);
            let detail = format!("Reply from {peer}:\n{reply}");
            finish_entry(hub, &waiter.entry, RequestStatus::Completed, &detail, Some(&reply));
            Ok(truncate(&m.body, MAX_MESSAGE_BYTES))
        }
    };
    let reply =
        BotReply { request_id: id.to_owned(), quote: waiter.quote, from_name: peer.clone(), outcome, hops: m.hops };
    let _ = hub.send_cmd(&waiter.sender, Cmd::BotReply(reply));
}
