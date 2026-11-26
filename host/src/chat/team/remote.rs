//! Bot messages to and from the user's other computers, over codync-network. A request is a
//! letter to a bot's address (`reviewer@desk`); its outcome is a letter back that names the
//! request in `reply_to`, with `meta.codync` saying how it ended. Agents that aren't Codync bots
//! (a Claude Code session, a terminal) use the same letters, and a bot's request to a session on
//! this computer waits here too, for that session's answer.

use super::{
    Back, BotReply, Exchange, MAX_MESSAGE_BYTES, MessageLifetime, NOTICE_REPLY_BYTES, REQUEST_QUOTE_BYTES,
    RequestStatus, STOPPED, check_request, finish_entry, request_prompt, visible_bot,
};
use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::analytics::NetworkStage;
use crate::hub::Hub;
use crate::network::Target;
use crate::network::sessions::Session;
use crate::store::{BotConfig, EntryKind, Lane};
use anyhow::{Result, anyhow};
use codync_agent::acp::truncate;
use codync_network::AgentKind;
use codync_network::{Address, Letter, Message, Node, Sent};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Requests this computer's bots sent to other computers or to a session's mailbox here, by
/// letter id, until their outcome comes back.
#[derive(Default)]
pub struct Waiting {
    all: Mutex<HashMap<String, Waiter>>,
    /// The latest requests to sessions here that ended, by `(session key, id)`, and whether their
    /// session answered them: an answer sent again (a retry) is taken as delivered, and one after
    /// the request expired is refused, rather than either becoming a new request.
    /// The bot that asked and the address its letter was signed with, when it was answered.
    ended: Mutex<std::collections::VecDeque<Ended>>,
}

/// A request to a session here that ended: `(session key, id, the names its answer may give if
/// answered)`: the asking bot's id, the address its letter was signed with, and the one the
/// accepted answer named (the bot may be renamed before a retry).
type Ended = (String, String, Option<Vec<String>>);

/// How many ended requests to sessions `Waiting` remembers.
const ANSWERED_KEPT: usize = 256;

struct Waiter {
    to: ReplyTo,
    quote: String,
    /// How the recipient is shown ("Reviewer on desk").
    peer: String,
    /// The coding agent's session on this computer it went to, the only one whose answer ends it,
    /// and the address the letter was signed with (the bot's then), which its answer may name.
    session: Option<(String, String, AgentKind)>,
    /// The chain's hops when it went to that session, which its answer carries back.
    hops: u8,
    /// When it stops waiting (Unix ms): a request to an agent that isn't a Codync bot (a coding
    /// agent's session) may never be answered. `None` for a bot, which always answers.
    until: Option<i64>,
}

/// How long a request to an agent that isn't a Codync bot waits for its answer.
const SESSION_WAIT_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// Where an answer goes.
enum ReplyTo {
    /// One of this computer's bots, with its exchange notice.
    Bot { id: String, entry: String },
    /// The asker's replies, by the letter's id: the cloud assistant's, or an agent's on this
    /// computer that isn't a bot (`assistant::Asker`).
    Replies,
}

impl Waiting {
    fn insert(&self, id: String, waiter: Waiter) {
        self.all.locked().insert(id, waiter);
    }

    fn take(&self, id: &str) -> Option<Waiter> {
        self.all.locked().remove(id)
    }

