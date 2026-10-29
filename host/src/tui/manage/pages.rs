//! A bot's pages of details, as in the desktop inspector and the iPhone's details: Details (whether it
//! may use the computer, and its routines), Library (the artifacts it made and the files shared in its
//! chat), Memory and Settings. Shift+←→ moves between them. What an agent brings is on its own sheet.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::app::App;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Details,
    Library,
    Memory,
    Settings,
}

impl Page {
    pub const ALL: [Self; 4] = [Self::Details, Self::Library, Self::Memory, Self::Settings];

    pub fn label(self) -> &'static str {
        match self {
            Self::Details => "Details",
            Self::Library => "Library",
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
            Page::Library => self.open_library(),
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
        assert_eq!(Page::Details.step(key(KeyCode::Right)), Some(Page::Library));
        assert_eq!(Page::Memory.step(key(KeyCode::Left)), Some(Page::Library));
        assert_eq!(Page::Library.step(key(KeyCode::Left)), Some(Page::Details));
        assert_eq!(Page::Settings.step(key(KeyCode::Left)), Some(Page::Memory));
        assert_eq!(Page::Settings.step(key(KeyCode::Right)), None);
        assert_eq!(Page::Details.step(key(KeyCode::Left)), None);
        assert_eq!(
            Page::Memory.step(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)),
            None,
            "plain → is the page's own"
        );
    }
}
