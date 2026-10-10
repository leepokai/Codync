//! Row and wire types: bot configs, lanes, entries and their kinds.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How a bot handles the agent's permission requests (wire values `ask` / `auto`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    /// Every request becomes an approval card on the phone.
    Ask,
    /// Requests are approved once, automatically. New bots start here.
    #[default]
    Auto,
}

/// Transcript entry kinds (the `kind` column / wire field).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    User,
    Agent,
    Thought,
    Tool,
    Plan,
    Permission,
    Notice,
}

impl EntryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Agent => "agent",
            Self::Thought => "thought",
            Self::Tool => "tool",
            Self::Plan => "plan",
            Self::Permission => "permission",
            Self::Notice => "notice",
        }
    }
}

/// A roster row is an agent bot or a group chat of bots (wire values `agent` / `group`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BotKind {
    #[default]
    Agent,
    /// Several bots and the user in one conversation; it has no agent of its own (see `group`).
    Group,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[expect(clippy::struct_excessive_bools, reason = "independent user settings stored as plain JSON flags")]
pub struct BotConfig {
    pub id: String,
    #[serde(default)]
    pub kind: BotKind,
    /// A group's bots, in the order they were added.
    #[serde(default)]
    pub members: Vec<String>,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_color")]
    pub avatar_color: String,
    /// Grok-Bot-style character shape: blob | pebble | squircle | tablet | wedge | hex | cloud | teardrop.
    #[serde(default = "default_shape")]
    pub avatar_shape: String,
    /// Empty for a group.
    #[serde(default)]
    pub backend: String,
    #[serde(default)]
    pub command: Option<String>,
    /// Empty for a group.
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub permission: Permission,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    /// Place in the roster, lowest first (after pinned bots). All 0 until the user drags one:
    /// ties sort by recent activity (see `Hub::reorder_bots`).
    #[serde(default)]
    pub position: i64,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub notify: Option<bool>,
    /// Connector ids (MCP servers) handed to the agent; see `market`.
    #[serde(default)]
    pub connectors: Vec<String>,
    /// Skill ids the bot is told about; see `market`.
    #[serde(default)]
    pub skills: Vec<String>,
    /// The bot gets the built-in `computer` MCP server (see `screen`).
    #[serde(default)]
    pub computer: bool,
    /// Created without a name: the host names it from its first conversations (see `naming`)
    /// until the user renames it.
    #[serde(default)]
    pub auto_name: bool,
    #[serde(default)]
    pub created_at: i64,
}

impl BotConfig {
    pub fn is_group(&self) -> bool {
        self.kind == BotKind::Group
    }
}

fn default_color() -> String {
    "blue".into()
}
fn default_shape() -> String {
    "blob".into()
}

/// kv key for per-bot state in one lane (`what`: `session`, `seen`).
pub fn lane_key(what: &str, bot_id: &str, lane: &Lane) -> String {
    format!("lane.{what}.{bot_id}@{}", lane.key())
}

/// What the user has read (`markRead`): the main chat, one thread, or all of a chat.
#[derive(Clone, Copy, Debug)]
pub enum ReadScope<'a> {
    All,
    Chat,
    Thread(&'a str),
}

/// Where an entry lives: a bot's or group's chat (`bot_id`), and optionally a thread in it,
/// named by its root entry. Every lane a bot talks in has its own ACP session.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Lane {
    pub chat: String,
    pub thread: Option<String>,
}

impl Lane {
    pub fn main(chat: &str) -> Self {
        Self { chat: chat.to_owned(), thread: None }
    }

    pub fn in_thread(chat: &str, root: &str) -> Self {
        Self { chat: chat.to_owned(), thread: Some(root.to_owned()) }
    }

    /// Stable text form for kv keys: `chat` or `chat/root`.
    pub fn key(&self) -> String {
        match &self.thread {
            Some(t) => format!("{}/{t}", self.chat),
            None => self.chat.clone(),
        }
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub seq: i64,
    pub bot_id: String,
    /// The thread's root entry; `None` in the main chat.
    pub thread_id: Option<String>,
    pub rev: i64,
    pub kind: String,
    pub turn: i64,
    pub data: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct LastMessage {
    pub text: String,
    pub at: i64,
    /// The bot that wrote it; `None` when the user did.
    pub author: Option<String>,
}

pub struct BotRow {
    pub config: BotConfig,
    pub rev: i64,
    pub deleted: bool,
    pub session_id: Option<String>,
    pub read_rev: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_wire_values_round_trip() {
        let p: Permission = serde_json::from_str("\"auto\"").unwrap();
        assert_eq!(p, Permission::Auto);
        assert_eq!(serde_json::to_string(&Permission::Ask).unwrap(), "\"ask\"");
        assert!(serde_json::from_str::<Permission>("\"yolo\"").is_err(), "unknown modes are rejected at the boundary");
    }
}
