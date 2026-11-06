//! A question or a plan of Claude Code in a terminal, answered from any Codync client.
//!
//! The `PreToolUse` hook installed to wait (`hook --ask`, matching `AskUserQuestion` and `ExitPlanMode`) posts the
//! tool's input here. Each question becomes a card whose options are the question's own; the picked labels go back
//! as `updatedInput.answers`, which is how a hook answers `AskUserQuestion`. A plan becomes an Approve / Reject card.
//! With no answer in time, or for anything this does not understand, the reply is silence and the terminal asks.

use super::event::Event;
use super::permission::{target, wait_card};
use super::{Decision, tool};
use crate::hub::Hub;
use serde_json::{Map, Value, json};
use std::sync::Arc;

/// Claude Code asks at most this many questions at once, each with at most this many options.
const MAX_QUESTIONS: usize = 4;
const MAX_OPTIONS: usize = 4;

const APPROVE: &str = "allow";
const REJECT: &str = "deny";

/// The verdict to print for the hook, or `None` when nobody answered and the terminal should ask.
pub(super) async fn ask(hub: &Arc<Hub>, ev: &Event) -> Option<Value> {
    if !matches!(ev.tool.as_str(), "AskUserQuestion" | "ExitPlanMode") {
        return None;
    }
    let (bot, config) = target(hub, ev)?;
    if ev.tool == "ExitPlanMode" {
        let data = plan_card(ev, &bot, &config.cwd);
        return match wait_card(hub, ev, &bot, &config, &data, "Approve the plan").await? {
            Decision::Allow => Some(json!({"permissionDecision": "allow", "updatedInput": ev.input})),
            Decision::Deny => {
                Some(json!({"permissionDecision": "deny", "reason": "The plan was rejected from Codync."}))
            }
            Decision::Pick(_) => None,
        };
    }
    let questions = questions(&ev.input)?;
    let mut answers = Map::new();
    for question in &questions {
        let data = question_card(question, &bot, &config.cwd);
        let Decision::Pick(index) = wait_card(hub, ev, &bot, &config, &data, &question.text).await? else {
            return None;
        };
        answers.insert(question.text.clone(), question.options.get(index)?.label.clone().into());
    }
    let mut input = ev.input.clone();
    input["answers"] = Value::Object(answers);
    Some(json!({"permissionDecision": "allow", "updatedInput": input}))
}

struct Question {
    text: String,
    header: String,
    options: Vec<Choice>,
}

struct Choice {
    label: String,
    description: String,
}

/// The questions of an `AskUserQuestion`, when every one can be answered by tapping a single option. A multi-select
/// question, one without two to four options, or too many questions are left to the terminal.
fn questions(input: &Value) -> Option<Vec<Question>> {
    let list = input.get("questions")?.as_array().filter(|q| (1..=MAX_QUESTIONS).contains(&q.len()))?;
    list.iter()
        .map(|q| {
            if q.get("multiSelect").and_then(Value::as_bool) == Some(true) {
                return None;
            }
            let options: Vec<Choice> = q
                .get("options")?
                .as_array()?
                .iter()
                .filter_map(|o| {
                    let label = o.get("label")?.as_str()?.trim();
                    (!label.is_empty()).then(|| Choice {
                        label: label.to_owned(),
                        description: o.get("description").and_then(Value::as_str).unwrap_or_default().to_owned(),
                    })
                })
                .collect();
            let text = q.get("question")?.as_str()?.trim().to_owned();
            ((2..=MAX_OPTIONS).contains(&options.len()) && !text.is_empty()).then(|| Question {
                text,
                header: q.get("header").and_then(Value::as_str).unwrap_or_default().to_owned(),
                options,
            })
        })
        .collect()
}

/// A question as an approval card: the options keep their labels, their ids are their positions.
fn question_card(question: &Question, bot: &str, cwd: &str) -> Value {
    let detail: Vec<String> = question
        .options
        .iter()
        .filter(|o| !o.description.is_empty())
        .map(|o| format!("{}: {}", o.label, o.description))
        .collect();
    let options: Vec<Value> = question
        .options
        .iter()
        .enumerate()
        .map(|(i, o)| json!({"optionId": i.to_string(), "name": o.label, "kind": "allow_once"}))
        .collect();
    json!({
        "title": question.text,
        "toolKind": "question",
        "question": true,
        "header": question.header,
        "command": Value::Null,
        "detail": detail.join("\n"),
        "diffs": [],
        "cwd": cwd,
        "options": options,
        "status": "pending",
        "selected": Value::Null,
        "author": bot,
    })
}

fn plan_card(ev: &Event, bot: &str, cwd: &str) -> Value {
    json!({
        "title": tool::title(&ev.tool, &ev.input),
        "toolKind": "plan",
        "plan": true,
        "command": Value::Null,
        "detail": ev.input.get("plan").and_then(Value::as_str).unwrap_or_default(),
        "diffs": [],
        "cwd": cwd,
        "options": [
            {"optionId": APPROVE, "name": "Approve", "kind": "allow_once"},
            {"optionId": REJECT, "name": "Reject", "kind": "reject_once"},
        ],
        "status": "pending",
        "selected": Value::Null,
        "author": bot,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(question: &Value) -> Value {
        json!({"questions": [question]})
    }

    fn pick() -> Value {
        json!({"question": "Which database?", "header": "DB", "multiSelect": false,
            "options": [{"label": "SQLite", "description": "one file"}, {"label": "Postgres", "description": "a server"}]})
    }

    #[test]
    fn a_single_choice_question_becomes_a_card_with_its_own_options() {
        let list = questions(&input(&pick())).expect("answerable");
        let card = question_card(&list[0], "bot", "/work");
        assert_eq!(card["title"], "Which database?");
        assert_eq!(card["options"][1], json!({"optionId": "1", "name": "Postgres", "kind": "allow_once"}));
        assert_eq!(card["detail"], "SQLite: one file\nPostgres: a server");
    }

    #[test]
    fn what_a_tap_cannot_answer_is_left_to_the_terminal() {
        let mut many = pick();
        many["multiSelect"] = true.into();
        let one_option = json!({"question": "Sure?", "options": [{"label": "Yes"}]});
        let too_many: Vec<Value> = (0..5).map(|_| pick()).collect();
        for input in [
            input(&many),
            input(&one_option),
            json!({"questions": too_many}),
            json!({"questions": []}),
            json!({"questions": [{"options": []}]}),
            json!({}),
        ] {
            assert!(questions(&input).is_none(), "{input}");
        }
    }

    #[test]
    fn up_to_four_questions_are_asked() {
        let four: Vec<Value> = (0..4).map(|_| pick()).collect();
        assert_eq!(questions(&json!({"questions": four})).map(|q| q.len()), Some(4));
    }
}
