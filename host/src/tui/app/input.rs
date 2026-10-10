//! Keys, pastes and the mouse on the main screen and the composer.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;
use serde_json::json;

use super::editor::move_line;
use super::{
    After, App, Click, Confirm, ConfirmAct, Editor, Field, Focus, Goto, GroupField, Kind, Mark, Overlay, PendingSend,
    Status, TraceMode, Width, mismatch_text, page, tilde,
};

impl App {
    pub fn on_paste(&mut self, s: &str) {
        if let Some(sh) = self.shell.as_ref().filter(|s| s.exited.is_none()) {
            let _ = sh.input.send(s.as_bytes().to_vec());
            return;
        }
        let s = s.replace("\r\n", "\n").replace('\r', "\n");
        if let Some(ed) = self.top_editor() {
            ed.insert(&s);
        } else if let Some(key) = self.draft_key() {
            self.typing = true;
            self.focus = Focus::Chat;
            // A file dragged onto the terminal pastes its path: attach it.
            match dropped_files(&s) {
                Some(paths) => self.files.entry(key).or_default().extend(paths),
                None => self.drafts.entry(key).or_default().insert(&s),
            }
        }
    }

    fn top_editor(&mut self) -> Option<&mut Editor> {
        match self.overlays.last_mut()? {
            Overlay::Goto(g) => Some(&mut g.query),
            Overlay::Help(e) => Some(e),
            Overlay::Agents(a) => Some(&mut a.query),
            Overlay::Folder(p) => Some(&mut p.query),
            Overlay::Group(g) => match g.field {
                GroupField::Name => Some(&mut g.name),
                GroupField::About => Some(&mut g.about),
                GroupField::Bots => None,
            },
            Overlay::Form(f) => match f.current() {
                Field::Name => Some(&mut f.name),
                Field::Instructions => Some(&mut f.instructions),
                Field::Model => Some(&mut f.model),
                _ => None,
            },
            Overlay::Market(m) => Some(&mut m.query),
            Overlay::Fields(f) => f.current().map(|i| &mut i.ed),
            Overlay::Memory(m) => m.editor(),
            Overlay::Routine(f) => Some(match f.field {
                super::super::manage::RoutineField::Name => &mut f.name,
                super::super::manage::RoutineField::Instruction => &mut f.instruction,
                super::super::manage::RoutineField::When => &mut f.when,
            }),
            _ => None,
        }
    }

    pub fn on_key(&mut self, k: KeyEvent) {
        if self.shell.is_some() {
            self.shell_key(k);
            return;
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && k.code == KeyCode::Char('k') {
            if matches!(self.overlays.last(), Some(Overlay::Goto(_))) {
                self.overlays.pop();
            } else {
                self.overlays.push(Overlay::Goto(Goto { query: Editor::default(), filter: 0, cursor: 0 }));
            }
            return;
        }
        if !self.overlays.is_empty() {
            self.overlay_key(k);
            return;
        }
        if self.typing {
            self.composer_key(k);
            return;
        }
        self.nav_key(k);
    }

    fn composer_key(&mut self, k: KeyEvent) {
        let (Some(id), Some(key)) = (self.selected.clone(), self.draft_key()) else {
            self.typing = false;
            return;
        };
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let newline = k.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::ALT);
        let thread = self.thread.clone();
        let draft = self.drafts.entry(key.clone()).or_default();
        match k.code {
            KeyCode::Esc => self.typing = false,
            KeyCode::Enter if newline => draft.insert("\n"),
            KeyCode::Char('j') if ctrl => draft.insert("\n"),
            KeyCode::Enter => {
                let text = draft.text.trim().to_owned();
                let files = self.files.get(&key).cloned().unwrap_or_default();
                if text.is_empty() && files.is_empty() {
                    return;
                }
                if let Some(m) = &self.mismatch {
                    // The draft stays; it sends once the update is done.
                    let why = mismatch_text(m, &self.host);
                    self.flash(&why);
                    return;
                }
                if self.sends.get(&key).is_some_and(|s| s.busy) {
                    self.flash("Sending…");
                    return;
                }
                let nonce = self
                    .sends
                    .get(&key)
                    .filter(|s| s.text == text && s.files == files)
                    .map_or_else(|| uuid::Uuid::new_v4().to_string(), |s| s.nonce.clone());
                self.sends.insert(
                    key.clone(),
                    PendingSend { text: text.clone(), files: files.clone(), nonce: nonce.clone(), busy: true },
                );
                self.chat_scroll = 0;
                let mut body = json!({"botId": id, "text": text, "clientNonce": nonce});
                if let Some(root) = thread {
                    body["threadId"] = root.into();
                }
                let after = After::Sent(key);
                if files.is_empty() {
                    self.call("send", body, after);
                } else {
                    self.client.spawn_send_files(body, files, after, self.tx.clone());
                }
                self.flash("Sending… Your draft stays here until delivered.");
            }
            KeyCode::Backspace if draft.text.is_empty() => {
                if let Some(files) = self.files.get_mut(&key) {
                    files.pop();
                }
            }
            KeyCode::Char('c') if ctrl => {
                if draft.text.is_empty() {
                    self.flash("esc then s stops the bot");
                } else {
                    draft.clear();
                }
            }
            KeyCode::Up if draft.text.is_empty() => self.scroll_chat(1),
            KeyCode::Down if draft.text.is_empty() => self.scroll_chat(-1),
            KeyCode::PageUp => self.scroll_chat(page(self.chat_height)),
            KeyCode::PageDown => self.scroll_chat(-page(self.chat_height)),
            KeyCode::Up => move_line(draft, -1),
            KeyCode::Down => move_line(draft, 1),
            _ => {
                draft.key(k);
            }
        }
    }

