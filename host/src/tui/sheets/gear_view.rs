//! What an agent already brings, read only. One list, so the cursor scrolls rows that do not fit.

use super::{App, Buffer, Rect, centered, footer, frame_box, put, row, theme, truncate, window};
use crate::tui::l10n::tr;
use crate::tui::manage::{Gear, GearRow};

pub fn draw(buf: &mut Buffer, area: Rect, app: &App, gear: &Gear) {
    let t = theme();
    let frame = centered(area, 90, 28);
    let title = app.agent_name(&gear.backend);
    let inner = frame_box(buf, frame, t.text, t.panel, Some((&title, t.text)));
    let mut y = inner.y;
    let bottom = inner.bottom().saturating_sub(2);
    let intro = tr("What the agent brings");
    if y < bottom {
        put(buf, inner.x, y, inner.width, &intro, t.secondary.patch(t.panel));
        y += 2;
    }
    let rows = gear.rows();
    let room = usize::from(bottom.saturating_sub(y));
    let cursor = gear.cursor.min(rows.len().saturating_sub(1));
    for (offset, line) in rows.iter().enumerate().skip(window(cursor, room)).take(room) {
        if y >= bottom {
            break;
        }
        let base = row(buf, inner, y, offset == cursor);
        let (text, style) = match line {
            GearRow::Heading(label) => (format!("{}  {}", tr(label), gear.count_of(label)), t.bold.patch(base)),
            GearRow::Note => {
                let text = if gear.lists.is_none() && !gear.failed {
                    tr("Loading…")
                } else if gear.failed {
                    tr("Couldn't load what this agent brings.")
                } else {
                    tr("None from this agent.")
                };
                (text.into(), if gear.failed { t.red.patch(base) } else { t.dim.patch(base) })
            }
            GearRow::Item(section, index) => {
                let Some(item) = gear.item(*section, *index) else { continue };
                let text =
                    if item.detail.is_empty() { item.name.clone() } else { format!("{}  {}", item.name, item.detail) };
                (text, t.text.patch(base))
            }
        };
        put(buf, inner.x, y, inner.width, &truncate(&text, usize::from(inner.width)), style);
        y += 1;
    }
    footer(buf, inner, None, "esc");
}
