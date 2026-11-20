//! Choice cards are additive notice entries. Older clients display their text and can answer through normal send.
//! The actor owns the live waiter, validates every answer, and records it without scheduling a second turn.

use crate::agent::bot::Cmd;
use crate::hub::Hub;
use crate::store::{Entry, Store};
use anyhow::{Result, anyhow, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fmt::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const TOOL: &str = "request_decision";
pub(crate) const WAIT: Duration = Duration::from_secs(270);
const MAX_OPTIONS: usize = 12;
/// The longest question a card asks, in characters.
pub(crate) const MAX_QUESTION: usize = 2000;

pub const INSTRUCTIONS: &str = "When the user should pick among a few ways forward, use request_decision(question, \
options, recommended?) instead of repeating the question in a message. One card waits at a time. The user's \
button, typed choice or own words answer this tool (their own words come back with custom: true) and are recorded in \
the chat; it does not start a second turn. Without an answer in four and a half minutes the call returns \
answered: false and, in your own chat, the card stays open: the user's later choice comes as their next message. Do \
not choose the suggested option on the user's behalf.";

/// What a chat's call whose wait ran out is told; its card stays open (`agent::bot::decisions`).
const LEFT_OPEN: &str = "No answer yet. The card stays open: when the user chooses, their choice comes as their next \
message, quoting this question. Don't ask it again or choose for them; go on with what doesn't depend on it, or end \
your turn.";

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
        "description": "Ask the user to choose on a card in your own chat. Their choice returns to this call and is recorded as their reply; when they write their own answer instead, it comes back with custom: true. Ask one question at a time; do not repeat it in a message. Older clients can reply with the full option or its number.",
        "inputSchema": {"type": "object", "properties": {
            "question": {"type": "string", "minLength": 1, "maxLength": MAX_QUESTION},
            "options": {"type": "array", "minItems": 2, "maxItems": MAX_OPTIONS,
                "items": {"type": "string", "minLength": 1, "maxLength": 200}},
            "recommended": {"type": "string", "description": "Optional suggested option, exactly as written in options. Never selected automatically."}
        }, "required": ["question", "options"]},
        "annotations": {"readOnlyHint": false, "idempotentHint": false}
    })
}

fn card_data(args: &Value) -> Result<Value> {
    let question = args["question"].as_str().unwrap_or_default().trim();
    ensure!(
        !question.is_empty() && question.chars().count() <= MAX_QUESTION,
        "question must have 1 to {MAX_QUESTION} characters"
    );
    let raw = args["options"].as_array().ok_or_else(|| anyhow!("options must be an array of strings"))?;
    let choices = options(raw.iter().map(Value::as_str))?;
    let recommended = match args.get("recommended").filter(|v| !v.is_null()) {
        Some(value) => Some(
            choices
                .iter()
                .position(|c| Some(c.as_str()) == value.as_str())
                .ok_or_else(|| anyhow!("recommended must exactly name an option"))?,
        ),
        None => None,
    };
    Ok(card(question, &choices, recommended, Answers::OptionsOrOwnWords))
}

/// A card's options, checked: 2 to 12 different strings of 1 to 200 characters (`None`: not a string).
pub(crate) fn options<'a>(raw: impl ExactSizeIterator<Item = Option<&'a str>>) -> Result<Vec<String>> {
    ensure!((2..=MAX_OPTIONS).contains(&raw.len()), "options must hold 2 to {MAX_OPTIONS} different choices");
    let mut choices = Vec::new();
    for value in raw {
        let choice = value.ok_or_else(|| anyhow!("each option must be a string"))?.trim();
        ensure!(!choice.is_empty() && choice.chars().count() <= 200, "each option must have 1 to 200 characters");
        ensure!(
            !choices.iter().any(|old: &String| old.to_lowercase() == choice.to_lowercase()),
            "options must be different"
        );
        choices.push(choice.to_owned());
    }
    Ok(choices)
}

/// What answers a card besides a press on one of its options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answers {
    /// An option, its number, or the user's own words (`request_decision`; an agent's free-text question has no
    /// options, so only the user's words answer it).
    OptionsOrOwnWords,
    /// Only an option or its number: an agent's question whose answer must be one of them (`agent::elicitation`).
    OptionsOnly,
}

