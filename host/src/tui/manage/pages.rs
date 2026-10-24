//! A bot's three pages, as in the desktop inspector and the iPhone's details: Details (whether it
//! may use the computer, and its routines), Memory and Settings. Shift+←→ moves between them.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::app::App;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Details,
    Memory,
    Settings,
}

impl Page {
    pub const ALL: [Self; 3] = [Self::Details, Self::Memory, Self::Settings];

    pub fn label(self) -> &'static str {
        match self {
            Self::Details => "Details",
            Self::Memory => "Memory",
            Self::Settings => "Settings",
        }
    }

    /// The page shift+← or shift+→ moves to from this one.
    pub fn step(self, k: KeyEvent) -> Option<Self> {
        if !k.modifiers.contains(KeyModifiers::SHIFT) {
            return None;
        }
        let i = Self::ALL.iter().position(|p| *p == self)?;
        let next = match k.code {
            KeyCode::Left => i.checked_sub(1)?,
            KeyCode::Right => i + 1,
            _ => return None,
        };
        Self::ALL.get(next).copied()
    }
}

impl App {
    /// Opens `page` of the bot in view, in place of the page it came from.
    pub(in crate::tui) fn open_page(&mut self, page: Page) {
        match page {
            Page::Details => self.open_routines(),
            Page::Memory => self.open_memory(),
            Page::Settings => self.edit_bot(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_arrows_move_between_pages_and_stop_at_the_ends() {
        let key = |code| KeyEvent::new(code, KeyModifiers::SHIFT);
        assert_eq!(Page::Details.step(key(KeyCode::Right)), Some(Page::Memory));
        assert_eq!(Page::Memory.step(key(KeyCode::Left)), Some(Page::Details));
        assert_eq!(Page::Settings.step(key(KeyCode::Right)), None);
        assert_eq!(Page::Details.step(key(KeyCode::Left)), None);
        assert_eq!(
            Page::Memory.step(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)),
            None,
            "plain → is the page's own"
        );
    }
}
