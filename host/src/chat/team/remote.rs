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
use crate::analytics::NetworkStage;
use crate::hub::Hub;
use crate::network::Target;
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
    to: ReplyTo,
    quote: String,
    /// How the recipient is shown ("Reviewer on desk").
    peer: String,
}

/// Where an answer from another computer goes.
enum ReplyTo {
    /// One of this computer's bots, with its exchange notice.
    Bot { id: String, entry: String },
    /// The asker's replies, by the letter's id: the cloud assistant's, or an agent's on this
    /// computer that isn't a bot (`assistant::Asker`).
    Replies,
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
    /// The network the request came through (`network::Slot::id`).
    network: String,
    to: Address,
    request: String,
    /// The answering bot's address name when the request came.
    from: String,
}

/// The letters that reach this computer's bots.
pub async fn listen(hub: Arc<Hub>, network: String, mut inbox: mpsc::Receiver<Message>) {
    while let Some(m) = inbox.recv().await {
        if let Err(error) = take(&hub, &network, &m) {
            tracing::warn!(error = format!("{error:#}"), "couldn't take a letter from another computer");
            crate::analytics::network_failed(&hub, NetworkStage::Receive);
        }
    }
}

fn take(hub: &Arc<Hub>, network: &str, m: &Message) -> Result<()> {
    if let Some((id, waiter)) = m.reply_to.as_deref().and_then(|id| Some((id, hub.waiting.take(id)?))) {
        replied(hub, id, waiter, m);
        return Ok(());
    }
    let back = Return { network: network.to_owned(), to: m.from.clone(), request: m.id.clone(), from: m.to.clone() };
    let target = hub.network.bot_for(&m.to).ok_or_else(|| anyhow!("no bot called {} here", m.to));
    let target = target.and_then(|bot| visible_bot(hub, &bot)).and_then(|bot| {
        // The cloud assistant asking through another computer: this one's cloud switches decide too.
        if m.from.agent == super::assistant::HANDLE && m.reply_to.is_none() {
            super::assistant::admit_letter(hub, &bot)?;
        }
        Ok(bot)
    });
    match target {
        // A note, or a reply this computer no longer waits for (it restarted): the chat shows it.
        Ok(target) if !m.wake || m.reply_to.is_some() => {
            note(hub, network, &target, m);
            Ok(())
        }
        Ok(target) => requested(hub, network, &target, m, back),
        // The bot went away after this computer took the request, or it doesn't take the sender's
        // requests: the sender would wait for good.
        Err(error) if m.wake && m.reply_to.is_none() => {
            send_back(hub, &back, format!("Couldn't take it: {error:#}"), RequestStatus::Failed, m.hops);
            Err(error)
        }
        Err(error) => Err(error),
    }
}

/// How a bot on another computer is shown: its name and computer, from that network's directory.
fn peer_name(hub: &Hub, network: &str, address: &Address) -> String {
    if address.agent == super::assistant::HANDLE {
        return format!("{} on {}", super::assistant::NAME, address.computer);
    }
    let name = hub.network.slot(Some(network)).ok().and_then(|slot| {
        slot.node
            .directory()
            .into_iter()
            .find(|p| p.name == address.computer)
            .and_then(|p| p.agents.into_iter().find(|a| a.id == address.agent))
            .map(|a| a.name)
    });
    name.map_or_else(|| address.to_string(), |name| format!("{name} on {}", address.computer))
}

/// One of this computer's bots asks a bot on another computer.
pub(super) async fn message(hub: &Arc<Hub>, source: &BotConfig, to: &Target, message: &str) -> Result<Value> {
    check_request(hub, source, message)?;
    let node = hub.network.slot(Some(&to.network))?.node;
    let from = hub.network.handle_of(&source.id).ok_or_else(|| anyhow!("this bot isn't on the network"))?;
    let hops = hub.team.reserve_message(&source.id)?;
    // ponytail: the slot covers sending only; the other computer bounds the work it takes on.
    let letter = Letter::new(from, to.address.clone(), message).hops(hops);
    let result = send(hub, &node, source, to, letter).await;
    hub.team.release_message(&source.id);
    result
}

async fn send(hub: &Arc<Hub>, node: &Node, source: &BotConfig, to: &Target, letter: Letter) -> Result<Value> {
    let (id, address, computer) = (letter.id.clone(), hub.network.address_text(to), letter.to.computer.clone());
    let peer = peer_name(hub, &to.network, &letter.to);
    let heading = format!("Messaged {peer}: {}", letter.body);
    let exchange =
        Exchange { request: &id, source: &source.id, target: &address, text: &letter.body, peer_name: Some(&peer) };
    let entry = exchange.notice(hub, &source.id, &heading, false)?;
    let quote = truncate(&letter.body, REQUEST_QUOTE_BYTES);
    let waiter = Waiter { to: ReplyTo::Bot { id: source.id.clone(), entry: entry.clone() }, quote, peer: peer.clone() };
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
            crate::analytics::network_failed(hub, NetworkStage::Send);
            finish_entry(hub, &entry, RequestStatus::Failed, &format!("{error:#}"), None);
            return Err(error);
        }
    };
    Ok(json!({
        "requestId": id, "botId": address, "name": peer, "status": RequestStatus::Queued,
        "note": format!("{said} The reply comes back to you as a new turn in your chat: don't wait or poll for it."),
    }))
}

