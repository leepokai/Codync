//! The settings the other apps keep next to the chat, as TUI sheets: a bot's memory and
//! routines, the marketplace (agents, connectors, skills), agent sign-in with its setup
//! terminal, and the pieces of a message you can act on (reactions, sent files).

// Key handlers read best with k (key), m / l / f / a (the sheet), n (rows).
#![allow(clippy::many_single_char_names)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};
use std::str::FromStr as _;

use super::app::{After, App, Confirm, ConfirmAct, Editor, Overlay, Toggle, fuzzy};

/// Quick reactions, as the other apps offer them.
pub const REACTIONS: [&str; 6] = ["👍", "❤️", "😂", "🎉", "👀", "✅"];

/// Replies to the sheets' commands.
#[derive(Clone)]
pub enum Reply {
    Memory(String),
    Routines(String),
    RoutineSaved,
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

pub struct Fact {
    pub id: String,
    pub content: String,
    pub profile: bool,
}

pub struct MemorySheet {
    pub bot: String,
    pub facts: Option<Vec<Fact>>,
    pub cursor: usize,
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

    // ---------- memory ----------

    pub fn open_memory(&mut self) {
        let Some(b) = self.bot().filter(|b| !b.group) else {
            self.flash("A group has no memory of its own");
            return;
        };
        let bot = b.id.clone();
        self.sheet("memory", json!({"botId": bot}), Reply::Memory(bot.clone()));
        self.overlays.push(Overlay::Memory(MemorySheet { bot, facts: None, cursor: 0 }));
    }

    pub(super) fn memory_key(&mut self, mut m: MemorySheet, k: KeyEvent) -> Option<Overlay> {
        let n = m.facts.as_ref().map_or(0, Vec::len);
        match k.code {
            KeyCode::Esc | KeyCode::Char('q' | 'M') => return None,
            KeyCode::Down | KeyCode::Char('j') => m.cursor = (m.cursor + 1).min(n.saturating_sub(1)),
            KeyCode::Up | KeyCode::Char('k') => m.cursor = m.cursor.saturating_sub(1),
            KeyCode::Char('x' | 'd') | KeyCode::Delete | KeyCode::Backspace => {
                if let Some(f) = m.facts.as_mut().filter(|f| m.cursor < f.len()) {
                    let fact = f.remove(m.cursor);
                    m.cursor = m.cursor.min(f.len().saturating_sub(1));
                    self.sheet("forgetMemory", json!({"botId": m.bot, "id": fact.id}), Reply::Changed);
                }
            }
            KeyCode::Char('D') if n > 0 => {
                let name = self.bots.get(&m.bot).map_or_else(String::new, |b| b.name.clone());
                let c = Confirm {
                    title: format!("Forget everything {name} remembers?"),
                    detail: "Facts about you and its history go away.".into(),
                    note: "The chat stays.".into(),
                    button: "Forget everything",
                    act: ConfirmAct::ClearMemory(m.bot.clone()),
                };
                self.overlays.push(Overlay::Memory(m));
                self.overlays.push(Overlay::Confirm(c));
                return None;
            }
            _ => {}
        }
        Some(Overlay::Memory(m))
    }

    // ---------- routines ----------

    pub fn open_routines(&mut self) {
        let Some(b) = self.bot().filter(|b| !b.group) else {
            self.flash("Routines belong to a bot, not a group");
            return;
        };
        let bot = b.id.clone();
        self.sheet("routines", json!({"botId": bot}), Reply::Routines(bot.clone()));
        self.overlays.push(Overlay::Routines(RoutineList { bot, items: None, cursor: 0 }));
    }

    pub(super) fn routines_key(&mut self, mut l: RoutineList, k: KeyEvent) -> Option<Overlay> {
        let n = l.items.as_ref().map_or(0, Vec::len);
        let current = l.items.as_ref().and_then(|v| v.get(l.cursor)).cloned();
        let bot = l.bot.clone();
        match (k.code, current) {
            (KeyCode::Esc | KeyCode::Char('q' | 'R'), _) => return None,
            (KeyCode::Down | KeyCode::Char('j'), _) => l.cursor = (l.cursor + 1).min(n.saturating_sub(1)),
            (KeyCode::Up | KeyCode::Char('k'), _) => l.cursor = l.cursor.saturating_sub(1),
            (KeyCode::Char('n'), _) => {
                self.overlays.push(Overlay::Routines(l));
                self.overlays.push(Overlay::Routine(Box::new(RoutineForm {
                    bot,
                    id: None,
                    name: Editor::default(),
                    instruction: Editor::default(),
                    when: Editor::default(),
                    original: Value::Null,
                    original_text: String::new(),
                    field: RoutineField::Name,
                    error: None,
                    saving: false,
                })));
                return None;
            }
            (KeyCode::Enter | KeyCode::Char('e'), Some(r)) => {
                let triggers = r["triggers"].clone();
                let text = when_text(&triggers, &local_zone());
                let original_text = r["triggerDescriptions"]
                    .as_array()
                    .map(|d| d.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" · "))
                    .unwrap_or_default();
                self.overlays.push(Overlay::Routines(l));
                self.overlays.push(Overlay::Routine(Box::new(RoutineForm {
                    bot,
                    id: Some(s(&r, "id")),
                    name: Editor::with(&s(&r, "name")),
                    instruction: Editor::with(&s(&r, "instruction")),
                    when: Editor::with(text.as_deref().unwrap_or_default()),
                    original: if text.is_some() { Value::Null } else { triggers },
                    original_text,
                    field: RoutineField::Name,
                    error: None,
                    saving: false,
                })));
                return None;
            }
            (KeyCode::Char(' '), Some(r)) => {
                let on = !r["enabled"].as_bool().unwrap_or(false);
                self.sheet(
                    "setRoutineEnabled",
                    json!({"botId": bot, "id": r["id"], "enabled": on}),
                    Reply::Routines(bot.clone()),
                );
                if let Some(item) = l.items.as_mut().and_then(|v| v.get_mut(l.cursor)) {
                    item["enabled"] = on.into();
                }
            }
            (KeyCode::Char('r'), Some(r)) => {
                self.sheet("runRoutine", json!({"botId": bot, "id": r["id"]}), Reply::Routines(bot.clone()));
                self.flash(&format!("Running {} now", s(&r, "name")));
            }
            (KeyCode::Char('w'), Some(r)) => {
                if r["triggers"].as_array().into_iter().flatten().any(|t| t["type"] == "webhook") {
                    self.sheet("routineWebhook", json!({"botId": bot, "id": r["id"]}), Reply::Webhook);
                } else {
                    self.flash("Only a routine whose When is webhook can be called");
                }
            }
            (KeyCode::Char('x'), Some(r)) => {
                let c = Confirm {
                    title: format!("Delete the routine {}?", s(&r, "name")),
                    detail: "It stops running.".into(),
                    note: "What it already posted stays in the chat.".into(),
                    button: "Delete",
                    act: ConfirmAct::DeleteRoutine(bot, s(&r, "id")),
                };
                self.overlays.push(Overlay::Routines(l));
                self.overlays.push(Overlay::Confirm(c));
                return None;
            }
            _ => {}
        }
        Some(Overlay::Routines(l))
    }

