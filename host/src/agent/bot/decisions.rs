//! Choice waiters belong to the actor, so a click, typed answer, cancellation and turn end cannot answer twice.

use super::Actor;
use crate::agent::acp;
use crate::analytics::{self, Event};
use crate::chat::decision::{self, Expiry, Via};
use crate::hub::BotStatus;
use crate::remote::push::{self, AlertKind};
use crate::store::{Entry, EntryKind, Lane};
use anyhow::{Result, anyhow, ensure};
use serde_json::{Map, Value, json};
use std::time::Instant;
use tokio::sync::oneshot;

pub(super) struct Asked {
    reply: oneshot::Sender<Result<Value>>,
    deadline: Instant,
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
        if self.turn.is_none()
            || self.stop_requested
            || self.active_request.is_some()
            || self.active_group.is_some()
            || self.active_routine.is_some()
            || self.active_prompt
            || self.lane.chat != self.cfg.id
            || self.hub.runtime(&self.cfg.id).started_at != Some(started_at)
        {
            let _ = reply.send(Err(anyhow!("request_decision works only in the current turn of your own chat")));
            return;
        }
        if !self.asked.is_empty() {
            let _ = reply.send(Err(anyhow!("a decision is already waiting: ask one question at a time")));
            return;
        }
        self.close_seg();
        let Some(card) = self.add(EntryKind::Notice, self.turn.unwrap_or(0), data) else {
            let _ = reply.send(Err(anyhow!("couldn't save the card")));
            return;
        };
        let question = card.data["question"].as_str().unwrap_or_default();
        self.hub.set_runtime(&self.cfg.id, |r| {
            r.status = BotStatus::NeedsInput;
            r.activity = format!("Asks: {question}");
        });
        push::notify(&self.hub, &self.cfg, None, &self.cfg.name, &acp::truncate(question, 120), AlertKind::NeedsInput);
        analytics::capture(
            &self.hub,
            Event::DecisionAsked,
            Map::from_iter([("options".into(), decision::choices(&card).len().into())]),
        );
        self.asked.insert(card.id, Asked { reply, deadline });
    }

    pub(super) fn expire_abandoned_decisions(&mut self) {
        let gone: Vec<_> = self.asked.iter().filter(|(_, a)| a.reply.is_closed()).map(|(id, _)| id.clone()).collect();
        if gone.is_empty() {
            return;
        }
        for id in gone {
            let Some(asked) = self.asked.remove(&id) else { continue };
            decision::expire(&self.hub, &id);
            // The call's own timeout drops it at its deadline; anything earlier is the agent cancelling.
            let why = if Instant::now() >= asked.deadline { Expiry::Timeout } else { Expiry::Cancelled };
            why.capture(&self.hub);
        }
        self.continue_after_decision();
    }

    pub(super) fn expire_decisions(&mut self, why: Expiry) {
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

    /// Returns the tool's acknowledged answer. It never queues `Cmd::Send`.
    pub(super) fn decided(&mut self, id: &str, wanted: &str, typed: Option<&Entry>) -> Result<Value> {
        self.expire_abandoned_decisions();
        let mut card =
            self.hub.store.entry(id).filter(decision::is_card).ok_or_else(|| anyhow!("unknown decision card"))?;
        if !self.asked.contains_key(id) || self.stop_requested || self.turn != Some(card.turn) {
            decision::expire(&self.hub, id);
            return Err(anyhow!("that card expired: no live call is waiting"));
        }
        ensure!(
            decision::Status::of(&card.data) == Some(decision::Status::Pending),
            "that card is already answered or expired"
        );
        let (choice, via) = decision::answer(&decision::choices(&card), wanted, typed.is_some())
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
        let user = match typed {
            Some(e) => self.hub.set_entry_turn(&e.id, Some(card.turn), &data),
            None => self.hub.add_entry(&self.lane, EntryKind::User, card.turn, &data),
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
        analytics::capture(&self.hub, Event::DecisionAnswered, Map::from_iter([("via".into(), json!(via))]));
        Ok(answer)
    }

    /// A plain typed message answers the waiting card: an exact choice, its number, or the user's own words.
    /// Files, quotes, `/commands` and call speech that names no option are ordinary messages and expire it.
    pub(super) fn try_decision_message(&mut self, lane: &Lane, entry_id: &str) -> bool {
        self.expire_abandoned_decisions();
        if self.asked.len() != 1 || lane.chat != self.cfg.id {
            return false;
        }
        let Some(entry) = self.hub.store.entry(entry_id) else { return false };
        // Host notes and voice relays are not the user's typed answer; a host note leaves the card waiting.
        match entry.data["from"].as_str() {
            Some("host") => return false,
            Some(_) => {
                self.expire_decisions(Expiry::OtherMessage);
                return false;
            }
            None => {}
        }
        let id = self.asked.keys().next().cloned().unwrap_or_default();
        let Some(card) = self.hub.store.entry(&id) else { return false };
        let text = entry.data["text"].as_str().unwrap_or_default();
        let plain = entry.data["attachments"].as_array().is_none_or(Vec::is_empty) && entry.data["replyTo"].is_null();
        // Speech from a call may name an option, but only typed words are the user's own answer.
        let answers = decision::answer(&decision::choices(&card), text, true)
            .is_some_and(|(_, via)| via != Via::Custom || entry.data["call"].is_null());
        if plain && answers {
            match self.decided(&id, text, Some(&entry)) {
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
}