    pub(super) fn scroll_chat(&mut self, delta: isize) {
        self.chat_scroll = self.chat_scroll.saturating_add_signed(delta);
    }

    pub(super) fn scroll_trace(&mut self, delta: isize) {
        self.trace_scroll = self.trace_scroll.saturating_add_signed(delta);
    }

    fn nav_key(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let half = page(self.chat_height) / 2;
        let narrow = self.width == Width::Narrow;
        let roster_page = narrow && !self.chat_page;
        if self.pick.is_some() && self.pick_key(k) {
            return;
        }
        match k.code {
            KeyCode::Char('c') if ctrl => self.flash("q quits"),
            KeyCode::Char('d') if ctrl => self.scroll_focused(-half),
            KeyCode::Char('u') if ctrl => self.scroll_focused(half),
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('C') => self.open_connection(),
            KeyCode::Char('I') if self.error.is_some() && self.bots.is_empty() => self.install_host(),
            KeyCode::Char('?') => self.overlays.push(Overlay::Help(Editor::default())),
            KeyCode::Char('!') => self.jump(Mark::Need),
            KeyCode::Char('u') => self.jump(Mark::Unread),
            KeyCode::Char('[') => self.move_bot(-1),
            KeyCode::Char(']') => self.move_bot(1),
            KeyCode::Char(c @ '1'..='9') => {
                let i = (c as usize) - ('1' as usize);
                let pending = if roster_page { None } else { self.pending().map(|e| (e.id.clone(), e.options())) };
                match pending {
                    Some((entry, opts)) if !opts.is_empty() => {
                        if let Some((option, _, _)) = opts.get(i) {
                            self.answer(&entry, option);
                        }
                    }
                    _ => {
                        if let Some(id) = self.roster().get(i).map(|b| b.id.clone()) {
                            self.select(&id);
                        }
                    }
                }
            }
            KeyCode::Char(c @ ('y' | 'a' | 'n' | 'N')) if !roster_page && self.pending().is_some() => {
                self.answer_kind(c);
            }
            KeyCode::Char('n') => self.new_bot(),
            KeyCode::Char('m') => self.new_group(),
            KeyCode::Char('i') if !roster_page => self.start_typing(),
            KeyCode::Char('r') if !roster_page && self.selected.is_some() => {
                if self.thread.is_some() {
                    self.start_typing();
                } else {
                    self.chat_page = true;
                    self.focus = Focus::Chat;
                    self.step_pick(0);
                }
            }
            KeyCode::Enter => {
                if roster_page || self.focus == Focus::Roster {
                    if self.selected.is_some() {
                        self.chat_page = true;
                        self.focus = Focus::Chat;
                        self.start_typing();
                    }
                } else {
                    self.start_typing();
                }
            }
            KeyCode::Esc | KeyCode::Backspace => {
                if self.trace == TraceMode::Full {
                    self.trace = TraceMode::Off;
                    self.focus = Focus::Chat;
                } else if self.thread.is_some() {
                    self.thread = None;
                    self.chat_scroll = 0;
                    self.trace_turn = None;
                    self.trace_scroll = 0;
                } else if narrow && self.chat_page {
                    self.chat_page = false;
                    self.focus = Focus::Roster;
                } else if self.focus == Focus::Trace {
                    self.focus = Focus::Chat;
                }
            }
            KeyCode::Tab | KeyCode::BackTab => self.cycle_focus(k.code == KeyCode::BackTab),
            KeyCode::Char('h') | KeyCode::Left => self.focus_side(-1),
            KeyCode::Char('l') | KeyCode::Right => self.focus_side(1),
            KeyCode::Char('j') | KeyCode::Down => self.line(1),
            KeyCode::Char('k') | KeyCode::Up => self.line(-1),
            KeyCode::PageDown => self.scroll_focused(-page(self.chat_height)),
            KeyCode::PageUp => self.scroll_focused(page(self.chat_height)),
            KeyCode::Char('g') | KeyCode::Home => {
                if self.focus == Focus::Trace {
                    self.trace_scroll = 0;
                } else {
                    self.chat_scroll = usize::MAX / 2;
                }
            }
            KeyCode::Char('G') | KeyCode::End => {
                if self.focus == Focus::Trace {
                    self.trace_scroll = usize::MAX / 2;
                } else {
                    self.chat_scroll = 0;
                }
            }
            KeyCode::Char('{') => self.step_turn(-1),
            KeyCode::Char('}') => self.step_turn(1),
            KeyCode::Char('t') => {
                self.trace = if self.trace == TraceMode::Pane { TraceMode::Off } else { TraceMode::Pane };
                if self.trace == TraceMode::Off && self.focus == Focus::Trace {
                    self.focus = Focus::Chat;
                }
                if self.width == Width::Narrow || self.width == Width::Mid {
                    self.trace = if self.trace == TraceMode::Off { TraceMode::Off } else { TraceMode::Full };
                }
            }
            KeyCode::Char('T') => {
                self.trace = if self.trace == TraceMode::Full { TraceMode::Off } else { TraceMode::Full };
                self.focus = if self.trace == TraceMode::Full { Focus::Trace } else { Focus::Chat };
            }
            KeyCode::Char('o') => {
                let Some(id) = self.selected.clone() else { return };
                // The newest turn in this lane that did something.
                self.trace_turn = self
                    .lane(&id)
                    .into_iter()
                    .filter(|e| matches!(e.kind, Kind::Tool | Kind::Plan))
                    .map(|e| e.turn)
                    .max();
                self.trace_scroll = 0;
                if self.trace == TraceMode::Off {
                    self.trace = if matches!(self.width, Width::Narrow | Width::Mid) {
                        TraceMode::Full
                    } else {
                        TraceMode::Pane
                    };
                }
                self.focus = Focus::Trace;
            }
            KeyCode::Char('b') => self.compact = !self.compact,
            KeyCode::Char('U') => self.overlays.push(Overlay::Usage),
            KeyCode::Char('M') => self.open_memory(),
            KeyCode::Char('R') => self.open_routines(),
            KeyCode::Char('A') => self.open_market(),
            KeyCode::Char('v') if !roster_page && self.selected.is_some() => {
                self.chat_page = true;
                self.focus = Focus::Chat;
                self.step_pick(0);
            }
            KeyCode::Char('P') => self.open_pair(),
            KeyCode::Char('s') => {
                if let Some(b) = self.bot() {
                    if matches!(b.status, Status::Working | Status::NeedsInput) {
                        let id = b.id.clone();
                        self.call("stop", json!({"botId": id}), After::Nothing);
                        self.flash("Stopping…");
                    } else {
                        self.flash("Not working right now");
                    }
                }
            }
            KeyCode::Char('S') => {
                if self.bot().is_some_and(|b| b.group) {
                    self.flash("A group has no session of its own");
                } else if let Some(b) = self.bot() {
                    let c = Confirm {
                        title: format!("Start a new session for {}?", b.name),
                        detail: "The agent forgets this conversation's context.".into(),
                        note: "The transcript stays here.".into(),
                        button: "New session",
                        act: ConfirmAct::NewSession(b.id.clone()),
                    };
                    self.overlays.push(Overlay::Confirm(c));
                }
            }
            KeyCode::Char('x') => {
                if let Some(b) = self.bot() {
                    let (detail, note) = if b.group {
                        ("Its conversation goes away.".into(), "Its bots stay.".into())
                    } else {
                        (
                            "Its conversation and settings go away.".into(),
                            format!("Files in {} stay where they are.", tilde(&b.cwd, &self.home)),
                        )
                    };
                    let c = Confirm {
                        title: format!("Delete {}?", b.name),
                        detail,
                        note,
                        button: "Delete",
                        act: ConfirmAct::Delete(b.id.clone()),
                    };
                    self.overlays.push(Overlay::Confirm(c));
                }
            }
            KeyCode::Char('e') => self.edit_bot(),
            KeyCode::Char('p') => {
                if let Some(b) = self.bot() {
                    let (id, pinned) = (b.id.clone(), !b.pinned);
                    self.call("updateBot", json!({"id": id, "pinned": pinned}), After::Nothing);
                }
            }
            KeyCode::Char('c') => self.copy_last(),
            _ => {}
        }
    }