impl Answers {
    /// How a card's text tells a client without buttons to answer.
    fn hint(self, has_options: bool) -> &'static str {
        match (self, has_options) {
            (Self::OptionsOrOwnWords, true) => TYPED_HINT,
            (Self::OptionsOrOwnWords, false) => OWN_WORDS_HINT,
            (Self::OptionsOnly, _) => OPTIONS_HINT,
        }
    }

    /// What a card with `data` takes; a card without the field takes own words, as every card did before it.
    pub fn of(data: &Value) -> Self {
        if data["ownWords"] == false { Self::OptionsOnly } else { Self::OptionsOrOwnWords }
    }
}

/// A pending card's data: the question, its numbered options and the suggested one, and how to answer it in a
/// message, for clients that show only its text.
pub(crate) fn card(question: &str, choices: &[String], recommended: Option<usize>, answers: Answers) -> Value {
    let mut text = question.to_owned();
    for (i, choice) in choices.iter().enumerate() {
        let _ = write!(text, "\n{}. {choice}", i + 1);
    }
    if let Some(choice) = recommended.and_then(|i| choices.get(i)) {
        let _ = write!(text, "\n\nSuggested: {choice}");
    }
    text.push_str(answers.hint(!choices.is_empty()));
    let mut data = json!({"decision": true, "text": text, "question": question, "choices": choices,
        "recommended": recommended, "status": Status::Pending, "selected": Value::Null});
    if answers == Answers::OptionsOnly {
        data["ownWords"] = false.into();
    }
    data
}

/// How a card's text tells a client without buttons to answer it: options or own words, own words alone, options alone.
const TYPED_HINT: &str =
    "\n\nReply with the full option, its number or your own answer. Exact option text takes priority over a number.";
const OWN_WORDS_HINT: &str = "\n\nReply with your answer.";
const OPTIONS_HINT: &str = "\n\nReply with the full option or its number.";

/// A card's data for a card only a press answers (a thread's, in its bot's chat): its text doesn't ask for a typed
/// reply, which would go to the bot in the chat.
pub fn pressed_only(mut data: Value) -> Value {
    let text = data["text"].as_str().unwrap_or_default();
    if let Some(text) = [TYPED_HINT, OWN_WORDS_HINT, OPTIONS_HINT].iter().find_map(|hint| text.strip_suffix(hint)) {
        data["text"] = text.to_owned().into();
    }
    data
}

pub fn is_card(card: &Entry) -> bool {
    card.kind == "notice" && card.data["decision"] == true
}

/// The choice card `id` while a press or reply can still answer it.
pub fn pending_card(store: &Store, id: &str) -> Result<Entry> {
    let card = store.entry(id).filter(is_card).ok_or_else(|| anyhow!("unknown decision card"))?;
    ensure!(Status::of(&card.data) == Some(Status::Pending), "that card is already answered or expired");
    Ok(card)
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

/// Who asked a card's question; the `asker` of `decision_asked`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Asker {
    /// The bot's `request_decision` tool call.
    Tool,
    /// The agent's own question, `elicitation/create` (`agent::elicitation`).
    Agent,
}

/// How the user answered a card; the `via` of `decision_answered` (never which option or the words).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Via {
    /// A press on an option.
    Button,
    /// A typed option or its number.
    Text,
    /// The user's own words, typed instead of an option.
    Custom,
    /// A press or typed reply after the call stopped waiting, sent as the user's next message.
    Late,
}

impl Via {
    /// Records the answer, content-free.
    pub fn capture(self, hub: &Hub) {
        crate::analytics::capture(
            hub,
            crate::analytics::Event::DecisionAnswered,
            serde_json::Map::from_iter([("via".into(), json!(self))]),
        );
    }
}

/// A press must name an option. A typed reply may also give its number, or else is the user's own words where the
/// card takes them, unless it is empty or a `/command`, which runs as its own message.
fn answer(choices: &[String], answers: Answers, wanted: &str, typed: bool) -> Option<(String, Via)> {
    if let Some(choice) = pick(choices, wanted, typed) {
        return Some((choice, if typed { Via::Text } else { Via::Button }));
    }
    let own = wanted.trim();
    (typed && answers == Answers::OptionsOrOwnWords && !own.is_empty() && crate::market::slash_command(own).is_none())
        .then(|| (own.to_owned(), Via::Custom))
}