    /// The session `key` answers request `id`, naming a bot `named` (its id) or `to` (as typed):
    /// what the request is, checked and, when it ends here, taken and remembered as answered, all
    /// under one lock, so two copies of an answer (or one during an expiry) can't both miss it.
    /// `alive`: whether the bot that asked is still there to take an answer.
    fn answer(&self, id: &str, key: &str, named: &str, to: &str, alive: impl FnOnce(&str) -> bool) -> Answer {
        let mut all = self.all.locked();
        let Some(w) = all.get(id) else {
            let ended = self.ended.locked();
            return match ended.iter().find(|(k, i, _)| k == key && i == id) {
                None => Answer::Unknown,
                Some((.., None)) => Answer::Ended(false),
                // A retry counts as delivered only to the bot it went to.
                Some((.., Some(usable))) if usable[0] != named && !usable.iter().any(|n| n == to) => {
                    Answer::WrongBot(usable[0].clone())
                }
                Some(_) => Answer::Ended(true),
            };
        };
        let (asker, signed) = match (&w.session, &w.to) {
            (Some((k, signed, _)), ReplyTo::Bot { id, .. }) if k == key => (id, signed),
            (Some(_), _) => return Answer::OtherSession,
            (None, _) => return Answer::Unknown,
        };
        // By the bot's id, its address now, or the one its letter was signed with (it may have
        // been renamed since).
        if named != asker && to != signed {
            return Answer::WrongBot(asker.clone());
        }
        let gone = !alive(asker);
        let answered = (!gone).then(|| vec![asker.clone(), signed.clone(), to.to_owned()]);
        let waiter = all.remove(id).expect("found above, under the same lock");
        self.mark_ended(key, id, answered);
        if gone { Answer::Ended(false) } else { Answer::Ends(waiter) }
    }

    /// The request `id`, taken when it went to the session `key` (`None`: to another computer).
    fn take_for(&self, id: &str, key: Option<&str>) -> Option<Waiter> {
        let mut all = self.all.locked();
        let fits = all.get(id).is_some_and(|w| w.session.as_ref().map(|(k, ..)| k.as_str()) == key);
        fits.then(|| all.remove(id)).flatten()
    }

    /// Request `id` to the session `key` ended, answered or not. Called with `all` locked.
    fn mark_ended(&self, key: &str, id: &str, answered: Option<Vec<String>>) {
        let mut ended = self.ended.locked();
        ended.push_back((key.to_owned(), id.to_owned(), answered));
        if ended.len() > ANSWERED_KEPT {
            ended.pop_front();
        }
    }

    /// Request `id` stops waiting at `until`.
    fn wait_until(&self, id: &str, until: Option<i64>) {
        if let Some(w) = self.all.locked().get_mut(id) {
            w.until = until;
        }
    }

    /// The requests whose wait is over, taken.
    fn expired(&self, now: i64) -> Vec<(String, Waiter)> {
        let mut all = self.all.locked();
        let over: Vec<String> =
            all.iter().filter(|(_, w)| w.until.is_some_and(|until| until <= now)).map(|(id, _)| id.clone()).collect();
        let taken: Vec<(String, Waiter)> = over.into_iter().filter_map(|id| all.remove(&id).map(|w| (id, w))).collect();
        for (id, w) in &taken {
            if let Some((key, ..)) = &w.session {
                self.mark_ended(key, id, None);
            }
        }
        taken
    }
}

/// What an answer from a session here is to (`Waiting::answer`).
enum Answer {
    /// The request it ends, now taken.
    Ends(Waiter),
    /// A request to it that ended lately: answered (a retry), or not (it expired).
    Ended(bool),
    /// A request to another session.
    OtherSession,
    /// Its own request, but the bot it names (a mix-up) isn't the one that asked, which is this.
    WrongBot(String),
    Unknown,
}

/// Ends requests whose wait ran out, every hour, so one to a session that never answers doesn't
/// wait on in the asker's chat until the next request comes.
pub(crate) async fn expire_loop(hub: Arc<Hub>) {
    let mut timer = tokio::time::interval(std::time::Duration::from_hours(1));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        timer.tick().await;
        expire(&hub);
    }
}

/// Ends the requests that waited their full time for an agent that never answered, as failed.
fn expire(hub: &Hub) {
    for (id, waiter) in hub.waiting.expired(crate::store::now_ms()) {
        let why = "No answer within a week: it never read or answered this.";
        if let Some((_, _, kind)) = &waiter.session {
            crate::analytics::network_session(hub, crate::analytics::SessionAction::Expired, *kind);
        }
        let hops = waiter.hops;
        replied(hub, &id, waiter, why, Some(RequestStatus::Failed), hops, codync_network::Access::Allow);
    }
}

