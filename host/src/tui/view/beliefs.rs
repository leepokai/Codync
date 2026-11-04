//! The "What I know about you" sheet.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::super::app::BeliefsSheet;
use super::super::app::beliefs::{CONFIDENCES, KINDS};
use super::super::l10n::tr;
use super::super::md::truncate;
use super::buffer::{centered, frame_box, put, restyle, u};
use super::style::theme;

pub(super) fn beliefs(buf: &mut Buffer, area: Rect, s: &BeliefsSheet) {
    let t = theme();
    let shown = s.shown();
    let rows = shown.len() * 2 + 8;
    let r = centered(area, 90, u(rows.clamp(14, usize::from(area.height.saturating_sub(2)).max(14))));
    let title = format!("{} · {}", tr("What I know about you"), s.bot_name);
    let inner = frame_box(buf, r, t.text, t.panel, Some((title.as_str(), t.text)));
    if inner.height < 6 {
        return;
    }
    let width = usize::from(inner.width);
    let kind = s.kind.map_or("All", |k| KINDS[k].1);
    let confidence = s.confidence.map_or("Any", |c| CONFIDENCES[c].1);
    let heading = if s.forgotten { "Forgotten notes" } else { "Least sure first" };
    put(
        buf,
        inner.x,
        inner.y + 1,
        inner.width,
        &truncate(
            &format!("{} · {}: {} · {}: {}", tr(heading), tr("Kind"), tr(kind), tr("Confidence"), tr(confidence)),
            width,
        ),
        t.secondary.patch(t.panel),
    );
    let mut y = inner.y + 3;
    match &s.items {
        None => {
            put(buf, inner.x, y, inner.width, &tr("Loading…"), t.dim.patch(t.panel));
        }
        Some(_) if shown.is_empty() => {
            put(buf, inner.x, y, inner.width, &tr("Nothing here yet."), t.dim.patch(t.panel));
        }
        Some(_) => {
            let room = usize::from(inner.bottom().saturating_sub(y)).saturating_sub(2) / 2;
            let off = s.sel.saturating_sub(room.max(1) - 1);
            for (i, belief) in shown.iter().enumerate().skip(off).take(room.max(1)) {
                let selected = i == s.sel;
                if selected {
                    restyle(buf, Rect::new(inner.x.saturating_sub(1), y, inner.width + 2, 2), t.sel);
                }
                let base = if selected { t.sel } else { t.panel };
                let [first, second] = BeliefsSheet::lines(belief);
                put(buf, inner.x, y, inner.width, &truncate(&first, width), t.text.patch(base));
                put(buf, inner.x, y + 1, inner.width, &truncate(&second, width), t.secondary.patch(base));
                y += 2;
                if selected && let Some(at) = s.history {
                    for (n, (_, label, restorable)) in BeliefsSheet::versions(belief).into_iter().enumerate() {
                        if y + 1 >= inner.bottom() {
                            break;
                        }
                        let mark = if n == at { "▸" } else { " " };
                        let back = if restorable { "  (b brings it back)" } else { "" };
                        put(
                            buf,
                            inner.x + 2,
                            y,
                            inner.width.saturating_sub(2),
                            &truncate(&format!("{mark} {}{back}", tr(label)), width),
                            t.dim.patch(t.panel),
                        );
                        y += 1;
                    }
                }
            }
        }
    }
    let keys = if s.forgotten {
        "↑↓ pick  ↵ history  g back  f kind  c confidence  esc close"
    } else {
        "↑↓ pick  y still true  x forget  ↵ history  g forgotten  f kind  c confidence  esc close"
    };
    put(buf, inner.x, inner.bottom() - 1, inner.width, &truncate(&tr(keys), width), t.dim.patch(t.panel));
}
