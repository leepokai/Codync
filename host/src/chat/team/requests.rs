//! Admission, capacity and the hop limit for bot messages.
//!
//! Past the limit, one open lift per actor asks the lead for a reset. The grant waits for the approval
//! wake, which is the only caller that takes it while that wake can still run. A wake that never starts
//! is remembered so the next reply begins at hop 0, and a user message drops the lift, the grant and that
//! memory. The refused message is never stored here: a lift keeps only what the blocked bot tells the lead.

use crate::LockExt;
use anyhow::{Result, bail};
use codync_agent::acp::truncate;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use super::MAX_PENDING_MESSAGES;

const MAX_REQUEST_HOPS: u8 = 8;
const _: () = assert!(MAX_REQUEST_HOPS == 8, "limit::NOTICE and its translations say 8");
// No production caller sets the limit yet. Tests cover the bounds; the running host stays at 8.
#[cfg_attr(not(test), allow(dead_code))]
const MIN_HOP_LIMIT: u8 = MAX_REQUEST_HOPS;
#[cfg_attr(not(test), allow(dead_code))]
const MAX_HOP_LIMIT: u8 = 32;
/// Successful `approve` calls for one actor since that actor's last user message.
const MAX_RESETS: u8 = 3;
/// What a lift keeps of the blocked bot's explanation or answer, as the lead receives it.
const MAX_LIFT_TEXT: usize = 400;
pub(super) const MAX_PENDING_MESSAGES_PER_SENDER: usize = 16;

/// Nothing waits on a message, so there is no wait graph: capacity and the chain's hop count
/// bound what bots can start.
#[derive(Default)]
pub struct Requests(pub(super) Mutex<RequestState>);

/// A running turn, as bot messages see it.
#[derive(Clone, Copy)]
pub enum Turn {
    /// A turn in the bot's own chat, `hops` handoffs into a chain (0 for the user's own turn).
    Chat(u8),
    /// Answering another bot's request: the answer goes back in its reply, so it messages no one.
    Request,
}

pub(super) struct RequestState {
    /// Bots in a turn.
    pub(super) active: HashMap<String, Turn>,
    pub(super) messages: usize,
    pub(super) messages_by_sender: HashMap<String, usize>,
    /// Turns whose chain already stopped at the hop limit: the user gets one notice per turn.
    stopped: HashSet<String>,
    /// Handoffs allowed in one chain. The default is [`MAX_REQUEST_HOPS`].
    hop_limit: u8,
    /// Next refusal id. `0` is never issued.
    next_refusal: u64,
    actors: HashMap<String, ActorState>,
}

impl Default for RequestState {
    fn default() -> Self {
        Self {
            active: HashMap::new(),
            messages: 0,
            messages_by_sender: HashMap::new(),
            stopped: HashSet::new(),
            hop_limit: MAX_REQUEST_HOPS,
            next_refusal: 1,
            actors: HashMap::new(),
        }
    }
}

/// Reset budget, open lift and unused grant for one actor.
#[derive(Default)]
struct ActorState {
    /// Incremented when the user messages this actor. Older lifts and grants are ignored.
    generation: u64,
    /// A clarification wake. `message_bot` stays closed for it, including at hop 0.
    control: bool,
    /// Successful resets since the last user message.
    resets: u8,
    /// The lead denied a reset. Another notice waits until a user message.
    denied: bool,
    lift: Option<Lift>,
    grant: Option<Grant>,
    /// A granted reset whose wake did not start a turn. The next reply begins at hop 0. This is not a pending
    /// grant: a reply must not take a grant that still has its wake.
    carry_reset: bool,
    /// The carry was consumed by a turn that has not yet delivered its prompt, at this generation: while set,
    /// no new lift may open (a delayed refusal must not take the refusal over), and the turn's failure will
    /// restore the carry. Cleared when the prompt is sent, a user message starts, or the carry is restored. A
    /// restore only applies at its own generation: a user message voids the failed turn's approval.
    pending_restore: Option<u64>,
}

/// The one open notice for an actor.
struct Lift {
    id: u64,
    /// `message_bot`'s sender. The actor (map key) can be a different bot.
    sender: String,
    generation: u64,
    /// The lead's clarification question went out; the answer waits for it.
    asked: bool,
    /// The blocked bot's explanation, kept as the lead receives it (never the refused message). `Some` means it
    /// was given: an explanation happens once.
    explanation: Option<String>,
    /// The blocked bot's answer, kept the same way, once and only after the question.
    answer: Option<String>,
}

/// A reset the running chat did not absorb. Applied once, at a later chat start of the same generation.
struct Grant {
    refusal: u64,
    sender: String,
    generation: u64,
}

#[derive(Clone, Copy, Debug)]
enum AuditKind {
    Notice,
    Explain,
    Ask,
    Answer,
    Reset,
    Deny,
}

/// A hop-limit rejection. [`HopLimited::generation`] is the actor's generation under the same lock as the rejection,
/// so a user message that starts afterwards cannot retag the refusal.
#[derive(Debug)]
pub(super) struct HopLimited {
    pub(super) generation: u64,
    limit: u8,
}

impl std::fmt::Display for HopLimited {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "bot request hop limit ({}) reached: the user has a Continue button in the chat to start a new chain; tell them where things stand and end your turn",
            self.limit
        )
    }
}

impl std::error::Error for HopLimited {}

/// The generation captured with a hop-limit rejection, when `error` is one.
pub fn hop_generation(error: &anyhow::Error) -> Option<u64> {
    error.downcast_ref::<HopLimited>().map(|hop| hop.generation)
}

impl Requests {
    #[cfg(test)]
    pub(super) fn reserve_message(&self, from: &str) -> Result<u8> {
        self.reserve_message_in(from, from)
    }

    /// Takes one of `from`'s places for a message it sends from `turn`'s running turn: its own, or that of a thread of
    /// its that acts as it. The chain's hops so far are that turn's.
    pub(super) fn reserve_message_in(&self, from: &str, turn: &str) -> Result<u8> {
        let mut state = self.0.locked();
        let hops = state.next_hop(turn)?;
        if state.messages >= MAX_PENDING_MESSAGES {
            bail!("too many pending bot messages");
        }
        let count = state.messages_by_sender.entry(from.to_owned()).or_default();
        if *count >= MAX_PENDING_MESSAGES_PER_SENDER {
            bail!("too many pending bot messages from this bot (limit {MAX_PENDING_MESSAGES_PER_SENDER})");
        }
        *count += 1;
        state.messages += 1;
        Ok(hops)
    }

    /// Takes `from`'s place again for a message the last host process admitted (`restart`): past the limits, which
    /// held when it came in.
    pub(super) fn readmit(&self, from: &str) {
        let mut state = self.0.locked();
        *state.messages_by_sender.entry(from.to_owned()).or_default() += 1;
        state.messages += 1;
    }