/// When a request to `to` stops waiting: never for a Codync bot.
fn wait_until(hub: &Hub, to: &Target) -> Option<i64> {
    (!offers_bot(hub, to)).then(|| crate::store::now_ms() + SESSION_WAIT_MS)
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
    if let Some((id, waiter)) = m.reply_to.as_deref().and_then(|id| Some((id, hub.waiting.take_for(id, None)?))) {
        let status = m.meta.as_ref().and_then(|meta| serde_json::from_value(meta["codync"].clone()).ok());
        replied(hub, id, waiter, &m.body, status, m.hops, m.access);
        return Ok(());
    }
    let back = Return { network: network.to_owned(), to: m.from.clone(), request: m.id.clone(), from: m.to.clone() };
    if let Some(session) = hub.network.sessions.by_address(&hub.store, &m.to) {
        return to_session(hub, &session, m, &back);
    }
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

/// A letter for a coding agent's session here goes to its mailbox, under its own id: the session
/// answers it (`replyTo`) with a letter of its own, which reaches the sender's waiting request.
fn to_session(hub: &Arc<Hub>, session: &Session, m: &Message, back: &Return) -> Result<()> {
    let request = m.wake && m.reply_to.is_none();
    // The cloud assistant writing through another computer: this one's cloud switch decides too.
    // Any `replyTo` here matched none of this computer's requests (`take` ends those first), so
    // it gets no pass.
    if m.from.agent == super::assistant::HANDLE
        && let Err(error) = super::assistant::switched_on(hub)
    {
        if request {
            send_back(hub, back, format!("Couldn't take it: {error:#}"), RequestStatus::Failed, m.hops);
        }
        return Err(error);
    }
    // Named with its network while this computer is in several, so the session can answer it.
    let from = hub.network.address_text(&Target { network: back.network.clone(), address: m.from.clone() });
    super::mailbox::post(hub, session, Some(&m.id), &from, &m.body, m.reply_to.as_deref())?;
    Ok(())
}

/// A session here answers letter `reply_to` from another computer: the answer that says how it
/// ended (`meta`), which that computer takes since it asked, into the asker's chat or mailbox.
pub(super) fn session_answer(
    hub: &Arc<Hub>,
    from: &str,
    to: &Target,
    reply_to: &str,
    body: &str,
) -> impl Future<Output = Result<Sent>> + Send + 'static {
    let ready = hub.network.slot(Some(&to.network)).and_then(|slot| hub.network.send_as(from).map(|()| slot.node));
    let letter = Letter::new(from, to.address.clone(), body)
        .reply_to(reply_to.to_owned())
        .meta(json!({"codync": RequestStatus::Completed}));
    let hub = hub.clone();
    async move { ready?.send(letter).await.inspect_err(|_| crate::analytics::network_failed(&hub, NetworkStage::Reply)) }
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
pub(super) async fn message(
    hub: &Arc<Hub>,
    source: &BotConfig,
    turn: &str,
    to: &Target,
    message: &str,
) -> Result<Value> {
    check_request(hub, turn, message)?;
    let node = hub.network.slot(Some(&to.network))?.node;
    let from = hub.network.handle_of(&source.id).ok_or_else(|| anyhow!("this bot isn't on the network"))?;
    let hops = super::reserve(hub, &source.id, turn)?;
    // ponytail: the slot covers sending only; the other computer bounds the work it takes on.
    let letter = Letter::new(from, to.address.clone(), message).hops(hops);
    let result = send(hub, &node, source, to, letter).await;
    hub.team.release_message(&source.id);
    result
}

async fn send(hub: &Arc<Hub>, node: &Node, source: &BotConfig, to: &Target, letter: Letter) -> Result<Value> {
    let (id, address, computer) = (letter.id.clone(), hub.network.address_text(to), letter.to.computer.clone());
    let peer = peer_name(hub, &to.network, &letter.to);
    expire(hub);
    let (entry, heading) = open_request(hub, source, &id, &address, &peer, &letter.body, None)?;
    hub.waiting.wait_until(&id, wait_until(hub, to));
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
            crate::analytics::network_failed(hub, NetworkStage::Send);
            return Err(fail_request(hub, &id, &entry, error));
        }
    };
    Ok(json!({
        "requestId": id, "botId": address, "name": peer, "status": RequestStatus::Queued,
        "note": format!("{said} The reply comes back to you as a new turn in your chat: don't wait or poll for it."),
    }))
}

