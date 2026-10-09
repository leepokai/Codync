//! The settings the other apps keep next to the chat, as TUI sheets: a bot's memory and
//! routines, the marketplace (agents, connectors, skills), agent sign-in with its setup
//! terminal, and the pieces of a message you can act on (reactions, sent files).

// Key handlers read best with k (key), m / l / f / a (the sheet), n (rows).
#![allow(clippy::many_single_char_names)]

mod agent;
mod fields;
mod market;
mod memory;
mod replies;
mod routines;

pub use memory::{MemoryMode, MemorySheet};
pub use routines::local_zone;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use super::app::{After, App, Editor};

/// Quick reactions, as the other apps offer them.
pub const REACTIONS: [&str; 6] = ["👍", "❤️", "😂", "🎉", "👀", "✅"];

/// Replies to the sheets' commands.
#[derive(Clone)]
pub enum Reply {
    Memory(String),
    MemorySaved(String),
    MemoryDetail(String),
    MemoryExport(String),
    Routines(String),
    RoutineSaved,
    /// A routine's webhook URL and key: copied as a ready-to-run curl.
    Webhook,
    Registry {
        more: bool,
    },
    Skills,
    Auth(String),
    Setup,
    FieldsSaved,
    SignIn,
    Models(String),
    Downloaded,
    /// Something changed: every open sheet loads again.
    Changed,
}

pub struct RoutineList {
    pub bot: String,
    pub items: Option<Vec<Value>>,
    pub cursor: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RoutineField {
    Name,
    Instruction,
    When,
}

pub struct RoutineForm {
    pub bot: String,
    pub id: Option<String>,
    pub name: Editor,
    pub instruction: Editor,
    pub when: Editor,
    /// What an edited routine runs on; kept when `when` stays empty.
    pub original: Value,
    pub original_text: String,
    pub field: RoutineField,
    pub error: Option<String>,
    pub saving: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Agents,
    Connectors,
    Skills,
}

pub const TABS: [(Tab, &str); 3] = [(Tab::Agents, "Agents"), (Tab::Connectors, "Connectors"), (Tab::Skills, "Skills")];

pub struct Market {
    pub tab: Tab,
    pub query: Editor,
    /// The registry search the connector list shows; `None` before the first page.
    pub searched: Option<String>,
    pub registry: Vec<Value>,
    pub next: Option<String>,
    pub skills: Option<Vec<Value>>,
    pub loading: bool,
    pub cursor: usize,
    pub error: Option<String>,
}

pub enum Row<'a> {
    Agent(&'a Value),
    Installed(&'a Value),
    Custom,
    Registry(&'a Value),
    More,
    OwnSkill(&'a Value),
    Skill(&'a Value),
}

pub struct AgentSetup {
    pub backend: String,
    pub auth: Option<Value>,
    /// A check or a browser sign-in is running.
    pub busy: bool,
    pub cursor: usize,
    pub error: Option<String>,
}

pub enum SetupRow {
    Install,
    Login,
    Method(Value),
    Check,
}

pub struct Input {
    pub name: String,
    pub label: String,
    pub secret: bool,
    pub required: bool,
    pub multiline: bool,
    pub placeholder: String,
    pub ed: Editor,
}

pub enum Submit {
    AgentEnv(String),
    Connection(Box<super::connections::Connection>),
    Connector(Value),
    Import,
}

/// A few labeled text fields: an agent's keys, a connector's setup, a pasted MCP config.
pub struct Fields {
    pub title: String,
    pub note: String,
    /// A connector's ways to run (row 0 when there are several).
    pub choices: Vec<String>,
    pub choice: usize,
    pub inputs: Vec<Input>,
    pub cursor: usize,
    pub error: Option<String>,
    pub saving: bool,
    pub submit: Submit,
}

impl Fields {
    fn offset(&self) -> usize {
        usize::from(self.choices.len() > 1)
    }

    pub fn current(&mut self) -> Option<&mut Input> {
        let i = self.cursor.checked_sub(self.offset())?;
        self.inputs.get_mut(i)
    }
}

/// The running setup terminal, which takes over the whole screen.
pub struct Shell {
    pub id: String,
    /// Keys in the order typed (one sender, one request at a time).
    pub input: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    pub backend: String,
    pub exited: Option<i64>,
    /// Output not yet written to the screen.
    pub out: Vec<u8>,
}

fn s(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or_default().to_owned()
}

fn ctrl(k: KeyEvent) -> bool {
    k.modifiers.contains(KeyModifiers::CONTROL)
}

fn newline(k: KeyEvent) -> bool {
    k.code == KeyCode::Enter && k.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::ALT)
        || k.code == KeyCode::Char('j') && ctrl(k)
}

impl App {
    fn sheet(&self, method: &'static str, body: Value, r: Reply) {
        self.call(method, body, After::Sheet(r));
    }

    // ---------- a message's actions ----------

    pub(super) fn react(&mut self, entry: &str, i: usize) {
        if let Some(e) = REACTIONS.get(i) {
            self.call("react", json!({"entryId": entry, "emoji": e}), After::Nothing);
        }
    }

    /// Saves a message's files to Downloads.
    pub(super) fn save_files(&mut self, bot: &str, data: &Value) {
        let files: Vec<(String, String)> = data["attachments"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| Some((a["id"].as_str()?.to_owned(), a["name"].as_str()?.to_owned())))
            .collect();
        if files.is_empty() {
            self.flash("No files on that message");
            return;
        }
        self.flash("Saving…");
        self.client.spawn_download(bot.to_owned(), files, self.tx.clone());
    }

    // ---------- model picker ----------

    /// Loads the models `backend` offers, once per run.
    pub(super) fn want_models(&mut self, backend: &str) {
        if !backend.is_empty() && !self.models.contains_key(backend) {
            self.models.insert(backend.to_owned(), None);
            self.sheet("agentModels", json!({"backend": backend}), Reply::Models(backend.to_owned()));
        }
    }

    /// ←→ on the model field: "default", then each advertised model.
    pub(super) fn cycle_model(&self, backend: &str, current: &str, d: isize) -> Option<String> {
        let list = self.models.get(backend)?.as_ref()?;
        let mut ids = vec![String::new()];
        ids.extend(list.iter().map(|(id, _)| id.clone()));
        let i = ids.iter().position(|x| x == current).unwrap_or(0);
        Some(ids[super::app::cycle(i, d, ids.len())].clone())
    }
}

/// Opens `url` in this computer's browser, when the host is this computer.
pub(super) fn open_browser(url: &str, host: &str) -> bool {
    let local = ["//127.0.0.1", "//localhost", "//[::1]"].iter().any(|h| host.contains(h));
    if !local || url.is_empty() {
        return false;
    }
    let mut command = if cfg!(windows) {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    } else {
        std::process::Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" })
    };
    command.arg(url).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().is_ok()
}
