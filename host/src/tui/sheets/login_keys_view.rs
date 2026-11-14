//! The login shell's variables an API key login profile can import: one list, ticked with space.

use super::{App, Buffer, Rect, centered, footer, frame_box, put, row, theme, truncate, window};
use crate::tui::l10n::{fill, tr};
use crate::tui::manage::KeyImport;

pub fn draw(buf: &mut Buffer, area: Rect, app: &App, i: &KeyImport) {
    let t = theme();
    let frame = centered(area, 90, 28);
    let inner = frame_box(buf, frame, t.text, t.panel, Some((&tr("Import from login shell"), t.text)));
    let hint = tr("Pick the variables this login profile should use. Their values are copied now and kept on {name}.");
    let hint = fill(&hint, &[("name", &app.host)]);
    put(buf, inner.x, inner.y, inner.width, &truncate(&hint, usize::from(inner.width)), t.dim.patch(t.panel));
    let y = inner.y + 2;
    let bottom = inner.bottom().saturating_sub(2);
    let room = usize::from(bottom.saturating_sub(y));
    let note = match i.names.as_deref() {
        None => Some("Loading…"),
        Some([]) if i.error.is_none() => Some("Nothing to import."),
        Some(_) => None,
    };
    if let Some(note) = note {
        put(buf, inner.x, y, inner.width, &tr(note), t.dim.patch(t.panel));
    }
    let names = i.names.as_deref().unwrap_or_default();
    for (y, (at, name)) in (y..).zip(names.iter().enumerate().skip(window(i.cursor, room)).take(room)) {
        let base = row(buf, inner, y, at == i.cursor);
        let tick = if i.picked.contains(name) { "[x]" } else { "[ ]" };
        let line = format!("{tick} {name}");
        put(buf, inner.x, y, inner.width, &truncate(&line, usize::from(inner.width)), t.text.patch(base));
    }
    let keys = if i.importing { "Working…" } else { "↑↓ move · space pick · ↵ import · esc back" };
    footer(buf, inner, i.error.as_deref(), keys);
}
