//! A rollover's handoff. When the bot's main conversation is due to start over (see `agent::bot::compaction`), a
//! card in its chat shows what the new conversation will carry; the user can edit it, or keep the old conversation,
//! until their next message starts the new one with it. The card is a notice whose `handoff` field holds the draft,
//! so an app that doesn't know it still shows the text.

use crate::hub::Hub;
use crate::store::{BotConfig, Entry, EntryKind, Lane, Store};
use anyhow::{Result, anyhow, bail};
use codync_mem::compaction::{Handoff, Request};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The longest handoff a user can save.
const MAX_CHARS: usize = 20_000;
/// The user's messages a handoff repeats word for word.
const REQUESTS: usize = 4;
/// Main-chat entries read to build one.
const READ: i64 = 120;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// The next message starts a new conversation with it.
    Draft,
    Sent,
    /// The user kept the old conversation.
    Kept,
}

/// Why the conversation starts over.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    /// A large conversation whose prompt cache expired: going on would read it all again at full price.
    Cold,
    /// A long absence after several summaries.
    Away,
    /// Summaries stopped freeing room.
    Full,
}

impl Reason {
    fn line(self) -> &'static str {
        match self {
            Self::Cold => {
                "This conversation is long and has gone quiet, so your next message starts a fresh one carrying this summary."
            }
            Self::Away => {
                "You've been away a while and this conversation has been summarized several times, so your next message starts a fresh one carrying this summary."
            }
            Self::Full => {
                "Summarizing this conversation no longer frees enough room, so your next message starts a fresh one carrying this summary."
            }
        }
    }
}

fn key(bot: &str) -> String {
    format!("handoff.{bot}")
}

/// Where the harness's last summary of a session is kept.
pub fn summary_key(bot: &str, session: &str) -> String {
    format!("compaction.summary.{bot}.{session}")
}

/// Posts the card for `session`, the bot's main conversation, built from its chat.
pub fn draft(hub: &Hub, cfg: &BotConfig, session: &str, reason: Reason) -> Result<Entry> {
    let text = build(&hub.store, &cfg.id, session)?;
    let lane = Lane::main(&cfg.id);
    let data = json!({
        "text": reason.line(),
        "style": "info",
        "author": cfg.id,
        "handoff": {"status": Status::Draft, "reason": reason, "text": text},
    });
    let entry = hub
        .add_entry(&lane, EntryKind::Notice, hub.store.max_turn(&cfg.id), &data)
        .ok_or_else(|| anyhow!("couldn't post the handoff"))?;
    hub.store.kv_set(&key(&cfg.id), &entry.id)?;
    Ok(entry)
}

fn build(store: &Store, bot: &str, session: &str) -> Result<String> {
    let entries = store.history(bot, i64::MAX, READ)?;
    let user = EntryKind::User.as_str();
    let requests: Vec<(String, String)> = entries
        .iter()
        .filter(|e| e.kind == user)
        .rev()
        .take(REQUESTS)
        .map(|e| (crate::chat::memory::ymd(e.created_at), e.data["text"].as_str().unwrap_or_default().to_owned()))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let requests: Vec<Request> = requests.iter().map(|(date, text)| Request { date, text }).collect();
    let reply = entries.iter().rev().find(|e| e.kind == "agent" && e.data["final"] == true);
    let plan = entries.iter().rev().find(|e| e.kind == "plan").map(|e| plan_text(&e.data));
    let summary = store.kv_get(&summary_key(bot, session));
    let bookmark = entries.iter().rev().find(|e| e.kind == user || (e.kind == "agent" && e.data["final"] == true));
    Ok(Handoff {
        summary: summary.as_deref(),
        requests: &requests,
        reply: reply.and_then(|e| e.data["text"].as_str()),
        plan: plan.as_deref(),
        bookmark: bookmark.map(|e| e.id.as_str()),
    }
    .render())
}

pub(crate) fn plan_text(data: &Value) -> String {
    let steps = data["entries"].as_array().into_iter().flatten();
    steps
        .map(|s| {
            format!("- [{}] {}", s["status"].as_str().unwrap_or("pending"), s["content"].as_str().unwrap_or_default())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The draft the next main-chat message starts a new conversation with: its card and text.
pub fn pending(store: &Store, bot: &str) -> Option<(String, String)> {
    let id = store.kv_get(&key(bot)).filter(|id| !id.is_empty())?;
    let entry = store.entry(&id)?;
    let card = &entry.data["handoff"];
    (card["status"] == json!(Status::Draft)).then(|| (id, card["text"].as_str().unwrap_or_default().to_owned()))
}

/// Whether the user kept the old conversation over the current card; forgets the card either way once it's settled.
pub fn kept(store: &Store, bot: &str) -> bool {
    let Some(id) = store.kv_get(&key(bot)).filter(|id| !id.is_empty()) else { return false };
    let status = store.entry(&id).map(|e| e.data["handoff"]["status"].clone());
    let kept = status.as_ref() == Some(&json!(Status::Kept));
    if status.as_ref() != Some(&json!(Status::Draft)) {
        let _ = store.kv_delete(&key(bot));
    }
    kept
}

/// The new conversation started with the card's text.
pub fn sent(hub: &Hub, bot: &str, id: &str) {
    set_status(hub, id, Status::Sent, "Started a fresh conversation with this summary.");
    let _ = hub.store.kv_delete(&key(bot));
}

fn set_status(hub: &Hub, id: &str, status: Status, line: &str) -> Option<Entry> {
    let mut data = hub.store.entry(id)?.data;
    data["handoff"]["status"] = json!(status);
    data["text"] = line.into();
    hub.set_entry(id, &data)
}

/// `editHandoff` and `keepConversation`.
pub fn call(hub: &Hub, method: &str, b: &Value) -> Result<Value> {
    let id = b["entryId"].as_str().ok_or_else(|| anyhow!("entryId required"))?;
    let entry = hub.store.entry(id).filter(|e| e.data["handoff"].is_object()).ok_or_else(|| anyhow!("no such card"))?;
    if entry.data["handoff"]["status"] != json!(Status::Draft) {
        bail!("This conversation already moved on.");
    }
    let entry = match method {
        "editHandoff" => {
            let text =
                b["text"].as_str().map(str::trim).filter(|t| !t.is_empty()).ok_or_else(|| anyhow!("text required"))?;
            if text.chars().count() > MAX_CHARS {
                bail!("A handoff can be at most {MAX_CHARS} characters.");
            }
            let mut data = entry.data;
            data["handoff"]["text"] = text.into();
            hub.set_entry(id, &data)
        }
        "keepConversation" => set_status(hub, id, Status::Kept, "Kept this conversation."),
        _ => bail!("unknown method {method}"),
    };
    Ok(json!({"entry": entry}))
}
