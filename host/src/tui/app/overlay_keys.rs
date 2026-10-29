//! Keys in the pickers: go-to, actions and folders.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::json;

use super::super::manage::{AnalyticsSheet, Reply};
use super::{
    ACTIONS, Action, After, App, Bot, ConfirmAct, Dir, Editor, FILTERS, FolderPicker, Goto, GotoItem, Overlay,
    PairCode, contains, fuzzy,
};

impl App {
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
            // First among the actions, so they show without scrolling past the long list.
            let unhide = super::super::l10n::tr("Unhide");
            let mut hidden: Vec<&Bot> =
                self.bots.values().filter(|b| b.hidden && fuzzy(q, &[&b.name, "Unhide", unhide.as_ref()])).collect();
            hidden.sort_by(|a, b| a.name.cmp(&b.name));
            items.extend(hidden.into_iter().map(|b| GotoItem::Unhide(b.id.clone())));
            items.extend(
                ACTIONS
                    .iter()
                    .filter(|(label, _, _)| {
                        let translated = super::super::l10n::tr(label);
                        fuzzy(q, &[label, translated.as_ref()])
                    })
                    .map(|(_, _, a)| GotoItem::Action(*a)),
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
            Action::Details => self.open_routines(),
            Action::Library => self.open_library(),
            Action::RefreshAgents => self.call("refreshBackends", json!({}), After::Backends),
            Action::Pair => self.open_pair(),
            Action::Network => self.open_network(),
            Action::Usage => self.overlays.push(Overlay::Usage(0)),
            Action::CheckUpdate => self.call("checkHostUpdate", json!({}), After::Update),
            Action::Analytics => self.toggle_analytics(),
            Action::AboutAnalytics => self.overlays.push(Overlay::Analytics(AnalyticsSheet::default())),
            Action::Appearance(mode) => match super::super::appearance::select(mode) {
                Ok(()) => self.flash(&format!("Appearance: {}", mode.name())),
                Err(error) => self.flash(&format!("Couldn't save appearance: {error}")),
            },
            Action::Language => {
                let title = super::super::l10n::cycle_language();
                self.flash(title.as_ref());
            }
            Action::ThinkingDetails => self.flash(if super::super::details::toggle() {
                "Thinking details on: > unfolds a working bot's steps"
            } else {
                "Thinking details off"
            }),
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
                        ConfirmAct::KeepHarnessMemory(id) => {
                            self.call("updateBot", json!({"id": id, "harnessMemory": true}), changed);
                        }
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
            Overlay::NewBot(n) => self.new_bot_key(n, k),
            Overlay::Folder(p) => self.folder_key(p, k),
            Overlay::Form(f) => self.form_key(f, k),
            Overlay::Group(g) => self.group_key(g, k),
            Overlay::Memory(m) => self.memory_key(m, k),
            Overlay::BotChat(c) => Self::bot_chat_key(c, k),
            Overlay::Routines(l) => self.routines_key(l, k),
            Overlay::Routine(f) => self.routine_key(f, k),
            Overlay::Library(l) => self.library_key(l, k),
            Overlay::Gear(g) => Self::gear_key(g, k),
            Overlay::Market(m) => self.market_key(m, k),
            Overlay::Agent(a) => self.agent_key(a, k),
            Overlay::Fields(f) => self.fields_key(f, k),
            Overlay::Skill(s) => self.skill_key(s, k),
            Overlay::Analytics(s) => s.key(k),
            Overlay::Network(n) => self.network_key(n, k),
            // A new code: after a failure, or in place of the one shown.
            Overlay::Pair(_) if k.code == KeyCode::Char('r') => {
                self.call("pairing", json!({}), After::Pairing);
                Some(Overlay::Pair(PairCode::Loading))
            }
            Overlay::LeadAgent(scroll) => {
                let (width, height) = crossterm::terminal::size().unwrap_or((80, 24));
                let max = super::super::view::lead_agent_scroll_max(width, height);
                match k.code {
                    KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q' | 'I') => None,
                    KeyCode::Down | KeyCode::Char('j') => Some(Overlay::LeadAgent((scroll + 1).min(max))),
                    KeyCode::Up | KeyCode::Char('k') => Some(Overlay::LeadAgent(scroll.saturating_sub(1))),
                    _ => Some(Overlay::LeadAgent(scroll.min(max))),
                }
            }
            Overlay::Usage(scroll) => {
                let (_, height) = crossterm::terminal::size().unwrap_or((80, 24));
                let max = super::super::view::usage_scroll_max(self, height);
                match k.code {
                    KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q' | 'U' | 'P') => None,
                    KeyCode::Down | KeyCode::Char('j') => Some(Overlay::Usage((scroll + 1).min(max))),
                    KeyCode::Up | KeyCode::Char('k') => Some(Overlay::Usage(scroll.saturating_sub(1))),
                    KeyCode::PageDown | KeyCode::Char(' ') => Some(Overlay::Usage((scroll + 10).min(max))),
                    KeyCode::PageUp => Some(Overlay::Usage(scroll.saturating_sub(10))),
                    _ => Some(Overlay::Usage(scroll)),
                }
            }
            o @ Overlay::Pair(_) => {
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
                    Some(GotoItem::Unhide(id)) => {
                        self.call("updateBot", json!({"id": id, "hidden": false}), After::Nothing);
                    }
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

#[cfg(test)]
mod tests {
    use super::super::{Bot, Editor, Goto, GotoItem, test_app};
    use serde_json::json;

    #[tokio::test]
    async fn the_palette_offers_to_unhide_hidden_bots() {
        let mut app = test_app();
        for (id, hidden) in [("shown", false), ("hidden", true)] {
            let bot = Bot::parse(&json!({"id": id, "name": id, "hidden": hidden})).expect("a bot with an id parses");
            app.bots.insert(id.into(), bot);
        }
        let g = Goto { query: Editor::default(), filter: 0, cursor: 0 };
        let unhide: Vec<String> = app
            .goto_items(&g)
            .into_iter()
            .filter_map(|i| match i {
                GotoItem::Unhide(id) => Some(id),
                _ => None,
            })
            .collect();
        assert_eq!(unhide, ["hidden"]);
    }
}
