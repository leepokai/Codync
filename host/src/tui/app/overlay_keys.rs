//! Keys in the pickers: go-to, actions, agents and folders.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use super::super::manage::Reply;
use super::{
    ACTIONS, Action, After, AgentPicker, App, Bot, ConfirmAct, Dir, Editor, FILTERS, FolderPicker, Goto, GotoItem,
    NEW_GROUP, Overlay, contains, fuzzy,
};

impl App {
    /// Agents offered for a new bot: runnable ones on this computer first.
    pub fn agent_choices(&self, query: &str) -> Vec<&Value> {
        let mut v: Vec<&Value> = self
            .backends
            .iter()
            .filter(|b| b["available"].as_bool().unwrap_or(false) || b["registry"].is_string())
            .filter(|b| fuzzy(query, &[b["name"].as_str().unwrap_or_default(), b["id"].as_str().unwrap_or_default()]))
            .collect();
        v.sort_by_key(|b| !b["installed"].as_bool().unwrap_or(false));
        v
    }

    pub fn goto_items(&self, g: &Goto) -> Vec<GotoItem> {
        let q = g.query.text.trim();
        let want = FILTERS[g.filter].1;
        let mut bots: Vec<&Bot> = self
            .bots
            .values()
            .filter(|b| want.is_none_or(|m| b.mark() == m))
            .filter(|b| fuzzy(q, &[&b.name, &b.backend]) || contains(q, &[&b.cwd, &b.last_message]))
            .collect();
        bots.sort_by(|a, b| {
            b.mark().priority().cmp(&a.mark().priority()).then(b.last_at.cmp(&a.last_at)).then(a.name.cmp(&b.name))
        });
        let mut items: Vec<GotoItem> = bots.into_iter().map(|b| GotoItem::Bot(b.id.clone())).collect();
        if want.is_none() {
            items.extend(
                ACTIONS.iter().filter(|(label, _, _)| fuzzy(q, &[label])).map(|(_, _, a)| GotoItem::Action(*a)),
            );
        }
        items
    }

    fn run_action(&mut self, a: Action) {
        match a {
            Action::NewBot => self.new_bot(),
            Action::NewGroup => self.new_group(),
            Action::Market => self.open_market(),
            Action::Memory => self.open_memory(),
            Action::Routines => self.open_routines(),
            Action::RefreshAgents => self.call("refreshBackends", json!({}), After::Backends),
            Action::Pair => self.open_pair(),
            Action::Usage => self.overlays.push(Overlay::Usage),
            Action::CheckUpdate => self.call("checkHostUpdate", json!({}), After::Update),
            Action::Analytics => self.toggle_analytics(),
            Action::Keys => self.overlays.push(Overlay::Help(Editor::default())),
        }
    }

    pub(super) fn overlay_key(&mut self, k: KeyEvent) {
        let Some(top) = self.overlays.pop() else { return };
        let depth = self.overlays.len();
        let keep = match top {
            Overlay::Goto(g) => self.goto_key(g, k),
            Overlay::Help(mut e) => {
                if k.code == KeyCode::Esc || k.code == KeyCode::Char('?') && e.text.is_empty() {
                    None
                } else {
                    if k.code != KeyCode::Char('/') || !e.text.is_empty() {
                        e.key(k);
                    }
                    Some(Overlay::Help(e))
                }
            }
            Overlay::Confirm(c) => match k.code {
                KeyCode::Enter => {
                    let changed = After::Sheet(Reply::Changed);
                    match &c.act {
                        ConfirmAct::Delete(id) => self.call("deleteBot", json!({"botId": id}), After::Nothing),
                        ConfirmAct::NewSession(id) => self.call("newSession", json!({"botId": id}), After::Nothing),
                        ConfirmAct::ClearMemory(id) => self.call("clearMemory", json!({"botId": id}), changed),
                        ConfirmAct::DeleteRoutine(bot, id) => {
                            self.call("deleteRoutine", json!({"botId": bot, "id": id}), changed);
                        }
                        ConfirmAct::RotateRoutineKey(bot, id) => self.call(
                            "routineWebhook",
                            json!({"botId": bot, "id": id, "rotate": true}),
                            After::Sheet(Reply::Webhook),
                        ),
                        ConfirmAct::RemoveConnector(id) => self.call("removeConnector", json!({"id": id}), changed),
                        ConfirmAct::RemoveSkill(id) => self.call("removeSkill", json!({"id": id}), changed),
                    }
                    None
                }
                KeyCode::Esc | KeyCode::Char('q') => None,
                _ => Some(Overlay::Confirm(c)),
            },
            Overlay::Agents(a) => self.agents_key(a, k),
            Overlay::Folder(p) => self.folder_key(p, k),
            Overlay::Form(f) => self.form_key(f, k),
            Overlay::Group(g) => self.group_key(g, k),
            Overlay::Memory(m) => self.memory_key(m, k),
            Overlay::BotChat(c) => Self::bot_chat_key(c, k),
            Overlay::Routines(l) => self.routines_key(l, k),
            Overlay::Routine(f) => self.routine_key(f, k),
            Overlay::Market(m) => self.market_key(m, k),
            Overlay::Agent(a) => self.agent_key(a, k),
            Overlay::Fields(f) => self.fields_key(f, k),
            Overlay::Consent(share) => self.consent_key(share, k),
            o @ (Overlay::Usage | Overlay::Pair(_)) => {
                if matches!(k.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q' | 'U' | 'P')) {
                    None
                } else {
                    Some(o)
                }
            }
        };
        if let Some(o) = keep {
            // Anything the handler opened stays on top of the overlay it came from.
            self.overlays.insert(depth.min(self.overlays.len()), o);
        }
    }

