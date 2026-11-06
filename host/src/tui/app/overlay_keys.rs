//! Keys in the pickers: go-to, actions and folders.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use super::super::manage::{AnalyticsSheet, Reply};
use super::super::view::usage_services;
use super::{
    ACTIONS, Action, After, App, Bot, ConfirmAct, Dir, Editor, FILTERS, FolderMode, FolderPicker, Goto, GotoItem,
    Overlay, PairCode, contains, fuzzy,
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
            Overlay::DevServers(sheet) => self.dev_servers_key(sheet, k),
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
            // A phone that scanned the code waits for this computer to let it in.
            o @ Overlay::Pair(_) if matches!(k.code, KeyCode::Char('y' | 'n')) && !self.pairing_requests.is_empty() => {
                self.decide_pairing(k.code == KeyCode::Char('y'));
                Some(o)
            }
            // A new code: after a failure, or in place of the one shown.
            Overlay::Pair(_) if k.code == KeyCode::Char('r') => {
                self.request_pairing_code();
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
                    KeyCode::Char('s') if !usage_services(self).is_empty() => {
                        self.overlays.push(Overlay::UsageChooser { row: 0, moving: false });
                        Some(Overlay::Usage(scroll))
                    }
                    _ => Some(Overlay::Usage(scroll)),
                }
            }
            Overlay::UsageChooser { row, moving } => {
                let services = usage_services(self);
                let last = services.len().saturating_sub(1);
                // The list can shrink under the cursor: an agent turned off, a bot deleted on another device.
                let row = row.min(last);
                match k.code {
                    KeyCode::Esc | KeyCode::Char('q' | 's') => None,
                    KeyCode::Down | KeyCode::Char('j') => {
                        Some(Overlay::UsageChooser { row: (row + 1).min(last), moving })
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        Some(Overlay::UsageChooser { row: row.saturating_sub(1), moving })
                    }
                    KeyCode::Char(key @ ('J' | 'K')) => {
                        let to = if key == 'J' { (row + 1).min(last) } else { row.saturating_sub(1) };
                        let starts = !moving && to != row;
                        if starts {
                            let mut ids: Vec<_> = services.iter().map(|service| &service["id"]).collect();
                            ids.swap(row, to);
                            self.call("setUsageOrder", json!({"ids": ids}), After::UsageOrder(to));
                        }
                        Some(Overlay::UsageChooser { row, moving: moving || starts })
                    }
                    KeyCode::Char(' ') | KeyCode::Enter => {
                        if let Some(service) = services.get(row) {
                            let shown = service["shown"].as_bool() != Some(true);
                            self.call("setUsageShown", json!({"id": service["id"], "shown": shown}), After::Nothing);
                        }
                        Some(Overlay::UsageChooser { row, moving })
                    }
                    _ => Some(Overlay::UsageChooser { row, moving }),
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

    /// The host's answer to a move. Only now do the list change and the cursor follow, so the next move starts from
    /// the order this one left, and a refused one leaves both where they were.
    pub(super) fn usage_order_answered(&mut self, to: usize, answer: Result<Value, String>) {
        let moved = match answer {
            Ok(usage) => {
                self.usage = usage;
                true
            }
            Err(error) => {
                self.flash(&error);
                false
            }
        };
        if let Some(Overlay::UsageChooser { row, moving }) = self.overlays.last_mut() {
            *moving = false;
            if moved {
                *row = to;
            }
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
        let request_id = uuid::Uuid::new_v4().to_string();
        self.overlays.push(Overlay::Folder(FolderPicker {
            request_id: request_id.clone(),
            mode: FolderMode::Recent,
            recent: vec![],
            remembers: false,
            busy: true,
            path: start.to_owned(),
            parent: None,
            dirs: vec![],
            query: Editor::default(),
            cursor: 0,
            error: None,
        }));
        let body = if start.is_empty() { json!({}) } else { json!({"path": start}) };
        self.call("listDirs", body, After::Dirs(request_id));
    }

    pub fn folder_matches(p: &FolderPicker) -> Vec<&Dir> {
        let items = if p.mode == FolderMode::Recent { &p.recent } else { &p.dirs };
        items.iter().filter(|d| fuzzy(&p.query.text, &[&d.name, &d.path])).collect()
    }

    fn folder_key(&mut self, mut p: FolderPicker, k: KeyEvent) -> Option<Overlay> {
        if k.code == KeyCode::Esc && p.mode != FolderMode::Path {
            return None;
        }
        if p.busy {
            return Some(Overlay::Folder(p));
        }
        if k.modifiers.contains(KeyModifiers::CONTROL) {
            match k.code {
                KeyCode::Char('r') => {
                    p.mode = FolderMode::Recent;
                    p.query.clear();
                    p.cursor = 0;
                }
                KeyCode::Char('b') => {
                    p.mode = FolderMode::Browse;
                    p.query.clear();
                    p.cursor = 0;
                }
                KeyCode::Char('l') => {
                    p.mode = FolderMode::Path;
                    p.query.clear();
                }
                _ => {
                    p.query.key(k);
                }
            }
            return Some(Overlay::Folder(p));
        }
        if p.mode == FolderMode::Path {
            match k.code {
                KeyCode::Esc => {
                    p.mode = FolderMode::Browse;
                    p.query.clear();
                }
                KeyCode::Enter if !p.query.text.trim().is_empty() => {
                    self.call("listDirs", json!({"path": p.query.text.trim()}), After::Dirs(p.request_id.clone()));
                    p.mode = FolderMode::Browse;
                    p.busy = true;
                }
                _ => {
                    p.query.key(k);
                }
            }
            return Some(Overlay::Folder(p));
        }
        let matches: Vec<Dir> = Self::folder_matches(&p).into_iter().cloned().collect();
        match k.code {
            KeyCode::Esc => return None,
            KeyCode::Down => p.cursor = (p.cursor + 1).min(matches.len().saturating_sub(1)),
            KeyCode::Up => p.cursor = p.cursor.saturating_sub(1),
            KeyCode::Right | KeyCode::Tab => {
                if let Some(d) = matches.get(p.cursor) {
                    self.call("listDirs", json!({"path": d.path}), After::Dirs(p.request_id.clone()));
                    p.mode = FolderMode::Browse;
                    p.busy = true;
                }
            }
            KeyCode::Left | KeyCode::Backspace if p.query.text.is_empty() => {
                if let Some(parent) = p.parent.clone() {
                    self.call("listDirs", json!({"path": parent}), After::Dirs(p.request_id.clone()));
                    p.mode = FolderMode::Browse;
                    p.busy = true;
                }
            }
            KeyCode::Enter | KeyCode::Char('.') if k.code == KeyCode::Enter || p.query.text.is_empty() => {
                let current = k.code == KeyCode::Char('.') || k.modifiers.contains(KeyModifiers::ALT);
                let chosen = if current {
                    p.path.clone()
                } else {
                    matches.get(p.cursor).map_or_else(String::new, |d| d.path.clone())
                };
                if !chosen.is_empty() {
                    // Older hosts still validate using their existing directory endpoint.
                    self.call(
                        if p.remembers { "selectWorkspace" } else { "listDirs" },
                        json!({"path": chosen}),
                        After::WorkspaceSelected(p.request_id.clone()),
                    );
                    p.busy = true;
                }
            }
            _ => {
                if p.query.key(k) {
                    p.cursor = 0;
                }
            }
        }
        Some(Overlay::Folder(p))
    }

    pub(super) fn folder_chosen(&mut self, path: String) {
        // The drive list (Windows) has no path of its own to use.
        if path.is_empty() {
            return;
        }
        if let Some(Overlay::Form(f)) = self.overlays.last_mut() {
            f.cwd = path;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::net::Client;
    use super::super::{After, App, Bot, Editor, Goto, GotoItem, Msg, Overlay, test_app};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use serde_json::json;

    /// A host that hands each request it gets to `requests`, answering them all alike.
    async fn host() -> (String, tokio::sync::mpsc::UnboundedReceiver<String>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a free port");
        let base = format!("http://{}", listener.local_addr().expect("its address"));
        let (seen, requests) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut request = Vec::new();
                while !request.ends_with(b"}") {
                    let mut chunk = [0; 4096];
                    let read = socket.read(&mut chunk).await.expect("a request");
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..read]);
                }
                let _ = seen.send(String::from_utf8_lossy(&request).into_owned());
                let answer = b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}";
                let _ = socket.write_all(answer).await;
            }
        });
        (base, requests)
    }

    #[tokio::test]
    async fn the_usage_sheet_opens_the_chooser_and_a_toggle_reaches_the_host() {
        let (base, mut requests) = host().await;
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(Client::new(&base, Some("token")), tx, String::new());
        app.usage = json!({"providers": [], "services": [
            {"id": "claude", "name": "Claude", "shown": true},
            {"id": "cursor", "name": "Cursor", "shown": false},
        ]});
        let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
        app.overlays.push(Overlay::Usage(0));
        app.overlay_key(key(KeyCode::Char('s')));
        assert!(matches!(app.overlays.last(), Some(Overlay::UsageChooser { row: 0, .. })));
        app.overlay_key(key(KeyCode::Down));
        assert!(matches!(app.overlays.last(), Some(Overlay::UsageChooser { row: 1, .. })));
        app.overlay_key(key(KeyCode::Char(' ')));
        let request = next_request(&mut requests).await;
        assert!(request.starts_with("POST /api/setUsageShown"), "{request}");
        assert!(request.contains(r#""id":"cursor""#) && request.contains(r#""shown":true"#), "{request}");
        assert!(matches!(app.overlays.last(), Some(Overlay::UsageChooser { row: 1, .. })), "the chooser stays open");
        app.overlay_key(key(KeyCode::Esc));
        assert!(matches!(app.overlays.last(), Some(Overlay::Usage(0))), "back to the limits");
    }

    #[tokio::test]
    async fn j_and_k_move_the_selected_agent_as_the_host_answers() {
        let (base, mut requests) = host().await;
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(Client::new(&base, Some("token")), tx, String::new());
        let usage = |ids: &[&str]| {
            let services: Vec<_> = ids.iter().map(|id| json!({"id": id, "name": id, "shown": true})).collect();
            json!({"providers": [], "services": services})
        };
        let press = |app: &mut App, c| app.overlay_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        let row = |app: &App| match app.overlays.last() {
            Some(Overlay::UsageChooser { row, .. }) => *row,
            other => panic!("the chooser is closed: {:?}", other.is_some()),
        };
        app.usage = usage(&["a", "b", "c"]);
        app.overlays.push(Overlay::UsageChooser { row: 0, moving: false });

        press(&mut app, 'K');
        press(&mut app, 'J');
        press(&mut app, 'J');
        assert_eq!(row(&app), 0, "the cursor waits for the host");
        let sent = next_request(&mut requests).await;
        assert!(sent.starts_with("POST /api/setUsageOrder"), "{sent}");
        assert!(sent.contains(r#""ids":["b","a","c"]"#), "the top's K and the waiting J sent nothing: {sent}");
        app.on_msg(Msg::Reply(After::UsageOrder(1), Ok(usage(&["b", "a", "c"]))));
        assert_eq!(row(&app), 1, "the cursor follows the agent");

        press(&mut app, 'J');
        assert!(next_request(&mut requests).await.contains(r#""ids":["b","c","a"]"#));
        app.on_msg(Msg::Reply(After::UsageOrder(2), Ok(usage(&["b", "c", "a"]))));
        press(&mut app, 'J');
        assert_eq!(row(&app), 2, "the end of the list does nothing");

        press(&mut app, 'K');
        assert!(next_request(&mut requests).await.contains(r#""ids":["b","a","c"]"#), "the last J sent nothing");
        app.on_msg(Msg::Reply(After::UsageOrder(1), Err("unknown method".into())));
        assert_eq!(row(&app), 2, "a refused move keeps the cursor on its agent");

        // An agent turned off leaves the cursor past the end.
        app.usage = usage(&["b", "c"]);
        press(&mut app, 'K');
        assert!(next_request(&mut requests).await.contains(r#""ids":["c","b"]"#));
    }

    async fn next_request(requests: &mut tokio::sync::mpsc::UnboundedReceiver<String>) -> String {
        tokio::time::timeout(std::time::Duration::from_secs(10), requests.recv())
            .await
            .expect("the host hears of it in time")
            .expect("the host is up")
    }

    #[tokio::test]
    async fn a_host_with_no_list_offers_no_chooser() {
        let mut app = test_app();
        app.usage = json!({"providers": []});
        app.overlays.push(Overlay::Usage(0));
        app.overlay_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        assert!(matches!(app.overlays.last(), Some(Overlay::Usage(0))));
    }

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
