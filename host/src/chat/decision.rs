//! Choice cards are additive notice entries. Older clients display their text and can answer through normal send.
//! The actor owns the live waiter, validates every answer, and records it without scheduling a second turn.

use crate::agent::bot::Cmd;
use crate::hub::Hub;
use crate::store::Entry;
use anyhow::{Result, anyhow, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fmt::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const TOOL: &str = "request_decision";
pub(crate) const WAIT: Duration = Duration::from_secs(270);
const MAX_OPTIONS: usize = 12;

pub const INSTRUCTIONS: &str = "When the user should pick among a few ways forward, use request_decision(question, \
options, recommended?) instead of repeating the question in a message. One card waits at a time, for up to four and a \
half minutes. The user's button or typed choice answers this tool and is recorded in the chat; it does not start a \
second turn. An expired card takes no answer. Do not choose the suggested option on the user's behalf.";

/// A choice card's lifecycle, stored lowercase in `data.status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Answered,
    Expired,
}

impl Status {
    /// A card's status from its data; `None` for a value this host doesn't know.
    pub fn of(data: &Value) -> Option<Self> {
        Self::deserialize(&data["status"]).ok()
    }
}

pub fn tool() -> Value {
    json!({
        "name": TOOL,
        "description": "Ask the user to choose on a card in your own chat. Their choice returns to this call and is recorded as their reply. Ask one question at a time; do not repeat it in a message. Older clients can reply with the full option or its number.",
        "inputSchema": {"type": "object", "properties": {
            "question": {"type": "string", "minLength": 1, "maxLength": 2000},
            "options": {"type": "array", "minItems": 2, "maxItems": 12,
                "items": {"type": "string", "minLength": 1, "maxLength": 200}},
            "recommended": {"type": "string", "description": "Optional suggested option, exactly as written in options. Never selected automatically."}
        }, "required": ["question", "options"]},
        "annotations": {"readOnlyHint": false, "idempotentHint": false}
    })
}

fn card_data(args: &Value) -> Result<Value> {
    let question = args["question"].as_str().unwrap_or_default().trim();
    ensure!(!question.is_empty() && question.chars().count() <= 2000, "question must have 1 to 2000 characters");
    let raw = args["options"].as_array().ok_or_else(|| anyhow!("options must be an array of strings"))?;
    ensure!((2..=MAX_OPTIONS).contains(&raw.len()), "options must hold 2 to 12 different choices");
    let mut choices = Vec::new();
    for value in raw {
        let choice = value.as_str().ok_or_else(|| anyhow!("each option must be a string"))?.trim();
        ensure!(!choice.is_empty() && choice.chars().count() <= 200, "each option must have 1 to 200 characters");
        ensure!(
            !choices.iter().any(|old: &String| old.to_lowercase() == choice.to_lowercase()),
            "options must be different"
        );
        choices.push(choice.to_owned());
    }
    let recommended = match args.get("recommended").filter(|v| !v.is_null()) {
        Some(value) => Some(
            choices
                .iter()
                .position(|c| Some(c.as_str()) == value.as_str())
                .ok_or_else(|| anyhow!("recommended must exactly name an option"))?,
        ),
        None => None,
    };
    let mut text = question.to_owned();
    for (i, choice) in choices.iter().enumerate() {
        let _ = write!(text, "\n{}. {choice}", i + 1);
    }
    if let Some(i) = recommended {
        let _ = write!(text, "\n\nSuggested: {}", choices[i]);
    }
    text.push_str("\n\nReply with the full option or its number. Exact option text takes priority over a number.");
    Ok(json!({"decision": true, "text": text, "question": question, "choices": choices,
        "recommended": recommended, "status": Status::Pending, "selected": Value::Null}))
}

pub fn is_card(card: &Entry) -> bool {
    card.kind == "notice" && card.data["decision"] == true
}

