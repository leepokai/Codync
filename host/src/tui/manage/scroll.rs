//! The scroll of the sheets that only show text: a memory note, What's new, usage analytics.

use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent};

use super::ctrl;

/// A read-only text's scroll, by wrapped row; the last draw sets how far it goes and a page's height.
#[derive(Default)]
pub struct Scroll {
    pub pos: usize,
    pub max: Cell<usize>,
    pub page: Cell<usize>,
}

impl Scroll {
    /// ↑↓, PageUp/PageDown and Ctrl-d/u; false for a key it doesn't take.
    pub(in crate::tui) fn key(&mut self, k: KeyEvent) -> bool {
        let max = self.max.get();
        let half = (self.page.get() / 2).max(1);
        // A refresh or a resize can leave the position past the end.
        let pos = self.pos.min(max);
        self.pos = match k.code {
            KeyCode::Down | KeyCode::Char('j') => (pos + 1).min(max),
            KeyCode::Up | KeyCode::Char('k') => pos.saturating_sub(1),
            KeyCode::PageDown => (pos + half).min(max),
            KeyCode::PageUp => pos.saturating_sub(half),
            KeyCode::Char('d') if ctrl(k) => (pos + half).min(max),
            KeyCode::Char('u') if ctrl(k) => pos.saturating_sub(half),
            _ => return false,
        };
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrolling_stops_at_the_end_the_draw_set() {
        let key = |code| KeyEvent::from(code);
        let mut scroll = Scroll { pos: 10, ..Scroll::default() };
        scroll.max.set(30);
        scroll.page.set(20);
        assert!(scroll.key(key(KeyCode::PageDown)));
        assert_eq!(scroll.pos, 20);
        assert!(scroll.key(key(KeyCode::PageDown)));
        assert_eq!(scroll.pos, 30, "no further than the last wrapped row");
        scroll.max.set(5);
        assert!(scroll.key(key(KeyCode::Up)));
        assert_eq!(scroll.pos, 4, "a shorter text: the first ↑ moves from its end");
        assert!(!scroll.key(key(KeyCode::Enter)));
    }
}
