//! What an agent says about a session (ACP `session/update`), as typed events an app builds its own view from.
//!
//! Each [`Event`] is one fact the agent reported, in the agent's own terms: what to make of it (defaults, filters,
//! where it shows) is the app's. An update this crate doesn't know yet is still delivered, as [`Event::Other`] with
//! the update as sent, so a newer adapter adding a kind never breaks reading.

use crate::acp::content_text;
use serde_json::Value;

mod claude;
mod task;
pub use claude::Subagent;
pub use task::{Task, TaskChange, TaskEnd, TaskKind};

/// One `session/update`: the session it is about and what it says.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Update {
    /// The session it is about; a session the app didn't open (a subagent's, a background one's) is still named.
    pub session: Option<String>,
    pub event: Event,
}

impl Update {
    /// The update in a `session/update`'s `params`.
    pub fn parse(params: &Value) -> Self {
        Self { session: params["sessionId"].as_str().map(str::to_owned), event: Event::parse(&params["update"]) }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Event {
    /// Reply text (`agent_message_chunk`). `notice`: the level of a notification the adapter passed on as text
    /// (pi-acp's `_meta.piAcp.notify.level`), which isn't the reply's words.
    Text { text: String, notice: Option<String> },
    /// The agent's reasoning (`agent_thought_chunk`).
    Thought { text: String },
    /// The user's words, as the agent replays them with a session's history (`user_message_chunk`).
    UserText { text: String },
    /// A tool call the agent started (`tool_call`).
    ToolStarted(Box<ToolCall>),
    /// What changed about a tool call (`tool_call_update`): only the fields it sent are `Some`.
    ToolUpdated(Box<ToolCall>),
    /// The agent's plan (`plan`): its entries as sent (`content`, `priority`, `status`).
    Plan { entries: Value },
    /// How much of its context the session uses (`usage_update`).
    Usage(Usage),
    /// The agent summarized the session's context (`compaction_update`).
    Compaction(Compaction),
    /// The session's config options changed (`config_option_update`): the update as sent, which
    /// [`Modes::from_session`](crate::modes::Modes::from_session) reads.
    ConfigOptions(Value),
    /// The session's mode changed (`current_mode_update`).
    ModeChanged { mode: Option<String> },
    /// The `/` commands the agent offers (`available_commands_update`).
    Commands(Vec<Command>),
    /// Work the agent runs apart from its turn began, moved on or ended (`async_task_*`, Claude's adapter).
    Task(Task),
    /// An update this crate doesn't know: as sent.
    Other(Value),
}

impl Event {
    /// The event an ACP `update` object (`params.update`) carries.
    pub fn parse(u: &Value) -> Self {
        let text = || content_text(&u["content"]);
        match u["sessionUpdate"].as_str().unwrap_or_default() {
            "agent_message_chunk" => {
                Self::Text { text: text(), notice: u["_meta"]["piAcp"]["notify"]["level"].as_str().map(str::to_owned) }
            }
            "agent_thought_chunk" => Self::Thought { text: text() },
            "user_message_chunk" => Self::UserText { text: text() },
            "tool_call" => Self::ToolStarted(Box::new(ToolCall::parse(u))),
            "tool_call_update" => Self::ToolUpdated(Box::new(ToolCall::parse(u))),
            "plan" => Self::Plan { entries: u["entries"].clone() },
            "usage_update" => Self::Usage(Usage {
                used: u["used"].as_u64(),
                size: u["size"].as_u64(),
                claude_rate_limit: Some(&u["_meta"]["_claude/rateLimit"]).filter(|v| v.is_object()).cloned(),
            }),
            "compaction_update" => Self::Compaction(Compaction {
                id: u["compactionId"].as_str().unwrap_or_default().to_owned(),
                done: u["status"] == "completed",
                summary: Some(content_text(&u["summary"])).filter(|s| !s.trim().is_empty()),
            }),
            "config_option_update" => Self::ConfigOptions(u.clone()),
            "current_mode_update" => Self::ModeChanged { mode: u["currentModeId"].as_str().map(str::to_owned) },
            "available_commands_update" => Self::Commands(
                u["availableCommands"].as_array().into_iter().flatten().filter_map(Command::parse).collect(),
            ),
            kind if kind.starts_with("async_task_") => {
                Task::parse(u).map_or_else(|| Self::Other(u.clone()), Self::Task)
            }
            _ => Self::Other(u.clone()),
        }
    }
}

/// A tool call, as started or as changed since.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct ToolCall {
    pub id: String,
    pub title: Option<String>,
    /// ACP's tool kind: `read`, `edit`, `execute`, …
    pub kind: Option<String>,
    /// `pending`, `in_progress`, `completed` or `failed`.
    pub status: Option<String>,
    /// What the call produced, when the update carries it.
    pub content: Option<Vec<ToolContent>>,
    /// The files it works on, as sent.
    pub locations: Option<Value>,
    /// The tool call that launched the subagent making this call (Claude's adapter only).
    pub parent: Option<String>,
    /// What this call, a subagent's launch, says about its subagent (Claude's adapter only).
    pub subagent: Option<Subagent>,
}

impl ToolCall {
    /// A `tool_call`, a `tool_call_update` or a permission request's `toolCall`.
    pub fn parse(u: &Value) -> Self {
        let text = |key: &str| u[key].as_str().map(str::to_owned);
        Self {
            id: u["toolCallId"].as_str().unwrap_or_default().to_owned(),
            title: text("title"),
            kind: text("kind"),
            status: text("status"),
            content: u["content"].as_array().map(|items| items.iter().map(ToolContent::parse).collect()),
            locations: Some(&u["locations"]).filter(|l| !l.is_null()).cloned(),
            parent: claude::parent(u),
            subagent: claude::subagent(u),
        }
    }
}

/// One item of a tool call's content.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ToolContent {
    /// Text it produced (empty for a kind with none, an image).
    Text(String),
    /// A file it changed: the text before (empty for a new file) and after.
    Diff { path: String, old_text: String, new_text: String },
    /// A terminal the client runs for it.
    Terminal { id: String },
}

impl ToolContent {
    fn parse(item: &Value) -> Self {
        let text = |key: &str| item[key].as_str().unwrap_or_default().to_owned();
        match item["type"].as_str() {
            Some("diff") => Self::Diff { path: text("path"), old_text: text("oldText"), new_text: text("newText") },
            Some("terminal") => Self::Terminal { id: text("terminalId") },
            _ => Self::Text(content_text(item)),
        }
    }
}

/// A session's context use.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct Usage {
    /// Tokens in the context now.
    pub used: Option<u64>,
    /// The context window.
    pub size: Option<u64>,
    /// Claude's adapter's rate-limit reading (`_meta["_claude/rateLimit"]`, the Agent SDK's `SDKRateLimitInfo`).
    pub claude_rate_limit: Option<Value>,
}

/// A compaction of a session's context, under way or done.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Compaction {
    pub id: String,
    pub done: bool,
    /// The summary the context became, when the agent says it (Claude's adapter, once done).
    pub summary: Option<String>,
}

/// A `/` command the agent offers, as it names it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Command {
    pub name: String,
    pub description: Option<String>,
    /// What to type after the name, for a command that takes input.
    pub hint: Option<String>,
}

impl Command {
    fn parse(c: &Value) -> Option<Self> {
        Some(Self {
            name: c["name"].as_str()?.to_owned(),
            description: c["description"].as_str().map(str::to_owned),
            hint: c["input"]["hint"].as_str().map(str::to_owned),
        })
    }
}

#[cfg(test)]
mod tests;
