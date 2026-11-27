//! The settings the other apps keep next to the chat, as TUI sheets: a bot's memory and
//! routines, the marketplace (agents, connectors, skills), agent sign-in with its setup
//! terminal, and the pieces of a message you can act on (reactions, sent files).

// Key handlers read best with k (key), m / l / f / a (the sheet), n (rows).
#![allow(clippy::many_single_char_names)]

mod agent;
mod all_work;
mod analytics;
mod bot_chat;
mod computer;
mod dev_servers;
mod fields;
mod gear;
mod icons;
mod library;
mod login_keys;
mod market;
mod memory;
mod network;
mod network_rules;
mod network_setup;
mod older;
mod pages;
mod replies;
mod routines;
mod scroll;
mod skill;
mod threads;
mod when;
mod work;
mod workflow_edit;
mod workflow_outline;
mod workflow_sync;
mod workflows;

pub use all_work::{AllWork, AllWorkRow};
pub use analytics::AnalyticsSheet;
pub use dev_servers::{DevServer, DevServerSummary, DevServersSheet, LogsView, summary as dev_server_summary};
pub use fields::{handoff_fields, name_fields};
pub use gear::{Gear, GearRow};
pub use icons::icon_fields;
pub use library::{ArtifactTitles, LibraryItem};
pub use login_keys::KeyImport;
pub use memory::{MemoryMode, MemorySheet, Section};
pub use network::{NetReply, NetworkSheet, Recovery, Step};
pub use network_rules::{RuleField, RuleForm, label, level_detail, level_name};
pub use network_setup::{CONNECTORS, Setup, SetupField};
pub use older::OlderThreads;
pub use pages::Page;
pub use scroll::Scroll;
pub use skill::SkillSheet;
pub(super) use threads::ThreadNavigationBlock;
pub use threads::{Fetched, ThreadEntries, ThreadSheet};
pub use when::complete_zone;
pub use work::{CutIn, DetailRow, DetailSection, WorkAct, giving_way};
pub(in crate::tui) use workflow_edit::{Lane, step_rows};
pub(in crate::tui) use workflow_outline::{add_kinds, feeds, field_rows, kind_name};
pub use workflows::{WorkflowAdd, WorkflowDetail, WorkflowEdit, WorkflowPage, WorkflowRun, WorkflowSheet};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use std::cell::Cell;

use super::app::{After, App, Editor, Entry};

/// Quick reactions, as the other apps offer them.
pub const REACTIONS: [&str; 6] = ["👍", "❤️", "😂", "🎉", "👀", "✅"];

/// Replies to the sheets' commands.
#[derive(Clone)]
pub enum Reply {
    /// A dev server's log, by the server's id.
    DevServerLogs(String),
    Memory(String),
    MemorySaved(String),
    /// A note the bot's memory sheet has open.
    MemoryFile(String),
    Routines(String),
    /// A page of a bot's older threads (`olderThreads`), for that request (`older::ask`).
    OlderThreads(String, u64),
    /// `cancelFollowUp` for that bot: its row leaves the list once the host has dropped it.
    FollowUpCancelled(String),
    RoutineSaved,
    /// A bot's Library (`library {botId}`).
    Library(String),
    /// What an agent brings on its own (`agentGear`), for its sign-in sheet.
    Gear(String),
    /// A routine's webhook URL and key: copied as a ready-to-run curl.
    Webhook,
    /// A bot conversation's fetched history.
    BotChat {
        bot: String,
        peer: String,
    },
    /// A voice call's own messages, by call id.
    CallEntries(String),
    ThreadEntries {
        bot: String,
        thread: String,
        epoch: u64,
        connection: u64,
    },
    ThreadSent {
        request: uuid::Uuid,
        epoch: u64,
        connection: Option<u64>,
    },
    /// A registry page: the search it lists and the cursor it was asked at (`None`: the first page).
    Registry {
        search: String,
        cursor: Option<String>,
    },
    Skills,
    /// A skill's instructions, for the details sheet with that id.
    SkillDetails(String),
    Auth(String),
    /// The setup terminal opened; for a login profile's sign-in, that login profile.
    Setup(Option<String>),
    /// An agent's login profiles changed (`backends` comes with the answer), on its sheet.
    LoginProfiles(String),
    /// `addLoginProfile` for that agent: the new login profile signs in, or gets its keys, next.
    LoginProfileAdded(String),
    /// `setLoginProfileKeys` for a login profile of that agent: its key form closes and its sheet shows the answer.
    LoginKeys(String),
    /// `loginShellVariables`, for the import list of that login profile.
    ShellVariables(String),
    FieldsSaved,
    /// `renameSection` from a Fields sheet: saved like `FieldsSaved`, and the fold follows the name.
    SectionRenamed {
        from: String,
        to: Option<String>,
    },
    SignIn,
    Models(super::app::ModelKey),
    Downloaded,
    /// Something changed: every open sheet loads again.
    Changed,
    /// A workflow sheet's command.
    Workflow(WorkflowReply),
}