    pub(super) fn release_message(&self, from: &str) {
        let mut state = self.0.locked();
        state.messages -= 1;
        if let Some(count) = state.messages_by_sender.get_mut(from) {
            *count -= 1;
            if *count == 0 {
                state.messages_by_sender.remove(from);
            }
        }
    }

    pub fn start_turn(&self, bot: &str, turn: Turn) {
        let mut state = self.0.locked();
        state.active.insert(bot.to_owned(), turn);
        state.stopped.remove(bot);
    }

    /// Whether `turn`'s running turn may make no more handoffs.
    pub(super) fn at_hop_limit(&self, turn: &str) -> bool {
        self.0.locked().at_hop_limit(turn)
    }

    /// Whether this is the first time `turn`'s running turn was stopped at a limit: the user gets one notice a turn.
    pub(super) fn first_stop(&self, turn: &str) -> bool {
        self.0.locked().stopped.insert(turn.to_owned())
    }

    /// Whether `bot`'s running turn answers another bot's request.
    pub fn answering(&self, bot: &str) -> bool {
        matches!(self.0.locked().active.get(bot), Some(Turn::Request))
    }

    /// `bot`'s turn ended (or it was deleted): it can't send more until its next turn.
    pub fn end_turn(&self, bot: &str) {
        let mut state = self.0.locked();
        state.active.remove(bot);
        state.stopped.remove(bot);
    }

    /// `bot`'s turn ended: the active turn is removed and a hop-limit clarification flag is cleared under the one
    /// lock, so a clarification turn cannot stay active while ordinary sends are already open (or the reverse).
    pub fn end_turn_in_control(&self, bot: &str) {
        let mut state = self.0.locked();
        state.active.remove(bot);
        state.stopped.remove(bot);
        if let Some(actor) = state.actors.get_mut(bot) {
            actor.control = false;
        }
    }

    /// `bot` was deleted: everything the hop limit kept for it goes too. Without this, [`Self::user_message_started`]
    /// leaves one entry per bot that ever ran, and `actors` grows for the life of the process. A lift the deleted bot
    /// held is gone with it: no approval can reach it any more.
    pub fn forget(&self, bot: &str) {
        self.0.locked().actors.remove(bot);
    }

    /// A clarification wake that queued but whose turn failed to start: the question never reached the bot,
    /// so the lead may ask again.
    pub fn abandon_open_ask(&self, actor: &str) {
        if let Some(entry) = self.0.locked().actors.get_mut(actor)
            && let Some(lift) = entry.lift.as_mut()
            && lift.asked
        {
            lift.asked = false;
        }
    }

    /// A queued clarification wake was dropped without running (a Stop): the question never reached the bot,
    /// so the lead may ask again.
    pub fn abandon_open_ask_by_refusal(&self, refusal: u64) {
        let mut state = self.0.locked();
        if let Some(lift) =
            state.actors.values_mut().find_map(|entry| entry.lift.as_mut().filter(|lift| lift.id == refusal))
            && lift.asked
        {
            lift.asked = false;
        }
    }
    /// `limit` is the handoffs one chain may use, from 8 through 32. Anything else leaves the current limit.
    /// Nothing outside tests calls this yet, so a running host stays at [`MAX_REQUEST_HOPS`].
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_hop_limit(&self, limit: u8) -> Result<()> {
        if !(MIN_HOP_LIMIT..=MAX_HOP_LIMIT).contains(&limit) {
            bail!("hop limit must be from {MIN_HOP_LIMIT} through {MAX_HOP_LIMIT}");
        }
        self.0.locked().hop_limit = limit;
        Ok(())
    }

    pub fn hop_limit(&self) -> u8 {
        self.0.locked().hop_limit
    }

    /// Opens the actor's one lift and returns its refusal id.
    ///
    /// `None` when a lift is already open, the lead's denial is still in force, a grant or a remembered reset is
    /// still unspent, or `generation` is not the actor's current one. The caller delivers a notice only for `Some`.
    pub fn open_lift(&self, sender: &str, actor: &str, generation: u64) -> Option<u64> {
        let mut state = self.0.locked();
        let (current, open, denied, granted, carried, pending_restore) = match state.actors.get(actor) {
            Some(entry) => (
                entry.generation,
                entry.lift.is_some(),
                entry.denied,
                entry.grant.is_some(),
                entry.carry_reset,
                entry.pending_restore.is_some(),
            ),
            None => (0, false, false, false, false, false),
        };
        // A pending grant, a remembered reset, or a turn that consumed one but has not delivered its prompt yet
        // still owns the last refusal. A new one must not overwrite it.
        if current != generation || open || denied || granted || carried || pending_restore {
            return None;
        }
        let id = state.next_refusal;
        state.next_refusal = state.next_refusal.saturating_add(1);
        state.actors.entry(actor.to_owned()).or_default().lift =
            Some(Lift { id, sender: sender.to_owned(), generation, asked: false, explanation: None, answer: None });
        RequestState::record(AuditKind::Notice, id);
        Some(id)
    }

    /// Generation last reached by a user message. An actor the user has not messaged is at 0.
    pub fn generation(&self, actor: &str) -> u64 {
        self.0.locked().actors.get(actor).map_or(0, |entry| entry.generation)
    }

    /// A user message for `actor`: the open lift and any unused grant are dropped, and a denial no longer blocks
    /// another notice. The reset budget starts over.
    pub fn user_message_started(&self, actor: &str) {
        let mut state = self.0.locked();
        let entry = state.actors.entry(actor.to_owned()).or_default();
        entry.generation += 1;
        entry.resets = 0;
        entry.denied = false;
        entry.lift = None;
        entry.grant = None;
        entry.carry_reset = false;
        // The user took over: a failed turn's approval is void, and nothing waits to be restored.
        entry.pending_restore = None;
    }

    /// The blocked actor's explanation, once, for its own open lift. Returns the stored text, which is what the
    /// lead receives.
    pub fn explain(&self, actor: &str, refusal: u64, text: &str) -> Result<String> {
        let mut state = self.0.locked();
        let owner = state.actor_of(refusal)?;
        if owner != actor {
            bail!("this hop-limit block belongs to another bot");
        }
        let trimmed = text.trim();
        if trimmed.is_empty() {
            bail!("text must be nonempty");
        }
        let stored = {
            let Some(lift) = state.actors.get_mut(actor).and_then(|entry| entry.lift.as_mut()) else {
                bail!("no open hop-limit block for this refusal");
            };
            if lift.explanation.is_some() {
                bail!("this hop-limit block was already explained");
            }
            // `truncate` keeps a nonempty prefix: the cap is 400 bytes, then one ellipsis when it cuts.
            let stored = truncate(trimmed, MAX_LIFT_TEXT);
            lift.explanation = Some(stored.clone());
            stored
        };
        RequestState::record(AuditKind::Explain, refusal);
        Ok(stored)
    }