    // ---------- mouse ----------

    pub fn on_mouse(&mut self, m: MouseEvent) {
        let at = Position::new(m.column, m.row);
        match m.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let d: isize = if m.kind == MouseEventKind::ScrollUp { -3 } else { 3 };
                if self.hits.trace.contains(at) {
                    self.scroll_trace(d);
                } else if self.hits.chat.contains(at) {
                    self.scroll_chat(-d);
                } else if self.hits.roster.contains(at) {
                    self.move_bot(d.signum());
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let hit = self.hits.clicks.iter().rev().find(|(r, _)| r.contains(at)).map(|(_, c)| c.clone());
                match hit {
                    Some(Click::Bot(id)) => {
                        self.overlays.clear();
                        self.select(&id);
                        self.focus = Focus::Roster;
                        if self.width == Width::Narrow {
                            self.chat_page = true;
                            self.focus = Focus::Chat;
                        }
                    }
                    Some(Click::Option { entry, option }) => self.answer(&entry, &option),
                    Some(Click::Filter(i)) => {
                        if let Some(Overlay::Goto(g)) = self.overlays.last_mut() {
                            g.filter = i;
                            g.cursor = 0;
                        } else {
                            self.overlays.push(Overlay::Goto(Goto { query: Editor::default(), filter: i, cursor: 0 }));
                        }
                    }
                    Some(Click::Composer) => self.start_typing(),
                    Some(Click::Thread(root)) => self.open_thread(root),
                    Some(Click::BotChat(entry)) => self.open_bot_chat(&entry),
                    None => {
                        if self.overlays.is_empty() {
                            if self.hits.trace.contains(at) {
                                self.focus = Focus::Trace;
                            } else if self.hits.chat.contains(at) {
                                self.focus = Focus::Chat;
                            } else if self.hits.roster.contains(at) {
                                self.focus = Focus::Roster;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Paths a terminal pastes for dropped files (shell-escaped, quoted or `file://`), when every one is a file.
fn dropped_files(s: &str) -> Option<Vec<std::path::PathBuf>> {
    let paths: Vec<std::path::PathBuf> = shlex::split(s.trim())?
        .into_iter()
        .map(|p| match p.strip_prefix("file://") {
            Some(url) => url.replace("%20", " "),
            None => p,
        })
        .map(std::path::PathBuf::from)
        .collect();
    (!paths.is_empty() && paths.iter().all(|p| p.is_absolute() && p.is_file())).then_some(paths)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    // Terminals escape dropped paths the way Unix shells do.
    #[test]
    fn dropped_paths_are_files_only() {
        let dir = std::env::temp_dir().join(format!("codync-drop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let a = dir.join("a b.txt");
        std::fs::write(&a, "x").expect("write");
        let esc = a.to_string_lossy().replace(' ', "\\ ");
        assert_eq!(dropped_files(&esc), Some(vec![a.clone()]));
        assert_eq!(dropped_files(&format!("'{}'", a.display())), Some(vec![a.clone()]));
        assert_eq!(dropped_files(&format!("file://{}", a.to_string_lossy().replace(' ', "%20"))), Some(vec![a]));
        assert_eq!(dropped_files(&dir.to_string_lossy()), None);
        assert_eq!(dropped_files("hello world"), None);
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }
}
