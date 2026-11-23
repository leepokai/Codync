//! ACP `elicitation/create` (unstable in ACP v1): an agent's own question to the user, apart from tool permissions.
//! A form with one field that a choice card (`chat::decision`) can show waits there for the user, whatever the bot's
//! approval setting: free text, one of a few options, or yes/no. Anything else is declined, never answered for them.

use crate::chat::decision::{self, Answers};
use crate::hub::Hub;
use serde::Serialize;
use serde_json::{Map, Value, json};

/// What `initialize` advertises: questions as forms, not as URLs to open.
pub fn capability() -> Value {
    json!({"form": {}})
}

/// A yes/no field's options, in this order.
const YES_NO: [(&str, bool); 2] = [("Yes", true), ("No", false)];

/// A question a choice card can ask: its one field, and what each answer sends back.
#[derive(Debug)]
pub struct Form {
    message: String,
    /// The field's name in the requested schema, which the answer goes under.
    key: String,
    /// Each option's label on the card and the value it sends; none: the user's own words answer it.
    options: Vec<(String, Value)>,
    /// The option the field's `default` names, shown as suggested (never picked for the user).
    default: Option<usize>,
}

impl Form {
    /// The question in `params`, or why a card can't ask it.
    pub fn parse(params: &Value) -> Result<Self, Declined> {
        if params["mode"] != "form" {
            return Err(Declined::Mode);
        }
        let message = params["message"]
            .as_str()
            .map(str::trim)
            .filter(|m| !m.is_empty() && m.chars().count() <= decision::MAX_QUESTION)
            .ok_or(Declined::Message)?;
        let properties = params["requestedSchema"]["properties"].as_object();
        let Some((key, field)) = properties.filter(|p| p.len() == 1).and_then(|p| p.iter().next()) else {
            return Err(Declined::Fields);
        };
        let options = match field["type"].as_str() {
            Some("string") => string_options(field)?,
            Some("boolean") => YES_NO.iter().map(|(label, value)| ((*label).to_owned(), (*value).into())).collect(),
            Some("number" | "integer") => return Err(Declined::Number),
            Some("array") => return Err(Declined::MultiSelect),
            _ => return Err(Declined::OtherType),
        };
        let default = options.iter().position(|(_, value)| *value == field["default"]);
        Ok(Self { message: message.to_owned(), key: key.clone(), options, default })
    }

    /// Only the user's own words answer it: a card with no options.
    pub fn free_text(&self) -> bool {
        self.options.is_empty()
    }

    /// The pending choice card's data.
    pub fn card(&self) -> Value {
        let labels: Vec<String> = self.options.iter().map(|(label, _)| label.clone()).collect();
        let answers = if self.free_text() { Answers::OptionsOrOwnWords } else { Answers::OptionsOnly };
        decision::card(&self.message, &labels, self.default, answers)
    }

    /// The agent's reply once the card stops waiting: `accept` with the user's answer (what the card's waiter got,
    /// `{choice, custom?}`), else `cancel`.
    pub fn reply(&self, answer: Option<&Value>) -> Value {
        match answer.and_then(|a| a["choice"].as_str()).and_then(|choice| self.value_of(choice)) {
            Some(value) => {
                json!({"action": Action::Accept, "content": Map::from_iter([(self.key.clone(), value)])})
            }
            None => json!({"action": Action::Cancel}),
        }
    }

    fn value_of(&self, choice: &str) -> Option<Value> {
        if self.free_text() {
            return Some(choice.into());
        }
        self.options.iter().find(|(label, _)| label == choice).map(|(_, value)| value.clone())
    }
}

/// A string field's options: its `enum`, or its titled `oneOf` (the title on the card, the `const` sent back). Plain
/// free text has none.
fn string_options(field: &Value) -> Result<Vec<(String, Value)>, Declined> {
    let offered: Vec<(Option<&str>, &Value)> = if let Some(values) = field["enum"].as_array() {
        values.iter().map(|value| (value.as_str(), value)).collect()
    } else if let Some(titled) = field["oneOf"].as_array() {
        titled.iter().map(|o| (titled_label(o), &o["const"])).collect()
    } else {
        return plain_text(field).map(|()| Vec::new());
    };
    let labels = decision::options(offered.iter().map(|(label, _)| *label)).map_err(|_| Declined::Options)?;
    Ok(labels.into_iter().zip(offered.into_iter().map(|(_, value)| value.clone())).collect())
}

