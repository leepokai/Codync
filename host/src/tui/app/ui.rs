//! The screen's state: panes, overlays and their forms, and where clicks land.

use ratatui::layout::Rect;
use std::collections::BTreeMap;
use std::time::Instant;

use super::super::manage::{
    AgentSetup, AnalyticsSheet, BotChatSheet, Fields, Gear, Library, Market, MemorySheet, NetworkSheet, RoutineForm,
    RoutineList, SkillSheet,
};
use super::{Editor, Mark};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Roster,
    Chat,
    Trace,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TraceMode {
    Off,
    Pane,
    Full,
}

/// Width class, set by the last draw.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Width {
    /// < 80 columns: roster and chat are separate pages.
    Narrow,
    /// 80–99: compact roster.
    Mid,
    /// 100–149.
    Wide,
    /// ≥ 150.
    Huge,
}

pub const FILTERS: [(&str, Option<Mark>); 5] = [
    ("all", None),
    ("needs you", Some(Mark::Need)),
    ("working", Some(Mark::Work)),
    ("unread", Some(Mark::Unread)),
    ("error", Some(Mark::Error)),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    NewBot,
    NewGroup,
    Market,
    Memory,
    Details,
    Library,
    RefreshAgents,
    Pair,
    Network,
    Usage,
    CheckUpdate,
    Analytics,
    AboutAnalytics,
    Language,
    ThinkingDetails,
    Keys,
    Appearance(super::super::appearance::Appearance),
}

pub const ACTIONS: [(&str, &str, Action); 19] = [
    ("New bot…", "n", Action::NewBot),
    ("New group chat…", "m", Action::NewGroup),
    ("Extensions: agents, connectors, skills…", "A", Action::Market),
    ("Memory of this bot…", "M", Action::Memory),
    ("Details of this bot…", "R", Action::Details),
    ("Library of this bot: artifacts and files…", "L", Action::Library),
    ("Refresh agents", "", Action::RefreshAgents),
    ("Pair a phone…", "P", Action::Pair),
    ("Network: your other computers…", "N", Action::Network),
    ("Usage", "U", Action::Usage),
    ("Check for updates", "", Action::CheckUpdate),
    ("Share usage analytics", "", Action::Analytics),
    ("About usage analytics…", "", Action::AboutAnalytics),
    ("Appearance: System", "", Action::Appearance(super::super::appearance::Appearance::System)),
    ("Appearance: Light", "", Action::Appearance(super::super::appearance::Appearance::Light)),
    ("Appearance: Dark", "", Action::Appearance(super::super::appearance::Appearance::Dark)),
    ("App language…", "", Action::Language),
    ("Show thinking details", "", Action::ThinkingDetails),
    ("Keys", "?", Action::Keys),
];

pub enum GotoItem {
    Bot(String),
    Action(Action),
    /// Shows a hidden bot in the roster again.
    Unhide(String),
}

pub struct Goto {
    pub query: Editor,
    pub filter: usize,
    pub cursor: usize,
}

pub enum ConfirmAct {
    Delete(String),
    /// Keeps a bot's harness's own memory on beside Codync's.
    KeepHarnessMemory(String),
    DeleteRoutine(String, String),
    RotateRoutineKey(String, String),
    RemoveConnector(String),
    RemoveSkill(String),
}

pub struct Confirm {
    pub title: String,
    pub detail: String,
    pub note: String,
    pub button: &'static str,
    pub act: ConfirmAct,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NewBotField {
    Name,
    Color,
    Shape,
}

/// A new bot: a name (optional: the host names it after its first conversations) and its looks.
/// Everything else takes the defaults and is changed afterwards in Settings.
pub struct NewBot {
    pub name: Editor,
    pub color: usize,
    pub shape: usize,
    pub field: NewBotField,
    pub error: Option<String>,
    pub saving: bool,
}

#[derive(Clone)]
pub struct Dir {
    pub name: String,
    pub path: String,
    pub git: bool,
}

#[derive(PartialEq, Eq)]
pub enum FolderMode {
    Recent,
    Browse,
    Path,
}

pub struct FolderPicker {
    pub request_id: String,
    pub mode: FolderMode,
    pub recent: Vec<Dir>,
    pub remembers: bool,
    pub busy: bool,
    pub path: String,
    pub parent: Option<String>,
    pub dirs: Vec<Dir>,
    pub query: Editor,
    pub cursor: usize,
    pub error: Option<String>,
}

pub use crate::store::{AVATAR_COLORS as COLORS, AVATAR_SHAPES as SHAPES};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    Name,
    Instructions,
    Agent,
    Model,
    ModelOptions,
    Fast,
    Fallbacks,
    SelfFallbacks,
    Folder,
    Approvals,
    Notify,
    Computer,
    Team,
    Network,
    Assistants,
    Color,
    Shape,
    Connectors,
    Skills,
}

