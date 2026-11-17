//! Buffer helpers: text, lines, boxes, fills and scrolled sheets.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;

use super::super::l10n::tr;
use super::super::manage::Scroll;
use super::super::md::width as w;
use super::style::theme;

pub(in crate::tui) fn put(buf: &mut Buffer, x: u16, y: u16, max: u16, s: &str, style: Style) -> u16 {
    if max == 0 || y >= buf.area.bottom() {
        return x;
    }
    // Callers translate interface labels. Names, drafts and message previews stay as written.
    let (end, _) = buf.set_stringn(x, y, s, usize::from(max), style);
    end
}

pub(super) fn put_line(buf: &mut Buffer, x: u16, y: u16, max: u16, line: &Line<'_>) -> u16 {
    if y >= buf.area.bottom() {
        return x;
    }
    let (end, _) = buf.set_line(x, y, line, max);
    end
}

pub(in crate::tui) fn rput(buf: &mut Buffer, right: u16, y: u16, s: &str, style: Style) -> u16 {
    let width = u16::try_from(w(s)).unwrap_or(0);
    let x = right.saturating_sub(width);
    put(buf, x, y, width, s, style);
    x
}

pub(super) fn fill(buf: &mut Buffer, r: Rect, style: Style) {
    for y in r.top()..r.bottom() {
        for x in r.left()..r.right() {
            if let Some(c) = buf.cell_mut(Position::new(x, y)) {
                c.set_symbol(" ");
                c.set_style(style);
            }
        }
    }
}

pub(in crate::tui) fn restyle(buf: &mut Buffer, r: Rect, style: Style) {
    buf.set_style(r, style);
}

pub(in crate::tui) fn hline(buf: &mut Buffer, x: u16, y: u16, width: u16, style: Style) {
    put(buf, x, y, width, &"─".repeat(usize::from(width)), style);
}

pub(super) fn vline(buf: &mut Buffer, x: u16, y: u16, h: u16, style: Style) {
    for yy in y..y + h {
        put(buf, x, yy, 1, "│", style);
    }
}

pub(in crate::tui) fn frame_box(
    buf: &mut Buffer,
    r: Rect,
    border: Style,
    bg: Style,
    title: Option<(&str, Style)>,
) -> Rect {
    fill(buf, r, bg);
    if r.width < 2 || r.height < 2 {
        return r;
    }
    let inner = usize::from(r.width - 2);
    let b = border.patch(bg);
    put(buf, r.x, r.y, r.width, &format!("╭{}╮", "─".repeat(inner)), b);
    for y in r.y + 1..r.bottom() - 1 {
        put(buf, r.x, y, 1, "│", b);
        put(buf, r.right() - 1, y, 1, "│", b);
    }
    put(buf, r.x, r.bottom() - 1, r.width, &format!("╰{}╯", "─".repeat(inner)), b);
    if let Some((t, s)) = title {
        put(buf, r.x + 2, r.y, r.width.saturating_sub(4), &format!(" {t} "), s.patch(bg).add_modifier(Modifier::BOLD));
    }
    Rect::new(r.x + 2, r.y + 1, r.width.saturating_sub(4), r.height.saturating_sub(2))
}

/// A sheet that only shows text: `lines` from `scroll`'s position, the scroll hint below; sets `scroll`'s bounds.
pub(super) fn scroll_sheet(buf: &mut Buffer, r: Rect, title: &str, lines: &[Line<'_>], scroll: &Scroll) {
    let t = theme();
    let inner = frame_box(buf, r, t.text, t.panel, Some((title, t.text)));
    if inner.height < 3 {
        return;
    }
    hline(buf, inner.x, inner.bottom() - 2, inner.width, t.dim.patch(t.panel));
    let hint = format!(" {}", tr("↑↓ scroll · esc close"));
    put(buf, inner.x, inner.bottom() - 1, inner.width, &hint, t.dim.patch(t.panel));

    let body = Rect::new(inner.x, inner.y, inner.width, inner.height - 2);
    let h = usize::from(body.height);
    let max = lines.len().saturating_sub(h);
    scroll.max.set(max);
    scroll.page.set(h);
    for (i, l) in lines.iter().skip(scroll.pos.min(max)).take(h).enumerate() {
        put_line(buf, body.x, body.y + u(i), body.width, l);
    }
}

pub(super) fn dim_all(buf: &mut Buffer) {
    let shade = theme().shade;
    for c in &mut buf.content {
        c.set_style(Style::default().fg(shade).bg(Color::Reset).remove_modifier(Modifier::all()));
    }
}

pub(in crate::tui) fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let wd = width.min(area.width.saturating_sub(2));
    let h = height.min(area.height.saturating_sub(2));
    Rect::new(area.x + (area.width - wd) / 2, area.y + (area.height - h) / 3, wd, h)
}

/// Rows to skip of a list that fits `h` lines, to keep `cursor` on screen.
pub(in crate::tui) fn window(cursor: usize, h: usize) -> usize {
    cursor.saturating_sub(h.saturating_sub(1))
}

pub(in crate::tui) fn u(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}
