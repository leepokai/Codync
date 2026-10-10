//! The screen's state: panes, overlays and their forms, and where clicks land.

use ratatui::layout::Rect;
use std::time::Instant;

use super::super::manage::{AgentSetup, BotChatSheet, Fields, Market, MemorySheet, RoutineForm, RoutineList};
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
    Routines,
    RefreshAgents,
    Pair,
    Usage,
    CheckUpdate,
    Analytics,
    Keys,
}

pub const NEW_GROUP: &str = "New group chat";

pub const ACTIONS: [(&str, &str, Action); 11] = [
    ("New bot…", "n", Action::NewBot),
    ("New group chat…", "m", Action::NewGroup),
    ("Marketplace: agents, connectors, skills…", "A", Action::Market),
    ("Memory of this bot…", "M", Action::Memory),
    ("Routines of this bot…", "R", Action::Routines),
    ("Refresh agents", "", Action::RefreshAgents),
    ("Pair a phone…", "P", Action::Pair),
    ("Usage", "U", Action::Usage),
    ("Check for updates", "", Action::CheckUpdate),
    ("Share usage analytics", "", Action::Analytics),
    ("Keys", "?", Action::Keys),
];

pub enum GotoItem {
    Bot(String),
    Action(Action),
}

pub struct Goto {
    pub query: Editor,
    pub filter: usize,
    pub cursor: usize,
}

pub enum ConfirmAct {
    Delete(String),
    NewSession(String),
    ClearMemory(String),
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

pub struct AgentPicker {
    pub query: Editor,
    pub cursor: usize,
}

#[derive(Clone)]
pub struct Dir {
    pub name: String,
    pub path: String,
    pub git: bool,
}

pub struct FolderPicker {
    pub path: String,
    pub parent: Option<String>,
    pub dirs: Vec<Dir>,
    pub query: Editor,
    pub cursor: usize,
    pub error: Option<String>,
}

pub const COLORS: [&str; 11] =
    ["black", "brown", "red", "orange", "yellow", "green", "cyan", "blue", "violet", "magenta", "gray"];
pub const SHAPES: [&str; 8] = ["blob", "pebble", "squircle", "tablet", "wedge", "hex", "cloud", "teardrop"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    Name,
    Instructions,
    Agent,
    Model,
    Folder,
    Approvals,
    Notify,
    Computer,
    Color,
    Shape,
    Connectors,
    Skills,
}

pub const FIELDS: [Field; 12] = [
    Field::Name,
    Field::Instructions,
    Field::Agent,
    Field::Model,
    Field::Folder,
    Field::Approvals,
    Field::Notify,
    Field::Computer,
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
    pub bot_id: Option<String>,
    pub name: Editor,
    pub instructions: Editor,
    pub backend: String,
    pub model: Editor,
    pub cwd: String,
    pub auto: bool,
    pub notify: bool,
    pub computer: bool,
    pub color: usize,
    pub shape: usize,
    pub connectors: Vec<Toggle>,
    pub skills: Vec<Toggle>,
    pub field: usize,
    /// Cursor inside the connector / skill toggles.
    pub sub: usize,
    pub error: Option<String>,
    pub saving: bool,
}

impl Form {
    pub fn current(&self) -> Field {
        FIELDS[self.field]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GroupField {
    Name,
    About,
    Bots,
}

impl GroupField {
    pub(super) fn next(self) -> Self {
        match self {
            Self::Name => Self::About,
            Self::About => Self::Bots,
            Self::Bots => Self::Name,
        }
    }

    pub(super) fn prev(self) -> Self {
        self.next().next()
    }
}

/// Create a group chat, or rename one and change who's in it.
pub struct GroupForm {
    /// The group being edited; `None` creates one.
    pub group_id: Option<String>,
    pub name: Editor,
    /// What the group is for; sent as `description`, shown to its members.
    pub about: Editor,
    /// Picked bot ids, in the order they were picked.
    pub members: Vec<String>,
    pub field: GroupField,
    pub cursor: usize,
    pub error: Option<String>,
    pub saving: bool,
}

pub enum Overlay {
    Goto(Goto),
    Help(Editor),
    Confirm(Confirm),
    Agents(AgentPicker),
    Folder(FolderPicker),
    Form(Box<Form>),
    Group(GroupForm),
    Usage,
    Pair(Option<String>),
    Memory(MemorySheet),
    /// A read-only conversation between two bots.
    BotChat(BotChatSheet),
    Routines(RoutineList),
    Routine(Box<RoutineForm>),
    Market(Box<Market>),
    Agent(AgentSetup),
    Fields(Box<Fields>),
    /// The one-time analytics question; `true` highlights Share usage.
    Consent(bool),
}

pub struct Toast {
    pub text: String,
    pub need: bool,
    pub until: Instant,
}

#[derive(Clone)]
pub enum Click {
    Bot(String),
    Option {
        entry: String,
        option: String,
    },
    Filter(usize),
    Composer,
    /// The replies line under a message: opens its thread.
    Thread(String),
    /// A bot-message row (entry id): opens the conversation.
    BotChat(String),
}

#[derive(Default)]
pub struct Hits {
    pub roster: Rect,
    pub chat: Rect,
    pub trace: Rect,
    pub clicks: Vec<(Rect, Click)>,
}
