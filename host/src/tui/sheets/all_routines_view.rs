//! Every routine on the computer under the name of the bot that runs it, and the "Which bot runs
//! it?" chooser `n` opens when there are several bots.

use super::{App, Buffer, Rect, centered, footer, frame_box, name_of, put, row, rput, theme, u, window};
use crate::tui::l10n::tr;
use crate::tui::manage::AllRoutines;
use crate::tui::md::truncate;
use serde_json::Value;

pub fn draw(buf: &mut Buffer, area: Rect, app: &App, l: &AllRoutines) {
    let t = theme();
    let r = centered(area, 90, 28);
    let title = tr("Routines");
    let inner = frame_box(buf, r, t.text, t.panel, Some((title.as_ref(), t.text)));
    let groups = l.items.as_deref().map(|items| app.all_routines_grouped(items));
    match groups.as_deref() {
        None => {
            put(buf, inner.x, inner.y, inner.width, &tr("Loading routines…"), t.dim.patch(t.panel));
        }
        Some([]) => {
            put(
                buf,
                inner.x,
                inner.y,
                inner.width,
                &tr("No routines yet. Set one up for a bot with +."),
                t.secondary.patch(t.panel),
            );
        }
        Some(groups) => {
            // Rows and the bot names above them: `Err` is a bot's name.
            let mut lines: Vec<Result<(usize, &Value), &str>> = vec![];
            let mut i = 0;
            for (bot, routines) in groups {
                lines.push(Err(bot));
                for r in routines {
                    lines.push(Ok((i, *r)));
                    i += 1;
                }
            }
            let cursor = l.cursor.min(i.saturating_sub(1));
            let at = lines.iter().position(|line| matches!(line, Ok((i, _)) if *i == cursor)).unwrap_or(0);
            let h = usize::from(inner.height.saturating_sub(2));
            for (y, line) in (inner.y..).zip(lines.iter().skip(window(at, h)).take(h)) {
                match line {
                    Err(bot) => {
                        put(buf, inner.x, y, inner.width, &name_of(app, bot), t.secondary.patch(t.panel));
                    }
                    Ok((i, r)) => routine_row(buf, inner, y, r, *i == cursor),
                }
            }
        }
    }
    let keys = if l.chooser.is_some() { "" } else { "n new · ↵ edit · space on/off · esc" };
    footer(buf, inner, None, keys);
    if let Some(i) = l.chooser {
        chooser(buf, area, app, i);
    }
}

/// `● name  when it runs` with on / off on the right.
fn routine_row(buf: &mut Buffer, inner: Rect, y: u16, r: &Value, sel: bool) {
    let t = theme();
    let base = row(buf, inner, y, sel);
    let on = r["enabled"] == true;
    put(buf, inner.x, y, 2, if on { "●" } else { "○" }, if on { t.bold } else { t.dim }.patch(base));
    let state = tr(if on { "On" } else { "Off" });
    let right = rput(buf, inner.right(), y, state.as_ref(), if on { t.text } else { t.secondary }.patch(base));
    let room = usize::from(right.saturating_sub(inner.x + 4));
    let name = truncate(r["name"].as_str().unwrap_or_default(), room);
    let x = put(buf, inner.x + 2, y, u(room), &name, t.bold.patch(base));
    let detail = if r["lastError"].is_string() {
        tr("Schedule needs attention").into_owned()
    } else {
        crate::tui::manage::when_line(r)
    };
    let left = usize::from(right.saturating_sub(x + 4));
    let style = if r["lastError"].is_string() { t.red } else { t.dim };
    put(buf, x + 2, y, u(left), &truncate(&detail, left), style.patch(base));
}

/// "Which bot runs it?": the agent bots, over the list.
fn chooser(buf: &mut Buffer, area: Rect, app: &App, cursor: usize) {
    let t = theme();
    let bots = app.routine_owners();
    let height = u16::try_from(bots.len()).unwrap_or(u16::MAX).saturating_add(4).min(area.height);
    let r = centered(area, 44, height);
    let title = tr("Which bot runs it?");
    let inner = frame_box(buf, r, t.text, t.panel, Some((title.as_ref(), t.text)));
    let h = usize::from(inner.height.saturating_sub(1));
    for (y, (i, bot)) in (inner.y..).zip(bots.iter().enumerate().skip(window(cursor, h)).take(h)) {
        let base = row(buf, inner, y, i == cursor);
        put(buf, inner.x, y, inner.width, &name_of(app, bot), t.text.patch(base));
    }
    footer(buf, inner, None, "↵ pick · esc");
}
