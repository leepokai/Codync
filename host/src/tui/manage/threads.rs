//! A message's thread, with its composer, and the chat's thread list.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::json;

use super::super::app::{After, App, Confirm, ConfirmAct, Editor, Overlay};
use super::{Reply, ThreadSheet, ThreadsSheet, ctrl};

impl App {
    pub(in crate::tui) fn thread_key(&mut self, mut t: ThreadSheet, k: KeyEvent) -> Option<Overlay> {
        if let Some(r) = &mut t.renaming {
            match k.code {
                KeyCode::Esc => t.renaming = None,
                KeyCode::Enter => {
                    let title = r.text.trim().to_owned();
                    t.renaming = None;
                    if !title.is_empty() {
                        t.title.clone_from(&title);
                        self.call(
                            "renameThread",
                            json!({"threadId": t.root, "title": title}),
                            After::Sheet(Reply::ThreadChanged(t.root.clone())),
                        );
                    }
                }
                _ => {
                    r.key(k);
                }
            }
            return Some(Overlay::Thread(Box::new(t)));
        }
        let max = t.max_scroll.get();
        let half = (t.max_scroll.get() / 2).max(1);
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return None,
            KeyCode::Up | KeyCode::Char('k') if k.modifiers.is_empty() => t.scroll = (t.scroll + 1).min(max),
            KeyCode::Down | KeyCode::Char('j') if k.modifiers.is_empty() => t.scroll = t.scroll.saturating_sub(1),
            KeyCode::PageUp => t.scroll = (t.scroll + half).min(max),
            KeyCode::PageDown => t.scroll = t.scroll.saturating_sub(half),
            KeyCode::Char('u') if ctrl(k) => t.scroll = (t.scroll + half).min(max),
            KeyCode::Char('d') if ctrl(k) => t.scroll = t.scroll.saturating_sub(half),
            KeyCode::Char('r') if k.modifiers.is_empty() => t.renaming = Some(Editor::with(&t.title)),
            KeyCode::Char('a') if k.modifiers.is_empty() => self.archive_thread(&t, !t.archived),
            KeyCode::Char('d') if k.modifiers.is_empty() => {
                self.confirm_thread_delete(&t);
                self.overlays.push(Overlay::Thread(Box::new(t)));
                return None;
            }
            KeyCode::Enter if k.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::ALT) => {
                t.composer.insert("\n");
            }
            KeyCode::Enter => self.send_thread(),
            _ => {
                t.composer.key(k);
            }
        }
        Some(Overlay::Thread(Box::new(t)))
    }

    pub(in crate::tui) fn threads_key(&mut self, mut l: ThreadsSheet, k: KeyEvent) -> Option<Overlay> {
        let n = l.items.as_ref().map_or(0, Vec::len);
        let current = l.items.as_ref().and_then(|v| v.get(l.cursor)).cloned();
        let bot = l.bot.clone();
        match (k.code, current) {
            (KeyCode::Esc | KeyCode::Char('q' | 'T'), _) => return None,
            (KeyCode::Down | KeyCode::Char('j'), _) => l.cursor = (l.cursor + 1).min(n.saturating_sub(1)),
            (KeyCode::Up | KeyCode::Char('k'), _) => l.cursor = l.cursor.saturating_sub(1),
            (KeyCode::Enter | KeyCode::Char('o'), Some(t)) => {
                let Some(root) = t["id"].as_str().map(str::to_owned) else {
                    return Some(Overlay::Threads(Box::new(l)));
                };
                self.overlays.push(Overlay::Threads(Box::new(l)));
                self.open_thread(&bot, &root);
                return None;
            }
            (KeyCode::Char('a'), _) => {
                let archived = !l.archived;
                l.archived = archived;
                l.items = None;
                l.cursor = 0;
                self.overlays.push(Overlay::Threads(Box::new(l)));
                self.fetch_threads(&bot, archived);
                return None;
            }
            (KeyCode::Char('r'), Some(t)) => {
                let Some(root) = t["id"].as_str().map(str::to_owned) else {
                    return Some(Overlay::Threads(Box::new(l)));
                };
                let title = t["title"].as_str().filter(|s| !s.is_empty()).unwrap_or("Thread");
                self.overlays.push(Overlay::Threads(Box::new(l)));
                self.open_thread(&bot, &root);
                if let Some(t) = self.top_thread_mut() {
                    t.renaming = Some(Editor::with(title));
                }
                return None;
            }
            (KeyCode::Char('x'), Some(t)) => {
                let Some(root) = t["id"].as_str().map(str::to_owned) else {
                    return Some(Overlay::Threads(Box::new(l)));
                };
                let archived = !t["archived"].as_bool().unwrap_or_default();
                self.call(
                    "archiveThread",
                    json!({"threadId": root, "archived": archived}),
                    After::Sheet(Reply::ThreadChanged(root)),
                );
                self.overlays.push(Overlay::Threads(Box::new(l)));
                self.fetch_threads(&bot, archived);
                return None;
            }
            (KeyCode::Char('d'), Some(t)) => {
                let Some(root) = t["id"].as_str().map(str::to_owned) else {
                    return Some(Overlay::Threads(Box::new(l)));
                };
                self.confirm_thread_delete_in(&bot, &root, t["title"].as_str().unwrap_or("Thread"));
                self.overlays.push(Overlay::Threads(Box::new(l)));
                return None;
            }
            _ => {}
        }
        Some(Overlay::Threads(Box::new(l)))
    }

    fn archive_thread(&self, t: &ThreadSheet, archived: bool) {
        self.call(
            "archiveThread",
            json!({"threadId": t.root, "archived": archived}),
            After::Sheet(Reply::ThreadChanged(t.root.clone())),
        );
    }

    fn confirm_thread_delete(&mut self, t: &ThreadSheet) {
        self.confirm_thread_delete_in(&t.bot, &t.root, &t.title);
    }

    fn confirm_thread_delete_in(&mut self, bot: &str, root: &str, title: &str) {
        let title = super::super::l10n::tr(title).into_owned();
        self.overlays.push(Overlay::Confirm(Confirm {
            title: format!("Delete “{title}”?"),
            detail: "Its replies go with it; the message it hangs off stays.".to_owned(),
            note: String::new(),
            button: "Delete",
            act: ConfirmAct::DeleteThread(bot.to_owned(), root.to_owned()),
        }));
    }
}