/// A titled option's label: its title, else its value; none when its value isn't a string.
fn titled_label(option: &Value) -> Option<&str> {
    let value = option["const"].as_str()?;
    Some(option["title"].as_str().unwrap_or(value))
}

/// Free text the card takes as typed: no pattern, format or length beyond "not empty", which nothing checks as the
/// user types.
fn plain_text(field: &Value) -> Result<(), Declined> {
    let rules = ["pattern", "format", "maxLength"].iter().any(|rule| !field[rule].is_null())
        || field["minLength"].as_u64().is_some_and(|n| n > 1);
    if rules { Err(Declined::TextRules) } else { Ok(()) }
}

/// The agent's reply to a question no card asks.
pub fn decline() -> Value {
    json!({"action": Action::Decline})
}

/// What the agent was told: the `action` of `elicitation_replied`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Action {
    /// The user answered.
    Accept,
    /// No card asked it (`Declined`).
    Decline,
    /// Its card stopped waiting unanswered: the turn ended or was stopped, or the host or agent went away.
    Cancel,
}

/// Why no card asked a question: the `reason` of a declined `elicitation_replied` (never the question).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Declined {
    /// Not a form: a URL to open, which Codync doesn't advertise, or a mode ACP may add later.
    Mode,
    /// Not for the session of the running turn: tied to a request, or to another session.
    Session,
    /// No turn is running.
    NoTurn,
    /// A turn whose chat takes no card from it: a room's, a routine's run, another bot's request, one the host
    /// started, a stopped one; or a thread's, for a question only typed words answer (a thread's card takes presses).
    Elsewhere,
    /// Another choice card already waits for the user: one question at a time.
    Busy,
    /// No question to show, or one longer than a card asks (`decision::MAX_QUESTION`).
    Message,
    /// No field, or several: a card asks one thing.
    Fields,
    /// A number field.
    Number,
    /// A multi-select field.
    MultiSelect,
    /// A field type ACP may add later.
    OtherType,
    /// Free text with a pattern, format or length limit.
    TextRules,
    /// Options a card can't show: fewer than 2 or more than 12, not all different, or too long.
    Options,
}

