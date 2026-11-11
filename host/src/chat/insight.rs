//! What another reader hears of a bot's work: whether it is idle or busy, what it is doing, the approval it waits for,
//! and its latest messages. A voice call's voice uses this ([`crate::chat::calls`], its `bot_status` and
//! `recent_messages`), and so does a bot looking at another bot ([`crate::chat::team`]), so both tell the same story.

use crate::hub::{Hub, Runtime};
use crate::store::{BotConfig, Entry, EntryKind, Lane};
use serde_json::{Value, json};

/// Entries looked through for a bot's current work and its pending approval.
pub const RECENT_ENTRIES: i64 = 300;
/// How much of each message `recent` keeps of its text.
const RECENT_CHARS: usize = 1_500;
const MESSAGES: i64 = 20;

/// A bot's runtime; a group's is that of the member working in it.
pub fn runtime(hub: &Hub, cfg: &BotConfig) -> Runtime {
    if cfg.is_group() { hub.group_runtime(cfg) } else { hub.runtime(&cfg.id) }
}

/// Whether the bot is idle or working, what it is doing, and any approval it waits for.
pub fn status(hub: &Hub, cfg: &BotConfig, recent: &[Entry]) -> Value {
    let rt = runtime(hub, cfg);
    let mut out = json!({"status": rt.status, "activity": rt.activity});
    if let Some(w) = waiting(recent) {
        out["approval"] = json!({"request": w.request, "options": w.options});
    }
    out
}

/// The latest `count` messages as they were said: who wrote each and its text.
pub fn recent(hub: &Hub, cfg: &BotConfig, count: i64) -> Value {
    json!({"messages": said(hub, cfg, count, RECENT_CHARS).into_iter().map(|(from, text)| json!({"from": from, "text": text})).collect::<Vec<_>>()})
}

/// The chat's last `count` messages as its readers name them: who wrote each, and its text shortened to `chars`.
pub fn said(hub: &Hub, cfg: &BotConfig, count: i64, chars: usize) -> Vec<(String, String)> {
    let messages = hub.store.messages_after(&Lane::main(&cfg.id), 0, count.clamp(1, MESSAGES)).unwrap_or_default();
    messages
        .iter()
        .map(|e| {
            let who =
                writer(e).map_or_else(|| hub.bot_name(e.data["author"].as_str().unwrap_or(&cfg.id)), str::to_owned);
            (who, crate::agent::acp::truncate(e.data["text"].as_str().unwrap_or_default(), chars))
        })
        .collect()
}

/// Who wrote a chat message among the user's own, as the voice and the title writer name them; `None` for a bot.
fn writer(e: &Entry) -> Option<&'static str> {
    match (e.kind.as_str(), e.data["from"].as_str()) {
        ("user", Some("voice")) => Some("Voice (relayed)"),
        ("user", _) => Some("User"),
        _ => None,
    }
}

/// An approval a bot waits for: its card, what it asks and its options.
pub struct Waiting<'a> {
    pub card: &'a Entry,
    /// What the card asks (its title), else the command it runs.
    pub request: &'a str,
    /// The options' names, in the order the card offers them.
    pub options: Vec<&'a str>,
}

/// The newest approval card still pending in `recent` (oldest first).
pub fn waiting(recent: &[Entry]) -> Option<Waiting<'_>> {
    let card =
        recent.iter().rev().find(|e| e.kind == EntryKind::Permission.as_str() && e.data["status"] == "pending")?;
    let request = card.data["title"].as_str().or_else(|| card.data["command"].as_str()).unwrap_or_default();
    let options = card.data["options"].as_array().into_iter().flatten().filter_map(option_name).collect();
    Some(Waiting { card, request, options })
}

/// An option's name, as its card shows it.
pub fn option_name(o: &Value) -> Option<&str> {
    o["name"].as_str()
}
