//! What sharing usage analytics covers: a read-only sheet beside the palette's switch.

use crossterm::event::{KeyCode, KeyEvent};

use super::super::app::Overlay;
use super::Scroll;

/// Scrolled from the top; the last draw sets the bounds.
#[derive(Default)]
pub struct AnalyticsSheet {
    pub scroll: Scroll,
}

impl AnalyticsSheet {
    pub(in crate::tui) fn key(mut self, k: KeyEvent) -> Option<Overlay> {
        if matches!(k.code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter) {
            return None;
        }
        self.scroll.key(k);
        Some(Overlay::Analytics(self))
    }
}
