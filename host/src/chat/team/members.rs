//! `ask_member`: a bot in a group chat asks the people there, and their short replies come back to
//! the same turn. At most two replies are taken; the card is answered by replying to it.

use super::{MAX_MESSAGE_BYTES, visible_bot};
use crate::LockExt;
use crate::hub::Hub;
use crate::store::{BotConfig, EntryKind, Lane};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

/// How long the bot waits for the first reply.
const FIRST: Duration = Duration::from_secs(600);
/// How long it keeps taking replies after the first one.
const SECOND: Duration = Duration::from_secs(20);
/// The most replies one request takes.
pub const MAX_ANSWERS: usize = 2;

#[derive(Default)]
pub struct Members {
    waiting: Mutex<HashMap<String, mpsc::UnboundedSender<String>>>,
}

impl Members {
    /// Delivers one reply to the card `entry`, if its bot is still waiting.
    pub fn answer(&self, entry: &str, who: &str, text: &str) -> bool {
        let sender = self.waiting.locked().get(entry).cloned();
        match sender {
            Some(sender) => sender.send(format!("{who}: {text}")).is_ok(),
            None => false,
        }
    }
    pub fn waiting(&self, entry: &str) -> bool {
        self.waiting.locked().contains_key(entry)
    }
}

pub(super) fn tool() -> Value {
    json!({
        "name": "ask_member",
        "description": "Ask the people of this group chat a short question and wait for their replies (up to 10 minutes). They answer by replying to the card; at most two replies come back. Use it only inside a group chat, for a decision or a detail only a person has. Keep the question to one line.",
        "inputSchema": {
            "type": "object", "properties": {
                "question": {"type": "string", "description": "One short question for the people here."},
            }, "required": ["question"],
        },
        "annotations": {"readOnlyHint": false, "idempotentHint": false},
    })
}

pub(super) async fn ask(hub: &Arc<Hub>, source: &BotConfig, question: &str) -> Result<Value> {
    let question = question.trim();
    if question.is_empty() || question.len() > MAX_MESSAGE_BYTES {
        bail!("question must be nonempty and at most {MAX_MESSAGE_BYTES} bytes");
    }
    let chat = hub
        .runtime(&source.id)
        .lane
        .map(|lane| lane.chat)
        .filter(|chat| chat != &source.id)
        .context("ask_member is only available inside a group chat")?;
    visible_bot(hub, &chat)?;
    let card = hub
        .add_entry(
            &Lane::main(&chat),
            EntryKind::Notice,
            hub.store.max_turn(&chat) + 1,
            &json!({"text": question, "style": "info", "memberQuestion": {"status": "pending", "answers": []},
                "author": source.id}),
        )
        .ok_or_else(|| anyhow!("couldn't save the question"))?;
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    hub.members.waiting.locked().insert(card.id.clone(), tx);
    let mut answers: Vec<String> = Vec::new();
    let first = tokio::time::timeout(FIRST, rx.recv()).await;
    if let Ok(Some(answer)) = first {
        answers.push(answer);
        while answers.len() < MAX_ANSWERS
            && let Ok(Some(answer)) = tokio::time::timeout(SECOND, rx.recv()).await
        {
            answers.push(answer);
        }
    }
    hub.members.waiting.locked().remove(&card.id);
    let status = if answers.is_empty() { "unanswered" } else { "answered" };
    hub.set_entry(
        &card.id,
        &json!({"text": question, "style": "info", "author": source.id,
            "memberQuestion": {"status": status, "answers": answers}}),
    );
    if answers.is_empty() {
        bail!("nobody answered; ask again or continue without them");
    }
    Ok(json!({"questionId": card.id, "answers": answers}))
}

pub(super) fn answer_tool() -> Value {
    tool()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_waiting_card_takes_an_answer() {
        let members = Members::default();
        assert!(!members.answer("x", "Ana", "yes"));
        assert!(!members.waiting("x"));
    }
}