pub const FIELDS: [Field; 19] = [
    Field::Name,
    Field::Instructions,
    Field::Agent,
    Field::Model,
    Field::ModelOptions,
    Field::Fast,
    Field::Fallbacks,
    Field::SelfFallbacks,
    Field::Folder,
    Field::Approvals,
    Field::Notify,
    Field::Computer,
    Field::Team,
    Field::Network,
    Field::Assistants,
    Field::Color,
    Field::Shape,
    Field::Connectors,
    Field::Skills,
];

pub struct Toggle {
    pub id: String,
    pub name: String,
    pub on: bool,
}

#[allow(clippy::struct_excessive_bools, reason = "independent bot settings and save state")]
pub struct Form {
    pub bot_id: String,
    pub name: Editor,
    pub instructions: Editor,
    pub backend: String,
    pub model: Editor,
    /// Model settings (effort, fast…) and the model they were picked for.
    pub model_options: (String, BTreeMap<String, String>),
    pub fast: bool,
    /// Agents that carry on, in order, when the bot's own can't answer.
    pub fallbacks: Vec<crate::store::Fallback>,
    pub self_fallbacks: bool,
    pub cwd: String,
    /// Approvals: Codync approves what still asks, and the agent mode (`agent::modes`).
    pub auto: bool,
    pub mode: Option<String>,
    pub notify: bool,
    pub computer: bool,
    /// `None`: the host has no Manage the team.
    pub manage_team: Option<bool>,
    /// `None`: the host has no per-bot network switches.
    pub offered_to_network: Option<bool>,
    pub reachable_by_assistants: Option<bool>,
    pub color: usize,
    pub shape: usize,
    pub connectors: Vec<Toggle>,
    pub skills: Vec<Toggle>,
    pub field: usize,
    /// Cursor inside the connector / skill toggles, the model options or the fallbacks.
    pub sub: usize,
    pub error: Option<String>,
    pub saving: bool,
    /// The settings as opened, to tell edits apart.
    pub opened: serde_json::Value,
}

impl Form {
    pub fn current(&self) -> Field {
        FIELDS[self.field]
    }

    /// The settings picked for the model now in the field; another model has none yet.
    pub fn model_options(&self) -> Option<&BTreeMap<String, String>> {
        (self.model_options.0 == self.model.text.trim()).then_some(&self.model_options.1)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GroupField {
    Name,
    About,
    Bots,
}

impl GroupField {
    /// The field after this one; a new group has no About (it is written afterwards, in the editor).
    pub(super) fn next(self, about: bool) -> Self {
        match self {
            Self::Name if about => Self::About,
            Self::Name | Self::About => Self::Bots,
            Self::Bots => Self::Name,
        }
    }

    pub(super) fn prev(self, about: bool) -> Self {
        match self {
            Self::Name => Self::Bots,
            Self::Bots if about => Self::About,
            Self::About | Self::Bots => Self::Name,
        }
    }
}

/// Create a group chat, or rename one and change who's in it.
pub struct GroupForm {
    /// The group being edited; `None` creates one.
    pub group_id: Option<String>,
    pub name: Editor,
    /// What the group is for; sent as `description`, shown to its members. Edited only:
    /// a new group starts without one.
    pub about: Editor,
    /// Picked bot ids, in the order they were picked.
    pub members: Vec<String>,
    pub field: GroupField,
    pub cursor: usize,
    pub error: Option<String>,
    pub saving: bool,
}

/// The pair-a-phone sheet's code: on its way, shown (its pairing link), or why none came.
pub enum PairCode {
    Loading,
    Ready(String),
    Failed(String),
}

pub enum Overlay {
    Goto(Goto),
    Help(Editor),
    LeadAgent(usize),
    Confirm(Confirm),
    NewBot(NewBot),
    Folder(FolderPicker),
    Form(Box<Form>),
    Group(GroupForm),
    /// How far its list is scrolled.
    Usage(usize),
    Pair(PairCode),
    Memory(MemorySheet),
    /// A read-only conversation between two bots.
    BotChat(BotChatSheet),
    Routines(RoutineList),
    Routine(Box<RoutineForm>),
    Library(Library),
    Gear(Gear),
    Market(Box<Market>),
    Agent(AgentSetup),
    Fields(Box<Fields>),
    Skill(SkillSheet),
    Analytics(AnalyticsSheet),
    Network(Box<NetworkSheet>),
}

pub struct Toast {
    pub text: String,
    pub need: bool,
    pub until: Instant,
}

#[derive(Clone)]
pub enum Click {
    LeadAgent,
    Bot(String),
    Option {
        entry: String,
        option: String,
    },
    Filter(usize),
    Composer,
    /// The working line: unfolds or folds the turn's steps.
    Steps,
    /// A bot-message row (entry id): opens the conversation.
    BotChat(String),
    /// A section's label (its name): folds or unfolds it, like `z`.
    Section(String),
}

#[derive(Default)]
pub struct Hits {
    pub roster: Rect,
    pub chat: Rect,
    pub trace: Rect,
    pub clicks: Vec<(Rect, Click)>,
}