/// The cloud assistant, or an agent on this computer that isn't a bot, asks a bot on another
/// computer: `asker` is whose replies the outcome waits in, `from` the address name it signs with.
/// Returns the request's id (the letter's) and how the bot is shown, at once: the letter goes in
/// the background.
pub(super) fn ask_for_replies(
    hub: &Arc<Hub>,
    asker: &str,
    from: &str,
    to: &Target,
    message: &str,
) -> Result<(String, String)> {
    let node = hub.network.slot(Some(&to.network))?.node;
    hub.network.send_as(from)?;
    let letter = Letter::new(from, to.address.clone(), message);
    let (id, peer) = (letter.id.clone(), peer_name(hub, &to.network, &to.address));
    // Kept before the letter goes, so an outcome as quick as a refusal finds them.
    hub.assistant.add(super::assistant::Reply::queued(asker, &id, &peer, message));
    awaits_in_replies(hub, &id, &peer, message);
    let (hub, request) = (hub.clone(), id.clone());
    tokio::spawn(async move {
        match node.send(letter).await {
            Ok(Sent::Delivered) => hub.assistant.started(&request),
            Ok(Sent::Queued) => {}
            Err(error) => {
                hub.waiting.take(&request);
                crate::analytics::network_failed(&hub, NetworkStage::Send);
                hub.assistant.answered(&request, Err(format!("{error:#}")));
            }
        }
    });
    Ok((id, peer))
}

/// The answer to letter `id` goes to its asker's replies.
pub(super) fn awaits_in_replies(hub: &Hub, id: &str, peer: &str, message: &str) {
    let quote = truncate(message, REQUEST_QUOTE_BYTES);
    hub.waiting.insert(id.to_owned(), Waiter { to: ReplyTo::Replies, quote, peer: peer.to_owned() });
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
fn requested(hub: &Arc<Hub>, network: &str, target: &BotConfig, m: &Message, back: Return) -> Result<()> {
    let peer = peer_name(hub, network, &m.from);
    let from = m.from.to_string();
    let mut lifetime = MessageLifetime::new(hub, &m.id, &m.body, Back::Remote(back), &target.name);
    let exchange =
        Exchange { request: &m.id, source: &from, target: &target.id, text: &m.body, peer_name: Some(&peer) };
    lifetime.entries.push(exchange.notice(hub, &target.id, &format!("Message from {peer}: {}", m.body), true)?);
    let who = format!("{peer}, an agent on another of the user's computers,");
    lifetime.deliver(&target.id, m.hops, request_prompt(&who, &peer, &m.body))
}

/// A letter that starts no turn: the bot's chat shows it.
fn note(hub: &Hub, network: &str, target: &BotConfig, m: &Message) {
    let peer = peer_name(hub, network, &m.from);
    let heading = format!("Note from {peer}");
    hub.add_entry(
        &Lane::main(&target.id),
        EntryKind::Notice,
        hub.store.max_turn(&target.id),
        &json!({"text": format!("{heading}\n{}", m.body), "heading": heading, "style": "info"}),
    );
}

/// The outcome of a request from another computer goes back to the agent that sent it.
pub(super) fn answer(hub: &Arc<Hub>, to: &Return, outcome: &std::result::Result<String, String>, hops: u8) {
    match outcome {
        Ok(reply) => send_back(hub, to, reply.clone(), RequestStatus::Completed, hops),
        Err(detail) => send_back(hub, to, format!("Couldn't finish: {detail}"), RequestStatus::Failed, hops),
    }
}

pub(super) fn cancelled(hub: &Arc<Hub>, to: &Return, hops: u8) {
    send_back(hub, to, STOPPED.to_owned(), RequestStatus::Cancelled, hops);
}

fn send_back(hub: &Arc<Hub>, to: &Return, mut body: String, status: RequestStatus, hops: u8) {
    let Ok(slot) = hub.network.slot(Some(&to.network)) else { return };
    let (node, hub) = (slot.node, hub.clone());
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
            crate::analytics::network_failed(&hub, NetworkStage::Reply);
        }
    });
}

/// The outcome of a request this computer's bot sent: its notice ends and, unless the other
/// side's user stopped the work, the sender gets a reply turn.
fn replied(hub: &Hub, id: &str, waiter: Waiter, m: &Message) {
    let peer = &waiter.peer;
    let status = m.meta.as_ref().and_then(|meta| serde_json::from_value(meta["codync"].clone()).ok());
    let ReplyTo::Bot { id: sender, entry } = waiter.to else {
        match status {
            Some(RequestStatus::Cancelled) => hub.assistant.cancelled(id),
            Some(RequestStatus::Failed) => hub.assistant.answered(id, Err(m.body.clone())),
            _ => hub.assistant.answered(id, Ok(truncate(&m.body, MAX_MESSAGE_BYTES))),
        }
        return;
    };
    let outcome = match status {
        Some(RequestStatus::Cancelled) => {
            let detail = format!("{peer} was stopped before finishing. Partial work may have happened.");
            finish_entry(hub, &entry, RequestStatus::Cancelled, &detail, None);
            return;
        }
        Some(RequestStatus::Failed) => {
            finish_entry(hub, &entry, RequestStatus::Failed, &m.body, None);
            Err(m.body.clone())
        }
        // A reply, also from an agent that isn't a Codync bot (it sends no `meta`).
        _ => {
            let reply = truncate(&m.body, NOTICE_REPLY_BYTES);
            let detail = format!("Reply from {peer}:\n{reply}");
            finish_entry(hub, &entry, RequestStatus::Completed, &detail, Some(&reply));
            Ok(truncate(&m.body, MAX_MESSAGE_BYTES))
        }
    };
    let reply =
        BotReply { request_id: id.to_owned(), quote: waiter.quote, from_name: peer.clone(), outcome, hops: m.hops };
    let _ = hub.send_cmd(&sender, Cmd::BotReply(reply));
}
