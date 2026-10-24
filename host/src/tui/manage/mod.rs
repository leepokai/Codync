//! The settings the other apps keep next to the chat, as TUI sheets: a bot's memory and
//! routines, the marketplace (agents, connectors, skills), agent sign-in with its setup
//! terminal, and the pieces of a message you can act on (reactions, sent files).

// Key handlers read best with k (key), m / l / f / a (the sheet), n (rows).
#![allow(clippy::many_single_char_names)]

mod agent;
mod analytics;
mod bot_chat;
mod fields;
mod library;
mod market;
mod memory;
mod pages;
mod replies;
mod routines;
mod skill;
mod when;

pub use analytics::AnalyticsSheet;
pub use fields::handoff_fields;
pub use library::{ArtifactTitles, LibraryItem};
pub use memory::{MemoryMode, MemorySheet, Scroll, Section};
pub use pages::Page;
pub use skill::SkillSheet;
pub use when::{complete_zone, local_zone};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use std::cell::Cell;

use super::app::{After, App, Editor, Entry};

/// Quick reactions, as the other apps offer them.
pub const REACTIONS: [&str; 6] = ["👍", "❤️", "😂", "🎉", "👀", "✅"];

/// Replies to the sheets' commands.
#[derive(Clone)]
pub enum Reply {
    Memory(String),
    MemorySaved(String),
    /// A note the bot's memory sheet has open.
    MemoryFile(String),
    Routines(String),
    RoutineSaved,
    /// A bot's Library (`library {botId}`).
    Library(String),
    /// A routine's webhook URL and key: copied as a ready-to-run curl.
    Webhook,
    /// A bot conversation's fetched history.
    BotChat {
        bot: String,
        peer: String,
    },
    Registry {
        more: bool,
    },
    Skills,
    /// A skill's instructions, for the details sheet with that id.
    SkillDetails(String),
    Auth(String),
    Setup,
    FieldsSaved,
    SignIn,
    Models(String),
    Downloaded,
    /// Something changed: every open sheet loads again.
    Changed,
}

/// A read-only conversation between `bot` and `peer`, scrolled from the bottom.
pub struct BotChatSheet {
    pub bot: String,
    pub peer: String,
    /// The host's history; `None` while it loads.
    pub fetched: Option<Vec<Entry>>,
    /// Lines up from the bottom.
    pub scroll: usize,
    /// Set by the last draw, for clamping the scroll and paging.
    pub max_scroll: Cell<usize>,
    pub page: Cell<usize>,
}

/// A bot's Library tab: its artifacts and the files shared in its chat, newest first.
pub struct Library {
    pub bot: String,
    pub items: Option<Vec<LibraryItem>>,
    pub cursor: usize,
}

pub struct RoutineList {
    pub bot: String,
    pub items: Option<Vec<Value>>,
    /// The bot's runs, newest first.
    pub runs: Vec<Value>,
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
    /// Some agents run on Node.js and the computer has none: adds Codync's copy.
    Node,
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
    Node,
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
    /// A handoff card's text (the card's entry id).
    Handoff(String),
    RenameArtifact {
        bot: String,
        artifact: String,
    },
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

    /// `f` while a download runs: stops it.
    fn cancel_download(&mut self) -> bool {
        let running = self.download.take().is_some();
        if running {
            self.flash("Download cancelled · f retries");
        }
        running
    }

    /// Saves a message's files, or its artifact, to Downloads.
    pub(super) fn save_files(&mut self, bot: &str, entry: &str, data: &Value) {
        if self.cancel_download() {
            return;
        }
        if let Some(artifact) = data["artifact"]["id"].as_str() {
            self.fetch_artifact(bot, artifact, super::net::artifacts::ArtifactTo::Downloads);
            return;
        }
        if let Some(meta) = data["files"].as_array().and_then(|files| files.first()) {
            match serde_json::from_value(meta.clone()) {
                Ok(file) => self.download_shared(entry.to_owned(), file),
                Err(error) => self.flash(&format!("Invalid file: {error}")),
            }
            return;
        }
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
        self.save_uploads(bot, files);
    }

    /// Saves a file the bot shared (`readFile`), with progress; `f` again cancels.
    fn download_shared(&mut self, entry: String, file: crate::chat::files::SharedFile) {
        self.download = Some(self.client.download_file(entry, file, self.tx.clone()));
        self.flash("Downloading… · f cancels");
    }

    /// Saves files the user attached (`(upload id, name)`, `readUpload`).
    fn save_uploads(&mut self, bot: &str, files: Vec<(String, String)>) {
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
        ids.extend(list.iter().map(|m| m.id.clone()));
        let i = ids.iter().position(|x| x == current).unwrap_or(0);
        Some(ids[super::app::cycle(i, d, ids.len())].clone())
    }

    /// The settings (effort, fast…) `backend`'s `model` offers; on Default (`""`), those of the
    /// model the agent runs by default.
    pub(crate) fn model_settings(&self, backend: &str, model: &str) -> &[super::app::ModelOption] {
        let list = self.models.get(backend).and_then(Option::as_ref);
        let chosen = |m: &&super::app::AgentModel| if model.is_empty() { m.current } else { m.id == model };
        list.and_then(|l| l.iter().find(chosen)).map_or(&[], |m| &m.options)
    }
}

/// Opens `url` in this computer's browser, when the host is this computer.
pub(super) fn open_browser(url: &str, host: &str) -> bool {
    let local = ["//127.0.0.1", "//localhost", "//[::1]"].iter().any(|h| host.contains(h));
    local && !url.is_empty() && open_here(std::ffi::OsStr::new(url))
}

/// Opens a link or file with this computer's default app for it (a web page: the browser).
pub(in crate::tui) fn open_here(target: &std::ffi::OsStr) -> bool {
    let mut command = if cfg!(windows) {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    } else {
        std::process::Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" })
    };
    command.arg(target).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().is_ok()
}