    /// The lead's clarification question, once. The caller checks that the caller is the lead.
    pub fn ask(&self, refusal: u64) -> Result<()> {
        let mut state = self.0.locked();
        let owner = state.actor_of(refusal)?;
        {
            let Some(lift) = state.actors.get_mut(&owner).and_then(|entry| entry.lift.as_mut()) else {
                bail!("no open hop-limit block for this refusal");
            };
            if lift.asked {
                bail!("a clarification was already asked for this hop-limit block");
            }
            lift.asked = true;
        }
        RequestState::record(AuditKind::Ask, refusal);
        Ok(())
    }

    /// The clarification wake was never queued. Clears `asked` so the lead can ask again, the same way an
    /// unsent explanation or answer is cleared for a retry.
    pub fn abort_unsent_ask(&self, refusal: u64) -> bool {
        let mut state = self.0.locked();
        let Ok(owner) = state.actor_of(refusal) else { return false };
        let aborted = {
            let Some(lift) = state.actors.get_mut(&owner).and_then(|entry| entry.lift.as_mut()) else {
                return false;
            };
            if lift.id != refusal || !lift.asked {
                return false;
            }
            lift.asked = false;
            true
        };
        if aborted {
            tracing::debug!(refusal, "hop-limit ask refunded");
        }
        aborted
    }

    /// The blocked actor's answer, once, and only after [`Requests::ask`]. Returns the stored text, which is what
    /// the lead receives.
    pub fn answer(&self, actor: &str, refusal: u64, text: &str) -> Result<String> {
        let mut state = self.0.locked();
        let owner = state.actor_of(refusal)?;
        if owner != actor {
            bail!("this hop-limit block belongs to another bot");
        }
        let trimmed = text.trim();
        if trimmed.is_empty() {
            bail!("text must be nonempty");
        }
        let stored = {
            let Some(lift) = state.actors.get_mut(actor).and_then(|entry| entry.lift.as_mut()) else {
                bail!("no open hop-limit block for this refusal");
            };
            if !lift.asked {
                bail!("answer a hop-limit clarification only after the lead asks");
            }
            if lift.answer.is_some() {
                bail!("this hop-limit clarification was already answered");
            }
            let stored = truncate(trimmed, MAX_LIFT_TEXT);
            lift.answer = Some(stored.clone());
            stored
        };
        RequestState::record(AuditKind::Answer, refusal);
        Ok(stored)
    }

    /// Grants the reset named by `refusal`.
    ///
    /// The running turn is left as it is, including a clarification or a later reply. The grant waits for the
    /// approval wake. That wake is the only caller that takes a grant which can still run. The fourth grant
    /// since the last user message is refused and does not reset.
    pub fn approve(&self, refusal: u64) -> Result<()> {
        let mut state = self.0.locked();
        let actor_id = state.actor_of(refusal)?;
        let (generation, sender, resets) = {
            let Some(actor) = state.actors.get(&actor_id) else {
                bail!("no open hop-limit block for this refusal");
            };
            let Some(lift) = actor.lift.as_ref() else {
                bail!("no open hop-limit block for this refusal");
            };
            (lift.generation, lift.sender.clone(), actor.resets)
        };
        if resets >= MAX_RESETS {
            tracing::warn!(actor = %actor_id, refusal, "hop reset refused");
            bail!("hop reset limit ({MAX_RESETS}) reached since the last user message");
        }
        let Some(actor) = state.actors.get_mut(&actor_id) else {
            bail!("no open hop-limit block for this refusal");
        };
        actor.resets += 1;
        actor.lift = None;
        actor.grant = Some(Grant { refusal, sender, generation });
        RequestState::record(AuditKind::Reset, refusal);
        Ok(())
    }

    /// The explanation notice was never queued. Clears it so the blocked bot can explain again.
    pub fn abort_unsent_explanation(&self, actor: &str, refusal: u64) -> bool {
        let mut state = self.0.locked();
        let aborted = {
            let Some(entry) = state.actors.get_mut(actor) else { return false };
            let Some(lift) = entry.lift.as_mut() else { return false };
            if lift.id != refusal || lift.explanation.is_none() {
                return false;
            }
            lift.explanation = None;
            true
        };
        if aborted {
            tracing::debug!(refusal, "hop-limit explanation refunded");
        }
        aborted
    }

    /// The answer notice was never queued. Clears it so the blocked bot can answer again.
    pub fn abort_unsent_answer(&self, actor: &str, refusal: u64) -> bool {
        let mut state = self.0.locked();
        let aborted = {
            let Some(entry) = state.actors.get_mut(actor) else { return false };
            let Some(lift) = entry.lift.as_mut() else { return false };
            if lift.id != refusal || lift.answer.is_none() {
                return false;
            }
            lift.answer = None;
            true
        };
        if aborted {
            tracing::debug!(refusal, "hop-limit answer refunded");
        }
        aborted
    }

    /// The approval wake was never queued. Drops that grant and the reset it counted, so a later refusal can open.
    pub fn abort_unsent_reset(&self, actor: &str, refusal: u64) -> bool {
        let mut state = self.0.locked();
        let aborted = {
            let Some(entry) = state.actors.get_mut(actor) else { return false };
            if entry.grant.as_ref().is_none_or(|grant| grant.refusal != refusal) {
                return false;
            }
            entry.grant = None;
            entry.resets = entry.resets.saturating_sub(1);
            true
        };
        if aborted {
            tracing::debug!(refusal, "hop-limit reset refunded");
        }
        aborted
    }

    /// A dropped wake's grant is taken and its carry recorded under the one lock: between them no lift may open.
    /// Returns whether the grant named by `refusal_id` existed and was converted.
    pub fn drop_wake_grant(&self, actor: &str, refusal_id: u64) -> bool {
        let mut state = self.0.locked();
        let Some(entry) = state.actors.get_mut(actor) else { return false };
        if entry.grant.as_ref().is_none_or(|grant| grant.refusal != refusal_id) {
            return false;
        }
        entry.grant = None;
        entry.carry_reset = true;
        true
    }

    /// A dropped wake left a reset for `actor`'s next reply. Does not consume it.
    pub fn dropped_wake(&self, actor: &str) -> bool {
        self.0.locked().actors.get(actor).is_some_and(|entry| entry.carry_reset)
    }

    /// Consumes a remembered reset once the reply turn that uses it has started. The consumption is pending
    /// until the turn's prompt is sent, at this generation: no new lift may open in between.
    pub fn take_dropped_wake(&self, actor: &str) -> bool {
        let mut state = self.0.locked();
        let Some(entry) = state.actors.get_mut(actor) else { return false };
        let remembered = entry.carry_reset;
        entry.carry_reset = false;
        if remembered {
            entry.pending_restore = Some(entry.generation);
        }
        remembered
    }

    /// The turn that consumed a carry delivered its prompt: the reset is used, nothing waits to be restored.
    pub fn wake_delivered(&self, actor: &str) {
        if let Some(entry) = self.0.locked().actors.get_mut(actor) {
            entry.pending_restore = None;
        }
    }