/// Opens `source`'s request `id` to `peer` (at `address`): its exchange notice, and the waiter its
/// answer ends. Returns the notice and its heading.
fn open_request(
    hub: &Hub,
    source: &BotConfig,
    id: &str,
    address: &str,
    peer: &str,
    body: &str,
    session: Option<(&Session, u8, String)>,
) -> Result<(String, String)> {
    let heading = format!("Messaged {peer}: {body}");
    let exchange =
        Exchange { request: id, source: &source.id, target: address, text: body, peer_name: Some(peer), hops: None };
    let entry = exchange.notice(hub, &source.id, &heading, false)?;
    let to = ReplyTo::Bot { id: source.id.clone(), entry: entry.clone() };
    let (session, hops) =
        session.map_or((None, 0), |(to, hops, signed)| (Some((to.key.clone(), signed, to.kind)), hops));
    // A session may never answer: its request stops waiting after a while.
    let until = session.is_some().then(|| crate::store::now_ms() + SESSION_WAIT_MS);
    let waiter = Waiter { to, quote: truncate(body, REQUEST_QUOTE_BYTES), peer: peer.to_owned(), session, hops, until };
    hub.waiting.insert(id.to_owned(), waiter);
    Ok((entry, heading))
}

/// Request `id` couldn't go: it waits no more and its notice ends failed.
fn fail_request(hub: &Hub, id: &str, entry: &str, error: anyhow::Error) -> anyhow::Error {
    hub.waiting.take(id);
    finish_entry(hub, entry, RequestStatus::Failed, &format!("{error:#}"), None, None);
    error
}

/// One of this computer's bots asks a coding agent's session here: the letter waits in its mailbox,
/// and the session's answer (`replyTo` = the request's id) comes back as the bot's reply turn.
pub(super) fn to_local_session(
    hub: &Arc<Hub>,
    source: &BotConfig,
    turn: &str,
    to: &Session,
    message: &str,
) -> Result<Value> {
    check_request(hub, turn, message)?;
    // Checked like any request (turn, hop limit); the slot goes back at once, since a session may never answer.
    let hops = super::reserve(hub, &source.id, turn)?;
    hub.team.release_message(&source.id);
    let id = uuid::Uuid::new_v4().to_string();
    // ponytail: kept in memory like a request to another computer; a host restart forgets it, and
    // the session's answer then reaches the bot as a new request.
    // Signed with the bot's id, which no rename or restart changes, so an answer always finds it.
    let from = source.id.clone();
    let session = Some((to, hops, from.clone()));
    // Either failure is the store refusing a write: one category, `ask_failed`.
    let failed = || crate::analytics::network_session(hub, crate::analytics::SessionAction::AskFailed, to.kind);
    let (entry, heading) =
        open_request(hub, source, &id, &to.address, &to.address, message, session).inspect_err(|_| failed())?;
    if let Err(error) = super::mailbox::post(hub, to, Some(&id), &from, message, None) {
        failed();
        return Err(fail_request(hub, &id, &entry, error));
    }
    crate::analytics::network_session(hub, crate::analytics::SessionAction::Asked, to.kind);
    waiting(hub, &entry, &format!("{heading}\nIn its inbox: it reads its mail itself."), RequestStatus::Sent);
    Ok(json!({
        "requestId": id, "botId": to.address, "status": RequestStatus::Queued,
        "note": "Left in that coding agent's inbox: it reads its mail itself, so an answer may take a while or not come. If it answers, the reply comes back to you as a new turn in your chat: don't wait or poll for it.",
    }))
}

