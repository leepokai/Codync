//! Row and wire types: bot configs, lanes, entries and their kinds.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

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
    /// The model's own settings (effort, fast…): ACP config option id → value.
    #[serde(default)]
    pub model_options: BTreeMap<String, String>,
    /// Ask the agent for its Fast mode (Claude Code, Codex): faster replies for more usage.
    #[serde(default)]
    pub fast_mode: bool,
    /// Agents that carry on, in order, when the bot's own can't answer (see `agent::fallback`).
    #[serde(default)]
    pub fallbacks: Vec<Fallback>,
    /// The bot may set its own fallbacks (`chat` server's `set_fallbacks`). On unless the user turns it off.
    #[serde(default = "yes")]
    pub self_fallbacks: bool,
    #[serde(default)]
    pub pinned: bool,
    /// The computer's lead bot, which the host makes (see `chat::lead`): first in the roster, never deleted or hidden.
    #[serde(default)]
    pub lead: bool,
    /// Place in the roster, lowest first (after the lead and pinned bots). All 0 until the user drags one:
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
    /// The bot gets the built-in `manage` MCP server: it creates bots and changes any bot's setup
    /// (see `chat::manage`). The user turns it on; the bots such a bot creates have it too.
    #[serde(default)]
    pub manage_team: bool,
    /// Other computers in the user's network list this bot and may message it (`network::publish`).
    #[serde(default = "yes")]
    pub offered_to_network: bool,
    /// The user's cloud assistant lists this bot and may message it (`chat::team::assistant`).
    #[serde(default = "yes")]
    pub reachable_by_assistants: bool,
    /// The bot that created this one with `create_bot`; none when the user did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// Uses the memory shared by all bots ("yours"). Unset: on for a bot in its own workspace, off for one in a
    /// project folder (see `chat::memory::shared`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_memory: Option<bool>,
    /// Keeps the harness's own memory (Claude Code's auto memory…) on beside Codync's. Off unless the user turns it
    /// on, and only where Codync can turn it off (see `agent::own_memory::kept`). A client sending back the bot as
    /// it got it sends `null` where there is no switch.
    #[serde(default, deserialize_with = "null_as_false")]
    pub harness_memory: bool,
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

/// One step of a bot's fallback chain: another agent and the model to use on it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Fallback {
    pub backend: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub model_options: BTreeMap<String, String>,
}

fn yes() -> bool {
    true
}

fn null_as_false<'de, D: serde::Deserializer<'de>>(value: D) -> Result<bool, D::Error> {
    Ok(Option::<bool>::deserialize(value)?.unwrap_or(false))
}

/// The avatar colors and shapes every client offers.
pub const AVATAR_COLORS: [&str; 11] =
    ["black", "brown", "red", "orange", "yellow", "green", "cyan", "blue", "violet", "magenta", "gray"];
pub const AVATAR_SHAPES: [&str; 8] = ["blob", "pebble", "squircle", "tablet", "wedge", "hex", "cloud", "teardrop"];

fn default_color() -> String {
    "blue".into()
}
fn default_shape() -> String {
    "blob".into()
}

/// kv key for the agent session of one of a bot's routine runs (`slot`: see `routines::session_slot`).
pub fn run_session_key(bot_id: &str, slot: &str) -> String {
    format!("lane.session.{bot_id}@{bot_id}/{slot}")
}

/// Where entries go and a turn talks: a bot's own chat or a group's (`chat`, a bot id).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Lane {
    pub chat: String,
}

impl Lane {
    pub fn main(chat: &str) -> Self {
        Self { chat: chat.to_owned() }
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub seq: i64,
    pub bot_id: String,
    pub rev: i64,
    pub kind: String,
    pub turn: i64,
    pub data: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A message's text, or the names of its files when it has none.
pub fn message_text(data: &Value) -> String {
    match data["text"].as_str().unwrap_or_default() {
        "" => {
            let names: Vec<&str> =
                data["attachments"].as_array().into_iter().flatten().filter_map(|a| a["name"].as_str()).collect();
            names.join(", ")
        }
        text => text.to_owned(),
    }
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