    /// Puts a consumed reset back: the turn that spent it failed to start, so the approval was never used.
    /// Applies only at the generation the carry was consumed at — a user message since voids the approval —
    /// and only when no newer approval or denial took the refusal over: restoring alongside would let one
    /// approval reset twice, and past a denial would restart the chain the lead closed.
    pub fn restore_dropped_wake(&self, actor: &str) {
        let mut state = self.0.locked();
        let Some(entry) = state.actors.get_mut(actor) else { return };
        if entry.pending_restore.is_none_or(|generation| generation != entry.generation) {
            return;
        }
        entry.pending_restore = None;
        if entry.grant.is_some() || entry.lift.is_some() || entry.denied {
            tracing::info!(actor, "carried reset not restored: a newer approval or denial owns the refusal");
            return;
        }
        entry.carry_reset = true;
    }

    /// Closes the lift. The chain is unchanged, and another notice waits for a user message.
    pub fn deny(&self, refusal: u64) -> Result<()> {
        let mut state = self.0.locked();
        let owner = state.actor_of(refusal)?;
        if let Some(entry) = state.actors.get_mut(&owner) {
            entry.lift = None;
            entry.denied = true;
        }
        RequestState::record(AuditKind::Deny, refusal);
        Ok(())
    }

    /// The lift's notice was never delivered. Closes the lift without a denial, so a later send can open a new
    /// one: the chain was never judged, and the refusal must not sit open forever.
    pub fn abort_unsent_notice(&self, actor: &str, refusal: u64) {
        let mut state = self.0.locked();
        if let Some(entry) = state.actors.get_mut(actor)
            && entry.lift.as_ref().is_some_and(|lift| lift.id == refusal)
        {
            entry.lift = None;
        }
        tracing::debug!(refusal, "hop-limit notice delivery finished");
    }

    #[cfg(test)]
    pub fn lift_open(&self, actor: &str) -> bool {
        self.0.locked().actors.get(actor).is_some_and(|entry| entry.lift.is_some())
    }

    /// Whether `refusal` is still the open lift (not denied, replaced or closed by a user message).
    pub fn refusal_open(&self, refusal: u64) -> bool {
        self.0.locked().actor_of(refusal).is_ok()
    }

    /// What the blocked bot explained for this lift, as the lead receives it.
    #[cfg(test)]
    pub fn explanation(&self, refusal: u64) -> Option<String> {
        self.0.locked().lift_of(refusal).and_then(|lift| lift.explanation.clone())
    }

    /// What the blocked bot answered for this lift, as the lead receives it.
    #[cfg(test)]
    pub fn answer_text(&self, refusal: u64) -> Option<String> {
        self.0.locked().lift_of(refusal).and_then(|lift| lift.answer.clone())
    }

    /// Marks `actor`'s running turn as a hop-limit clarification (`on`) or clears that when the turn ends.
    pub fn control(&self, actor: &str, on: bool) {
        self.0.locked().actors.entry(actor.to_owned()).or_default().control = on;
    }

    pub fn in_control(&self, actor: &str) -> bool {
        self.0.locked().actors.get(actor).is_some_and(|entry| entry.control)
    }

    /// `true` once, when a chat start for this generation should begin at hop 0.
    ///
    /// The grant is consumed and a carry is recorded under the same lock: if the turn that takes it never starts,
    /// the carry stands and the next reply begins at hop 0. In either case no lift can open in the gap. A grant
    /// from another generation is left unused and is not taken. `refusal`, when set, takes only that grant. A wake
    /// for a different refusal does not consume another one.
    pub fn take_grant_for_chat_start(&self, actor: &str, generation: u64, refusal: Option<u64>) -> bool {
        let mut state = self.0.locked();
        let Some(entry) = state.actors.get_mut(actor) else { return false };
        let Some(grant) = entry.grant.as_ref() else { return false };
        if grant.generation != generation || entry.generation != generation {
            return false;
        }
        if refusal.is_some_and(|id| id != grant.refusal) {
            return false;
        }
        let taken = grant.refusal;
        let sender = grant.sender.clone();
        entry.grant = None;
        entry.carry_reset = true;
        tracing::debug!(actor, refusal = taken, sender = %sender, generation, "hop reset applies at chat start");
        true
    }

    /// Whether `actor` still has its own explanation or answer waiting to reach the lead.
    #[cfg(test)]
    fn lift_text_pending(&self, actor: &str, refusal: u64) -> bool {
        self.0
            .locked()
            .actors
            .get(actor)
            .and_then(|entry| entry.lift.as_ref())
            .is_some_and(|lift| lift.id == refusal && (lift.explanation.is_some() || lift.answer.is_some()))
    }

    /// The actor that owns this open lift.
    pub fn lift_actor(&self, refusal: u64) -> Result<String> {
        self.0.locked().actor_of(refusal)
    }

    /// An approval is waiting for this actor's next chat turn.
    pub fn grant_is_pending(&self, actor: &str) -> bool {
        self.0.locked().actors.get(actor).is_some_and(|entry| entry.grant.is_some())
    }
}

impl RequestState {
    fn at_hop_limit(&self, turn: &str) -> bool {
        matches!(self.active.get(turn), Some(&Turn::Chat(hops)) if hops >= self.hop_limit)
    }

    fn next_hop(&self, from: &str) -> Result<u8> {
        // A clarification is a control turn: ordinary sends stay closed even at hop 0.
        if self.actors.get(from).is_some_and(|actor| actor.control) {
            bail!("message_bot isn't available on a hop-limit turn; use the hop-limit tools");
        }
        let limit = self.hop_limit;
        match self.active.get(from) {
            None => bail!("the requesting bot is no longer working"),
            Some(Turn::Request) => {
                bail!(
                    "message_bot isn't available while answering another bot's request: say what you need in your reply"
                )
            }
            Some(&Turn::Chat(hops)) if hops >= limit => {
                let generation = self.actors.get(from).map_or(0, |actor| actor.generation);
                Err(HopLimited { generation, limit }.into())
            }
            Some(&Turn::Chat(hops)) => Ok(hops + 1),
        }
    }

    fn actor_of(&self, refusal: u64) -> Result<String> {
        match self.actors.iter().find(|(_, actor)| actor.lift.as_ref().is_some_and(|lift| lift.id == refusal)) {
            Some((id, _)) => Ok(id.clone()),
            None => bail!("no open hop-limit block for this refusal"),
        }
    }

    #[cfg(test)]
    fn lift_of(&self, refusal: u64) -> Option<&Lift> {
        self.actors.values().find_map(|actor| actor.lift.as_ref().filter(|lift| lift.id == refusal))
    }