    pub(super) fn routine_key(&mut self, mut f: Box<RoutineForm>, k: KeyEvent) -> Option<Overlay> {
        use RoutineField::{Instruction, Name, When};
        match k.code {
            KeyCode::Esc => return None,
            _ if newline(k) && f.field == Instruction => f.instruction.insert("\n"),
            KeyCode::Enter => self.save_routine(&mut f),
            KeyCode::Char('s') if ctrl(k) => self.save_routine(&mut f),
            KeyCode::Tab | KeyCode::Down => {
                f.field = match f.field {
                    Name => Instruction,
                    Instruction => When,
                    When => Name,
                };
            }
            KeyCode::BackTab | KeyCode::Up => {
                f.field = match f.field {
                    Name => When,
                    Instruction => Name,
                    When => Instruction,
                };
            }
            _ => {
                let ed = match f.field {
                    Name => &mut f.name,
                    Instruction => &mut f.instruction,
                    When => &mut f.when,
                };
                ed.key(k);
            }
        }
        Some(Overlay::Routine(f))
    }

    fn save_routine(&mut self, f: &mut RoutineForm) {
        if f.saving {
            return;
        }
        let (name, instruction) = (f.name.text.trim(), f.instruction.text.trim());
        if name.is_empty() || instruction.is_empty() {
            f.error = Some("A routine needs a name and what to do.".into());
            return;
        }
        let triggers = if f.when.text.trim().is_empty() && !f.original.is_null() {
            f.original.clone()
        } else {
            match parse_when(&f.when.text, &local_zone(), crate::store::now_ms()) {
                Ok(t) => json!([t]),
                Err(e) => {
                    f.error = Some(e);
                    f.field = RoutineField::When;
                    return;
                }
            }
        };
        let mut body = json!({"botId": f.bot, "name": name, "instruction": instruction, "triggers": triggers});
        if let Some(id) = &f.id {
            body["id"] = id.clone().into();
        }
        f.saving = true;
        f.error = None;
        self.sheet("saveRoutine", body, Reply::RoutineSaved);
    }

    // ---------- marketplace ----------

    pub fn open_market(&mut self) {
        self.call("refreshBackends", json!({}), After::Backends);
        self.call("connectors", json!({}), After::Connectors);
        self.call("skills", json!({}), After::Skills);
        self.sheet("marketSkills", json!({}), Reply::Skills);
        self.sheet("marketConnectors", json!({"search": ""}), Reply::Registry { more: false });
        self.overlays.push(Overlay::Market(Box::new(Market {
            tab: Tab::Agents,
            query: Editor::default(),
            searched: None,
            registry: vec![],
            next: None,
            skills: None,
            loading: true,
            cursor: 0,
            error: None,
        })));
    }

    /// The marketplace's rows on its current tab, filtered by the query.
    pub fn market_rows<'a>(&'a self, m: &'a Market) -> Vec<Row<'a>> {
        let q = m.query.text.trim();
        let hit = |v: &Value, keys: &[&str]| {
            let fields: Vec<&str> = keys.iter().map(|k| v[*k].as_str().unwrap_or_default()).collect();
            fuzzy(q, &fields)
        };
        match m.tab {
            Tab::Agents => self
                .backends
                .iter()
                .filter(|b| b["available"] == true || b["installed"] == true || b["curated"] == true)
                .filter(|b| hit(b, &["name", "id"]))
                .map(Row::Agent)
                .collect(),
            Tab::Connectors => {
                let installed: Vec<&str> = self.plugins.0.iter().filter_map(|c| c["registryName"].as_str()).collect();
                let mut rows: Vec<Row> =
                    self.plugins.0.iter().filter(|c| hit(c, &["name", "description"])).map(Row::Installed).collect();
                rows.push(Row::Custom);
                rows.extend(
                    m.registry
                        .iter()
                        .filter(|c| !installed.contains(&c["name"].as_str().unwrap_or_default()))
                        .map(Row::Registry),
                );
                if m.next.is_some() {
                    rows.push(Row::More);
                }
                rows
            }
            Tab::Skills => {
                let installed: Vec<String> = self.plugins.1.iter().map(|v| s(v, "id").to_lowercase()).collect();
                let mut rows: Vec<Row> =
                    self.plugins.1.iter().filter(|v| hit(v, &["name", "description"])).map(Row::OwnSkill).collect();
                rows.extend(
                    m.skills
                        .iter()
                        .flatten()
                        .filter(|v| !installed.contains(&s(v, "source").to_lowercase()))
                        .filter(|v| hit(v, &["name", "description"]))
                        .map(Row::Skill),
                );
                rows
            }
        }
    }