/// A session here answers `reply_to`: when that is a bot's request to it, the bot gets the answer
/// as its reply turn. False when it isn't one.
pub(super) fn session_replied(hub: &Hub, session: &Session, reply_to: &str, to: &str, body: &str) -> Result<bool> {
    // A botId naming another bot is a mix-up to correct.
    let named = match hub.network.recipient(to) {
        Ok(crate::network::Recipient::Here(bot)) => bot,
        _ => to.to_owned(),
    };
    // Still there, hidden or not: a hidden bot still runs and takes its replies.
    let alive = |bot: &str| hub.store.bot(bot).ok().flatten().is_some();
    let waiter = match hub.waiting.answer(reply_to, &session.key, &named, to, alive) {
        Answer::Ends(waiter) => waiter,
        // Sent again after it went through (a retry): delivered already, not a new request.
        Answer::Ended(true) => return Ok(true),
        Answer::Ended(false) => anyhow::bail!(
            "request {reply_to} is closed (it waited a week unanswered, or the bot that asked is gone): your answer wasn't delivered"
        ),
        Answer::OtherSession => {
            anyhow::bail!("letter {reply_to} went to another session: only that session can answer it")
        }
        Answer::WrongBot(asker) => {
            let handle = hub.network.handle_of(&asker).unwrap_or(asker);
            anyhow::bail!("letter {reply_to} came from {handle}: answer it with botId {handle}");
        }
        Answer::Unknown => return Ok(false),
    };
    let hops = waiter.hops;
    replied(hub, reply_to, waiter, body, None, hops, codync_network::Access::Allow);
    crate::analytics::network_session(hub, crate::analytics::SessionAction::Answered, session.kind);
    Ok(true)
}