    /// Logs a decision with its refusal id; lift history is never retained.
    fn record(kind: AuditKind, refusal: u64) {
        tracing::debug!(refusal, ?kind, "hop-limit");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhausted_chain_rejects_messages_without_reserving_capacity() {
        let requests = Requests::default();
        requests.start_turn("a", Turn::Chat(MAX_REQUEST_HOPS));
        let message = requests.reserve_message("a");
        assert!(message.unwrap_err().to_string().contains("hop limit"));
        assert_eq!(requests.0.locked().messages, 0);
        requests.start_turn("a", Turn::Chat(0));
        assert_eq!(requests.reserve_message("a").unwrap(), 1);
    }

    #[test]
    fn last_allowed_handoff_is_admitted() {
        let requests = Requests::default();
        requests.start_turn("a", Turn::Chat(MAX_REQUEST_HOPS - 1));
        assert_eq!(requests.reserve_message("a").unwrap(), MAX_REQUEST_HOPS);
    }

    #[test]
    fn a_bot_answering_a_request_cannot_send() {
        let requests = Requests::default();
        requests.start_turn("a", Turn::Request);
        assert!(requests.reserve_message("a").unwrap_err().to_string().contains("answering another bot's request"));
        assert_eq!(requests.0.locked().messages, 0);
    }

    #[test]
    fn a_bot_outside_a_turn_cannot_send() {
        let requests = Requests::default();
        assert!(requests.reserve_message("a").unwrap_err().to_string().contains("no longer working"));
        requests.start_turn("a", Turn::Chat(0));
        requests.end_turn("a");
        assert!(requests.reserve_message("a").is_err());
    }

    #[test]
    fn sender_quota_leaves_capacity_for_other_bots() {
        let requests = Requests::default();
        for bot in ["a", "b"] {
            requests.start_turn(bot, Turn::Chat(0));
        }
        for _ in 0..MAX_PENDING_MESSAGES_PER_SENDER {
            requests.reserve_message("a").unwrap();
        }
        let result = requests.reserve_message("a");
        assert!(result.unwrap_err().to_string().contains("from this bot (limit 16)"));
        assert_eq!(requests.0.locked().messages, MAX_PENDING_MESSAGES_PER_SENDER);
        requests.reserve_message("b").unwrap();
        requests.release_message("a");
        requests.reserve_message("a").unwrap();
        for _ in 0..MAX_PENDING_MESSAGES_PER_SENDER {
            requests.release_message("a");
        }
        requests.release_message("b");
        let state = requests.0.locked();
        assert_eq!(state.messages, 0);
        assert!(state.messages_by_sender.is_empty());
    }

    #[test]
    fn global_capacity_still_bounds_messages_from_multiple_senders() {
        let requests = Requests::default();
        for bot in ["a", "b", "c", "d"] {
            requests.start_turn(bot, Turn::Chat(0));
            for _ in 0..MAX_PENDING_MESSAGES_PER_SENDER {
                requests.reserve_message(bot).unwrap();
            }
        }
        assert_eq!(requests.0.locked().messages, MAX_PENDING_MESSAGES);
        requests.start_turn("e", Turn::Chat(0));
        let result = requests.reserve_message("e");
        assert_eq!(result.unwrap_err().to_string(), "too many pending bot messages");
        assert!(!requests.0.locked().messages_by_sender.contains_key("e"));
        requests.release_message("a");
        requests.reserve_message("e").unwrap();
        assert_eq!(requests.0.locked().messages, MAX_PENDING_MESSAGES);
    }

    #[test]
    fn concurrent_reservations_cannot_exceed_the_sender_quota() {
        let requests = Requests::default();
        requests.start_turn("a", Turn::Chat(0));
        let barrier = std::sync::Barrier::new(MAX_PENDING_MESSAGES);
        let admitted = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..MAX_PENDING_MESSAGES)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        requests.reserve_message("a").is_ok()
                    })
                })
                .collect();
            handles.into_iter().map(|handle| handle.join().unwrap()).filter(|admitted| *admitted).count()
        });
        assert_eq!(admitted, MAX_PENDING_MESSAGES_PER_SENDER);
        assert_eq!(requests.0.locked().messages, admitted);
        for _ in 0..admitted {
            requests.release_message("a");
        }
        assert!(requests.0.locked().messages_by_sender.is_empty());
    }

    #[test]
    fn duplicate_open_lift_returns_no_second_id() {
        let requests = Requests::default();
        let first = requests.open_lift("sender", "a", 0).unwrap();
        assert!(requests.open_lift("sender", "a", 0).is_none());
        assert!(requests.open_lift("other", "a", 0).is_none());
        assert!(requests.lift_open("a"));
        let other = requests.open_lift("sender", "b", 0).unwrap();
        assert_ne!(first, other);
        assert!(requests.lift_open("b"));
    }

    #[test]
    fn denial_closes_the_lift_until_a_user_message() {
        let requests = Requests::default();
        requests.start_turn("a", Turn::Chat(MAX_REQUEST_HOPS));
        let refusal = requests.open_lift("sender", "a", 0).unwrap();
        assert!(requests.explain("b", refusal, "please").is_err());
        requests.explain("a", refusal, "please").unwrap();
        assert!(requests.explain("a", refusal, "again").is_err());
        assert!(requests.answer("a", refusal, "because").is_err());
        requests.ask(refusal).unwrap();
        assert!(requests.ask(refusal).is_err());
        assert!(requests.answer("b", refusal, "because").is_err());
        requests.answer("a", refusal, "because").unwrap();
        assert!(requests.answer("a", refusal, "again").is_err());
        requests.deny(refusal).unwrap();
        assert!(!requests.lift_open("a"));
        assert!(requests.deny(refusal).is_err());
        assert!(requests.approve(refusal).is_err());
        assert!(requests.open_lift("sender", "a", 0).is_none());
        assert!(matches!(requests.0.locked().active.get("a"), Some(&Turn::Chat(MAX_REQUEST_HOPS))));
        assert!(!requests.take_grant_for_chat_start("a", 0, None));
        assert!(!requests.lift_text_pending("a", refusal));
        assert_eq!(requests.0.locked().messages, 0);

        requests.user_message_started("a");
        assert_eq!(requests.generation("a"), 1);
        assert!(requests.open_lift("sender", "a", 1).is_some());
    }

    #[test]
    fn a_deleted_bot_leaves_no_entry_behind() {
        let requests = Requests::default();
        requests.start_turn("a", Turn::Chat(0));
        let refusal = requests.open_lift("sender", "a", 0).unwrap();
        requests.user_message_started("a");
        requests.control("a", true);
        assert!(requests.first_stop("a"));
        assert_eq!(requests.generation("a"), 1);

        requests.end_turn("a");
        requests.forget("a");

        let state = requests.0.locked();
        assert!(state.actors.is_empty(), "a deleted bot keeps no hop-limit entry");
        assert!(state.active.is_empty());
        assert!(state.stopped.is_empty());
        drop(state);
        assert_eq!(requests.generation("a"), 0);
        assert!(!requests.refusal_open(refusal), "nothing approves a lift for a bot that is gone");
        assert!(!requests.in_control("a"));
        // A re-created bot starts clean rather than inheriting the old generation.
        requests.start_turn("a", Turn::Chat(0));
        assert!(requests.open_lift("sender", "a", 0).is_some());
    }

    #[test]
    fn approve_during_an_active_chat_waits_for_its_wake() {
        let requests = Requests::default();
        requests.start_turn("a", Turn::Chat(MAX_REQUEST_HOPS));
        let refusal = requests.open_lift("sender", "a", 0).unwrap();
        requests.approve(refusal).unwrap();
        assert!(requests.open_lift("sender", "a", 0).is_none(), "a pending grant blocks another refusal");
        assert!(matches!(requests.0.locked().active.get("a"), Some(&Turn::Chat(MAX_REQUEST_HOPS))));
        assert!(!requests.lift_open("a"));
        assert!(requests.take_grant_for_chat_start("a", 0, Some(refusal)));
        assert!(requests.reserve_message("a").unwrap_err().to_string().contains("hop limit"));
        assert!(requests.approve(refusal).is_err());
    }

    #[test]
    fn approve_does_not_change_a_request_turn() {
        let requests = Requests::default();
        requests.start_turn("a", Turn::Request);
        let refusal = requests.open_lift("sender", "a", 0).unwrap();
        requests.approve(refusal).unwrap();
        assert!(requests.answering("a"));
        assert!(requests.reserve_message("a").unwrap_err().to_string().contains("answering another bot's request"));
        assert_eq!(requests.0.locked().messages, 0);
        assert!(!requests.take_grant_for_chat_start("a", 1, None));
        assert!(requests.take_grant_for_chat_start("a", 0, None));
        assert!(!requests.take_grant_for_chat_start("a", 0, None));
    }

    #[test]
    fn approve_when_the_turn_has_ended_is_taken_at_chat_start() {
        let requests = Requests::default();
        requests.start_turn("a", Turn::Chat(MAX_REQUEST_HOPS));
        requests.end_turn("a");
        let refusal = requests.open_lift("sender", "a", requests.generation("a")).unwrap();
        requests.approve(refusal).unwrap();
        assert!(!requests.0.locked().active.contains_key("a"));
        assert!(!requests.lift_open("a"));
        assert!(!requests.take_grant_for_chat_start("a", 99, None));
        assert!(requests.take_grant_for_chat_start("a", 0, None));
        assert!(!requests.take_grant_for_chat_start("a", 0, None));
    }

    #[test]
    fn stale_generation_after_user_message_started() {
        let requests = Requests::default();
        requests.start_turn("a", Turn::Chat(MAX_REQUEST_HOPS));
        let refusal = requests.open_lift("sender", "a", 0).unwrap();
        requests.user_message_started("a");
        assert_eq!(requests.generation("a"), 1);
        assert!(!requests.lift_open("a"));
        assert!(requests.approve(refusal).is_err());
        assert!(requests.explain("a", refusal, "please").is_err());
        assert!(requests.open_lift("sender", "a", 0).is_none());
        assert!(matches!(requests.0.locked().active.get("a"), Some(&Turn::Chat(MAX_REQUEST_HOPS))));

        requests.end_turn("a");
        let refusal = requests.open_lift("sender", "a", 1).unwrap();
        requests.approve(refusal).unwrap();
        assert!(!requests.take_grant_for_chat_start("a", 0, None));
        requests.user_message_started("a");
        assert!(!requests.take_grant_for_chat_start("a", 1, None));
        assert!(!requests.take_grant_for_chat_start("a", 2, None));
        assert_eq!(requests.generation("a"), 2);
    }

    #[test]
    fn fourth_approve_fails() {
        let requests = Requests::default();
        for _ in 0..3 {
            requests.start_turn("a", Turn::Chat(requests.hop_limit()));
            let refusal = requests.open_lift("sender", "a", requests.generation("a")).unwrap();
            requests.approve(refusal).unwrap();
            assert!(matches!(requests.0.locked().active.get("a"), Some(&Turn::Chat(MAX_REQUEST_HOPS))));
            assert!(requests.take_grant_for_chat_start("a", 0, Some(refusal)));
            // Taking the grant records a carry until the wake's turn starts; the wake then delivers its
            // prompt and spends it.
            assert!(requests.take_dropped_wake("a"));
            requests.wake_delivered("a");
        }
        requests.start_turn("a", Turn::Chat(requests.hop_limit()));
        let refusal = requests.open_lift("sender", "a", 0).unwrap();
        let resets = requests.0.locked().actors.get("a").unwrap().resets;
        let err = requests.approve(refusal).unwrap_err().to_string();
        assert!(err.contains("hop reset limit (3)"));
        assert_eq!(requests.0.locked().actors.get("a").unwrap().resets, resets);
        assert!(!requests.grant_is_pending("a"));
        assert!(requests.lift_open("a"));
        assert!(matches!(requests.0.locked().active.get("a"), Some(&Turn::Chat(MAX_REQUEST_HOPS))));
        let blocked = requests.reserve_message("a").unwrap_err().to_string();
        assert!(blocked.contains("hop limit"));
        assert_eq!(requests.0.locked().messages, 0);

        requests.user_message_started("a");
        let refusal = requests.open_lift("sender", "a", requests.generation("a")).unwrap();
        requests.approve(refusal).unwrap();
        assert!(matches!(requests.0.locked().active.get("a"), Some(&Turn::Chat(MAX_REQUEST_HOPS))));
        assert!(requests.take_grant_for_chat_start("a", requests.generation("a"), Some(refusal)));
    }

    #[test]
    fn set_hop_limit_rejects_7_and_33_and_accepts_16() {
        let requests = Requests::default();
        assert_eq!(requests.hop_limit(), MAX_REQUEST_HOPS);
        assert!(requests.set_hop_limit(7).is_err());
        assert!(requests.set_hop_limit(33).is_err());
        assert_eq!(requests.hop_limit(), MAX_REQUEST_HOPS);
        requests.set_hop_limit(8).unwrap();
        requests.set_hop_limit(32).unwrap();
        assert_eq!(requests.hop_limit(), 32);
        requests.set_hop_limit(16).unwrap();
        assert_eq!(requests.hop_limit(), 16);
        requests.start_turn("a", Turn::Chat(8));
        assert_eq!(requests.reserve_message("a").unwrap(), 9);
        requests.start_turn("a", Turn::Chat(16));
        let err = requests.reserve_message("a").unwrap_err().to_string();
        assert!(err.contains("hop limit (16)"));
        assert_eq!(requests.0.locked().messages, 1);
    }

    #[test]
    fn two_threads_calling_open_lift_produce_one_id() {
        let requests = Requests::default();
        let barrier = std::sync::Barrier::new(2);
        let opened: Vec<Option<u64>> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..2)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        requests.open_lift("sender", "a", 0)
                    })
                })
                .collect();
            handles.into_iter().map(|handle| handle.join().unwrap()).collect()
        });
        let ids: Vec<u64> = opened.into_iter().flatten().collect();
        assert_eq!(ids.len(), 1);
        assert!(requests.lift_open("a"));
        assert!(requests.open_lift("sender", "a", 0).is_none());
    }

    #[test]
    fn a_control_turn_cannot_reserve() {
        let requests = Requests::default();
        requests.start_turn("a", Turn::Chat(0));
        requests.control("a", true);
        assert!(requests.in_control("a"));
        let err = requests.reserve_message("a").unwrap_err().to_string();
        assert_eq!(err, "message_bot isn't available on a hop-limit turn; use the hop-limit tools");
        assert_eq!(requests.0.locked().messages, 0);
        requests.start_turn("a", Turn::Chat(MAX_REQUEST_HOPS));
        let at_limit = requests.reserve_message("a").unwrap_err().to_string();
        assert_eq!(at_limit, "message_bot isn't available on a hop-limit turn; use the hop-limit tools");
        requests.control("a", false);
        assert!(!requests.in_control("a"));
        assert!(requests.reserve_message("a").unwrap_err().to_string().contains("hop limit"));
        assert_eq!(requests.0.locked().messages, 0);
        requests.start_turn("a", Turn::Chat(0));
        assert_eq!(requests.reserve_message("a").unwrap(), 1);
    }

    #[test]
    fn an_explanation_and_an_answer_are_stored_for_the_lead() {
        let requests = Requests::default();
        requests.start_turn("a", Turn::Chat(MAX_REQUEST_HOPS));
        let refusal = requests.open_lift("sender", "a", 0).unwrap();
        assert_eq!(requests.explanation(refusal), None);
        assert!(requests.explain("a", refusal, "   ").is_err(), "blank text is not an explanation");

        let explained = requests.explain("a", refusal, "  the user asked for this exact handoff  ").unwrap();
        assert_eq!(explained, "the user asked for this exact handoff");
        assert_eq!(requests.explanation(refusal).as_deref(), Some("the user asked for this exact handoff"));

        requests.ask(refusal).unwrap();
        assert!(requests.answer("a", refusal, " ").is_err(), "blank text is not an answer");
        let answered = requests.answer("a", refusal, "answer text").unwrap();
        assert_eq!(answered, "answer text");
        assert_eq!(requests.answer_text(refusal).as_deref(), Some("answer text"));
        assert_eq!(requests.explanation(refusal).as_deref(), Some("the user asked for this exact handoff"));
        assert!(requests.lift_text_pending("a", refusal));
    }

    #[test]
    fn stored_lift_text_is_truncated_to_400_characters() {
        let requests = Requests::default();
        let refusal = requests.open_lift("sender", "a", 0).unwrap();
        let long = "x".repeat(MAX_LIFT_TEXT + 50);
        let stored = requests.explain("a", refusal, &long).unwrap();
        assert_eq!(stored.chars().count(), MAX_LIFT_TEXT + 1, "400 characters and the ellipsis");
        assert!(stored.ends_with('…'));
        assert_eq!(requests.explanation(refusal).as_deref(), Some(stored.as_str()));

        requests.ask(refusal).unwrap();
        let stored = requests.answer("a", refusal, &long).unwrap();
        assert_eq!(requests.answer_text(refusal).as_deref(), Some(stored.as_str()));
        assert!(stored.ends_with('…'));
    }

    #[test]
    fn a_denied_or_closed_lift_is_no_longer_open() {
        let requests = Requests::default();
        let denial = requests.open_lift("sender", "a", 0).unwrap();
        assert!(requests.refusal_open(denial));
        requests.deny(denial).unwrap();
        assert!(!requests.refusal_open(denial));

        let closed = requests.open_lift("sender", "b", 0).unwrap();
        assert!(requests.refusal_open(closed));
        requests.user_message_started("b");
        assert!(!requests.refusal_open(closed));

        let replaced = requests.open_lift("sender", "c", 0).unwrap();
        assert!(requests.refusal_open(replaced));
        requests.approve(replaced).unwrap();
        assert!(!requests.refusal_open(replaced));
    }

    #[test]
    fn a_pending_grant_waits_for_the_matching_hop_lift_wake() {
        let requests = Requests::default();
        requests.start_turn("a", Turn::Request);
        let refusal = requests.open_lift("sender", "a", 0).unwrap();
        requests.approve(refusal).unwrap();
        // A plain chat start (a reply turn) doesn't take it: only the wake for this refusal may.
        assert!(requests.grant_is_pending("a"));
        assert!(!requests.take_grant_for_chat_start("a", 0, Some(refusal + 1)));
        assert!(requests.grant_is_pending("a"));
        assert!(requests.take_grant_for_chat_start("a", 0, Some(refusal)));
        assert!(!requests.grant_is_pending("a"));
        assert!(!requests.take_grant_for_chat_start("a", 0, Some(refusal)));
    }

    #[test]
    fn an_unsent_reset_releases_the_grant_and_the_budget() {
        let requests = Requests::default();
        let refusal = requests.open_lift("sender", "a", 0).unwrap();
        requests.approve(refusal).unwrap();
        assert!(!requests.abort_unsent_reset("a", refusal + 1));
        assert!(requests.grant_is_pending("a"));
        assert!(requests.abort_unsent_reset("a", refusal));
        assert!(!requests.grant_is_pending("a"));
        assert_eq!(requests.0.locked().actors.get("a").unwrap().resets, 0);
        // The aborted reset did not count, and it does not block the next refusal.
        for _ in 0..3 {
            let id = requests.open_lift("sender", "a", 0).unwrap();
            requests.approve(id).unwrap();
            // Taking the grant records a carry until the wake's turn starts; the wake then delivers its
            // prompt and spends it.
            assert!(requests.take_grant_for_chat_start("a", 0, Some(id)));
            assert!(requests.take_dropped_wake("a"));
            requests.wake_delivered("a");
        }
        let blocked = requests.open_lift("sender", "a", 0).unwrap();
        assert!(requests.approve(blocked).unwrap_err().to_string().contains("hop reset limit (3)"));
    }

    #[test]
    fn a_failed_delivery_leaves_the_explanation_and_the_answer_for_a_retry() {
        let requests = Requests::default();
        let refusal = requests.open_lift("sender", "a", 0).unwrap();
        requests.explain("a", refusal, "because").unwrap();
        assert!(requests.explain("a", refusal, "again").is_err());
        assert!(requests.abort_unsent_explanation("a", refusal));
        assert!(!requests.abort_unsent_explanation("a", refusal));
        assert_eq!(requests.explanation(refusal), None);
        assert_eq!(requests.explain("a", refusal, "because").unwrap(), "because");
        assert!(requests.explain("a", refusal, "again").unwrap_err().to_string().contains("already explained"));

        requests.ask(refusal).unwrap();
        requests.answer("a", refusal, "the detail").unwrap();
        assert!(requests.answer("a", refusal, "again").is_err());
        assert!(requests.abort_unsent_answer("a", refusal));
        assert_eq!(requests.answer_text(refusal), None);
        assert!(!requests.abort_unsent_answer("other", refusal));
        assert_eq!(requests.answer("a", refusal, "the detail").unwrap(), "the detail");
        assert!(requests.answer("a", refusal, "again").unwrap_err().to_string().contains("already answered"));
    }

    #[test]
    fn a_user_message_before_open_lift_does_not_store_the_new_generation() {
        let requests = Requests::default();
        requests.user_message_started("a");
        requests.start_turn("a", Turn::Chat(MAX_REQUEST_HOPS));
        let rejected = requests.reserve_message("a").unwrap_err();
        let generation = hop_generation(&rejected).expect("the rejection carries the generation");
        assert_eq!(generation, 1);
        assert!(rejected.to_string().contains("hop limit (8)"));
        assert_eq!(requests.0.locked().messages, 0);

        // The user message lands after the rejection and before the refusal is stored.
        requests.user_message_started("a");
        assert_eq!(requests.generation("a"), generation + 1);
        assert!(requests.open_lift("sender", "a", generation).is_none());
        assert!(!requests.lift_open("a"), "the refusal must not be stored on the new generation");
    }

    #[test]
    fn a_dropped_wake_is_remembered_until_its_reply_starts() {
        let requests = Requests::default();
        let refusal = requests.open_lift("sender", "a", 0).unwrap();
        requests.approve(refusal).unwrap();
        // Taking the grant records the carry under the same lock (the wake's turn then spends it).
        assert!(requests.take_grant_for_chat_start("a", 0, Some(refusal)));
        assert!(requests.dropped_wake("a"));
        // The remembered reset still owns the refusal: a new lift waits until the reply spends the carry.
        assert!(requests.open_lift("sender", "a", 0).is_none());
        assert!(requests.take_dropped_wake("a"));
        requests.wake_delivered("a");
        assert!(!requests.dropped_wake("a"));

        // A wake whose grant was converted by Stop carries too, and a user message drops it.
        let refusal = requests.open_lift("sender", "a", 0).unwrap();
        requests.approve(refusal).unwrap();
        assert!(requests.drop_wake_grant("a", refusal));
        assert!(requests.dropped_wake("a"));
        // The carry blocks another lift until the reply spends it.
        assert!(requests.open_lift("sender", "a", 0).is_none());
        assert!(requests.take_dropped_wake("a"));
        requests.user_message_started("a");
        assert!(!requests.dropped_wake("a"));
    }

    #[test]
    fn a_carry_spent_by_a_failed_start_is_restored() {
        let requests = Requests::default();
        let refusal = requests.open_lift("sender", "a", 0).unwrap();
        requests.approve(refusal).unwrap();
        assert!(requests.take_grant_for_chat_start("a", 0, Some(refusal)));
        assert!(requests.dropped_wake("a"));
        // The wake's turn spent the carry, then failed to start: the approval is not lost.
        assert!(requests.take_dropped_wake("a"));
        assert!(!requests.dropped_wake("a"));
        requests.restore_dropped_wake("a");
        assert!(requests.dropped_wake("a"));
        // The restored carry still owns the refusal: no new lift opens until a reply spends it.
        assert!(requests.open_lift("sender", "a", 0).is_none());
        assert!(requests.take_dropped_wake("a"));
        assert!(!requests.dropped_wake("a"));
    }

    #[test]
    fn a_restored_carry_on_an_unknown_actor_is_a_no_op() {
        let requests = Requests::default();
        requests.restore_dropped_wake("nobody");
        assert!(!requests.dropped_wake("nobody"));
    }

    #[test]
    fn a_restore_skips_a_stale_generation_after_a_user_message() {
        let requests = Requests::default();
        let refusal = requests.open_lift("sender", "a", 0).unwrap();
        requests.approve(refusal).unwrap();
        assert!(requests.take_grant_for_chat_start("a", 0, Some(refusal)));
        // The wake's turn spent the carry, then failed to start. Before the failure is noticed, a user
        // message starts a new chain: the old approval is void.
        assert!(requests.take_dropped_wake("a"));
        requests.user_message_started("a");
        requests.restore_dropped_wake("a");
        assert!(!requests.dropped_wake("a"), "a stale restore must not hand the new chain a free reset");
        // The new chain earns its own notice.
        assert!(requests.open_lift("sender", "a", 1).is_some());
    }

    #[test]
    fn a_consumed_carry_blocks_a_new_lift_until_delivered_or_restored() {
        let requests = Requests::default();
        let refusal = requests.open_lift("sender", "a", 0).unwrap();
        requests.approve(refusal).unwrap();
        assert!(requests.take_grant_for_chat_start("a", 0, Some(refusal)));
        // The wake's turn consumed the carry: until its prompt is sent (or it fails and restores), a delayed
        // same-generation refusal must not open a new lift.
        assert!(requests.take_dropped_wake("a"));
        assert!(requests.open_lift("sender", "a", 0).is_none(), "the pending restore owns the refusal");
        // Failed start: the carry comes back, and the new lift is still refused.
        requests.restore_dropped_wake("a");
        assert!(requests.dropped_wake("a"));
        assert!(requests.open_lift("sender", "a", 0).is_none());
        // The wake's turn delivers its prompt instead: the reset is used and a new refusal may open.
        requests.take_dropped_wake("a");
        requests.wake_delivered("a");
        assert!(requests.open_lift("sender", "a", 0).is_some());
    }

    #[test]
    fn a_restore_applies_again_at_the_same_generation_after_a_second_failure() {
        let requests = Requests::default();
        let refusal = requests.open_lift("sender", "a", 0).unwrap();
        requests.approve(refusal).unwrap();
        assert!(requests.take_grant_for_chat_start("a", 0, Some(refusal)));
        assert!(requests.take_dropped_wake("a"));
        // The wake's turn failed twice (the second through a fallback retry): the first restore put the carry
        // back, the turn spent it again, and the second restore applies again at the same generation.
        requests.restore_dropped_wake("a");
        assert!(requests.dropped_wake("a"));
        assert!(requests.take_dropped_wake("a"));
        requests.restore_dropped_wake("a");
        assert!(requests.dropped_wake("a"));
        // The reply finally starts and delivers its prompt: the reset is used, nothing waits, and a new
        // refusal (another chain at the limit) may open its own lift again.
        assert!(requests.take_dropped_wake("a"));
        requests.wake_delivered("a");
        requests.restore_dropped_wake("a");
        assert!(!requests.dropped_wake("a"), "a delivered wake must not carry a reset");
        assert!(requests.open_lift("sender", "a", 0).is_some());
    }
}