    /// The query no longer matches what the registry list shows: ↵ searches first.
    pub fn market_stale(m: &Market) -> bool {
        m.tab == Tab::Connectors && m.searched.as_deref().is_some_and(|s| s != m.query.text.trim())
    }

    pub(super) fn market_key(&mut self, mut m: Box<Market>, k: KeyEvent) -> Option<Overlay> {
        let n = self.market_rows(&m).len();
        match k.code {
            KeyCode::Esc => return None,
            KeyCode::Tab | KeyCode::BackTab => {
                let i = TABS.iter().position(|(t, _)| *t == m.tab).unwrap_or(0);
                let d = if k.code == KeyCode::Tab { 1 } else { TABS.len() - 1 };
                m.tab = TABS[(i + d) % TABS.len()].0;
                m.cursor = 0;
            }
            KeyCode::Down => m.cursor = (m.cursor + 1).min(n.saturating_sub(1)),
            KeyCode::Up => m.cursor = m.cursor.saturating_sub(1),
            KeyCode::Char('n' | 'j') if ctrl(k) => m.cursor = (m.cursor + 1).min(n.saturating_sub(1)),
            KeyCode::Char('p') if ctrl(k) => m.cursor = m.cursor.saturating_sub(1),
            KeyCode::Char('r') if ctrl(k) => {
                self.call("connectors", json!({}), After::Connectors);
                self.call("skills", json!({}), After::Skills);
                self.call("refreshBackends", json!({}), After::Backends);
            }
            KeyCode::Enter if Self::market_stale(&m) => {
                let search = m.query.text.trim().to_owned();
                m.loading = true;
                m.cursor = 0;
                m.registry.clear();
                m.next = None;
                self.sheet("marketConnectors", json!({"search": search}), Reply::Registry { more: false });
                m.searched = Some(search);
            }
            KeyCode::Enter => return self.market_enter(m),
            KeyCode::Char('x') if ctrl(k) => {
                let (id, name, skill) = match self.market_rows(&m).get(m.cursor) {
                    Some(Row::Installed(c)) => (s(c, "id"), s(c, "name"), false),
                    Some(Row::OwnSkill(v)) => (s(v, "id"), s(v, "name"), true),
                    _ => return Some(Overlay::Market(m)),
                };
                let c = Confirm {
                    title: format!("Remove {name}?"),
                    detail: if skill {
                        "Bots stop using this skill.".into()
                    } else {
                        "Bots lose it, and the keys saved for it are deleted.".into()
                    },
                    note: String::new(),
                    button: "Remove",
                    act: if skill { ConfirmAct::RemoveSkill(id) } else { ConfirmAct::RemoveConnector(id) },
                };
                self.overlays.push(Overlay::Market(m));
                self.overlays.push(Overlay::Confirm(c));
                return None;
            }
            _ => {
                if m.query.key(k) {
                    m.cursor = 0;
                }
            }
        }
        Some(Overlay::Market(m))
    }

    fn market_enter(&mut self, mut m: Box<Market>) -> Option<Overlay> {
        enum Act {
            Agent(String),
            SignIn(String),
            Custom,
            Install(Value),
            More,
            Skill(String),
            None,
        }
        let act = match self.market_rows(&m).get(m.cursor) {
            Some(Row::Agent(b)) => Act::Agent(s(b, "id")),
            Some(Row::Installed(c)) if c["auth"] == "signedOut" => Act::SignIn(s(c, "id")),
            Some(Row::Custom) => Act::Custom,
            Some(Row::Registry(c)) => Act::Install((*c).clone()),
            Some(Row::More) => Act::More,
            Some(Row::Skill(v)) => Act::Skill(s(v, "source")),
            _ => Act::None,
        };
        match act {
            Act::Agent(id) => {
                self.overlays.push(Overlay::Market(m));
                self.open_agent(&id);
                return None;
            }
            Act::SignIn(id) => {
                self.flash("Starting the sign-in…");
                self.sheet("connectorSignIn", json!({"id": id}), Reply::SignIn);
            }
            Act::Custom => {
                self.overlays.push(Overlay::Market(m));
                self.overlays.push(Overlay::Fields(Box::new(Fields {
                    title: "Custom connector".into(),
                    note: "Paste the MCP config from a README or another app (Claude, Cursor, VS Code). Every server in it is added.".into(),
                    choices: vec![],
                    choice: 0,
                    inputs: vec![Input {
                        name: "config".into(),
                        label: "Config".into(),
                        secret: false,
                        required: true,
                        multiline: true,
                        placeholder: r#"{"mcpServers": {"name": {"command": "npx", "args": ["-y", "…"]}}}"#.into(),
                        ed: Editor::default(),
                    }],
                    cursor: 0,
                    error: None,
                    saving: false,
                    submit: Submit::Import,
                })));
                return None;
            }
            Act::Install(item) => {
                let options = item["options"].as_array().cloned().unwrap_or_default();
                if options.len() == 1 && options[0]["inputs"].as_array().is_none_or(Vec::is_empty) {
                    self.flash(&format!("Adding {}…", s(&item, "title")));
                    self.sheet(
                        "installConnector",
                        json!({"registryName": item["name"], "option": options[0]["id"], "inputs": {}}),
                        Reply::FieldsSaved,
                    );
                } else {
                    let host = self.host.clone();
                    let choices = options
                        .iter()
                        .map(|o| {
                            if o["kind"] == "remote" {
                                format!("Hosted by {}", s(&item, "title"))
                            } else {
                                format!("On {host} ({})", s(o, "kind"))
                            }
                        })
                        .collect();
                    let mut f = Fields {
                        title: format!("Add {}", s(&item, "title")),
                        note: format!("Keys are saved on {host} only."),
                        choices,
                        choice: 0,
                        inputs: vec![],
                        cursor: 0,
                        error: None,
                        saving: false,
                        submit: Submit::Connector(item),
                    };
                    connector_inputs(&mut f);
                    super::connections::option_fields(&mut f);
                    self.overlays.push(Overlay::Market(m));
                    self.overlays.push(Overlay::Fields(Box::new(f)));
                    return None;
                }
            }
            Act::More => {
                m.loading = true;
                let search = m.searched.clone().unwrap_or_default();
                self.sheet(
                    "marketConnectors",
                    json!({"search": search, "cursor": m.next}),
                    Reply::Registry { more: true },
                );
            }
            Act::Skill(source) => {
                self.flash("Installing the skill…");
                self.sheet("installSkill", json!({"source": source}), Reply::FieldsSaved);
            }
            Act::None => {}
        }
        Some(Overlay::Market(m))
    }

