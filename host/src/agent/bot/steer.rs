//! The user's queued messages before their turn comes: handled now (`steer`) or taken back (`unqueue`).

use super::{Actor, Cmd, Done, Queued};
use crate::analytics::{self, Steer, TurnSource};
use crate::chat::team::RequestStatus;
use crate::chat::uploads;
use crate::store::Lane;
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::sync::mpsc;

/// How long the agent may take to say whether a steered message joined its turn.
const STEER_ANSWER: Duration = Duration::from_secs(15);

/// A queued message handed to the agent to join its running turn, until the agent answers.
pub(super) struct InFlight {
    /// The user took it back meanwhile (`unqueue`): it goes unless the agent took it.
    taken_back: bool,
}

/// What the agent's answer to a steer made of the message.
fn steer_outcome(answer: &Result<Value>) -> Steer {
    match answer {
        Ok(v) if v["outcome"] == "injected" => Steer::Joined,
        Ok(v) if v["outcome"] == "promptRequired" => Steer::Next,
        _ => Steer::Failed,
    }
}

impl Actor {
    /// Where the user's queued message `entry_id` waits.
    fn queued_at(&self, entry_id: &str) -> Option<usize> {
        self.queue.iter().position(|q| matches!(q, Queued::User { entry_id: id, .. } if id == entry_id))
    }

    /// The running turn answers the user's own messages in `lane`: not a routine's, a room's, another bot's or the
    /// host's, nor a compaction, nor one the user already stopped.
    fn answers_user_in(&self, lane: &Lane) -> bool {
        self.turn.is_some()
            && !self.stop_requested
            && self.lane == *lane
            && self.compacting.is_none()
            && self.source() == TurnSource::User
    }

    /// `steer`: the agent takes the message into its running turn when it can (`_session/steering`). Otherwise the
    /// user's turn stops for it, or, behind other work, it goes first in line.
    pub(super) async fn steer(&mut self, entry_id: String) {
        // Already with the agent.
        if self.steers_in_flight.contains_key(&entry_id) {
            return;
        }
        let Some(at) = self.queued_at(&entry_id) else { return };
        let Some(Queued::User { lane, text, .. }) = self.queue.get(at) else { return };
        let own = self.answers_user_in(lane);
        let text = text.clone();
        if own && self.hand_to_agent(entry_id, &text) {
            return;
        }
        self.move_first(at);
        let how = if own {
            self.cancel_turn().await;
            Steer::Interrupted
        } else {
            Steer::MovedUp
        };
        analytics::message_steered(&self.hub, &self.cfg.backend, how);
    }

    /// Hands the message to the agent to join its running turn (`steered` gets the answer); false when it can't steer.
    fn hand_to_agent(&mut self, entry_id: String, text: &str) -> bool {
        let (Some(conn), Some(sid)) = (self.conn.as_ref().filter(|c| c.sessions.steering), self.turn_session.clone())
        else {
            return false;
        };
        let (acp, hub, bot) = (conn.acp.clone(), self.hub.clone(), self.cfg.id.clone());
        let params = json!({
            "sessionId": sid,
            "prompt": [{"type": "text", "text": text}],
            // A turn that just ended doesn't take it: it stays queued for the next one.
            "_meta": {"steering": {"idleBehavior": "promptRequired"}},
        });
        // What the agent writes from here may answer it: a reply of its own, after the message in the chat.
        self.close_seg();
        self.steers_in_flight.insert(entry_id.clone(), InFlight { taken_back: false });
        tokio::spawn(async move {
            let answer = tokio::time::timeout(STEER_ANSWER, acp.request("_session/steering", params)).await;
            let outcome = answer.unwrap_or_else(|_| Err(anyhow!("the agent didn't answer the steer")));
            // Refused only once the bot was deleted meanwhile: no one is left to tell.
            let _ = hub.send_cmd(&bot, Cmd::Steered { entry_id, outcome });
        });
        true
    }

    /// Moves what waits at `at` first in line; a compaction that is due still goes before it.
    fn move_first(&mut self, at: usize) {
        let Some(item) = self.queue.remove(at) else { return };
        let first = self.queue.iter().position(|q| !matches!(q, Queued::Compact { .. })).unwrap_or(self.queue.len());
        self.queue.insert(first, item);
        self.renumber_waiting();
    }

    /// Hands the waiting messages' turn numbers out again in their new order: a batch runs as its last message's turn
    /// (`start_turn`), and the clients take the highest for the running one.
    fn renumber_waiting(&self) {
        let waiting: Vec<_> = self
            .queue
            .iter()
            .filter_map(|q| match q {
                Queued::User { entry_id, .. } => self.hub.store.entry(entry_id),
                _ => None,
            })
            .collect();
        let mut turns: Vec<i64> = waiting.iter().map(|e| e.turn).collect();
        turns.sort_unstable();
        for (e, turn) in waiting.iter().zip(turns) {
            if e.turn != turn {
                self.hub.set_entry_turn(&e.id, Some(turn), &e.data);
            }
        }
    }