/// `answer` on `card`, with its options and what else it takes.
pub fn answer_card(card: &Entry, wanted: &str, typed: bool) -> Option<(String, Via)> {
    answer(&choices(card), Answers::of(&card.data), wanted, typed)
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
    match tokio::time::timeout_at(deadline.into(), answered).await {
        Ok(answer) => answer.map_err(|_| anyhow!("the turn is no longer waiting for this choice"))?,
        // A thread's card ends with its call: a press there has no next message to become.
        Err(_) if cfg.is_thread() => bail!("{}", Expiry::Timeout.text()),
        Err(_) => Ok(left_open()),
    }
}

/// What a chat's call gets when its wait runs out unanswered.
pub fn left_open() -> Value {
    json!({"answered": false, "note": LEFT_OPEN})
}

/// An API success means the actor delivered this answer to its live tool waiter.
pub async fn respond(hub: &Arc<Hub>, entry_id: &str, option: &str) -> Result<Value> {
    let card = pending_card(&hub.store, entry_id)?;
    let owner = crate::chat::work::card_actor(&card);
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
    /// A thread's card waited four and a half minutes without an answer (a chat's own card stays open instead).
    Timeout,
    /// The agent cancelled its tool call.
    Cancelled,
    /// The user stopped the turn.
    Stopped,
    /// The turn ended with the card unanswered.
    TurnEnded,
    /// The user sent a message the card can't take as an answer: with files or a quote, a `/command`, or call speech
    /// that names no option.
    OtherMessage,
    /// The host restarted while the card waited.
    Restart,
    /// The bot asked a new question while an open card waited.
    AskedAgain,
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
            Self::AskedAgain => "the bot asked again",
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
    fn typed_words_that_name_no_option_are_the_users_own_answer() {
        let choices = vec!["Left".into(), "Right".into()];
        let any = Answers::OptionsOrOwnWords;
        assert_eq!(answer(&choices, any, "Right", false), Some(("Right".into(), Via::Button)));
        assert_eq!(answer(&choices, any, "2", true), Some(("Right".into(), Via::Text)));
        assert_eq!(answer(&choices, any, " Neither, go up ", true), Some(("Neither, go up".into(), Via::Custom)));
        assert_eq!(answer(&choices, any, "Neither", false), None, "a press must name an option");
        for value in ["", "  ", "/pdf merge these", "/compact"] {
            assert_eq!(answer(&choices, any, value, true), None, "{value:?} runs as its own message");
        }
    }

    #[test]
    fn a_card_that_takes_only_its_options_leaves_other_words_to_the_chat() {
        let choices = vec!["Left".into(), "Right".into()];
        let only = Answers::OptionsOnly;
        assert_eq!(answer(&choices, only, "left", true), Some(("Left".into(), Via::Text)));
        assert_eq!(answer(&choices, only, "2", true), Some(("Right".into(), Via::Text)));
        assert_eq!(answer(&choices, only, "Neither, go up", true), None, "own words are an ordinary message");
        let data = card("Which way?", &choices, None, only);
        assert_eq!(data["ownWords"], false);
        assert!(data["text"].as_str().unwrap().ends_with(OPTIONS_HINT));
        assert!(!pressed_only(data)["text"].as_str().unwrap().contains("Reply"));
    }

    #[test]
    fn a_card_without_options_is_answered_in_the_users_own_words() {
        let data = card("What should the release be called?", &[], None, Answers::OptionsOrOwnWords);
        assert_eq!(data["choices"], json!([]));
        assert!(data.get("ownWords").is_none(), "clients and older hosts read a missing field as own words");
        assert!(data["text"].as_str().unwrap().ends_with(OWN_WORDS_HINT));
        let any = Answers::OptionsOrOwnWords;
        assert_eq!(answer(&[], any, "1", true), Some(("1".into(), Via::Custom)), "a number names no option here");
        assert_eq!(answer(&[], any, "Aurora", true), Some(("Aurora".into(), Via::Custom)));
        assert_eq!(answer(&[], any, "Aurora", false), None, "there is nothing to press");
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
