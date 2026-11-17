//! The Ko-fi invitation's sheet: the desktop app's sentence, then the link (terminals open a link they recognize).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

use super::super::app::{KOFI_URL, KofiSheet};
use super::super::l10n::tr;
use super::super::md;
use super::buffer::{centered, scroll_sheet, u};
use super::style::theme;

pub(super) fn kofi(buf: &mut Buffer, area: Rect, s: &KofiSheet) {
    let t = theme();
    let most = centered(area, 60, area.height.saturating_sub(4));
    let width = usize::from(most.width.saturating_sub(4));
    let text = tr("Codync is free and made by one person. If it helps you, you can support it on Ko-fi.");
    let pad = [Span::raw(" ")];
    let mut lines = md::wrap(&[Span::styled(text.into_owned(), t.secondary)], width, &pad, &pad);
    lines.push(Line::default());
    lines.push(Line::from(vec![Span::raw(" "), Span::styled(KOFI_URL, t.text.add_modifier(Modifier::UNDERLINED))]));
    let r = centered(area, most.width, u(lines.len() + 4).min(most.height));
    scroll_sheet(buf, r, &tr("Support Codync"), &lines, &s.scroll);
}
