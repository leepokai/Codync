//! A bot's Computer page: the computer it lives on (the host this terminal is attached to) and whether it
//! may use it, which `u` switches. The apps' live picture of the screen isn't here: a terminal can't draw it.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::json;

use super::super::app::{After, App, Overlay};
use super::Page;

impl App {
    /// A group has no computer page; the key does nothing on one.
    pub fn open_computer(&mut self) {
        if let Some(bot) = self.bot().filter(|b| !b.group).map(|b| b.id.clone()) {
            self.overlays.push(Overlay::Computer(bot));
        }
    }

    pub(in crate::tui) fn computer_key(&mut self, bot: String, k: KeyEvent) -> Option<Overlay> {
        if let Some(page) = Page::Computer.step(k) {
            self.open_page(page);
            return None;
        }
        match k.code {
            KeyCode::Esc | KeyCode::Char('q' | 'O') => return None,
            // Only a computer that reports a screen offers it, as in the apps.
            KeyCode::Char('u') if !self.screen.is_null() => {
                let on = self.bots.get(&bot).is_some_and(|b| b.computer);
                self.call("updateBot", json!({"id": bot, "computer": !on}), After::Nothing);
            }
            _ => {}
        }
        Some(Overlay::Computer(bot))
    }
}

#[cfg(test)]
mod tests {
    use crate::tui::app::{Bot, Overlay, test_app};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use serde_json::json;

    #[tokio::test]
    async fn o_opens_a_bots_computer_page_and_shift_arrows_leave_it() {
        let mut app = test_app();
        for (id, kind) in [("b", "bot"), ("g", "group")] {
            app.bots.insert(id.into(), Bot::parse(&json!({"id": id, "name": id, "kind": kind})).expect("a bot"));
        }
        app.selected = Some("g".into());
        app.on_key(KeyEvent::from(KeyCode::Char('O')));
        assert!(app.overlays.is_empty(), "a group has no computer page");
        app.selected = Some("b".into());
        app.on_key(KeyEvent::from(KeyCode::Char('O')));
        assert!(matches!(app.overlays.last(), Some(Overlay::Computer(b)) if b == "b"));
        app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT));
        assert!(matches!(app.overlays.as_slice(), [Overlay::Routines(_)]), "in place of the Computer page");
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
        assert!(matches!(app.overlays.as_slice(), [Overlay::Computer(_)]));
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.overlays.is_empty());
    }
}
