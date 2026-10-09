//! The same memory operations as the graphical clients, driven from the keyboard.

use super::super::app::{App, Confirm, ConfirmAct, Editor, Overlay};
use super::{Reply, ctrl, newline, s};
use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

pub const FILTERS: [&str; 4] = ["all", "profile", "pinned", "review"];

pub struct MemorySheet {
    pub bot: String,
    pub facts: Option<Vec<Value>>,
    pub cursor: usize,
    pub query: String,
    pub filter: usize,
    pub offset: usize,
    pub next_offset: Option<usize>,
    pub total: usize,
    pub mode: MemoryMode,
    pub error: Option<String>,
    pub busy: bool,
}

pub enum MemoryMode {
    Browse,
    Search(Editor),
    Edit(MemoryForm),
    Detail { text: String, scroll: usize, id: String, history_cursor: Option<i64> },
    Export { text: String, scroll: usize },
    Import(Editor),
}

pub struct MemoryForm {
    pub id: Option<String>,
    pub fields: [Editor; 5],
    pub focus: usize,
}

impl MemorySheet {
    pub fn body(&self) -> Value {
        json!({"botId":self.bot, "query":self.query, "filter":FILTERS[self.filter], "offset":self.offset})
    }

    pub fn editor(&mut self) -> Option<&mut Editor> {
        match &mut self.mode {
            MemoryMode::Search(ed) | MemoryMode::Import(ed) => Some(ed),
            MemoryMode::Edit(form) => Some(&mut form.fields[form.focus]),
            _ => None,
        }
    }
}

impl App {
    pub fn open_memory(&mut self) {
        let Some(bot) = self.bot().filter(|b| !b.group).map(|b| b.id.clone()) else {
            self.flash("A group has no memory of its own");
            return;
        };
        let m = MemorySheet {
            bot: bot.clone(),
            facts: None,
            cursor: 0,
            query: String::new(),
            filter: 0,
            offset: 0,
            next_offset: None,
            total: 0,
            mode: MemoryMode::Browse,
            error: None,
            busy: true,
        };
        self.sheet("memory", m.body(), Reply::Memory(bot));
        self.overlays.push(Overlay::Memory(m));
    }

