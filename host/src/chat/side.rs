//! `/side <question>`: one question about the chat so far, answered off to the side.
//!
//! The bot's room may be busy for an hour on the user's last request, and a question about what it has found so far
//! shouldn't wait in that queue or talk the bot into a new turn. `/side` takes the chat's recent messages as a
//! snapshot and asks a throwaway agent of the bot's harness (see [`crate::chat::oneshot`]) — its own session, no
//! tools, nothing kept: the answer lands in the chat as a notice, and the bot's own turn, session and memory never
//! see it. Only a Claude Code bot can run this way — its session can be opened with no tools at all — so a bot on
//! another harness says so instead of answering with tools it can't put down. The same text without the command is
//! an ordinary message, and a group has no single bot to ask.

use crate::agent::bot::NoticeStyle;
use crate::hub::Hub;
use crate::store::{BotConfig, Entry, EntryKind, Lane, Store};
use anyhow::Result;
use serde_json::json;
use std::sync::Arc;

/// Messages the snapshot shows the side agent: what a long turn has found, not the whole history.
const SNAPSHOT_LINES: usize = 24;
/// Chars one snapshot line is clipped to: a pasted log shouldn't crowd out the question.
const LINE_CHARS: usize = 2000;

/// The question in a `/side` message, or `None` when the text isn't one. `Some("")` is `/side` alone.
/// (`/sidebar` and `/sideswipe` are other commands, and `@bot /side …` is an ordinary message.)
pub fn parse(text: &str) -> Option<&str> {
    let rest = text.trim_start().strip_prefix("/side")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some(rest.trim())
}

/// The chat's recent messages, oldest first: what the side agent is shown. `/side` itself and its answers aren't
/// part of the conversation it answers from.
pub fn snapshot(store: &Store, bot_id: &str, limit: usize) -> Result<String> {
    let name = store.bot(bot_id)?.map_or_else(|| "Bot".to_owned(), |r| r.config.name);
    // Steps and earlier side lines don't count against the window: take a wider slice, then the last `limit`
    // conversation lines from it.
    let window = i64::try_from(limit.saturating_mul(4)).unwrap_or(i64::MAX);
    let entries = store.history(bot_id, i64::MAX, window)?;
    let mut lines: Vec<String> = entries.iter().filter_map(|e| line(e, &name)).collect();
    let skip = lines.len().saturating_sub(limit);
    lines.drain(..skip);
    Ok(lines.join("\n"))
}

fn line(e: &Entry, name: &str) -> Option<String> {
    let text = e.data["text"].as_str()?.trim();
    if text.is_empty() || e.data["side"] == true {
        return None;
    }
    let text = codync_agent::acp::truncate(text, LINE_CHARS);
    match e.kind.as_str() {
        "user" => Some(format!("User: {text}")),
        "agent" => Some(format!("{name}: {text}")),
        _ => None,
    }
}

/// What the side agent is told it is doing: the same instructions for every harness.
pub const SYSTEM: &str = "\
Answer from the conversation so far, based on progress up to this snapshot. Do not call any tool. If the answer \
needs information that is not in the conversation, say so. Never claim to have done anything.";

/// The one prompt the side agent gets: the marker, the rules, the snapshot, then the question.
pub fn prompt(snapshot: &str, question: &str) -> String {
    let conversation = if snapshot.trim().is_empty() { "No messages yet." } else { snapshot.trim() };
    format!("[codync/side]\n\n{SYSTEM}\n\nConversation so far, oldest first:\n\n{conversation}\n\nQuestion: {question}")
}

/// Asks the throwaway agent `question` about the chat's snapshot and returns its answer.
pub async fn ask(hub: &Arc<Hub>, cfg: &BotConfig, question: &str) -> Result<String> {
    let snapshot = snapshot(&hub.store, &cfg.id, SNAPSHOT_LINES)?;
    let tools = crate::chat::oneshot::Tools::Off;
    crate::chat::oneshot::one_shot_with(hub, cfg, SYSTEM, &prompt(&snapshot, question), tools).await
}

/// What the user is told when the bot's harness can't open a session without tools.
pub const NEEDS_CLAUDE: &str =
    "Side questions need a bot on Claude Code: it is the only agent Codync can open without tools.";

/// Answers `question` in the background, as a notice in `cfg`'s chat: the bot's own turn never sees it.
pub fn start(hub: &Arc<Hub>, cfg: &BotConfig, question: &str) {
    let hub = hub.clone();
    let cfg = cfg.clone();
    let question = question.to_owned();
    crate::analytics::side_asked(&hub, &cfg.backend);
    tokio::spawn(async move {
        let started = std::time::Instant::now();
        let (text, style, outcome) = match ask(&hub, &cfg, &question).await {
            Ok(answer) if !answer.trim().is_empty() => {
                (answer.trim().to_owned(), NoticeStyle::Info, crate::analytics::SideOutcome::Answered)
            }
            Ok(_) => (
                "The side question got no answer.".to_owned(),
                NoticeStyle::Error,
                crate::analytics::SideOutcome::Failed,
            ),
            Err(error) if error.downcast_ref::<crate::chat::oneshot::ToolsCannotBeOff>().is_some() => {
                (NEEDS_CLAUDE.to_owned(), NoticeStyle::Info, crate::analytics::SideOutcome::Unsupported)
            }
            Err(error) => {
                tracing::warn!(bot = %cfg.id, error = format!("{error:#}"), "side question failed");
                (
                    format!("The side question failed: {error:#}"),
                    NoticeStyle::Error,
                    crate::analytics::SideOutcome::Failed,
                )
            }
        };
        let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        crate::analytics::side_answered(&hub, &cfg.backend, outcome, ms);
        // The bot (or its chat) may be gone by the time the answer lands.
        if hub.store.bot(&cfg.id).ok().flatten().is_none_or(|r| r.deleted) {
            return;
        }
        let data = json!({"text": text, "style": style, "side": true});
        hub.add_entry(&Lane::main(&cfg.id), EntryKind::Notice, hub.store.max_turn(&cfg.id), &data);
    });
}

/// `/side` alone: there is no question to answer.
pub fn asked_nothing(hub: &Hub, cfg: &BotConfig) {
    notice(hub, cfg, "Write a question after `/side`.", NoticeStyle::Info);
}

/// A group is several bots in one room with no bot of its own: a side question belongs in a bot's own chat.
pub fn not_in_a_group(hub: &Hub, cfg: &BotConfig) {
    notice(
        hub,
        cfg,
        "Side questions work in a bot's own chat, not in a group. Ask the bot you mean directly.",
        NoticeStyle::Info,
    );
}

fn notice(hub: &Hub, cfg: &BotConfig, text: &str, style: NoticeStyle) {
    let data = json!({"text": text, "style": style, "side": true});
    hub.add_entry(&Lane::main(&cfg.id), EntryKind::Notice, hub.store.max_turn(&cfg.id), &data);
}

#[cfg(test)]
mod tests;