    fn goto_key(&mut self, mut g: Goto, k: KeyEvent) -> Option<Overlay> {
        let items_len = self.goto_items(&g).len();
        match k.code {
            KeyCode::Esc => return None,
            KeyCode::Tab => {
                g.filter = (g.filter + 1) % FILTERS.len();
                g.cursor = 0;
            }
            KeyCode::BackTab => {
                g.filter = (g.filter + FILTERS.len() - 1) % FILTERS.len();
                g.cursor = 0;
            }
            KeyCode::Down => g.cursor = (g.cursor + 1).min(items_len.saturating_sub(1)),
            KeyCode::Up => g.cursor = g.cursor.saturating_sub(1),
            KeyCode::Char('n' | 'j') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                g.cursor = (g.cursor + 1).min(items_len.saturating_sub(1));
            }
            KeyCode::Char('p') if k.modifiers.contains(KeyModifiers::CONTROL) => g.cursor = g.cursor.saturating_sub(1),
            KeyCode::Enter => {
                let items = self.goto_items(&g);
                match items.get(g.cursor) {
                    Some(GotoItem::Bot(id)) => {
                        let id = id.clone();
                        self.select(&id);
                        self.start_typing();
                    }
                    Some(GotoItem::Action(a)) => self.run_action(*a),
                    None => {}
                }
                return None;
            }
            _ => {
                if g.query.key(k) {
                    g.cursor = 0;
                }
            }
        }
        Some(Overlay::Goto(g))
    }

    /// The agent picker's last row, "New group chat", when the filter matches it.
    pub fn agent_group_row(query: &str) -> bool {
        fuzzy(query, &[NEW_GROUP])
    }

    fn agents_key(&mut self, mut a: AgentPicker, k: KeyEvent) -> Option<Overlay> {
        let agents = self.agent_choices(&a.query.text).len();
        let n = agents + usize::from(Self::agent_group_row(&a.query.text));
        match k.code {
            KeyCode::Esc => return None,
            KeyCode::Down => a.cursor = (a.cursor + 1).min(n.saturating_sub(1)),
            KeyCode::Up => a.cursor = a.cursor.saturating_sub(1),
            KeyCode::Enter if a.cursor == agents && Self::agent_group_row(&a.query.text) => {
                self.new_group();
                return None;
            }
            KeyCode::Enter => {
                let backend =
                    self.agent_choices(&a.query.text).get(a.cursor).and_then(|b| b["id"].as_str()).map(str::to_owned);
                if let Some(backend) = backend {
                    self.overlays.push(Overlay::Agents(a));
                    self.new_bot_form(backend);
                    return None;
                }
            }
            _ => {
                if a.query.key(k) {
                    a.cursor = 0;
                }
            }
        }
        Some(Overlay::Agents(a))
    }

    pub(super) fn open_folder(&mut self, start: &str) {
        self.overlays.push(Overlay::Folder(FolderPicker {
            path: start.to_owned(),
            parent: None,
            dirs: vec![],
            query: Editor::default(),
            cursor: 0,
            error: None,
        }));
        let body = if start.is_empty() { json!({}) } else { json!({"path": start}) };
        self.call("listDirs", body, After::Dirs);
    }

    pub fn folder_matches(p: &FolderPicker) -> Vec<&Dir> {
        p.dirs.iter().filter(|d| fuzzy(&p.query.text, &[&d.name])).collect()
    }

    fn folder_key(&mut self, mut p: FolderPicker, k: KeyEvent) -> Option<Overlay> {
        let matches: Vec<Dir> = Self::folder_matches(&p).into_iter().cloned().collect();
        match k.code {
            KeyCode::Esc => return None,
            KeyCode::Down => p.cursor = (p.cursor + 1).min(matches.len().saturating_sub(1)),
            KeyCode::Up => p.cursor = p.cursor.saturating_sub(1),
            KeyCode::Right | KeyCode::Tab => {
                if let Some(d) = matches.get(p.cursor) {
                    let path = d.path.clone();
                    self.call("listDirs", json!({"path": path}), After::Dirs);
                }
            }
            KeyCode::Left | KeyCode::Backspace if p.query.text.is_empty() => {
                if let Some(parent) = p.parent.clone() {
                    self.call("listDirs", json!({"path": parent}), After::Dirs);
                }
            }
            KeyCode::Enter => {
                // Enter on a highlighted child uses it; with nothing typed and no match, the current folder.
                let chosen = if p.query.text.is_empty() && k.modifiers.contains(KeyModifiers::ALT) {
                    p.path.clone()
                } else {
                    matches.get(p.cursor).map_or_else(|| p.path.clone(), |d| d.path.clone())
                };
                self.folder_chosen(chosen);
                return None;
            }
            KeyCode::Char('.') if p.query.text.is_empty() => {
                let chosen = p.path.clone();
                self.folder_chosen(chosen);
                return None;
            }
            _ => {
                if p.query.key(k) {
                    p.cursor = 0;
                }
            }
        }
        Some(Overlay::Folder(p))
    }

    fn folder_chosen(&mut self, path: String) {
        if let Some(Overlay::Form(f)) = self.overlays.last_mut() {
            f.cwd = path;
        }
    }
}