    /// The agent ended the running turn. Its answer to a steer travels apart from this, and a message it took belongs
    /// to this turn, so the end waits for every answer (at most `STEER_ANSWER`).
    pub(super) async fn turn_ended(&mut self, done: Done, done_tx: &mpsc::UnboundedSender<Done>) {
        if self.steers_in_flight.is_empty() {
            self.on_turn_done(done, done_tx).await;
        } else {
            self.held_done = Some(done);
        }
    }

    /// The agent's answer to a steer; the last one lets a held turn end (`turn_ended`).
    pub(super) async fn steer_answered(
        &mut self,
        entry_id: &str,
        outcome: &Result<Value>,
        done_tx: &mpsc::UnboundedSender<Done>,
    ) {
        self.steered(entry_id, outcome);
        if self.steers_in_flight.is_empty()
            && let Some(done) = self.held_done.take()
        {
            self.on_turn_done(done, done_tx).await;
        }
    }

    /// A message the agent took is the running turn's, sent; one it left for later goes next; any other stays queued,
    /// unless the user took it back meanwhile.
    fn steered(&mut self, entry_id: &str, outcome: &Result<Value>) {
        let Some(steer) = self.steers_in_flight.remove(entry_id) else { return };
        if let Err(error) = outcome {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't steer; the message waits");
        }
        let how = steer_outcome(outcome);
        analytics::message_steered(&self.hub, &self.cfg.backend, how);
        // A Stop meanwhile dropped it from the queue (and marked it cancelled).
        let Some(at) = self.queued_at(entry_id) else { return };
        if how != Steer::Joined {
            if steer.taken_back {
                self.unqueue(entry_id);
            } else if how == Steer::Next {
                self.move_first(at);
            }
            return;
        }
        let Some(Queued::User { text, .. }) = self.queue.remove(at) else { return };
        // The host's copy, never `removed`, also brings it back to a client that took it back meanwhile.
        if let Some(mut e) = self.hub.store.entry(entry_id) {
            e.data["status"] = json!(RequestStatus::Sent);
            self.hub.set_entry_turn(entry_id, self.turn, &e.data);
        }
        self.record_joined_input(entry_id, &text);
    }

    /// A user input delivered inside this turn belongs in its reply, memory and fallback context.
    pub(super) fn record_joined_input(&mut self, entry_id: &str, text: &str) {
        self.joined = true;
        self.turn_entries.push(entry_id.to_owned());
        if let Some(turn_text) = &mut self.turn_text {
            turn_text.push_str("\n\n");
            turn_text.push_str(text);
        }
        self.relay_joined(entry_id, text);
        // `send --wait` learns a started message's turn from the bot's next event, and a status and activity that stay
        // the same send none.
        self.hub.emit_bot(&self.cfg.id);
    }

    /// `unqueue`: the message never goes out, the chat no longer shows it, and its files leave the bot's folder.
    pub(super) fn unqueue(&mut self, entry_id: &str) {
        // Already with the agent, which may take it: `steered` decides.
        if let Some(steer) = self.steers_in_flight.get_mut(entry_id) {
            steer.taken_back = true;
            return;
        }
        let Some(at) = self.queued_at(entry_id) else { return };
        self.queue.remove(at);
        if let Some(mut e) = self.hub.store.entry(entry_id) {
            e.data["status"] = json!(RequestStatus::Cancelled);
            e.data["removed"] = true.into();
            self.hub.set_entry(entry_id, &e.data);
            self.delete_uploads(&e.data);
            // The roster previews the chat's newest message.
            self.hub.emit_bot(&self.cfg.id);
        }
    }

    /// Deletes the files of a message (its saved `data`) that never went out.
    fn delete_uploads(&self, data: &Value) {
        let ids: Vec<String> = data["attachments"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| a["id"].as_str().map(str::to_owned))
            .collect();
        if ids.is_empty() {
            return;
        }
        let (root, bot) = (uploads::root(&self.cfg), self.cfg.id.clone());
        tokio::task::spawn_blocking(move || {
            for id in ids {
                if let Err(error) = uploads::remove(&root, &id) {
                    tracing::warn!(%bot, error = format!("{error:#}"), "couldn't delete a removed message's file");
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agents_answer_says_what_became_of_the_message() {
        assert_eq!(steer_outcome(&Ok(json!({"outcome": "injected"}))), Steer::Joined);
        assert_eq!(steer_outcome(&Ok(json!({"outcome": "promptRequired"}))), Steer::Next);
        assert_eq!(steer_outcome(&Ok(json!({"outcome": "somethingNew"}))), Steer::Failed);
        assert_eq!(steer_outcome(&Err(anyhow!("gone"))), Steer::Failed);
        assert_eq!(json!(Steer::MovedUp), "moved_up", "as the event reports it");
    }
}