    pub(in crate::tui) fn memory_key(&mut self, mut m: MemorySheet, k: KeyEvent) -> Option<Overlay> {
        if !matches!(m.mode, MemoryMode::Browse) {
            self.memory_mode_key(&mut m, k);
            return Some(Overlay::Memory(m));
        }
        let n = m.facts.as_ref().map_or(0, Vec::len);
        let selected = m.facts.as_ref().and_then(|f| f.get(m.cursor)).cloned();
        match k.code {
            KeyCode::Esc | KeyCode::Char('q' | 'M') => return None,
            KeyCode::Down | KeyCode::Char('j') => m.cursor = (m.cursor + 1).min(n.saturating_sub(1)),
            KeyCode::Up | KeyCode::Char('k') => m.cursor = m.cursor.saturating_sub(1),
            KeyCode::Char('/') => m.mode = MemoryMode::Search(Editor::with(&m.query)),
            KeyCode::Char('f') => {
                m.filter = (m.filter + 1) % FILTERS.len();
                m.offset = 0;
                self.load_memory(&mut m);
            }
            KeyCode::Char('r') => self.load_memory(&mut m),
            KeyCode::Char(']') if m.next_offset.is_some() => {
                m.offset = m.next_offset.unwrap_or(0);
                self.load_memory(&mut m);
            }
            KeyCode::Char('[') => {
                m.offset = m.offset.saturating_sub(50);
                self.load_memory(&mut m);
            }
            KeyCode::Char('n') => m.mode = MemoryMode::Edit(form(None)),
            KeyCode::Enter | KeyCode::Char('e') => {
                if let Some(fact) = &selected {
                    m.mode = MemoryMode::Edit(form(Some(fact)));
                }
            }
            KeyCode::Char('p' | 'v' | 'x') if !m.busy => {
                if let Some(fact) = &selected {
                    let method = match k.code {
                        KeyCode::Char('p') => "pinMemory",
                        KeyCode::Char('v') => "reviewMemory",
                        _ => "forgetMemory",
                    };
                    m.busy = true;
                    self.sheet(
                        method,
                        json!({"botId":m.bot, "id":fact["id"], "pinned":fact["pinned"] != true}),
                        Reply::MemorySaved(m.bot.clone()),
                    );
                }
            }
            KeyCode::Char('h') => {
                if let Some(fact) = &selected {
                    m.mode = MemoryMode::Detail {
                        text: "Loading history…".into(),
                        scroll: 0,
                        id: s(fact, "id"),
                        history_cursor: None,
                    };
                    self.sheet(
                        "memoryDetail",
                        json!({"botId":m.bot, "id":fact["id"]}),
                        Reply::MemoryDetail(m.bot.clone()),
                    );
                }
            }
            KeyCode::Char('E') => {
                m.mode = MemoryMode::Export { text: "Preparing backup…".into(), scroll: 0 };
                self.sheet("exportMemory", json!({"botId":m.bot}), Reply::MemoryExport(m.bot.clone()));
            }
            KeyCode::Char('I') => m.mode = MemoryMode::Import(Editor::default()),
            KeyCode::Char('D') if n > 0 && !m.busy => {
                let c = Confirm {
                    title: "Forget everything this bot remembers?".into(),
                    detail: "Saved memories are removed.".into(),
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

    fn load_memory(&self, m: &mut MemorySheet) {
        m.busy = true;
        m.error = None;
        self.sheet("memory", m.body(), Reply::Memory(m.bot.clone()));
    }

    fn memory_mode_key(&mut self, m: &mut MemorySheet, k: KeyEvent) {
        if k.code == KeyCode::Esc {
            m.mode = MemoryMode::Browse;
            return;
        }
        match &mut m.mode {
            MemoryMode::Search(ed) if k.code == KeyCode::Enter => {
                m.query.clone_from(&ed.text);
                m.offset = 0;
                m.mode = MemoryMode::Browse;
                self.load_memory(m);
            }
            MemoryMode::Edit(f) if k.code == KeyCode::Tab => f.focus = (f.focus + 1) % 5,
            MemoryMode::Edit(f) if k.code == KeyCode::BackTab => f.focus = (f.focus + 4) % 5,
            MemoryMode::Edit(f) if k.code == KeyCode::Char('s') && ctrl(k) && !m.busy => {
                m.busy = true;
                self.sheet(
                    "saveMemory",
                    json!({"botId":m.bot, "id":f.id,
                    "title":f.fields[0].text, "content":f.fields[1].text, "scope":f.fields[2].text,
                    "memoryType":f.fields[3].text, "topicKey":f.fields[4].text}),
                    Reply::MemorySaved(m.bot.clone()),
                );
            }
            MemoryMode::Import(ed) if k.code == KeyCode::Char('s') && ctrl(k) && !m.busy => {
                m.busy = true;
                self.sheet("importMemory", json!({"botId":m.bot, "json":ed.text}), Reply::MemorySaved(m.bot.clone()));
            }
            MemoryMode::Detail { id, history_cursor: Some(cursor), .. } if k.code == KeyCode::Char('o') => {
                self.sheet(
                    "memoryDetail",
                    json!({"botId":m.bot, "id":id, "historyCursor":cursor}),
                    Reply::MemoryDetail(m.bot.clone()),
                );
            }
            MemoryMode::Detail { text, scroll, .. } | MemoryMode::Export { text, scroll } => match k.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    *scroll = (*scroll + 1).min(text.lines().count().saturating_sub(1));
                }
                KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
                KeyCode::Char('c') => {
                    super::super::app::copy(text);
                    self.flash("Copied");
                }
                _ => {}
            },
            _ => {
                if let Some(ed) = m.editor() {
                    if newline(k) || k.code == KeyCode::Enter {
                        ed.insert("\n");
                    } else {
                        ed.key(k);
                    }
                }
            }
        }
    }
}

fn form(fact: Option<&Value>) -> MemoryForm {
    MemoryForm {
        id: fact.map(|f| s(f, "id")),
        focus: 0,
        fields: ["title", "content", "scope", "memoryType", "topicKey"].map(|key| {
            Editor::with(&fact.map_or_else(
                || match key {
                    "scope" => "project".into(),
                    "memoryType" => "learning".into(),
                    _ => String::new(),
                },
                |f| s(f, key),
            ))
        }),
    }
}