pub fn choices(card: &Entry) -> Vec<String> {
    card.data["choices"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect()
}

/// Exact text wins, including numeric labels. Only typed replies may also select a one-based position.
pub fn pick(choices: &[String], wanted: &str, numbered: bool) -> Option<String> {
    let wanted = wanted.trim();
    if let Some(choice) = choices.iter().find(|c| c.to_lowercase() == wanted.to_lowercase()) {
        return Some(choice.clone());
    }
    if numbered {
        let i = wanted.parse::<usize>().ok()?.checked_sub(1)?;
        return choices.get(i).cloned();
    }
    None
}

pub async fn ask(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value> {
    let cfg = hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?.config;
    ensure!(!cfg.is_group(), "request_decision works only in a bot's own chat");
    let rt = hub.runtime(&cfg.id);
    ensure!(rt.lane.is_some_and(|lane| lane.chat == cfg.id), "request_decision works only in your own chat");
    let started_at = rt.started_at.ok_or_else(|| anyhow!("no running turn is waiting"))?;
    let (reply, answered) = tokio::sync::oneshot::channel();
    // The actor reads the same deadline, so a call dropped by this timeout is reported as one.
    let deadline = Instant::now() + WAIT;
    hub.send_cmd(&cfg.id, Cmd::Ask { data: card_data(args)?, started_at, deadline, reply })?;
    tokio::time::timeout_at(deadline.into(), answered)
        .await
        .map_err(|_| anyhow!("the card expired after four and a half minutes without an answer"))?
        .map_err(|_| anyhow!("the turn is no longer waiting for this choice"))?
}

/// An API success means the actor delivered this answer to its live tool waiter.
pub async fn respond(hub: &Arc<Hub>, entry_id: &str, option: &str) -> Result<Value> {
    let card = hub.store.entry(entry_id).ok_or_else(|| anyhow!("unknown card"))?;
    ensure!(is_card(&card), "that entry is not a decision card");
    if Status::of(&card.data) != Some(Status::Pending) {
        bail!("that card is already answered or expired");
    }
    let owner = card.data["author"].as_str().unwrap_or(&card.bot_id);
    let (reply, received) = tokio::sync::oneshot::channel();
    hub.send_cmd(owner, Cmd::Decision { entry_id: entry_id.to_owned(), choice: option.to_owned(), reply })?;
    tokio::time::timeout(Duration::from_secs(10), received)
        .await
        .map_err(|_| anyhow!("the bot did not acknowledge the choice"))?
        .map_err(|_| anyhow!("the bot stopped before acknowledging the choice"))?
}

/// Why a choice card stopped waiting; the `reason` of `decision_expired` (never the question or its options).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Expiry {
    /// Four and a half minutes passed without an answer.
    Timeout,
    /// The agent cancelled its tool call.
    Cancelled,
    /// The user stopped the turn.
    Stopped,
    /// The turn ended with the card unanswered.
    TurnEnded,
    /// The user sent a message that isn't one of the options.
    OtherMessage,
    /// The host restarted while the card waited.
    Restart,
}

impl Expiry {
    /// What the waiting tool call is told.
    pub fn text(self) -> &'static str {
        match self {
            Self::Timeout => "the card expired after four and a half minutes without an answer",
            Self::Cancelled => "the call was cancelled",
            Self::Stopped => "the user stopped the turn",
            Self::TurnEnded => "the turn ended",
            Self::OtherMessage => "the user sent a different message",
            Self::Restart => "the host restarted",
        }
    }

    /// Records the expiry, content-free.
    pub fn capture(self, hub: &Hub) {
        crate::analytics::capture(
            hub,
            crate::analytics::Event::DecisionExpired,
            serde_json::Map::from_iter([("reason".into(), json!(self))]),
        );
    }
}

pub fn expire(hub: &Hub, entry_id: &str) {
    let Some(mut card) = hub.store.entry(entry_id).filter(is_card) else { return };
    if Status::of(&card.data) == Some(Status::Pending) {
        mark_expired(&mut card.data, "the bot moved on");
        hub.set_entry(entry_id, &card.data);
    }
}

/// Marks a card's data answered with `choice`, also in the text older clients show.
pub fn mark_answered(data: &mut Value, choice: &str) {
    data["status"] = json!(Status::Answered);
    data["selected"] = choice.into();
    append_text(data, &format!("Answered: {choice}"));
}

/// Marks a card's data expired, saying why in the text older clients show.
pub fn mark_expired(data: &mut Value, why: &str) {
    data["status"] = json!(Status::Expired);
    append_text(data, &format!("No answer — {why}."));
}

fn append_text(data: &mut Value, line: &str) {
    let text = data["text"].as_str().unwrap_or_default();
    data["text"] = format!("{text}\n\n{line}").into();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_clients_get_notice_text_without_retyping_permission_options() {
        let card = card_data(&json!({"question":"Ship?","options":["Ship","Wait"],"recommended":"Wait"})).unwrap();
        assert_eq!(card["choices"], json!(["Ship", "Wait"]));
        assert!(card.get("options").is_none(), "old phone decoders expect object options");
        assert_eq!(card["recommended"], 1);
        assert!(card["text"].as_str().unwrap().contains("2. Wait"));
        assert_eq!(card["status"], "pending", "the wire value stays lowercase");
    }

    #[test]
    fn numeric_labels_win_and_invalid_numbers_never_choose() {
        let choices = vec!["2".into(), "4".into()];
        assert_eq!(pick(&choices, "2", true), Some("2".into()));
        assert_eq!(pick(&choices, "4", false), Some("4".into()));
        assert_eq!(pick(&choices, "1", true), Some("2".into()));
        for value in ["0", "3", "-1", "", "a"] {
            assert_eq!(pick(&choices, value, true), None);
        }
        assert_eq!(pick(&choices, "1", false), None, "button API accepts labels only");
    }

    #[test]
    fn typed_choices_do_not_guess_partial_text() {
        let choices = vec!["Ship now".into(), "Ship later".into()];
        assert_eq!(pick(&choices, "ship", true), None);
        assert_eq!(pick(&choices, "SHIP NOW", true), Some("Ship now".into()));
        assert_eq!(pick(&choices, "2", true), Some("Ship later".into()));
    }

    #[test]
    fn invalid_options_and_recommendations_are_not_silently_changed() {
        for options in [json!(["a"]), json!(["a", "A"]), json!(["a", 4, "b"]), json!(["a", "", "b"]), json!([])] {
            assert!(card_data(&json!({"question":"Q", "options":options})).is_err());
        }
        assert!(card_data(&json!({"question":"Q", "options":["a","b"],"recommended":"c"})).is_err());
        assert!(card_data(&json!({"question":"Q", "options":["a", "b".repeat(201)]})).is_err());
        let options: Vec<_> = (1..=12).map(|i| i.to_string()).collect();
        assert!(card_data(&json!({"question":"Q","options":options})).is_ok());
    }
}