    // ---------- agent sign-in ----------

    pub fn open_agent(&mut self, backend: &str) {
        let known = self.backends.iter().find(|b| b["id"] == backend).is_some_and(|b| b["signedIn"] == true);
        let mut a = AgentSetup { backend: backend.to_owned(), auth: None, busy: false, cursor: 0, error: None };
        if !known {
            a.busy = true;
            self.sheet("agentAuth", json!({"backend": backend}), Reply::Auth(backend.to_owned()));
        }
        self.overlays.push(Overlay::Agent(a));
    }

    pub fn agent_rows(&self, a: &AgentSetup) -> Vec<SetupRow> {
        let b = self.backends.iter().find(|b| b["id"] == a.backend.as_str());
        let mut rows = vec![];
        if b.is_some_and(|b| b["curated"] == true && b["installed"] != true && b["canInstall"] == true) {
            rows.push(SetupRow::Install);
        }
        let signed_in =
            a.auth.as_ref().map_or_else(|| b.map(|b| &b["signedIn"]) == Some(&json!(true)), |v| v["signedIn"] == true);
        if !signed_in && !a.busy {
            if a.auth.as_ref().is_some_and(|v| v["login"] == true) {
                rows.push(SetupRow::Login);
            }
            for m in a.auth.as_ref().and_then(|v| v["methods"].as_array()).into_iter().flatten() {
                rows.push(SetupRow::Method(m.clone()));
            }
        }
        rows.push(SetupRow::Check);
        rows
    }

