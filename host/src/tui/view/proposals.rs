//! The proposed skills sheet.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::super::app::ProposalsSheet;
use super::super::app::proposals::{inbox_title, steps_preview, tools_text};
use super::super::l10n::tr;
use super::super::md::truncate;
use super::buffer::{centered, frame_box, put, restyle, u};
use super::style::theme;

pub(super) fn proposals(buf: &mut Buffer, area: Rect, s: &ProposalsSheet) {
    let t = theme();
    let count = s.items.as_ref().map_or(0, Vec::len);
    let r = centered(area, 90, u((count * 6 + 7).clamp(12, usize::from(area.height.saturating_sub(2)).max(12))));
    let title = format!("{} · {}", tr(&inbox_title(count)), s.bot_name);
    let inner = frame_box(buf, r, t.text, t.panel, Some((title.as_str(), t.text)));
    if inner.height < 6 {
        return;
    }
    let width = usize::from(inner.width);
    let mut y = inner.y + 1;
    match &s.items {
        None => {
            put(buf, inner.x, y, inner.width, &tr("Loading…"), t.dim.patch(t.panel));
        }
        Some(items) if items.is_empty() => {
            put(buf, inner.x, y, inner.width, &tr("No proposals."), t.dim.patch(t.panel));
        }
        Some(items) => {
            let room = (usize::from(inner.bottom().saturating_sub(y)).saturating_sub(2) / 6).max(1);
            let off = s.sel.saturating_sub(room - 1);
            for (i, p) in items.iter().enumerate().skip(off).take(room) {
                let selected = i == s.sel;
                if selected {
                    restyle(buf, Rect::new(inner.x.saturating_sub(1), y, inner.width + 2, 5), t.sel);
                }
                let base = if selected { t.sel } else { t.panel };
                put(buf, inner.x, y, inner.width, &truncate(p["name"].as_str().unwrap_or_default(), width), t.text.patch(base));
                put(buf, inner.x, y + 1, inner.width, &truncate(p["description"].as_str().unwrap_or_default(), width), t.secondary.patch(base));
                for (n, step) in steps_preview(p, 2).iter().enumerate() {
                    put(buf, inner.x + 2, y + 2 + u(n), inner.width.saturating_sub(2), &truncate(step, width), t.dim.patch(base));
                }
                put(buf, inner.x, y + 4, inner.width, &truncate(&format!("{}: {}", tr("Used"), tools_text(p)), width), t.dim.patch(base));
                y += 6;
            }
        }
    }
    put(buf, inner.x, inner.bottom() - 1, inner.width, &tr("↑↓ pick  a approve  x dismiss  r refresh  esc close"), t.dim.patch(t.panel));
}
