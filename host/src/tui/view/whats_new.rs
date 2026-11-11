//! The What's new sheet: each announcement's version, then its notes.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use super::super::app::WhatsNew;
use super::super::l10n::tr;
use super::super::md;
use super::buffer::{centered, scroll_sheet, u};
use super::style::theme;

pub(super) fn whats_new(buf: &mut Buffer, area: Rect, s: &WhatsNew) {
    let t = theme();
    let most = centered(area, 72, area.height.saturating_sub(4));
    let width = usize::from(most.width.saturating_sub(4));
    let mut lines = Vec::new();
    for a in &s.due {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        lines.push(Line::from(Span::styled(format!(" {}", a.version), t.bold)));
        // A row apart, like the apps' rows.
        for (i, note) in a.notes.iter().enumerate() {
            if i > 0 {
                lines.push(Line::default());
            }
            let note = Span::styled(tr(note).into_owned(), t.secondary);
            lines.extend(md::wrap(&[note], width, &[Span::raw(" ")], &[Span::raw(" ")]));
        }
    }
    // Borders and the footer around the lines, as tall as they need.
    let r = centered(area, most.width, u(lines.len() + 4).min(most.height));
    scroll_sheet(buf, r, tr("What's new").as_ref(), &lines, &s.scroll);
}
