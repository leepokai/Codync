//! The bot list, full and compact.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

use super::super::app::{App, Bot, Click, Focus, Mark, Status};
use super::super::l10n::tr;
use super::super::md::{self, truncate, width as w};
use super::buffer::{fill, hline, put, put_line, restyle, rput, u};
use super::style::{avatar, glyph, theme};
use super::time::{elapsed, when};

pub(super) fn roster(buf: &mut Buffer, r: Rect, app: &mut App, narrow: bool) {
    let t = theme();
    app.hits.roster = r;
    let focused = app.focus == Focus::Roster || narrow;
    let bots: Vec<Bot> = app.roster().into_iter().cloned().collect();
    let x0 = r.x;
    let wd = r.width;
    if narrow {
        fill(buf, Rect::new(r.x, r.y, r.width, 1), t.panel);
    }
    put(buf, x0 + 1, r.y, wd.saturating_sub(10), if app.host.is_empty() { "codync" } else { &app.host }, t.bold);
    rput(buf, r.right() - 1, r.y, &format!("{} chat{}", bots.len(), if bots.len() == 1 { "" } else { "s" }), t.dim);
    if !narrow {
        hline(buf, x0, r.y + 1, wd, t.line);
    }
    let top = r.y + if narrow { 1 } else { 2 };
    let bottom = r.bottom().saturating_sub(1);
    if bots.is_empty() {
        let msg: Vec<(String, Style)> = if let Some(e) = &app.error {
            let mut m = vec![(e.clone(), t.amber)];
            if app.url.contains("127.0.0.1") || app.url.contains("localhost") {
                m.push(("Press I to install and start it here.".into(), t.bold));
            }
            m
        } else if !app.online {
            vec![("Connecting…".into(), t.secondary)]
        } else {
            vec![("No bots yet.".into(), t.text), ("n creates one.".into(), t.dim)]
        };
        let mut y = top + 1;
        for (m, s) in &msg {
            for l in md::wrap(&[Span::styled(m.clone(), *s)], usize::from(wd.saturating_sub(2)), &[], &[]) {
                put_line(buf, x0 + 1, y, wd - 2, &l);
                y += 1;
            }
            y += 1;
        }
    }
    // Rows: an optional section label, then 2 lines per bot and a gap. The lead, on top, has no label.
    let any_pinned = bots.iter().any(|b| b.pinned);
    let mut rows: Vec<(u16, Option<&'static str>, Option<usize>)> = vec![];
    let mut y = 0u16;
    for (i, b) in bots.iter().enumerate() {
        if any_pinned && !b.lead && (i == 0 || bots[i - 1].band() != b.band()) {
            rows.push((y, Some(if b.pinned { "PINNED" } else { "BOTS" }), None));
            y += 1;
        }
        rows.push((y, None, Some(i)));
        y += 3;
    }
    let sel_idx = app.selected.as_ref().and_then(|s| bots.iter().position(|b| &b.id == s));
    let view_h = bottom.saturating_sub(top);
    let sel_y = sel_idx.and_then(|i| rows.iter().find(|r| r.2 == Some(i)).map(|r| r.0)).unwrap_or(0);
    let offset = (sel_y + 3).saturating_sub(view_h);
    for (ry, label, idx) in rows {
        let Some(yy) = (top + ry).checked_sub(offset) else { continue };
        if ry < offset || yy + 1 >= bottom {
            continue;
        }
        if let Some(l) = label {
            put(buf, x0 + 1, yy, wd, l, t.dim.add_modifier(Modifier::BOLD));
            continue;
        }
        let Some(i) = idx else { continue };
        let b = &bots[i];
        let selected = Some(i) == sel_idx;
        let row = Rect::new(x0, yy, wd, 2);
        app.hits.clicks.push((row, Click::Bot(b.id.clone())));
        if selected {
            restyle(buf, row, t.sel);
            let bar = if focused { t.bold } else { t.secondary };
            put(buf, x0, yy, 1, "▌", bar);
            put(buf, x0, yy + 1, 1, "▌", bar);
        }
        put_line(buf, x0 + 2, yy, 2, &avatar(app, b));
        let (g, gs) = glyph(app, b.mark());
        let time = if b.status == Status::Working { elapsed(b.started_at) } else { when(b.last_at) };
        let tx = rput(buf, r.right() - 1, yy, &time, if b.unread > 0 { t.bold } else { t.dim });
        let gx = tx.saturating_sub(2);
        put(buf, gx, yy, 1, g, gs);
        // The badge sits below the avatar, leaving activity and unread indicators visible.
        if b.lead {
            put(buf, x0 + 3, yy + 1, 1, "★", t.amber);
            app.hits.clicks.push((Rect::new(x0 + 3, yy + 1, 1, 1), Click::LeadAgent));
        }
        let name_x = x0 + 5;
        put(buf, name_x, yy, gx.saturating_sub(name_x + 1), &b.name, t.bold);
        let (pv, ps) = preview(b);
        let badge = if b.unread > 0 { format!(" {} ", b.unread) } else { String::new() };
        let room = usize::from(wd.saturating_sub(7)).saturating_sub(w(&badge) + usize::from(!badge.is_empty()));
        put(buf, x0 + 5, yy + 1, u(room), &truncate(&pv, room), ps);
        if !badge.is_empty() {
            rput(buf, r.right() - 1, yy + 1, &badge, t.btn_primary);
        }
    }
    if !narrow {
        let hy = r.bottom() - 1;
        let mut x = put(buf, x0 + 1, hy, 2, "n", t.secondary.add_modifier(Modifier::BOLD)) + 1;
        x = put(buf, x, hy, 10, "new bot", t.dim) + 2;
        x = put(buf, x, hy, 2, "m", t.secondary.add_modifier(Modifier::BOLD)) + 1;
        x = put(buf, x, hy, 10, "group", t.dim) + 2;
        x = put(buf, x, hy, 3, "^k", t.secondary.add_modifier(Modifier::BOLD)) + 1;
        put(buf, x, hy, 8, "go to", t.dim);
    }
}

/// A one-line preview without Markdown punctuation: its first line (also how a reply's quote shows).
pub(super) fn plain(s: &str) -> String {
    let line = s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
    line.trim_start_matches(['#', '>', '-', '*', ' ']).replace("**", "").replace('`', "")
}

pub(super) fn preview(b: &Bot) -> (String, Style) {
    let (text, style) = preview_raw(b);
    (plain(&text), style)
}

fn preview_raw(b: &Bot) -> (String, Style) {
    let t = theme();
    match b.mark() {
        Mark::Need => (
            if b.activity.is_empty() { tr("Needs your approval").into_owned() } else { tr(&b.activity).into_owned() },
            t.amber,
        ),
        Mark::Work => (
            if b.activity.is_empty() { tr("Working…").into_owned() } else { tr(&b.activity).into_owned() },
            t.secondary,
        ),
        Mark::Error => (
            if b.last_message.is_empty() { tr("Something went wrong").into_owned() } else { b.last_message.clone() },
            t.red,
        ),
        Mark::Unread => (b.last_message.clone(), t.text),
        Mark::Idle if b.last_message.is_empty() => (tr("No messages yet").into_owned(), t.dim),
        Mark::Idle => (b.last_message.clone(), t.secondary),
    }
}

pub(super) fn compact_roster(buf: &mut Buffer, r: Rect, app: &mut App) {
    let t = theme();
    app.hits.roster = r;
    let bots: Vec<Bot> = app.roster().into_iter().cloned().collect();
    let sel = app.selected.clone();
    let sel_idx = sel.as_ref().and_then(|s| bots.iter().position(|b| &b.id == s)).unwrap_or(0);
    let per = 2u16;
    let view = r.height.saturating_sub(1) / per;
    let offset = (u(sel_idx) + 1).saturating_sub(view);
    for (i, b) in bots.iter().enumerate().skip(usize::from(offset)) {
        let y = r.y + 1 + (u(i) - offset) * per;
        if y >= r.bottom() {
            break;
        }
        let row = Rect::new(r.x, y, r.width, 1);
        app.hits.clicks.push((row, Click::Bot(b.id.clone())));
        if sel.as_deref() == Some(b.id.as_str()) {
            restyle(buf, row, t.sel);
            put(buf, r.x, y, 1, "▌", if app.focus == Focus::Roster { t.bold } else { t.secondary });
        }
        put_line(buf, r.x + 1, y, 2, &avatar(app, b));
        let (g, gs) = glyph(app, b.mark());
        put(buf, r.x + 3, y, 1, g, gs);
        if b.lead && y + 1 < r.bottom() {
            put(buf, r.x + 2, y + 1, 1, "★", t.amber);
            app.hits.clicks.push((Rect::new(r.x + 2, y + 1, 1, 1), Click::LeadAgent));
        }
    }
}
