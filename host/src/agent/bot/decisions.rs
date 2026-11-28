//! Choice waiters belong to the actor, so a click, typed answer, cancellation and turn end cannot answer twice.

use super::Actor;
use crate::analytics::{self, Event};
use crate::chat::decision::{self, Asker, Expiry, Via};
use crate::chat::replies;
use crate::chat::work::external::StartedBy;
use crate::hub::BotStatus;
use crate::remote::push::{self, AlertKind};
use crate::store::{Entry, EntryKind, Lane};
use anyhow::{Result, anyhow, ensure};
use codync_agent::acp;
use serde_json::{Map, Value, json};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

/// How near its deadline a chat's call counts as run out: a press then can't race the call's own timer.
const RUN_OUT: Duration = Duration::from_secs(1);

pub(super) struct Asked {
    reply: oneshot::Sender<Result<Value>>,
    /// When the asking call stops waiting; none for the agent's own question (`elicitations`), which waits until it is
    /// answered or its turn ends.
    deadline: Option<Instant>,
    asker: Asker,
}

impl Actor {
    pub(super) fn ask(
        &mut self,
        data: Value,
        started_at: i64,
        deadline: Instant,
        reply: oneshot::Sender<Result<Value>>,
    ) {
        self.expire_abandoned_decisions();
        if reply.is_closed() {
            return;
        }
        if !self.can_ask() || self.hub.runtime(&self.cfg.id).started_at != Some(started_at) {
            let _ = reply.send(Err(anyhow!("request_decision works only in the current turn of your own chat")));
            return;
        }
        if !self.asked.is_empty() {
            let _ = reply.send(Err(anyhow!("a decision is already waiting: ask one question at a time")));
            return;
        }
        self.post_decision(data, Some(deadline), Asker::Tool, reply);
    }

    /// Whether the running turn may put a choice card in front of the user: one of the bot's own chat (a task's
    /// included), not stopped and not started by the host. Room turns, routines and bot requests ask in a message.
    pub(super) fn can_ask(&self) -> bool {
        self.turn.is_some()
            && !self.stop_requested
            && self.in_own_chat()
            && self.host_turn.is_none()
            && self.lane.chat == self.cfg.id
    }

    /// Shows a card and waits for its answer on `reply`, telling the user it needs them.
    pub(super) fn post_decision(
        &mut self,
        data: Value,
        deadline: Option<Instant>,
        asker: Asker,
        reply: oneshot::Sender<Result<Value>>,
    ) {
        if let Some(open) = self.open_card.take() {
            decision::expire(&self.hub, &open);
            Expiry::AskedAgain.capture(&self.hub);
        }
        self.close_seg();
        // A thread's card is answered by a press alone: what the user types in the chat is for the bot there.
        let data = if self.cfg.is_thread() { decision::pressed_only(data) } else { data };
        let Some(card) = self.add_for_user(EntryKind::Notice, data) else {
            let _ = reply.send(Err(anyhow!("couldn't save the card")));
            return;
        };
        let question = card.data["question"].as_str().unwrap_or_default();
        self.hub.set_runtime(&self.cfg.id, |r| {
            r.status = BotStatus::NeedsInput;
            r.activity = format!("Asks: {question}");
        });
        // Tapping it opens where the card is: its bot's chat, for a thread.
        let chat = self.chat_of(&card);
        push::notify(&self.hub, &chat, None, &chat.name, &acp::truncate(question, 120), AlertKind::NeedsInput, None);
        analytics::capture(
            &self.hub,
            Event::DecisionAsked,
            Map::from_iter([("options".into(), decision::choices(&card).len().into()), ("asker".into(), json!(asker))]),
        );
        self.asked.insert(card.id, Asked { reply, deadline, asker });
    }

    pub(super) fn expire_abandoned_decisions(&mut self) {
        let now = Instant::now();
        let thread = self.cfg.is_thread();
        let gone: Vec<_> = self
            .asked
            .iter()
            .filter(|(_, a)| a.reply.is_closed() || (!thread && a.deadline.is_some_and(|d| now + RUN_OUT >= d)))
            .map(|(id, _)| id.clone())
            .collect();
        if gone.is_empty() {
            return;
        }
        for id in gone {
            let Some(asked) = self.asked.remove(&id) else { continue };
            // The call's own timeout drops it at its deadline; anything earlier is the agent cancelling.
            let timed_out = asked.deadline.is_some_and(|d| now >= d);
            // A chat's card outlives its call, which is told so (here, or by its own timeout at the deadline); a
            // thread's ends with it (a press there has no next message to become), and so does the agent's own question.
            if !thread && asked.asker == Asker::Tool && (timed_out || !asked.reply.is_closed()) {
                let _ = asked.reply.send(Ok(decision::left_open()));
                self.open_card = Some(id);
                continue;
            }
            decision::expire(&self.hub, &id);
            (if timed_out { Expiry::Timeout } else { Expiry::Cancelled }).capture(&self.hub);
        }
        self.continue_after_decision();
    }

