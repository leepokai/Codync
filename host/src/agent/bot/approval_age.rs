//! How long an approval card has waited: a reminder push after 4 hours and, at 24 hours, the card
//! closes as cancelled (the agent gets a cancel), so a forgotten request neither blocks its bot
//! forever nor stays answerable long after anyone remembers what it was.
//!
//! Idea from Coworkers' approval reminders; no code copied.

use super::Actor;
use crate::remote::push::{self, AlertKind};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const REMIND_AFTER: Duration = Duration::from_hours(4);
const CLOSE_AFTER: Duration = Duration::from_hours(24);

/// What `Waiting::due` found.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Due {
    pub remind: Vec<String>,
    pub close: Vec<String>,
}

/// When each open card was shown, and whether it was already reminded.
#[derive(Default)]
pub(super) struct Waiting {
    cards: HashMap<String, (Instant, bool)>,
}

impl Waiting {
    pub fn asked(&mut self, entry_id: &str, now: Instant) {
        self.cards.insert(entry_id.to_owned(), (now, false));
    }

    pub fn answered(&mut self, entry_id: &str) {
        self.cards.remove(entry_id);
    }

    /// Cards to remind (once each) and cards to close at `now`; closed ones are forgotten.
    pub fn due(&mut self, now: Instant) -> Due {
        let mut due = Due::default();
        for (id, (since, reminded)) in &mut self.cards {
            let waited = now.saturating_duration_since(*since);
            if waited >= CLOSE_AFTER {
                due.close.push(id.clone());
            } else if waited >= REMIND_AFTER && !*reminded {
                *reminded = true;
                due.remind.push(id.clone());
            }
        }
        for id in &due.close {
            self.cards.remove(id);
        }
        due
    }
}

/// Whether a card already sent its one reminder.
fn reminded(data: &Value) -> bool {
    data["reminded"] == true
}

/// What a phone needs to answer a card without opening it: its id and the options the card itself
/// shows (at most three), so a button can never offer something the card doesn't.
pub(super) fn approval_payload(entry_id: &str, data: &Value) -> Value {
    let options = data["options"]
        .as_array()
        .into_iter()
        .flatten()
        .take(3)
        .filter_map(|o| {
            Some(json!({
                "optionId": o["optionId"].as_str()?,
                "name": crate::agent::acp::truncate(o["name"].as_str()?, 60),
                "kind": o["kind"].as_str().unwrap_or(""),
            }))
        })
        .collect::<Vec<_>>();
    json!({"entryId": entry_id, "options": options})
}

impl Actor {
    /// Reminds and closes the cards that waited long enough; run on the actor's tick.
    pub(super) async fn age_approvals(&mut self) {
        if self.perms.is_empty() {
            return;
        }
        let due = self.waiting.due(Instant::now());
        for id in due.remind {
            self.remind_approval(&id);
        }
        for id in due.close {
            self.answer_permission(&id, None).await;
        }
    }

    fn remind_approval(&self, entry_id: &str) {
        let Some(entry) =
            self.hub.store.entry(entry_id).filter(|e| e.data["status"] == "pending" && !reminded(&e.data))
        else {
            return;
        };
        // The flag is in the card itself, so a second reminder can't follow, whoever asks.
        let mut data = entry.data.clone();
        data["reminded"] = true.into();
        self.hub.set_entry(entry_id, &data);
        let title = entry.data["title"].as_str().unwrap_or("A request");
        let target = self.hub.store.bot(&self.lane.chat).ok().flatten().map_or_else(|| self.cfg.clone(), |r| r.config);
        push::notify_with(
            &self.hub,
            &target,
            Some(&self.cfg.id),
            &format!("{} is still waiting for you", self.cfg.name),
            title,
            AlertKind::NeedsInput,
            Some(approval_payload(entry_id, &entry.data)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: Duration = Duration::from_secs(3600);

    #[test]
    fn a_card_is_reminded_once_at_four_hours_and_closed_at_twenty_four() {
        let start = Instant::now();
        let mut waiting = Waiting::default();
        waiting.asked("a", start);
        assert_eq!(waiting.due(start + 3 * HOUR), Due::default());
        assert_eq!(waiting.due(start + 4 * HOUR).remind, ["a"]);
        assert_eq!(waiting.due(start + 5 * HOUR), Due::default(), "reminded only once");
        assert_eq!(waiting.due(start + 24 * HOUR).close, ["a"]);
        assert_eq!(waiting.due(start + 25 * HOUR), Due::default(), "a closed card is forgotten");
    }

    #[test]
    fn an_answered_card_is_never_reminded() {
        let start = Instant::now();
        let mut waiting = Waiting::default();
        waiting.asked("a", start);
        waiting.answered("a");
        assert_eq!(waiting.due(start + 30 * HOUR), Due::default());
    }

    #[test]
    fn a_card_that_carries_the_flag_is_not_reminded_again() {
        assert!(!reminded(&json!({"status": "pending"})));
        assert!(reminded(&json!({"status": "pending", "reminded": true})));
    }

    #[test]
    fn the_payload_offers_only_the_cards_options() {
        let data = json!({"options": [
            {"optionId": "allow", "name": "Allow", "kind": "allow_once"},
            {"optionId": "x"},
            {"optionId": "deny", "name": "Deny", "kind": "reject_once"},
        ]});
        let payload = approval_payload("e1", &data);
        assert_eq!(payload["entryId"], "e1");
        assert_eq!(payload["options"].as_array().map(Vec::len), Some(2), "an option without a name is skipped");
    }
}
