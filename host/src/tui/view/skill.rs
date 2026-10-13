//! A skill's details sheet: its description and instructions.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use super::super::app::App;
use super::super::manage::SkillSheet;
use super::super::md;
use super::buffer::{centered, frame_box, hline, put, put_line, u};
use super::style::theme;

pub(super) fn skill(buf: &mut Buffer, area: Rect, app: &App, s: &SkillSheet) {
    let t = theme();
    let r = centered(area, 84, area.height.saturating_sub(4));
    let inner = frame_box(buf, r, t.text, t.panel, Some((&s.name, t.text)));
    if inner.height < 5 {
        return;
    }
    let keys = if app.skill_installed(s) {
        " installed · ↑↓ scroll · esc close"
    } else {
        " ↵ install · ↑↓ scroll · esc close"
    };
    hline(buf, inner.x, inner.bottom() - 2, inner.width, t.dim.patch(t.panel));
    put(buf, inner.x, inner.bottom() - 1, inner.width, keys, t.dim.patch(t.panel));

    let body = Rect::new(inner.x, inner.y, inner.width, inner.height - 2);
    let width = usize::from(body.width);
    let mut lines =
        md::wrap(&[Span::styled(s.description.clone(), t.secondary)], width, &[Span::raw(" ")], &[Span::raw(" ")]);
    lines.push(Line::default());
    match (&s.markdown, &s.error) {
        (_, Some(e)) => lines.push(Line::from(Span::styled(format!(" {e}"), t.red))),
        (None, None) => lines.push(Line::from(Span::styled(" Loading…", t.dim))),
        (Some(m), None) if m.is_empty() => lines.push(Line::from(Span::styled(" No instructions.", t.dim))),
        (Some(m), None) => lines.extend(md::render(m, width, 1)),
    }
    let h = usize::from(body.height);
    let max = lines.len().saturating_sub(h);
    s.max_scroll.set(max);
    s.page.set(h);
    for (i, l) in lines.iter().skip(s.scroll.min(max)).take(h).enumerate() {
        put_line(buf, body.x, body.y + u(i), body.width, l);
    }
}