/// Records what the agent was told about one question, content-free.
pub fn report(hub: &Hub, reply: &Value, declined: Option<Declined>) {
    let mut properties = Map::from_iter([("action".into(), reply["action"].clone())]);
    if let Some(reason) = declined {
        properties.insert("reason".into(), json!(reason));
    }
    crate::analytics::capture(hub, crate::analytics::Event::ElicitationReplied, properties);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A question as Hermes sends its `clarify` prompts: one `answer` field.
    fn ask(field: &Value) -> Value {
        json!({"sessionId": "s1", "mode": "form", "message": "Which database?",
            "requestedSchema": {"type": "object", "properties": {"answer": field}, "required": ["answer"]}})
    }

    #[test]
    fn free_text_is_answered_in_the_users_own_words() {
        let form = Form::parse(&ask(&json!({"type": "string"}))).unwrap();
        let card = form.card();
        assert_eq!(card["question"], "Which database?");
        assert_eq!(card["choices"], json!([]));
        assert!(card.get("ownWords").is_none(), "own words answer it");
        let reply = form.reply(Some(&json!({"choice": "Postgres, please", "custom": true})));
        assert_eq!(reply, json!({"action": "accept", "content": {"answer": "Postgres, please"}}));
    }

    #[test]
    fn options_answer_with_their_value_and_nothing_else() {
        let form = Form::parse(&ask(&json!({"type": "string", "enum": ["sqlite", "postgres"], "default": "postgres"})))
            .unwrap();
        let card = form.card();
        assert_eq!(card["choices"], json!(["sqlite", "postgres"]));
        assert_eq!(card["recommended"], 1, "the default is suggested, never picked");
        assert_eq!(card["ownWords"], false);
        assert_eq!(form.reply(Some(&json!({"choice": "sqlite"})))["content"], json!({"answer": "sqlite"}));
        assert_eq!(form.reply(Some(&json!({"choice": "mysql", "custom": true}))), json!({"action": "cancel"}));
    }

    #[test]
    fn titled_options_show_their_title_and_send_their_const() {
        let titled = json!({"type": "string",
            "oneOf": [{"const": "pg", "title": "Postgres"}, {"const": "lite", "title": "SQLite"}]});
        let form = Form::parse(&ask(&titled)).unwrap();
        assert_eq!(form.card()["choices"], json!(["Postgres", "SQLite"]));
        assert_eq!(form.reply(Some(&json!({"choice": "SQLite"})))["content"], json!({"answer": "lite"}));
    }

    #[test]
    fn yes_or_no_sends_a_boolean() {
        let form = Form::parse(&ask(&json!({"type": "boolean", "default": false}))).unwrap();
        assert_eq!(form.card()["choices"], json!(["Yes", "No"]));
        assert_eq!(form.card()["recommended"], 1);
        assert_eq!(form.reply(Some(&json!({"choice": "Yes"})))["content"], json!({"answer": true}));
        assert_eq!(form.reply(Some(&json!({"choice": "No"})))["content"], json!({"answer": false}));
    }

    #[test]
    fn a_card_that_stops_waiting_cancels_the_question() {
        let form = Form::parse(&ask(&json!({"type": "string"}))).unwrap();
        assert_eq!(form.reply(None), json!({"action": "cancel"}));
    }

    #[test]
    fn forms_a_card_cannot_show_faithfully_are_declined() {
        let cases = [
            (ask(&json!({"type": "array", "items": {"type": "string", "enum": ["a", "b"]}})), Declined::MultiSelect),
            (ask(&json!({"type": "integer"})), Declined::Number),
            (ask(&json!({"type": "number"})), Declined::Number),
            (ask(&json!({"type": "string", "format": "email"})), Declined::TextRules),
            (ask(&json!({"type": "string", "maxLength": 10})), Declined::TextRules),
            (ask(&json!({"type": "string", "enum": ["only"]})), Declined::Options),
            (ask(&json!({"type": "string", "enum": ["a", "A"]})), Declined::Options),
            (
                ask(
                    &json!({"type": "string", "oneOf": [{"const": 1, "title": "One"}, {"const": "2", "title": "Two"}]}),
                ),
                Declined::Options,
            ),
            (ask(&json!({"type": "_future"})), Declined::OtherType),
        ];
        for (params, why) in cases {
            assert_eq!(Form::parse(&params).unwrap_err(), why, "{params}");
        }
        let two = json!({"mode": "form", "message": "Name and age?", "requestedSchema": {"type": "object",
            "properties": {"name": {"type": "string"}, "age": {"type": "integer"}}}});
        assert_eq!(Form::parse(&two).unwrap_err(), Declined::Fields);
        let url = json!({"sessionId": "s1", "mode": "url", "message": "Sign in", "elicitationId": "e1",
            "url": "https://example.com"});
        assert_eq!(Form::parse(&url).unwrap_err(), Declined::Mode);
        let mut silent = ask(&json!({"type": "string"}));
        silent["message"] = " ".into();
        assert_eq!(Form::parse(&silent).unwrap_err(), Declined::Message);
        let mut long = ask(&json!({"type": "string"}));
        long["message"] = "a".repeat(decision::MAX_QUESTION + 1).into();
        assert_eq!(Form::parse(&long).unwrap_err(), Declined::Message, "a card asks what request_decision would");
        long["message"] = "a".repeat(decision::MAX_QUESTION).into();
        assert!(Form::parse(&long).is_ok());
        assert!(Form::parse(&ask(&json!({"type": "string", "minLength": 1}))).is_ok(), "not empty is what a card asks");
        assert_eq!(decline(), json!({"action": "decline"}));
    }
}