    pub(super) fn expire_decisions(&mut self, why: Expiry) {
        // A call that already ran out leaves its card open rather than expired by this.
        self.expire_abandoned_decisions();
        for (id, asked) in std::mem::take(&mut self.asked) {
            decision::expire(&self.hub, &id);
            let _ = asked.reply.send(Err(anyhow!("the choice expired: {}", why.text())));
            why.capture(&self.hub);
        }
        self.continue_after_decision();
    }

    fn continue_after_decision(&self) {
        if self.asked.is_empty() && self.perms.is_empty() && self.turn.is_some() && !self.stop_requested {
            self.hub.set_runtime(&self.cfg.id, |r| {
                if r.status == BotStatus::NeedsInput {
                    r.status = BotStatus::Working;
                    r.activity = "Continuing…".into();
                }
            });
        }
    }

    /// Returns the tool's acknowledged answer. It never queues `Cmd::Send`. `by`: the outside app that picked it.
    pub(super) fn decided(
        &mut self,
        id: &str,
        wanted: &str,
        typed: Option<&Entry>,
        by: Option<&StartedBy>,
    ) -> Result<Value> {
        self.expire_abandoned_decisions();
        let mut card = decision::pending_card(&self.hub.store, id)?;
        // A thread's card is in its bot's chat, numbered with that chat's turns.
        let this_turn = self.cfg.is_thread() || self.turn == Some(card.turn);
        if !self.asked.contains_key(id) || self.stop_requested || !this_turn {
            decision::expire(&self.hub, id);
            return Err(anyhow!("that card expired: no live call is waiting"));
        }
        let (choice, via) = decision::answer_card(&card, wanted, typed.is_some())
            .ok_or_else(|| anyhow!("no such option on that card"))?;
        let mut answer = json!({"choice": choice});
        if via == Via::Custom {
            answer["custom"] = true.into();
        }
        // Save one user entry in this same turn; typed input already has an entry, so update it instead.
        let mut data = typed.map_or_else(|| json!({"text": choice}), |e| e.data.clone());
        if data["from"].is_null() {
            data["from"] = "decision".into();
        }
        data["decisionId"] = id.into();
        data["decisionChoice"] = choice.clone().into();
        data["status"] = "sent".into();
        if let Some(by) = by {
            by.mark(&mut data, &choice);
        }
        let user = match typed {
            Some(e) => self.hub.set_entry_turn(&e.id, Some(card.turn), &data),
            None => self.hub.add_entry(&Lane { chat: card.bot_id.clone() }, EntryKind::User, card.turn, &data),
        }
        .ok_or_else(|| anyhow!("couldn't save the user's choice"))?;
        let asked = self.asked.remove(id).ok_or_else(|| anyhow!("the card stopped waiting"))?;
        if asked.reply.send(Ok(answer.clone())).is_err() {
            decision::expire(&self.hub, id);
            data["status"] = "failed".into();
            self.hub.set_entry(&user.id, &data);
            analytics::capture(&self.hub, Event::DecisionFailed, Map::new());
            self.continue_after_decision();
            return Err(anyhow!("the call stopped waiting before receiving the choice"));
        }
        // The tool delivered the label even when the old client typed a number. Carry both without losing either.
        let input = match typed {
            Some(entry) if via == Via::Text => {
                format!("{}\n[Selected: {choice}]", entry.data["text"].as_str().unwrap_or_default())
            }
            _ => choice.clone(),
        };
        self.record_joined_input(&user.id, &input);
        decision::mark_answered(&mut card.data, &choice);
        let saved = self.hub.set_entry(id, &card.data);
        self.continue_after_decision();
        ensure!(saved.is_some(), "the choice was delivered but the card could not be saved; do not repeat the action");
        via.capture(&self.hub);
        Ok(answer)
    }