/// Which workflow command the reply belongs to.
#[derive(Clone)]
pub enum WorkflowReply {
    List,
    /// `id` empty: the workflow just created.
    Open {
        id: String,
        edit: bool,
    },
    Refresh(String),
    Runs(String),
    Saved(String),
    Started,
    Run(String),
    Deleted,
    Versions(String),
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

/// A bot's Details page: what it does. Its tasks come from the bot (`work.rs` lays out the rows); the routines and
/// follow-ups are loaded here.
#[derive(Default)]
pub struct DetailsPage {
    pub bot: String,
    pub routines: Option<Vec<Value>>,
    /// The bot's runs, newest first.
    pub runs: Vec<Value>,
    /// The follow-ups the bot set for itself that still wait.
    pub follow_ups: Vec<Value>,
    /// The bot's threads that ended before its `work` carries them, listed under Recent.
    pub older: Box<OlderThreads>,
    /// Over `rows`.
    pub cursor: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RoutineField {
    Name,
    Instruction,
    Folder,
    When,
}

pub struct RoutineForm {
    pub bot: String,
    pub id: Option<String>,
    pub name: Editor,
    pub instruction: Editor,
    pub when: Editor,
    /// The full path its runs work in, empty for the bot's own folder; `None` on a host without `routineFolder`.
    pub folder: Option<Editor>,
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
    OwnSkill(&'a Value),
    Skill(&'a Value),
}

pub struct AgentSetup {
    pub backend: String,
    pub auth: Option<Value>,
    /// A check or a browser sign-in is running.
    pub busy: bool,
    /// The login profile its agent signs in itself, in a browser on the computer, until the host answers.
    pub signing_in: Option<String>,
    pub cursor: usize,
    pub error: Option<String>,
}

pub enum SetupRow {
    Install,
    Node,
    Login,
    Method(Value),
    Check,
    /// Opens the read-only lists of what this agent already brings.
    Brings,
    /// The default login profile, the login the computer already has (the agent's `defaultLoginProfile`): ↵ checks
    /// it again, `r` names it.
    DefaultLoginProfile,
    /// One of the agent's other login profiles: ↵ signs an account in or edits an API key's keys, `r` names it, `x`
    /// removes it.
    LoginProfile(Value),
    /// Adds an account login profile; its sign-in follows. Only for an agent with a `loginProfileSignIn`.
    AddAccount,
    /// Adds an API key login profile; its key form follows.
    AddApiKey,
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
    Icon(String),
    AgentEnv(String),
    Connection(Box<super::connections::Connection>),
    Connector(Value),
    Import,
    /// A handoff card's text (the card's entry id).
    Handoff(String),
    /// A bot's section (its id): a name, or empty for none.
    Section(String),
    /// A section's new name (its current one), or empty to remove it.
    RenameSection(String),
    /// This computer's name, or empty for its hostname (`setComputerName`).
    ComputerName,
    RenameArtifact {
        bot: String,
        artifact: String,
    },
    /// A login profile's name, or empty for who it signs in; no `login_profile`: the agent's default one.
    RenameLoginProfile {
        backend: String,
        login_profile: Option<String>,
    },
    /// An API key login profile's variables; `saved`: the names it already has, which ^x removes.
    LoginProfileKeys {
        backend: String,
        login_profile: String,
        saved: Vec<String>,
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

    /// What is typed, by input name: a secret as it is, other text trimmed. Empty inputs are left out.
    fn typed(&self) -> serde_json::Map<String, Value> {
        let typed = self.inputs.iter().filter(|i| !i.ed.text.trim().is_empty());
        typed.map(|i| (i.name.clone(), if i.secret { i.ed.text.as_str() } else { i.ed.text.trim() }.into())).collect()
    }
}

/// The running setup terminal, which takes over the whole screen.
pub struct Shell {
    pub id: String,
    /// Keys in the order typed (one sender, one request at a time).
    pub input: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    pub backend: String,
    /// The login profile it signs in; `None`: the computer's own sign-in.
    pub login_profile: Option<String>,
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

    /// Loads what voice call `call` in `bot`'s chat carried, for its unfolded conversation.
    pub(in crate::tui) fn fetch_call(&self, bot: &str, call: &str) {
        self.sheet("callEntries", json!({"botId": bot, "callId": call}), Reply::CallEntries(call.to_owned()));
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

    /// The key a model list is kept under; an empty login profile is the default login.
    pub(in crate::tui) fn model_key(backend: &str, login_profile: Option<&str>) -> super::app::ModelKey {
        (backend.to_owned(), login_profile.filter(|s| !s.is_empty()).map(str::to_owned))
    }

    /// The model list `backend` on `login_profile` offers: `Some(None)` while it loads.
    pub(in crate::tui) fn models_for(
        &self,
        backend: &str,
        login_profile: Option<&str>,
    ) -> Option<&Option<Vec<super::app::AgentModel>>> {
        self.models.get(&Self::model_key(backend, login_profile))
    }

    /// Whether that model list came with Fast mode.
    pub(in crate::tui) fn has_fast_mode(&self, backend: &str, login_profile: Option<&str>) -> bool {
        self.fast_mode.contains(&Self::model_key(backend, login_profile))
    }

    /// Loads the models `backend` (on `login_profile`, if any) offers, once per run.
    pub(super) fn want_models(&mut self, backend: &str, login_profile: Option<&str>) {
        let key = Self::model_key(backend, login_profile);
        if !backend.is_empty() && !self.models.contains_key(&key) {
            self.models.insert(key.clone(), None);
            self.sheet("agentModels", json!({"backend": backend, "loginProfile": key.1}), Reply::Models(key));
        }
    }

    /// ←→ on the model field: "default", then each advertised model.
    pub(super) fn cycle_model(
        &self,
        backend: &str,
        login_profile: Option<&str>,
        current: &str,
        d: isize,
    ) -> Option<String> {
        let list = self.models_for(backend, login_profile)?.as_ref()?;
        let mut ids = vec![String::new()];
        ids.extend(list.iter().filter(|m| !m.id.is_empty()).map(|m| m.id.clone()));
        let i = ids.iter().position(|x| x == current).unwrap_or(0);
        Some(ids[super::app::cycle(i, d, ids.len())].clone())
    }

    /// The settings (effort, fast…) `backend`'s `model` offers.
    pub(crate) fn model_settings(
        &self,
        backend: &str,
        login_profile: Option<&str>,
        model: &str,
    ) -> &[super::app::ModelOption] {
        let list = self.models_for(backend, login_profile).and_then(Option::as_ref);
        list.and_then(|l| l.iter().find(|m| m.id == model)).map_or(&[], |m| &m.options)
    }
}

/// Opens `url` in this computer's browser, when the host is this computer.
pub(super) fn open_browser(url: &str, host: &str) -> bool {
    let local = ["//127.0.0.1", "//localhost", "//[::1]"].iter().any(|h| host.contains(h));
    local && !url.is_empty() && crate::service::open(std::ffi::OsStr::new(url))
}
