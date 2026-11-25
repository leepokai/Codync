//! Row and wire types: bot configs, lanes, entries and their kinds.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// How a bot handles the permission requests that reach Codync (wire values `ask` / `auto`). Which ones reach it is
/// the agent's mode (`BotConfig::mode`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    /// Every request becomes an approval card on the phone. New bots start here, in the agent's Auto mode.
    #[default]
    Ask,
    /// Requests are approved once, automatically.
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
    /// The login profile it runs on (`agent::login_profiles`). None: the default one, the login this computer
    /// already has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_profile: Option<String>,
    /// The bot's own folder (`agent::workspace::path`), always; empty for a group. A thread's: the folder it works
    /// in (a project, a worktree of one, or its bot's own folder).
    #[serde(default)]
    pub cwd: String,
    /// The user's folders the bot sends work to (`hub::projects`): absolute paths, no two alike, the first its
    /// default. Never a thread's or a group's.
    #[serde(default)]
    pub projects: Vec<String>,
    /// Per project (a path in `projects`): the ignored folders of its main checkout, relative to it, that each new
    /// worktree of it links to (`node_modules`), so a thread there doesn't fill them again (`chat::work::fill`).
    #[serde(default)]
    pub shared_folders: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub permission: Permission,
    /// The agent mode to work in (see `codync_agent::modes`): its own id, or `auto` for its Auto mode under any id (an agent
    /// without one keeps its own). None: the mode its sessions open in, the agent's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
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
    /// Advisor mode: the agent the bot's threads run on when it isn't the chat's own (`agent::fallback::threads`);
    /// none: the chat's, with the chat's fallbacks.
    #[serde(default)]
    pub thread_agent: Option<Fallback>,
    /// In Advisor mode, the agents that carry on, in order, when the threads' own can't answer.
    #[serde(default)]
    pub thread_fallbacks: Vec<Fallback>,
    /// The bot may set its own fallbacks (`chat` server's `set_fallbacks`). On unless the user turns it off.
    #[serde(default = "yes")]
    pub self_fallbacks: bool,
    #[serde(default)]
    pub pinned: bool,
    /// The computer's lead bot (see `chat::lead`): first in the roster. The host makes the first; the user may pick
    /// another (`setLead`), never a client's or a bot's `createBot` / `updateBot`.
    #[serde(default)]
    pub lead: bool,
    /// Place in the roster, lowest first (after the lead and pinned bots). All 0 until the user drags one:
    /// ties sort by recent activity (see `Hub::reorder_bots`).
    #[serde(default)]
    pub position: i64,
    #[serde(default)]
    pub hidden: bool,
    /// The computer's box for threads without a bot (`chat::work::plain`): they run an agent on its own, told
    /// nothing of Codync's (no instructions, memory, tools or connectors). Always hidden; apps that know it keep it
    /// out of every list of bots.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub plain: bool,
    /// The workflow a thread was started to build (`Setup::workflow`): its workflow tools read, save and run that
    /// one only, and the thread gets them even without a bot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow: Option<String>,
    /// Paused by the user (or a managing bot): it answers nothing and spends nothing until it is resumed. New
    /// messages are refused with a notice, queued routine runs are cancelled, and no turn of its starts.
    #[serde(default)]
    pub paused: bool,
    /// The roster section it shows in; none when unset. Trimmed and checked (`hub::sections`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
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
    /// When Codync summarizes the bot's chat ahead of time (see `agent::bot::compaction`); only for an agent it can
    /// ask to, and the apps show it only there. `null` and values from a newer app read as the default.
    #[serde(default)]
    pub compaction: codync_mem::compaction::Threshold,
    /// Created without a name: the host names it from its first conversations (see `naming`)
    /// until the user renames it.
    #[serde(default)]
    pub auto_name: bool,
    /// Set on a thread bot: the bot whose work it does away from the chat (see `chat::work`). It has its own actor,
    /// agent process, session and transcript; clients never see it as a bot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_of: Option<String>,
    /// The most threads that work on the bot's box at once (see `chat::work`): 1 to [`MAX_THREADS`].
    #[serde(default = "default_max_threads")]
    pub max_threads: u8,
    /// When a thread of the bot's own work or the user's gets a git worktree of its own (see
    /// `chat::work::worktree`). Only matters where the thread's folder is in a repository.
    #[serde(default)]
    pub worktree_mode: WorktreeMode,
    #[serde(default)]
    pub created_at: i64,
}

/// When a thread gets a git worktree (`worktreeMode`; older apps read and send `threadWorktrees`, a bool).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WorktreeMode {
    /// Never: every thread works in its folder.
    Off,
    /// When another thread of the bot works in the same checkout.
    #[default]
    SideBySide,
    /// Every thread of the bot's own work or the user's, the first included.
    Always,
}

