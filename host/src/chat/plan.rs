//! A bot keeps a visible checklist of its work with the `chat` server's `update_plan`, for agents that have no task
//! list of their own (Claude Code through its ACP adapter has none here). It is the same `plan` entry an agent's own
//! ACP plan makes: written once per turn and replaced on each call, so the clients show one checklist that moves,
//! above the message box, and the full conversation keeps it.

use crate::hub::Hub;
use crate::store::{EntryKind, Lane};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::sync::Arc;

pub const TOOL: &str = "update_plan";

/// Added to the `chat` server's instructions: when to keep the checklist.
pub const INSTRUCTIONS: &str = "For work with several steps (more than two actions, or a request that lists \
several things), keep the user's checklist current with update_plan: all the steps first, then each one in_progress \
and completed as you go. They see it above their message box. Don't use it for a single answer, and don't \
repeat the list in your messages.";

const MAX_ITEMS: usize = 30;
const MAX_CHARS: usize = 300;
const STATUSES: [&str; 3] = ["pending", "in_progress", "completed"];

pub fn tool() -> Value {
    json!({
        "name": TOOL,
        "description": format!(
            "Keep a visible checklist for work with several steps: the user sees it, with how far you are, above \
             the message box. Call it when you start such work with every step pending, then again each time a step \
             starts (in_progress, one at a time) or ends (completed), always with the whole list. Skip it for a \
             single quick answer. At most {MAX_ITEMS} steps of {MAX_CHARS} characters."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "items": {
                    "type": "array",
                    "description": "The whole checklist, in order.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": {"type": "string", "description": "What the step is, in the user's language."},
                            "status": {"type": "string", "enum": STATUSES},
                        },
                        "required": ["content", "status"],
                    },
                },
            },
            "required": ["items"],
        },
        "annotations": {"readOnlyHint": false, "idempotentHint": true},
    })
}

/// The checklist as an ACP plan's entries: trimmed, in order, none empty.
pub fn checked(args: &Value) -> Result<Vec<Value>> {
    let Some(items) = args["items"].as_array() else { bail!("`items` must be a list of steps") };
    if items.len() > MAX_ITEMS {
        bail!("at most {MAX_ITEMS} steps");
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let content = item["content"].as_str().unwrap_or_default().trim();
        if content.is_empty() || content.chars().count() > MAX_CHARS {
            bail!("each step needs text of at most {MAX_CHARS} characters");
        }
        let status = item["status"].as_str().unwrap_or("pending");
        if !STATUSES.contains(&status) {
            bail!("a step's status is pending, in_progress or completed");
        }
        out.push(json!({"content": content, "priority": "medium", "status": status}));
    }
    Ok(out)
}

pub async fn call(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value> {
    let entries = checked(args)?;
    let (hub, bot) = (hub.clone(), bot.to_owned());
    let count = entries.len();
    tokio::task::spawn_blocking(move || {
        let turn = hub.store.max_turn(&bot);
        let data = json!({"entries": entries, "author": bot});
        // This turn's checklist, if the bot already wrote one: it moves in place.
        let kept = hub
            .store
            .history(&bot, i64::MAX, 200)?
            .into_iter()
            .rev()
            .find(|e| e.kind == "plan" && e.turn == turn && e.data["author"] == bot.as_str());
        match kept {
            Some(e) => {
                hub.set_entry(&e.id, &data);
            }
            None => {
                hub.add_entry(&Lane::main(&bot), EntryKind::Plan, turn, &data);
            }
        }
        anyhow::Ok(())
    })
    .await??;
    Ok(json!({"saved": true, "steps": count}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_checklist_is_trimmed_and_checked() {
        let ok = checked(&json!({"items": [
            {"content": "  lire  ", "status": "completed"},
            {"content": "écrire", "status": "in_progress"},
            {"content": "relire"},
        ]}))
        .unwrap();
        assert_eq!(ok.len(), 3);
        assert_eq!(ok[0], json!({"content": "lire", "priority": "medium", "status": "completed"}));
        assert_eq!(ok[2]["status"], "pending", "a step with no status is still to do");
        assert!(checked(&json!({})).is_err(), "no list");
        assert!(checked(&json!({"items": [{"content": " ", "status": "pending"}]})).is_err(), "an empty step");
        assert!(checked(&json!({"items": [{"content": "a", "status": "done"}]})).is_err(), "an unknown status");
        let long = "x".repeat(MAX_CHARS + 1);
        assert!(checked(&json!({"items": [{"content": long, "status": "pending"}]})).is_err());
        let many: Vec<Value> =
            (0..=MAX_ITEMS).map(|i| json!({"content": format!("step {i}"), "status": "pending"})).collect();
        assert!(checked(&json!({"items": many})).is_err(), "too many steps");
        assert!(checked(&json!({"items": []})).unwrap().is_empty(), "an empty list clears it");
    }
}
