//! A turn that stopped half done (out of steps, or its answer cut at the output limit) leaves a card in the chat:
//! the person can continue it with one tap. A continuation is a short message of its own, and at most
//! [`MAX_CONTINUATIONS`] follow one another, so a task that never finishes cannot loop on its own.

use super::Actor;
use super::verdict::TurnVerdict;
use crate::store::EntryKind;
use anyhow::{Result, bail};
use serde_json::{Value, json};

/// Continuations in a row before the card asks the person to say what remains.
pub const MAX_CONTINUATIONS: u32 = 2;
/// What a continuation says. It is visible in the chat as the person's own message.
pub const CONTINUE_TEXT: &str = "Continue where you left off. Finish what remains, then stop.";

/// The card for a turn that ended with `verdict`, after `round` continuations in a row.
pub fn card(verdict: TurnVerdict, round: u32) -> Value {
    let (text, label) = match verdict {
        TurnVerdict::Truncated => ("The agent's answer was cut at its output limit.", "truncated"),
        _ => ("The agent used all its steps for this turn before it was done.", "paused"),
    };
    json!({
        "text": text,
        "style": "info",
        "pausedTurn": {
            "verdict": label,
            "status": "waiting",
            "round": round,
            "canContinue": round < MAX_CONTINUATIONS,
        }
    })
}

/// Checks that `data` is a card that may be continued now, and returns the card's data once it is used.
pub fn begin(data: &Value) -> Result<Value> {
    let paused = &data["pausedTurn"];
    if !paused.is_object() {
        bail!("That isn't a paused turn");
    }
    if paused["status"] != "waiting" {
        bail!("This turn was already continued or closed");
    }
    if paused["canContinue"] != true {
        bail!("Say what remains instead: this turn was continued {MAX_CONTINUATIONS} times already");
    }
    let mut used = data.clone();
    used["pausedTurn"]["status"] = "continued".into();
    Ok(used)
}

/// A user message the person sent from a pause card.
pub fn is_continuation(data: &Value) -> bool {
    data["from"] == "continue"
}

impl Actor {
    /// Posts the card for a turn that ended `Paused` or `Truncated`. A scheduled check-in that stops short leaves none:
    /// it is private until it has something to say.
    pub(super) fn pause_card(&self, verdict: TurnVerdict) {
        if self.proactive_turn {
            return;
        }
        let data = card(verdict, self.continuation);
        self.hub.add_entry(&self.lane, EntryKind::Notice, self.turn.unwrap_or(0), &data);
    }

    /// Whether every message this turn answers came from a pause card.
    pub(super) fn continue_batch(&self, ids: &[String]) -> bool {
        !ids.is_empty() && ids.iter().all(|id| self.hub.store.entry(id).is_some_and(|e| is_continuation(&e.data)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_can_be_continued_once_and_a_limited_number_of_times() {
        let first = card(TurnVerdict::Paused, 0);
        assert_eq!(first["pausedTurn"]["canContinue"], true);
        let used = begin(&first).expect("a waiting card");
        assert_eq!(used["pausedTurn"]["status"], "continued");
        assert!(begin(&used).is_err(), "a second tap on the same card is refused");
        let last = card(TurnVerdict::Paused, MAX_CONTINUATIONS);
        assert_eq!(last["pausedTurn"]["canContinue"], false);
        assert!(begin(&last).is_err(), "the chain stops after {MAX_CONTINUATIONS} continuations");
        assert!(begin(&json!({"text": "hi"})).is_err(), "not a paused turn");
    }

    #[test]
    fn a_cut_answer_and_a_pause_say_so() {
        assert_eq!(card(TurnVerdict::Truncated, 0)["pausedTurn"]["verdict"], "truncated");
        assert_eq!(card(TurnVerdict::Paused, 0)["pausedTurn"]["verdict"], "paused");
        assert!(is_continuation(&json!({"from": "continue"})) && !is_continuation(&json!({"from": "proactive"})));
    }
}