impl WorktreeMode {
    /// `threadWorktrees` on the wire: on unless off.
    pub fn as_bool(self) -> bool {
        self != Self::Off
    }

    /// The mode an older app means by `threadWorktrees` (`on`) over the saved `self`: on keeps Always, off is off.
    #[must_use]
    pub fn with_switch(self, on: bool) -> Self {
        match (on, self) {
            (false, _) => Self::Off,
            (true, Self::Off) => Self::SideBySide,
            (true, mode) => mode,
        }
    }
}

/// The most threads a bot may be set to run at once.
pub const MAX_THREADS: u8 = 6;
/// The threads a bot runs at once unless the user changed it.
pub const DEFAULT_THREADS: u8 = 3;

fn default_max_threads() -> u8 {
    DEFAULT_THREADS
}

/// A thread bot's id starts with this; a bot's and a group's are plain UUIDs.
pub const THREAD_PREFIX: &str = "thread-";

/// A thread bot started before this release keeps the id it had: its transcript, session and cards are keyed by it.
const OLD_THREAD_PREFIX: &str = "task-";

/// SQL for "`column` is a thread bot's id", for the queries that keep thread bots and their transcripts apart.
pub(super) fn thread_ids(column: &str) -> String {
    format!("({column} GLOB '{THREAD_PREFIX}*' OR {column} GLOB '{OLD_THREAD_PREFIX}*')")
}

/// Whether `id` is a thread bot's (and so a chat no client lists).
pub fn is_thread_id(id: &str) -> bool {
    id.starts_with(THREAD_PREFIX) || id.starts_with(OLD_THREAD_PREFIX)
}

impl BotConfig {
    pub fn is_group(&self) -> bool {
        self.kind == BotKind::Group
    }

    pub fn is_thread(&self) -> bool {
        self.thread_of.is_some()
    }

    /// The bot it is to the user: a thread's bot, else itself.
    pub fn owner(&self) -> &str {
        self.thread_of.as_deref().unwrap_or(&self.id)
    }

    /// This config under its owner's id: what memory and the instruction snapshot key on, so a thread reads and
    /// writes its bot's.
    #[must_use]
    pub fn as_owner(&self) -> Self {
        Self { id: self.owner().to_owned(), thread_of: None, ..self.clone() }
    }

    /// The steps the bot runs besides its own agent: its fallbacks and, in Advisor mode, its threads' agent and theirs.
    pub fn fallback_steps(&self) -> impl Iterator<Item = &Fallback> {
        self.fallbacks.iter().chain(&self.thread_agent).chain(&self.thread_fallbacks)
    }

    /// [`Self::fallback_steps`], to change.
    pub fn fallback_steps_mut(&mut self) -> impl Iterator<Item = &mut Fallback> {
        self.fallbacks.iter_mut().chain(&mut self.thread_agent).chain(&mut self.thread_fallbacks)
    }
}

/// One step of a bot's fallback chain: another agent and the model to use on it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Fallback {
    pub backend: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// The login profile this step runs on (`agent::login_profiles`). None: the default one. Always on the wire,
    /// `null` for none: an edit that leaves the key out keeps the saved one (`hub::patch`).
    #[serde(default)]
    pub login_profile: Option<String>,
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

impl Entry {
    /// A message the chat shows as one: the user's, or an agent's finished reply.
    pub fn is_message(&self) -> bool {
        self.kind == EntryKind::User.as_str() || (self.kind == EntryKind::Agent.as_str() && self.data["final"] == true)
    }
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
    fn a_thread_bot_is_known_by_its_id_new_or_from_before_this_release() {
        assert!(is_thread_id("thread-1") && is_thread_id("task-1") && !is_thread_id("b1"));
        assert_eq!(thread_ids("id"), "(id GLOB 'thread-*' OR id GLOB 'task-*')");
    }

    #[test]
    fn the_old_names_of_a_bots_thread_settings_are_no_longer_read() {
        // Stored bots were renamed once (`schema::migrate_9`); `updateBot` takes only the new names.
        let old = serde_json::json!({"id": "b", "name": "B", "backend": "claude", "taskOf": "b1",
            "maxAgents": 5, "taskWorktrees": false});
        let cfg: BotConfig = serde_json::from_value(old).expect("a bot");
        assert_eq!(
            (cfg.thread_of, cfg.max_threads, cfg.worktree_mode),
            (None, DEFAULT_THREADS, WorktreeMode::SideBySide)
        );
    }

    #[test]
    fn permission_wire_values_round_trip() {
        let p: Permission = serde_json::from_str("\"auto\"").unwrap();
        assert_eq!(p, Permission::Auto);
        assert_eq!(serde_json::to_string(&Permission::Ask).unwrap(), "\"ask\"");
        assert!(serde_json::from_str::<Permission>("\"yolo\"").is_err(), "unknown modes are rejected at the boundary");
    }
}
