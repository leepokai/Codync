//! What sharing usage analytics covers: a read-only sheet beside the palette's switch.

use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent};

use super::super::app::Overlay;
use super::ctrl;

/// Scrolled from the top; the last draw sets the bounds.
#[derive(Default)]
pub struct AnalyticsSheet {
    pub scroll: usize,
    pub max_scroll: Cell<usize>,
    pub page: Cell<usize>,
}

impl AnalyticsSheet {
    pub(in crate::tui) fn key(mut self, k: KeyEvent) -> Option<Overlay> {
        let max = self.max_scroll.get();
        let half = (self.page.get() / 2).max(1);
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => return None,
            KeyCode::Down | KeyCode::Char('j') => self.scroll = (self.scroll + 1).min(max),
            KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::PageDown => self.scroll = (self.scroll + half).min(max),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(half),
            KeyCode::Char('d') if ctrl(k) => self.scroll = (self.scroll + half).min(max),
            KeyCode::Char('u') if ctrl(k) => self.scroll = self.scroll.saturating_sub(half),
            _ => {}
        }
        Some(Overlay::Analytics(self))
    }
}
