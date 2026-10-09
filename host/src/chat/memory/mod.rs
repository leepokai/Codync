//! Per-bot Engram memory with automatic extraction, recall and native MCP tools.
//! Transcripts stay in Codync; all durable memory lives in Engram's SQLite database.

mod dates;
mod engram;
mod extract;
mod facts;
mod keeper;
pub(crate) mod lifecycle;
pub(crate) mod maintenance;
pub(crate) mod manage;
mod read;
mod search;

pub use extract::{EpisodeTurn, is_memorable, set_pending_episode};
pub use facts::Memory;
pub(crate) use keeper::one_shot;
pub use keeper::{Exchange, KeeperEvent, spawn_keeper};
pub use search::{INSTRUCTIONS, available_tools, call, tools};

use anyhow::Result;
use facts::{Fact, Recall, ymd};
use std::path::Path;
use std::time::Duration;

/// Profile facts shown in the prompt; past this the keeper consolidates the profile.
const PROFILE_PROMPT_LIMIT: usize = 100;
/// What a consolidation merges the profile down to, leaving room to grow.
const PROFILE_TARGET: usize = 60;
/// Log facts shown in the prompt (newest first), within [`RECENT_CHAR_BUDGET`].
pub const RECENT_PROMPT_LIMIT: usize = 30;
const RECENT_CHAR_BUDGET: usize = 4_000;
const MAX_FACT_CHARS: usize = 500;
const RELEVANT_LIMIT: usize = 10;
/// An exchange side is cut to this before it goes to the keeper.
const EXCHANGE_CHARS: usize = 8_000;
const KEEPER_TIMEOUT: Duration = Duration::from_secs(180);
/// Quiet time after the last exchange before the keeper runs over the queued ones.
const KEEPER_IDLE: Duration = Duration::from_secs(5 * 60);
/// Queued exchanges that make the keeper run without waiting.
const KEEPER_BATCH: usize = 8;

const NOTE_PREFIX: &str = "[note] ";
const NONE: &str = "NONE";

fn fact_line(f: &Fact) -> String {
    format!("- (learned {}) {}", ymd(f.created_at), f.content)
}

/// The memory section of the bot's instructions, and whether it holds any facts.
pub fn render(recall: &Recall, location: &Path) -> (String, bool) {
    let mut lines = vec![
        "Memory: durable facts you have learned about the user and their world.".to_owned(),
        "These persist across every session with this bot, even after a new session starts. Rely on them so you stay consistent and avoid re-asking what you already know.".to_owned(),
        format!(
            "Your memory is managed by Engram in {}. Use its memory tools to read and write it; do not edit the database or the legacy Markdown backup directly.",
            location.display()
        ),
        "At the start of related work, call mem_context and mem_search to recall prior knowledge; retrieve full observations before relying on previews. Save durable preferences, decisions, discoveries and fixes proactively with mem_save. Reuse a stable topic_key for evolving knowledge. When asked to remember or forget, perform the memory operation before confirming success. A background keeper also extracts durable facts from completed user conversations.".to_owned(),
        "Before ending a session, save a mem_session_summary. After compaction, recover context with mem_context. Use mem_review, mem_judge and mem_compare to review stale or conflicting knowledge; never treat a proposed conflict as resolved until it has been judged. Use search_history for original chat messages. Only this bot's memory is available through these tools.".to_owned(),
    ];
    if !recall.profile.is_empty() {
        lines.push("About the user:".into());
        lines.extend(recall.profile.iter().map(fact_line));
    }
    if !recall.recent.is_empty() {
        lines.push("Recently:".into());
        let mut budget = RECENT_CHAR_BUDGET;
        let mut shown = 0;
        for f in &recall.recent {
            let line = fact_line(f);
            if shown > 0 && line.len() > budget {
                break;
            }
            budget = budget.saturating_sub(line.len());
            lines.push(line);
            shown += 1;
        }
        let omitted = recall.recent.len() - shown;
        if omitted > 0 {
            lines.push(format!("({omitted} more recent memories — use mem_search to retrieve them.)"));
        }
    }
    let has_facts = !recall.profile.is_empty() || !recall.recent.is_empty();
    if !has_facts {
        lines.push("No facts recorded yet.".into());
    }
    (lines.join("\n"), has_facts)
}

/// Ensure installation, migration and the native memory process before starting a session.
pub async fn prepare(bot_id: &str) -> Result<()> {
    engram::prepare(bot_id).await
}