    pub(super) fn agent_key(&mut self, mut a: AgentSetup, k: KeyEvent) -> Option<Overlay> {
        let rows = self.agent_rows(&a);
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return None,
            KeyCode::Down | KeyCode::Char('j') => a.cursor = (a.cursor + 1).min(rows.len().saturating_sub(1)),
            KeyCode::Up | KeyCode::Char('k') => a.cursor = a.cursor.saturating_sub(1),
            KeyCode::Enter if !a.busy => {
                let backend = a.backend.clone();
                let (cols, rows_n) = crossterm::terminal::size().unwrap_or((80, 24));
                let size = json!({"cols": cols, "rows": rows_n});
                match rows.get(a.cursor) {
                    Some(SetupRow::Install) => self.start_setup(&backend, "install", None, &size),
                    Some(SetupRow::Login) => self.start_setup(&backend, "login", None, &size),
                    Some(SetupRow::Method(m)) => match m["kind"].as_str() {
                        Some("terminal") => self.start_setup(&backend, "login", m["id"].as_str(), &size),
                        Some("envVar") => {
                            let saved: Vec<String> = a
                                .auth
                                .as_ref()
                                .and_then(|v| v["savedEnv"].as_array())
                                .into_iter()
                                .flatten()
                                .filter_map(|x| x.as_str().map(str::to_owned))
                                .collect();
                            let f = env_fields(&backend, m, &saved, &self.host);
                            self.overlays.push(Overlay::Agent(a));
                            self.overlays.push(Overlay::Fields(Box::new(f)));
                            return None;
                        }
                        _ => {
                            a.busy = true;
                            a.error = None;
                            self.flash("Finish signing in in the browser on this computer");
                            self.sheet(
                                "agentAuthenticate",
                                json!({"backend": backend, "method": m["id"]}),
                                Reply::Auth(backend.clone()),
                            );
                        }
                    },
                    Some(SetupRow::Check) | None => {
                        a.busy = true;
                        a.error = None;
                        self.sheet("agentAuth", json!({"backend": backend}), Reply::Auth(backend.clone()));
                    }
                }
            }
            _ => {}
        }
        Some(Overlay::Agent(a))
    }

    fn start_setup(&mut self, backend: &str, step: &str, method: Option<&str>, size: &Value) {
        let mut body = json!({"backend": backend, "step": step, "cols": size["cols"], "rows": size["rows"]});
        if let Some(m) = method {
            body["method"] = m.into();
        }
        self.sheet("agentSetup", body, Reply::Setup);
        self.flash("Opening the terminal…");
    }

    /// Keys while the setup terminal has the screen: everything goes to it; ^] leaves.
    pub(super) fn shell_key(&mut self, k: KeyEvent) {
        let Some(sh) = &self.shell else { return };
        let id = sh.id.clone();
        if sh.exited.is_some() {
            self.close_shell();
            return;
        }
        if ctrl(k) && matches!(k.code, KeyCode::Char(']' | '5')) {
            self.call("termClose", json!({"term": id}), After::Nothing);
            self.close_shell();
            return;
        }
        let bytes = key_bytes(k);
        if !bytes.is_empty() {
            self.term_input(&bytes);
        }
    }

    pub(super) fn term_input(&self, bytes: &[u8]) {
        if let Some(sh) = &self.shell {
            let _ = sh.input.send(bytes.to_vec());
        }
    }

    pub fn on_resize(&self, cols: u16, rows: u16) {
        if let Some(sh) = self.shell.as_ref().filter(|s| s.exited.is_none()) {
            self.call("termResize", json!({"term": sh.id, "cols": cols, "rows": rows}), After::Nothing);
        }
    }

    pub(super) fn on_term(&mut self, id: &str, v: &Value) {
        let Some(sh) = self.shell.as_mut().filter(|s| s.id == id) else { return };
        match v["type"].as_str() {
            Some("output") => {
                use base64::Engine as _;
                if let Ok(bytes) =
                    base64::engine::general_purpose::STANDARD.decode(v["data"].as_str().unwrap_or_default())
                {
                    sh.out.extend_from_slice(&bytes);
                }
            }
            Some("exit") => {
                let code = v["code"].as_i64().unwrap_or(-1);
                sh.exited = Some(code);
                let how = if code == 0 { "Done" } else { "Stopped" };
                sh.out.extend_from_slice(
                    format!("\r\n\x1b[2m{how}. Press any key to go back to Codync.\x1b[0m\r\n").as_bytes(),
                );
            }
            _ => {}
        }
    }

    fn close_shell(&mut self) {
        let Some(sh) = self.shell.take() else { return };
        self.call("refreshBackends", json!({}), After::Backends);
        if let Some(Overlay::Agent(a)) = self.overlays.last_mut() {
            a.busy = true;
            self.sheet("agentAuth", json!({"backend": sh.backend}), Reply::Auth(sh.backend.clone()));
        }
    }

    // ---------- fields ----------

    pub(super) fn fields_key(&mut self, mut f: Box<Fields>, k: KeyEvent) -> Option<Overlay> {
        if f.saving {
            return if k.code == KeyCode::Esc { None } else { Some(Overlay::Fields(f)) };
        }
        let rows = f.offset() + f.inputs.len();
        let on_choice = f.offset() == 1 && f.cursor == 0;
        let multiline = f.current().is_some_and(|i| i.multiline);
        match k.code {
            KeyCode::Esc => return None,
            KeyCode::Char('x') if ctrl(k) && matches!(f.submit, Submit::Connection(_)) => {
                if let Submit::Connection(c) = &f.submit {
                    f.saving = true;
                    self.call(
                        "connectorRequestFinish",
                        json!({"entryId":c.entry,"cancel":true}),
                        After::Connection(c.entry.clone(), super::connections::Step::Finish),
                    );
                }
            }
            _ if newline(k) && multiline => {
                if let Some(i) = f.current() {
                    i.ed.insert("\n");
                }
            }
            KeyCode::Enter => self.save_fields(&mut f),
            KeyCode::Char('s') if ctrl(k) => self.save_fields(&mut f),
            KeyCode::Tab | KeyCode::Down => f.cursor = (f.cursor + 1) % rows.max(1),
            KeyCode::BackTab | KeyCode::Up => f.cursor = (f.cursor + rows.max(1) - 1) % rows.max(1),
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if on_choice => {
                f.choice = (f.choice + 1) % f.choices.len();
                connector_inputs(&mut f);
                super::connections::option_fields(&mut f);
            }
            _ => {
                if let Some(i) = f.current() {
                    i.ed.key(k);
                }
            }
        }
        Some(Overlay::Fields(f))
    }

    fn save_fields(&mut self, f: &mut Fields) {
        if f.saving {
            return;
        }
        if let Some(i) = f.inputs.iter().find(|i| i.required && i.ed.text.trim().is_empty()) {
            f.error = Some(format!("{} is required.", i.label));
            return;
        }
        let values: serde_json::Map<String, Value> = f
            .inputs
            .iter()
            .filter(|i| !i.ed.text.trim().is_empty())
            .map(|i| (i.name.clone(), if i.secret { i.ed.text.as_str() } else { i.ed.text.trim() }.into()))
            .collect();
        f.saving = true;
        f.error = None;
        match &f.submit {
            Submit::Connection(_) => self.submit_connection(f, &values),
            Submit::AgentEnv(backend) => {
                if values.is_empty() {
                    f.saving = false;
                    f.error = Some("Type at least one key.".into());
                    return;
                }
                self.sheet("setAgentEnv", json!({"backend": backend, "vars": values}), Reply::FieldsSaved);
            }
            Submit::Connector(item) => {
                let option = item["options"][f.choice]["id"].clone();
                self.sheet(
                    "installConnector",
                    json!({"registryName": item["name"], "option": option, "inputs": values}),
                    Reply::FieldsSaved,
                );
            }
            Submit::Import => self.sheet("importConnectors", json!({"config": values["config"]}), Reply::FieldsSaved),
        }
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

    // ---------- replies ----------

    pub(super) fn on_sheet_reply(&mut self, r: Reply, res: Result<Value, String>) {
        let v = match res {
            Ok(v) => v,
            Err(e) => {
                self.sheet_error(&r, e);
                return;
            }
        };
        match r {
            Reply::Memory(bot) => {
                for o in &mut self.overlays {
                    if let Overlay::Memory(m) = o
                        && m.bot == bot
                    {
                        let facts: Vec<Fact> = v["facts"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|f| Fact { id: s(f, "id"), content: s(f, "content"), profile: f["kind"] == "profile" })
                            .collect();
                        m.cursor = m.cursor.min(facts.len().saturating_sub(1));
                        m.facts = Some(facts);
                    }
                }
            }
            Reply::Routines(bot) => {
                if v["routines"].is_array() {
                    for o in &mut self.overlays {
                        if let Overlay::Routines(l) = o
                            && l.bot == bot
                        {
                            let items = v["routines"].as_array().cloned().unwrap_or_default();
                            l.cursor = l.cursor.min(items.len().saturating_sub(1));
                            l.items = Some(items);
                        }
                    }
                } else {
                    // A change: the list comes again.
                    self.sheet("routines", json!({"botId": bot}), Reply::Routines(bot.clone()));
                }
            }
            Reply::RoutineSaved => {
                if let Some(i) = self.overlays.iter().rposition(|o| matches!(o, Overlay::Routine(_))) {
                    self.overlays.truncate(i);
                }
                self.refresh_sheets();
            }
            Reply::Webhook => {
                let text =
                    format!("curl -X POST '{}' -H 'Authorization: Bearer {}' -d '{{}}'", s(&v, "url"), s(&v, "key"));
                super::app::copy(&text);
                self.flash("Copied a curl command that runs it (with its key)");
            }
            Reply::Registry { more } => {
                if let Some(Overlay::Market(m)) =
                    self.overlays.iter_mut().rev().find(|o| matches!(o, Overlay::Market(_)))
                {
                    let items = v["items"].as_array().cloned().unwrap_or_default();
                    if more {
                        m.registry.extend(items);
                    } else {
                        m.registry = items;
                    }
                    m.next = v["nextCursor"].as_str().filter(|c| !c.is_empty()).map(str::to_owned);
                    m.searched.get_or_insert_with(String::new);
                    m.loading = false;
                    m.error = None;
                }
            }
            Reply::Skills => {
                if let Some(Overlay::Market(m)) =
                    self.overlays.iter_mut().rev().find(|o| matches!(o, Overlay::Market(_)))
                {
                    m.skills = Some(v["items"].as_array().cloned().unwrap_or_default());
                }
            }
            Reply::Auth(backend) => {
                for o in &mut self.overlays {
                    if let Overlay::Agent(a) = o
                        && a.backend == backend
                    {
                        a.auth = Some(v.clone());
                        a.busy = false;
                        a.error = None;
                        a.cursor = 0;
                    }
                }
                self.call("refreshBackends", json!({}), After::Nothing);
            }
            Reply::Setup => {
                let Some(id) = v["term"].as_str() else { return };
                let backend = match self.overlays.last() {
                    Some(Overlay::Agent(a)) => a.backend.clone(),
                    _ => String::new(),
                };
                let input = self.client.spawn_term(id.to_owned(), self.tx.clone());
                self.shell = Some(Shell { id: id.to_owned(), input, backend, exited: None, out: vec![] });
            }
            Reply::FieldsSaved => {
                if let Some(i) = self.overlays.iter().rposition(|o| matches!(o, Overlay::Fields(_))) {
                    self.overlays.truncate(i);
                }
                if let Some(Overlay::Agent(a)) = self.overlays.last_mut()
                    && v.get("signedIn").is_some()
                {
                    a.auth = Some(v);
                    a.busy = false;
                }
                self.flash("Saved");
                self.refresh_sheets();
            }
            Reply::SignIn => {
                let url = s(&v, "url");
                super::app::copy(&url);
                if open_browser(&url, &self.url) {
                    self.flash("Finish in the browser, then ^r refreshes");
                } else {
                    self.flash("Sign-in link copied: open it on this computer, then ^r refreshes");
                }
            }
            Reply::Models(backend) => {
                let list = v["models"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|m| {
                        let id = m["id"].as_str()?.to_owned();
                        Some((id.clone(), m["name"].as_str().unwrap_or(&id).to_owned()))
                    })
                    .collect();
                self.models.insert(backend, Some(list));
            }
            Reply::Downloaded => {
                let paths: Vec<String> =
                    v["paths"].as_array().into_iter().flatten().filter_map(|p| p.as_str().map(str::to_owned)).collect();
                let names: Vec<&str> =
                    paths.iter().filter_map(|p| std::path::Path::new(p).file_name()?.to_str()).collect();
                let dir = paths.first().and_then(|p| std::path::Path::new(p).parent()).map(|d| d.to_string_lossy());
                let home = dirs::home_dir().map(|h| h.to_string_lossy().into_owned()).unwrap_or_default();
                let dir = dir.map(|d| super::app::tilde(&d, &home)).unwrap_or_default();
                self.flash(&format!("Saved {} to {dir}", names.join(", ")));
            }
            Reply::Changed => self.refresh_sheets(),
        }
    }

    fn sheet_error(&mut self, r: &Reply, e: String) {
        match r {
            Reply::Models(backend) => {
                self.models.insert(backend.clone(), Some(vec![]));
            }
            Reply::RoutineSaved => {
                if let Some(Overlay::Routine(f)) = self.overlays.last_mut() {
                    f.saving = false;
                    f.error = Some(e);
                }
            }
            Reply::FieldsSaved => match self.overlays.last_mut() {
                Some(Overlay::Fields(f)) => {
                    f.saving = false;
                    f.error = Some(e);
                }
                _ => self.flash(&e),
            },
            Reply::Auth(backend) => {
                for o in &mut self.overlays {
                    if let Overlay::Agent(a) = o
                        && a.backend == *backend
                    {
                        a.busy = false;
                        a.error = Some(e.clone());
                    }
                }
            }
            Reply::Registry { .. } | Reply::Skills => {
                if let Some(Overlay::Market(m)) =
                    self.overlays.iter_mut().rev().find(|o| matches!(o, Overlay::Market(_)))
                {
                    m.loading = false;
                    m.skills.get_or_insert_with(Vec::new);
                    m.error = Some(e);
                }
            }
            Reply::Memory(_) | Reply::Routines(_) => {
                for o in &mut self.overlays {
                    match o {
                        Overlay::Memory(m) => {
                            m.facts.get_or_insert_with(Vec::new);
                        }
                        Overlay::Routines(l) => {
                            l.items.get_or_insert_with(Vec::new);
                        }
                        _ => {}
                    }
                }
                self.flash(&e);
            }
            _ => self.flash(&e),
        }
    }

    /// Loads what every open sheet shows again, after a change.
    pub(super) fn refresh_sheets(&mut self) {
        let mut calls: Vec<(&'static str, Value, After)> = vec![];
        for o in &self.overlays {
            match o {
                Overlay::Memory(m) => {
                    calls.push(("memory", json!({"botId": m.bot}), After::Sheet(Reply::Memory(m.bot.clone()))));
                }
                Overlay::Routines(l) => {
                    calls.push(("routines", json!({"botId": l.bot}), After::Sheet(Reply::Routines(l.bot.clone()))));
                }
                Overlay::Market(_) | Overlay::Form(_) => {
                    calls.push(("connectors", json!({}), After::Connectors));
                    calls.push(("skills", json!({}), After::Skills));
                }
                _ => {}
            }
        }
        calls.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
        for (m, body, after) in calls {
            self.call(m, body, after);
        }
    }

    /// Keeps the installed lists the marketplace and the bot form show.
    pub(super) fn set_plugins(&mut self, connectors: bool, items: Vec<Value>) -> Vec<Toggle> {
        let list = items
            .iter()
            .filter_map(|c| {
                let id = c["id"].as_str()?.to_owned();
                let name = c["name"].as_str().unwrap_or(&id).to_owned();
                Some(Toggle { id, name, on: false })
            })
            .collect();
        if connectors {
            self.plugins.0 = items;
        } else {
            self.plugins.1 = items;
        }
        list
    }
}

/// The inputs of a connector's chosen way to run, defaults filled in.
fn connector_inputs(f: &mut Fields) {
    let Submit::Connector(item) = &f.submit else { return };
    f.inputs = item["options"][f.choice]["inputs"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|i| {
            let name = s(i, "name");
            let description = s(i, "description");
            Input {
                label: name.clone(),
                name,
                secret: i["secret"] == true,
                required: i["required"] == true,
                multiline: false,
                placeholder: if description.is_empty() { s(i, "placeholder") } else { description },
                ed: Editor::with(i["default"].as_str().unwrap_or_default()),
            }
        })
        .collect();
    f.cursor = f.cursor.min(f.offset() + f.inputs.len().saturating_sub(1));
}

fn env_fields(backend: &str, method: &Value, saved: &[String], host: &str) -> Fields {
    let inputs = method["vars"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| {
            let name = s(v, "name");
            let has = saved.contains(&name);
            Input {
                label: s(v, "label"),
                secret: v["secret"] != false,
                required: v["optional"] != true && !has,
                multiline: false,
                placeholder: if has { "saved · type to replace".into() } else { name.clone() },
                name,
                ed: Editor::default(),
            }
        })
        .collect();
    let link = s(method, "link");
    Fields {
        title: s(method, "name"),
        note: if link.is_empty() {
            format!("Saved on {host} only.")
        } else {
            format!("Get one at {link} · saved on {host} only.")
        },
        choices: vec![],
        choice: 0,
        inputs,
        cursor: 0,
        error: None,
        saving: false,
        submit: Submit::AgentEnv(backend.to_owned()),
    }
}

/// Bytes a terminal sends for a key.
pub fn key_bytes(k: KeyEvent) -> Vec<u8> {
    let alt = k.modifiers.contains(KeyModifiers::ALT);
    let mut out: Vec<u8> = match k.code {
        KeyCode::Char(c) if ctrl(k) && c.is_ascii_alphabetic() => vec![(c.to_ascii_lowercase() as u8) & 0x1f],
        KeyCode::Char(c) => c.to_string().into_bytes(),
        KeyCode::Enter => b"\r".to_vec(),
        KeyCode::Tab => b"\t".to_vec(),
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        _ => vec![],
    };
    if alt && !out.is_empty() {
        out.insert(0, 0x1b);
    }
    out
}

/// This computer's IANA time zone (routines run on the host's clock in it).
pub fn local_zone() -> String {
    let valid = |z: &str| chrono_tz::Tz::from_str(z).is_ok();
    if let Ok(tz) = std::env::var("TZ")
        && valid(tz.trim_start_matches(':'))
    {
        return tz.trim_start_matches(':').to_owned();
    }
    std::fs::read_link("/etc/localtime")
        .ok()
        .and_then(|p| p.to_string_lossy().split_once("zoneinfo/").map(|(_, z)| z.to_owned()))
        .filter(|z| valid(z))
        .unwrap_or_else(|| "UTC".into())
}

/// One trigger from what's typed: `webhook`, `every 30m`, a date and time (once),
/// or five cron fields in `zone`.
pub fn parse_when(text: &str, zone: &str, now: i64) -> Result<Value, String> {
    let t = text.trim().to_lowercase();
    if t.is_empty() {
        return Err("Say when it runs.".into());
    }
    if t == "webhook" {
        return Ok(json!({"type": "webhook"}));
    }
    if let Some(rest) = t.strip_prefix("every") {
        let rest = rest.trim();
        let split = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        let (n, unit) = rest.split_at(split);
        let n: i64 = if n.is_empty() { 1 } else { n.parse().map_err(|_| "Use a whole number, like every 30m.")? };
        let unit = match unit.trim().trim_end_matches('s') {
            "sec" | "second" => 1,
            "m" | "min" | "minute" => 60,
            "h" | "hr" | "hour" => 3600,
            "d" | "day" => 86400,
            "" if n > 0 && !rest.is_empty() => return Err("Add a unit: s, m, h or d.".into()),
            _ => return Err("Use s, m, h or d, like every 2h.".into()),
        };
        return Ok(json!({"type": "interval", "seconds": n * unit}));
    }
    if let Ok(at) = chrono::NaiveDateTime::parse_from_str(&t, "%Y-%m-%d %H:%M") {
        let tz = chrono_tz::Tz::from_str(zone).map_err(|_| "Unknown time zone")?;
        let at = at.and_local_timezone(tz).earliest().ok_or("That time doesn't exist here.")?.timestamp_millis();
        if at <= now {
            return Err("That time has passed.".into());
        }
        return Ok(json!({"type": "once", "at": at}));
    }
    if t.split_whitespace().count() == 5 {
        return Ok(
            json!({"type": "cron", "expression": t.split_whitespace().collect::<Vec<_>>().join(" "), "timeZone": zone}),
        );
    }
    Err("Try 0 9 * * 1-5, every 2h, 2026-10-01 09:00 or webhook.".into())
}

/// A routine's triggers as `parse_when` reads them, when they're one it can write.
pub fn when_text(triggers: &Value, zone: &str) -> Option<String> {
    let [t] = triggers.as_array()?.as_slice() else { return None };
    match t["type"].as_str()? {
        "webhook" => Some("webhook".into()),
        "interval" => {
            let secs = t["seconds"].as_i64()?;
            let (n, u) = [(86400, "d"), (3600, "h"), (60, "m"), (1, "s")].into_iter().find(|(u, _)| secs % u == 0)?;
            Some(format!("every {}{u}", secs / n))
        }
        "cron" if t["timeZone"].as_str() == Some(zone) => t["expression"].as_str().map(str::to_owned),
        "once" => {
            let tz = chrono_tz::Tz::from_str(zone).ok()?;
            let at = chrono::DateTime::from_timestamp_millis(t["at"].as_i64()?)?.with_timezone(&tz);
            Some(at.format("%Y-%m-%d %H:%M").to_string())
        }
        _ => None,
    }
}

/// Opens `url` in this computer's browser, when the host is this computer.
pub(super) fn open_browser(url: &str, host: &str) -> bool {
    let local = ["//127.0.0.1", "//localhost", "//[::1]"].iter().any(|h| host.contains(h));
    if !local || url.is_empty() {
        return false;
    }
    let cmd = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    std::process::Command::new(cmd)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn when_reads_what_it_writes() {
        let now = 1_790_470_800_000;
        let zone = "Asia/Taipei";
        assert_eq!(parse_when("webhook", zone, now), Ok(json!({"type": "webhook"})));
        assert_eq!(parse_when("every 30m", zone, now), Ok(json!({"type": "interval", "seconds": 1800})));
        assert_eq!(parse_when("Every 2 hours", zone, now), Ok(json!({"type": "interval", "seconds": 7200})));
        assert!(parse_when("every 5", zone, now).is_err());
        assert!(parse_when("2020-01-01 09:00", zone, now).is_err());
        assert!(parse_when("tomorrow", zone, now).is_err());
        for text in ["0 9 * * 1-5", "every 90m", "every 1d", "webhook", "2030-10-01 09:00"] {
            let t = parse_when(text, zone, now).expect(text);
            let back = when_text(&json!([t]), zone).expect(text);
            assert_eq!(parse_when(&back, zone, now), Ok(t), "{text}");
        }
        // Another zone's cron, or several triggers, stay as they were.
        assert_eq!(when_text(&json!([{"type": "cron", "expression": "0 9 * * *", "timeZone": "UTC"}]), zone), None);
        assert_eq!(when_text(&json!([{"type": "webhook"}, {"type": "webhook"}]), zone), None);
    }

    #[test]
    fn keys_become_terminal_bytes() {
        let k = |code, m| KeyEvent::new(code, m);
        assert_eq!(key_bytes(k(KeyCode::Char('c'), KeyModifiers::CONTROL)), [3]);
        assert_eq!(key_bytes(k(KeyCode::Char('é'), KeyModifiers::NONE)), "é".as_bytes());
        assert_eq!(key_bytes(k(KeyCode::Enter, KeyModifiers::NONE)), b"\r");
        assert_eq!(key_bytes(k(KeyCode::Char('b'), KeyModifiers::ALT)), b"\x1bb");
        assert_eq!(key_bytes(k(KeyCode::Up, KeyModifiers::NONE)), b"\x1b[A");
    }
}
