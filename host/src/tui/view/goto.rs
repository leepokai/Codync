//! The go-to palette: bots and actions.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;

use super::super::app::{ACTIONS, Action, App, Click, FILTERS, GotoItem, Mark};
use super::super::md::truncate;
use super::buffer::{centered, frame_box, hline, put, put_line, restyle, rput};
use super::overlay::field_line;
use super::roster::preview;
use super::style::{avatar, glyph, theme};

pub(super) fn goto(buf: &mut Buffer, area: Rect, app: &mut App, g: &super::super::app::Goto, items: &[GotoItem]) {
    let t = theme();
    let r = centered(area, 80, 24);
    let inner = frame_box(buf, r, t.text, t.panel, Some(("Go to", t.text)));
    let nbots = items.iter().filter(|i| matches!(i, GotoItem::Bot(_))).count();
    rput(buf, inner.right(), inner.y, &format!("{nbots} of {}", app.bots.len()), t.dim.patch(t.panel));
    field_line(
        buf,
        Rect::new(inner.x, inner.y, inner.width.saturating_sub(10), 1),
        inner.y,
        &g.query,
        "search bots",
        true,
    );
    // Filter chips.
    let counts = app.counts();
    let mut x = inner.x;
    for (i, (label, mark)) in FILTERS.iter().enumerate() {
        let n = match mark {
            Some(Mark::Need) => format!(" {}", counts[0]),
            Some(Mark::Work) => format!(" {}", counts[1]),
            Some(Mark::Unread) => format!(" {}", counts[2]),
            Some(Mark::Error) => format!(" {}", counts[3]),
            _ => String::new(),
        };
        let chip = format!(" {label}{n} ");
        let s = if i == g.filter { t.btn_primary } else { t.secondary.patch(t.btn) };
        let start = x;
        x = put(buf, x, inner.y + 2, inner.right().saturating_sub(x), &chip, s) + 1;
        app.hits.clicks.push((Rect::new(start, inner.y + 2, x - start, 1), Click::Filter(i)));
    }
    let list_top = inner.y + 4;
    let list_h = usize::from(inner.bottom().saturating_sub(list_top + 3));
    let off = g.cursor.saturating_sub(list_h.saturating_sub(1));
    let mut y = list_top;
    let mut shown_actions = false;
    for (i, item) in items.iter().enumerate().skip(off).take(list_h) {
        let sel = i == g.cursor;
        let base = if sel { t.sel } else { t.panel };
        match item {
            GotoItem::Bot(id) => {
                let Some(b) = app.bots.get(id) else { continue };
                if sel {
                    restyle(buf, Rect::new(inner.x - 1, y, inner.width + 2, 1), t.sel);
                }
                app.hits.clicks.push((Rect::new(inner.x - 1, y, inner.width + 2, 1), Click::Bot(id.clone())));
                let (gl, gs) = glyph(app, b.mark());
                put(buf, inner.x, y, 1, gl, gs.patch(base));
                put_line(buf, inner.x + 2, y, 2, &avatar(app, b));
                put(buf, inner.x + 5, y, 10, &b.name, t.bold.patch(base));
                put(buf, inner.x + 16, y, 10, if b.group { "group" } else { &b.backend }, t.secondary.patch(base));
                let cwd =
                    if b.group { format!("{} bots", b.members.len()) } else { truncate(&b.folder(&app.home), 18) };
                put(buf, inner.x + 27, y, 19, &cwd, t.secondary.patch(base));
                let (pv, ps) = preview(b);
                let room = inner.width.saturating_sub(47);
                put(buf, inner.x + 47, y, room, &truncate(&pv, usize::from(room)), ps.patch(base));
            }
            GotoItem::Action(a) => {
                if !shown_actions {
                    shown_actions = true;
                    if y > list_top {
                        y += 1;
                    }
                    put(buf, inner.x, y, inner.width, "ACTIONS", t.dim.patch(t.panel).add_modifier(Modifier::BOLD));
                    y += 1;
                }
                if sel {
                    restyle(buf, Rect::new(inner.x - 1, y, inner.width + 2, 1), t.sel);
                }
                if let Some((label, key, _)) = ACTIONS.iter().find(|x| x.2 == *a) {
                    put(buf, inner.x + 2, y, inner.width, label, if sel { t.bold } else { t.secondary }.patch(base));
                    let hint = match (a, app.analytics) {
                        (Action::Analytics, Some(true)) => "on",
                        (Action::Analytics, Some(false)) => "off",
                        (Action::Analytics, None) => "",
                        _ => key,
                    };
                    rput(buf, inner.right(), y, hint, t.dim.patch(base));
                }
            }
        }
        y += 1;
        if y >= inner.bottom().saturating_sub(3) {
            break;
        }
    }
    if items.is_empty() {
        put(buf, inner.x, list_top, inner.width, "Nothing matches", t.dim.patch(t.panel));
    }
    hline(buf, inner.x, inner.bottom() - 3, inner.width, t.line.patch(t.panel));
    if let Some(GotoItem::Bot(id)) = items.get(g.cursor)
        && let Some(b) = app.bots.get(id)
    {
        let meta = if b.group {
            let names: Vec<String> = b.members.iter().map(|m| app.author_name(Some(m))).collect();
            format!("{} · group · {}", b.name, names.join(", "))
        } else {
            format!("{} · {} · {}", b.name, b.backend, b.folder(&app.home))
        };
        put(buf, inner.x, inner.bottom() - 2, inner.width, &meta, t.secondary.patch(t.panel));
    }
    put(
        buf,
        inner.x,
        inner.bottom() - 1,
        inner.width,
        "↵ open   tab filter   ↑↓ move   esc close",
        t.dim.patch(t.panel),
    );
}