    /// A plain typed message answers the waiting card: an exact choice, its number, or the user's own words.
    /// Files, quotes, `/commands` and call speech that names no option are ordinary messages and expire it.
    /// The message also closes the open card (`close_open_card`), which may change the `text` it goes to the bot with.
    pub(super) fn try_decision_message(&mut self, lane: &Lane, entry_id: &str, text: &mut String) -> bool {
        self.expire_abandoned_decisions();
        if lane.chat != self.cfg.id {
            return false;
        }
        let Some(entry) = self.hub.store.entry(entry_id) else { return false };
        // Host notes and voice relays are not the user's typed answer; a host note leaves a card waiting.
        if entry.data["from"] == "host" {
            return false;
        }
        if let Some(prompt) = self.close_open_card(&entry) {
            *text = prompt;
        }
        if self.asked.len() != 1 {
            return false;
        }
        if entry.data["from"].is_string() {
            self.expire_decisions(Expiry::OtherMessage);
            return false;
        }
        let id = self.asked.keys().next().cloned().unwrap_or_default();
        let Some(card) = self.hub.store.entry(&id) else { return false };
        if typed_answer(&card, &entry).is_some() {
            let text = entry.data["text"].as_str().unwrap_or_default();
            match self.decided(&id, text, Some(&entry), None) {
                Ok(_) => return true,
                Err(error) => {
                    tracing::warn!(%error, "typed choice was not acknowledged");
                    // Never send it again after delivery or a failed handoff.
                    return self.hub.store.entry(entry_id).is_some_and(|e| e.data["from"] == "decision");
                }
            }
        }
        self.expire_decisions(Expiry::OtherMessage);
        false
    }

    /// A press on the open card: answered, and the choice goes to the bot as the user's next message, quoting it.
    pub(super) fn answer_open_card(&mut self, id: &str, wanted: &str, by: Option<&StartedBy>) -> Result<Value> {
        let card = decision::pending_card(&self.hub.store, id)?;
        let (choice, _) =
            decision::answer_card(&card, wanted, false).ok_or_else(|| anyhow!("no such option on that card"))?;
        self.open_card = None;
        let mut data = json!({"text": choice, "from": "decision", "decisionId": id, "decisionChoice": choice,
            "replyTo": self.quote_card(&card)});
        if let Some(by) = by {
            by.mark(&mut data, &choice);
        }
        self.answered_late(card, &choice)?;
        crate::chat::messages::post(&self.hub, &self.cfg, data)?;
        Ok(json!({"choice": choice}))
    }

    /// The user's message after the open card's call stopped waiting closes it. When it's their answer the card is
    /// answered and the message quotes its question: the prompt it then goes to the bot with comes back. Anything else
    /// expires it.
    fn close_open_card(&mut self, entry: &Entry) -> Option<String> {
        let id = self.open_card.take()?;
        let card = decision::pending_card(&self.hub.store, &id).ok()?;
        let Some((choice, via)) = typed_answer(&card, entry).filter(|_| entry.data["from"].is_null()) else {
            decision::expire(&self.hub, &id);
            Expiry::OtherMessage.capture(&self.hub);
            return None;
        };
        let mut data = entry.data.clone();
        data["decisionId"] = id.into();
        data["decisionChoice"] = choice.clone().into();
        data["replyTo"] = self.quote_card(&card);
        let entry = self.hub.set_entry(&entry.id, &data)?;
        if let Err(error) = self.answered_late(card, &choice) {
            tracing::warn!(error = format!("{error:#}"), "late typed choice was not saved on its card");
        }
        let prompt = crate::chat::prompt::of_entry(&self.hub, &self.cfg, &entry);
        // The bot gets the option even when the user typed its number.
        Some(if via == Via::Text { format!("{prompt}\n[Selected: {choice}]") } else { prompt })
    }

    /// The open card's question, as the late answer's `replyTo`.
    fn quote_card(&self, card: &Entry) -> Value {
        replies::quote_of(&card.id, &self.cfg.id, card.data["question"].as_str().unwrap_or_default())
    }

    fn answered_late(&self, mut card: Entry, choice: &str) -> Result<()> {
        decision::mark_answered(&mut card.data, choice);
        ensure!(self.hub.set_entry(&card.id, &card.data).is_some(), "couldn't save the card");
        Via::Late.capture(&self.hub);
        Ok(())
    }
}

/// What a typed message answers on `card`: plain text naming an option or its number, or the user's own words.
/// Speech from a call may name an option, but only typed words are the user's own answer.
fn typed_answer(card: &Entry, entry: &Entry) -> Option<(String, Via)> {
    let plain = entry.data["attachments"].as_array().is_none_or(Vec::is_empty) && entry.data["replyTo"].is_null();
    let text = entry.data["text"].as_str().unwrap_or_default();
    decision::answer_card(card, text, true)
        .filter(|(_, via)| plain && (*via != Via::Custom || entry.data["call"].is_null()))
}
