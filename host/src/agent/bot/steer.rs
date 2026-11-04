//! The user's queued messages before their turn comes: handled now (`steer`) or taken back (`unqueue`).

use super::{Actor, Cmd, Queued};
use crate::analytics::{self, Steer, TurnSource};
use crate::chat::team::RequestStatus;
use crate::store::Lane;
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::time::Duration;

/// How long the agent may take to say whether a steered message joined its turn.
const STEER_ANSWER: Duration = Duration::from_secs(15);

/// A queued message handed to the agent to join its running turn, until the agent answers.
pub(super) struct InFlight {
    /// The turn it joins, even one that ends before the answer comes.
    turn: i64,
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
    /// host's, nor a compaction.
    fn answers_user_in(&self, lane: &Lane) -> bool {
        self.turn.is_some() && self.lane == *lane && self.compacting.is_none() && self.source() == TurnSource::User
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
        if own
            && let Some(turn) = self.turn
            && let Some(conn) = self.conn.as_ref().filter(|c| c.sessions.steering)
            && let Some(sid) = self.turn_session.clone()
        {
            let (acp, hub, bot) = (conn.acp.clone(), self.hub.clone(), self.cfg.id.clone());
            let params = json!({
                "sessionId": sid,
                "prompt": [{"type": "text", "text": text}],
                // A turn that just ended doesn't take it: it stays queued for the next one.
                "_meta": {"steering": {"idleBehavior": "promptRequired"}},
            });
            // What the agent writes from here may answer it: a reply of its own, after the message in the chat.
            self.close_seg();
            self.steers_in_flight.insert(entry_id.clone(), InFlight { turn, taken_back: false });
            tokio::spawn(async move {
                let answer = tokio::time::timeout(STEER_ANSWER, acp.request("_session/steering", params)).await;
                let outcome = answer.unwrap_or_else(|_| Err(anyhow!("the agent didn't answer the steer")));
                let _ = hub.send_cmd(&bot, Cmd::Steered { entry_id, outcome });
            });
            return;
        }
        let item = self.queue.remove(at).expect("found above");
        // A compaction that is due still goes first.
        let first = self.queue.iter().position(|q| !matches!(q, Queued::Compact { .. })).unwrap_or(self.queue.len());
        self.queue.insert(first, item);
        let how = if own {
            self.cancel_turn().await;
            Steer::Interrupted
        } else {
            Steer::MovedUp
        };
        analytics::message_steered(&self.hub, &self.cfg.backend, how);
    }

    /// The agent's answer to a steer: a message it took is that turn's, sent; any other stays queued, unless the
    /// user took it back meanwhile.
    pub(super) fn steered(&mut self, entry_id: &str, outcome: &Result<Value>) {
        let Some(steer) = self.steers_in_flight.remove(entry_id) else { return };
        if let Err(error) = outcome {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't steer; the message waits");
        }
        let how = steer_outcome(outcome);
        analytics::message_steered(&self.hub, &self.cfg.backend, how);
        if how != Steer::Joined {
            if steer.taken_back {
                self.unqueue(entry_id);
            }
            return;
        }
        // A Stop meanwhile dropped it from the queue (and marked it cancelled).
        let Some(at) = self.queued_at(entry_id) else { return };
        let Some(Queued::User { text, .. }) = self.queue.remove(at) else { return };
        // The host's copy, never `removed`, also brings it back to a client that took it back meanwhile.
        if let Some(mut e) = self.hub.store.entry(entry_id) {
            e.data["status"] = json!(RequestStatus::Sent);
            self.hub.set_entry_turn(entry_id, Some(steer.turn), &e.data);
        }
        if self.turn == Some(steer.turn) {
            self.turn_entries.push(entry_id.to_owned());
            if let Some(turn_text) = &mut self.turn_text {
                turn_text.push_str("\n\n");
                turn_text.push_str(&text);
            }
            self.relay_joined(entry_id, &text);
        }
    }

    /// `unqueue`: the message never goes out, and the chat no longer shows it.
    pub(super) fn unqueue(&mut self, entry_id: &str) {
        // Already with the agent, which may take it: `steered` decides.
        if let Some(steer) = self.steers_in_flight.get_mut(entry_id) {
            steer.taken_back = true;
            return;
        }
        let Some(at) = self.queued_at(entry_id) else { return };
        self.queue.remove(at);
        if let Some(mut e) = self.hub.store.entry(entry_id) {
            e.data["status"] = "cancelled".into();
            e.data["removed"] = true.into();
            self.hub.set_entry(entry_id, &e.data);
            // The roster previews the chat's newest message.
            self.hub.emit_bot(&self.cfg.id);
        }
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
