//! The lead badge's role explanation, also available with I on the lead bot.
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use super::super::{l10n::tr, md};
use super::buffer::{centered, frame_box, put, put_line};
use super::style::theme;

const DESCRIPTION: &str = "Your lead agent is your first point of contact. It follows your goals and unfinished tasks, suggests useful next steps, and coordinates work with your other bots. The lead Codync sets up has a Check in routine that runs twice a day by default; you can change or pause it in Routines.";

fn lines(width: u16) -> Vec<Line<'static>> {
    md::wrap(&[Span::styled(tr(DESCRIPTION).into_owned(), theme().text)], usize::from(width), &[], &[])
}

pub(in crate::tui) fn lead_agent_scroll_max(width: u16, height: u16) -> usize {
    let r = centered(Rect::new(0, 0, width, height), 62, 16);
    lines(r.width.saturating_sub(4)).len().saturating_sub(usize::from(r.height.saturating_sub(4)))
}

pub(super) fn lead_agent(buf: &mut Buffer, area: Rect, scroll: usize) {
    let t = theme();
    let r = centered(area, 62, 16);
    let inner = frame_box(buf, r, t.amber, t.panel, Some((tr("Lead agent").as_ref(), t.amber)));
    if inner.height < 2 || inner.width == 0 {
        return;
    }
    let content = lines(inner.width);
    let visible = usize::from(inner.height - 2);
    let start = scroll.min(content.len().saturating_sub(visible));
    for (offset, line) in content.iter().skip(start).take(visible).enumerate() {
        put_line(buf, inner.x, inner.y + super::buffer::u(offset), inner.width, line);
    }
    put(buf, inner.x, inner.bottom() - 1, inner.width, tr("↑↓ scroll · esc close").as_ref(), t.dim);
}