/// Letters the full mailbox of the session `key` dropped unread: a bot's request to that session
/// among them ends, as failed (a letter from another computer keeps its sender's id, which may
/// name an unrelated request here, so only one made for this mailbox counts).
pub(super) fn dropped_unread(hub: &Hub, to: &Session, ids: &[String]) {
    for id in ids {
        if let Some(waiter) = hub.waiting.take_for(id, Some(&to.key)) {
            crate::analytics::network_session(hub, crate::analytics::SessionAction::Dropped, to.kind);
            let hops = waiter.hops;
            let why = "The session's inbox was full, so this letter was dropped unread.";
            replied(hub, id, waiter, why, Some(RequestStatus::Failed), hops, codync_network::Access::Allow);
        }
    }
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
    // Kept before the letter goes, so an outcome as quick as a refusal finds them: the route first,
    // so the reply record that may push out older ones (and their routes) never outlives its own.
    expire(hub);
    awaits_in_replies(hub, &id, &peer, message, wait_until(hub, to));
    let reply = super::assistant::Reply::queued(asker, &id, &peer, message);
    // Refused before the letter goes when there's no room to track its reply.
    if let Err(error) = keep_reply(hub, if offers_bot(hub, to) { reply } else { reply.unlimited() }) {
        hub.waiting.take(&id);
        return Err(error);
    }
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

/// Whether `to` is a Codync bot there (as far as this computer heard), which always answers.
pub(super) fn offers_bot(hub: &Hub, to: &Target) -> bool {
    let Ok(slot) = hub.network.slot(Some(&to.network)) else { return true };
    let peer = slot.node.directory().into_iter().find(|p| p.name == to.address.computer);
    let agent = peer.and_then(|p| p.agents.into_iter().find(|a| a.id == to.address.agent));
    agent.is_none_or(|a| a.kind == AgentKind::Codync)
}

/// Keeps `reply` in its asker's replies, ending the routes of the running requests it pushes out.
pub(super) fn keep_reply(hub: &Hub, reply: super::assistant::Reply) -> Result<()> {
    let forgotten = hub.assistant.add(reply).ok_or_else(|| {
        anyhow!("too many requests are still running on this computer to track another: try again later")
    })?;
    for forgotten in forgotten {
        hub.waiting.take(&forgotten);
    }
    Ok(())
}

/// The answer to letter `id` goes to its asker's replies.
/// `until`: when it stops waiting (`wait_until`).
pub(super) fn awaits_in_replies(hub: &Hub, id: &str, peer: &str, message: &str, until: Option<i64>) {
    let quote = truncate(message, REQUEST_QUOTE_BYTES);
    hub.waiting.insert(
        id.to_owned(),
        Waiter { to: ReplyTo::Replies, quote, peer: peer.to_owned(), session: None, hops: 0, until },
    );
}

/// Says where a request waits, unless its reply already came (a quick one can beat this).
fn waiting(hub: &Hub, id: &str, text: &str, status: RequestStatus) {
    // Checked and changed together: a reply as quick as this must not be overwritten with the old copy.
    let _ = hub.edit_entry(id, |e| {
        if e.data["status"] != json!(RequestStatus::Queued) {
            return Ok(false);
        }
        e.data["text"] = text.into();
        e.data["status"] = json!(status);
        Ok(true)
    });
}

/// A bot on another computer (or any agent there) asks one of this computer's bots.
fn requested(hub: &Arc<Hub>, network: &str, target: &BotConfig, m: &Message, back: Return) -> Result<()> {
    let peer = peer_name(hub, network, &m.from);
    let from = m.from.to_string();
    let target_epoch = {
        let _transition = hub.pause.transition();
        if hub.paused(&target.id) {
            let error = anyhow!("{} is paused", target.name);
            send_back(hub, &back, format!("Couldn't take it: {error:#}"), RequestStatus::Failed, m.hops);
            // A working peer that is paused is not a transport failure. listen counts every Err as one.
            return Ok(());
        }
        hub.pause.epoch(&target.id)
    };
    let mut lifetime =
        MessageLifetime::new(hub, &m.id, &m.body, Back::Remote(back), &target.name, target_epoch, m.hops);
    let exchange = Exchange {
        request: &m.id,
        source: &from,
        target: &target.id,
        text: &m.body,
        peer_name: Some(&peer),
        hops: None,
    };
    lifetime.entries.push(exchange.notice(hub, &target.id, &format!("Message from {peer}: {}", m.body), true)?);
    let who = format!("{peer}, an agent on another of the user's computers,");
    lifetime.deliver(&target.id, m.access, request_prompt(&who, &peer, m.access, &m.body))
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
    // A missing slot used to come back as `Err`, and `listen` logged it. Returning here keeps a paused
    // peer from being counted as a transport failure, and the warning is the trace that return dropped.
    let slot = match hub.network.slot(Some(&to.network)) {
        Ok(slot) => slot,
        Err(error) => {
            tracing::warn!(error = format!("{error:#}"), "couldn't send a reply to another computer");
            return;
        }
    };
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

/// The outcome of a request this computer's bot sent: its notice ends and the sender gets a reply turn, also when
/// the other side's user stopped the work.
fn replied(
    hub: &Hub,
    id: &str,
    waiter: Waiter,
    body: &str,
    status: Option<RequestStatus>,
    hops: u8,
    access: codync_network::Access,
) {
    let peer = &waiter.peer;
    let ReplyTo::Bot { id: sender, entry } = waiter.to else {
        match status {
            Some(RequestStatus::Cancelled) => hub.assistant.cancelled(id),
            Some(RequestStatus::Failed) => hub.assistant.answered(id, Err(body.to_owned())),
            _ => hub.assistant.answered(id, Ok(truncate(body, MAX_MESSAGE_BYTES))),
        }
        return;
    };
    let outcome = match status {
        Some(RequestStatus::Cancelled) => {
            let detail = format!("{peer} was stopped before finishing. Partial work may have happened.");
            finish_entry(hub, &entry, RequestStatus::Cancelled, &detail, None, None);
            Err(detail)
        }
        Some(RequestStatus::Failed) => {
            finish_entry(hub, &entry, RequestStatus::Failed, body, None, None);
            Err(body.to_owned())
        }
        // A reply, also from an agent that isn't a Codync bot (it sends no `meta`).
        _ => {
            let reply = truncate(body, NOTICE_REPLY_BYTES);
            let detail = format!("Reply from {peer}:\n{reply}");
            finish_entry(hub, &entry, RequestStatus::Completed, &detail, Some(&reply), None);
            Ok(truncate(body, MAX_MESSAGE_BYTES))
        }
    };
    let reply = BotReply {
        request_id: id.to_owned(),
        quote: waiter.quote,
        from_name: peer.clone(),
        outcome,
        hops,
        access,
        entry: None,
    };
    let _ = hub.send_cmd(&sender, Cmd::BotReply(reply));
}

#[cfg(test)]
mod waiting_tests {
    use super::*;

    #[test]
    fn only_a_wait_that_is_over_is_taken() {
        let waiting = Waiting::default();
        let waiter = |until| Waiter {
            to: ReplyTo::Replies,
            quote: String::new(),
            peer: "x".into(),
            session: None,
            hops: 0,
            until,
        };
        waiting.insert("bot".into(), waiter(None));
        waiting.insert("session".into(), waiter(Some(100)));
        assert!(waiting.expired(99).is_empty());
        let over: Vec<String> = waiting.expired(100).into_iter().map(|(id, _)| id).collect();
        assert_eq!(over, ["session"]);
        assert!(waiting.take("bot").is_some(), "a bot's request waits for good");
        waiting.mark_ended("codex:t", "late", None);
        waiting.mark_ended("codex:t", "done", Some(vec!["b".into(), "b".into(), "reviewer".into()]));
        assert!(
            matches!(waiting.answer("done", "codex:t", "x", "reviewer", |_| true), Answer::Ended(true)),
            "the name it used"
        );
        assert!(
            matches!(waiting.answer("done", "codex:t", "other", "other", |_| true), Answer::WrongBot(_)),
            "a retry naming another bot isn't confirmed"
        );
        let ended = |key, id| match waiting.answer(id, key, "b", "b", |_| true) {
            Answer::Ended(answered) => Some(answered),
            _ => None,
        };
        assert_eq!(ended("codex:t", "late"), Some(false), "a late answer is refused");
        assert_eq!(ended("codex:t", "done"), Some(true), "a retry is delivered already");
        assert_eq!(ended("codex:u", "done"), None);
        // The bot that asked is gone: the answer isn't taken as delivered, now or on a retry.
        let to_bot = Waiter {
            to: ReplyTo::Bot { id: "b".into(), entry: "e".into() },
            quote: String::new(),
            peer: "x".into(),
            session: Some(("codex:t".into(), "b".into(), AgentKind::Codex)),
            hops: 0,
            until: None,
        };
        waiting.insert("gone".into(), to_bot);
        assert!(matches!(waiting.answer("gone", "codex:t", "b", "b", |_| false), Answer::Ended(false)));
        assert_eq!(ended("codex:t", "gone"), Some(false));
    }

    #[tokio::test]
    async fn a_remote_request_to_a_paused_bot_is_refused_at_admission() {
        let f = super::super::tests::Fixture::new();
        f.hub.update_bot(&json!({"id": "b", "paused": true})).unwrap();
        let target = f.hub.store.bot("b").unwrap().unwrap().config;
        let message = Message {
            id: "paused-remote-request".into(),
            from: "asker@elsewhere".parse().unwrap(),
            to: "b".into(),
            body: "must not queue".into(),
            wake: true,
            reply_to: None,
            hops: 1,
            meta: None,
            access: codync_network::Access::Allow,
        };
        let back = Return {
            network: "missing-test-network".into(),
            to: message.from.clone(),
            request: message.id.clone(),
            from: message.to.clone(),
        };

        // `back.network` is not a slot on this hub, so `send_back` returns before the Failed letter
        // is built. Ok here means no transport error. It does not mean the peer was answered. That
        // one Failed letter is covered only by the e2e lane.
        requested(&f.hub, "missing-test-network", &target, &message, back)
            .expect("a paused peer is refused without a transport error");
        assert!(f.hub.store.history("b", i64::MAX, 50).unwrap().iter().all(|e| e.data["delegationId"].is_null()));
        f.shutdown().await;
    }
}
