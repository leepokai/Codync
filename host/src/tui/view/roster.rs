//! The bot list, full and compact.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

use super::super::app::{App, Bot, Click, Focus, Mark, RosterLine, Status, cursor_line, roster_lines, stops};
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
    // Lines: a label takes one row, a bot two and a gap.
    let shown: Vec<&Bot> = bots.iter().collect();
    let lines = roster_lines(&shown, &app.folded);
    let mut rows: Vec<u16> = vec![];
    let mut y = 0u16;
    for line in &lines {
        rows.push(y);
        y += extent(line).0;
    }
    let cursor = cursor_line(&lines, app.selected.as_ref().and_then(|s| bots.iter().find(|b| &b.id == s)));
    let view_h = bottom.saturating_sub(top);
    let offset = cursor.map_or(0, |i| rows[i] + extent(&lines[i]).0).saturating_sub(view_h);
    let bar = if focused { t.bold } else { t.secondary };
    for (i, (line, &ry)) in lines.iter().zip(&rows).enumerate() {
        let Some(yy) = (top + ry).checked_sub(offset) else { continue };
        if ry < offset || yy + extent(line).1 > bottom {
            continue;
        }
        if let Some(click) = line.click() {
            app.hits.clicks.push((Rect::new(x0, yy, wd, extent(line).1), click));
        }
        let cursor_bar = (cursor == Some(i)).then_some(bar);
        match line {
            RosterLine::Label(l) => {
                put(buf, x0 + 1, yy, wd, l, t.dim.add_modifier(Modifier::BOLD));
            }
            RosterLine::Section { name, bots: count, unread, folded, .. } => {
                section_row(buf, Rect::new(x0, yy, wd, 1), cursor_bar, name, *count, *unread, *folded);
            }
            RosterLine::Bot(b) => bot_row(buf, Rect::new(x0, yy, wd, 2), app, b, cursor_bar),
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

/// (rows a line takes in the list, rows it draws): a bot has a gap row below it.
fn extent(line: &RosterLine<'_>) -> (u16, u16) {
    if matches!(line, RosterLine::Bot(_)) { (3, 2) } else { (1, 1) }
}

/// A section's label row, `▾ name (bots)`: the name is cut so `(bots)` and a gap before the unread
/// badge (shown while folded) always fit.
fn section_row(buf: &mut Buffer, row: Rect, bar: Option<Style>, name: &str, bots: usize, unread: i64, folded: bool) {
    let t = theme();
    if let Some(bar) = bar {
        mark_cursor(buf, row, bar);
    }
    let badge = if folded && unread > 0 { format!(" {unread} ") } else { String::new() };
    let room = usize::from(row.width.saturating_sub(2)).saturating_sub(w(&badge) + usize::from(!badge.is_empty()));
    let tail = format!(" ({bots})");
    let name = truncate(name, room.saturating_sub(2 + w(&tail)));
    let label = format!("{} {name}{tail}", if folded { "▸" } else { "▾" });
    put(buf, row.x + 1, row.y, u(room), &label, t.dim.add_modifier(Modifier::BOLD));
    if !badge.is_empty() {
        rput(buf, row.right() - 1, row.y, &badge, t.btn_primary);
    }
}

/// A bot's two rows: avatar, name, status glyph and time, then its preview and unread badge; the
/// lead's star below its avatar.
fn bot_row(buf: &mut Buffer, row: Rect, app: &mut App, b: &Bot, bar: Option<Style>) {
    let t = theme();
    let (x0, y) = (row.x, row.y);
    if let Some(bar) = bar {
        mark_cursor(buf, row, bar);
    }
    put_line(buf, x0 + 2, y, 2, &avatar(app, b));
    let (g, gs) = glyph(app, b.mark());
    let time = if b.status == Status::Working { elapsed(b.started_at) } else { when(b.last_at) };
    let tx = rput(buf, row.right() - 1, y, &time, if b.unread > 0 { t.bold } else { t.dim });
    let gx = tx.saturating_sub(2);
    put(buf, gx, y, 1, g, gs);
    // The badge sits below the avatar, leaving activity and unread indicators visible.
    if b.lead {
        put(buf, x0 + 3, y + 1, 1, "★", t.amber);
        app.hits.clicks.push((Rect::new(x0 + 3, y + 1, 1, 1), Click::LeadAgent));
    }
    let name_x = x0 + 5;
    put(buf, name_x, y, gx.saturating_sub(name_x + 1), &b.name, t.bold);
    let (pv, ps) = preview(b);
    let badge = if b.unread > 0 { format!(" {} ", b.unread) } else { String::new() };
    let room = usize::from(row.width.saturating_sub(7)).saturating_sub(w(&badge) + usize::from(!badge.is_empty()));
    put(buf, x0 + 5, y + 1, u(room), &truncate(&pv, room), ps);
    if !badge.is_empty() {
        rput(buf, row.right() - 1, y + 1, &badge, t.btn_primary);
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
    let shown: Vec<&Bot> = bots.iter().collect();
    let lines = roster_lines(&shown, &app.folded);
    let cursor = cursor_line(&lines, app.bot());
    let cells = stops(&lines);
    let at = cells.iter().position(|&(i, _)| Some(i) == cursor).unwrap_or(0);
    let per = 2u16;
    let view = r.height.saturating_sub(1) / per;
    let offset = (u(at) + 1).saturating_sub(view);
    for (n, &(i, _)) in cells.iter().enumerate().skip(usize::from(offset)) {
        let y = r.y + 1 + (u(n) - offset) * per;
        if y >= r.bottom() {
            break;
        }
        let row = Rect::new(r.x, y, r.width, 1);
        // A folded section's cell unfolds it, like its label in the full roster.
        if let Some(click) = lines[i].click() {
            app.hits.clicks.push((row, click));
        }
        if cursor == Some(i) {
            mark_cursor(buf, row, if app.focus == Focus::Roster { t.bold } else { t.secondary });
        }
        match &lines[i] {
            RosterLine::Section { bots: count, unread, .. } => {
                compact_section(buf, row, *count, (*unread > 0).then(|| glyph(app, Mark::Unread)));
            }
            RosterLine::Bot(b) => {
                put_line(buf, r.x + 1, y, 2, &avatar(app, b));
                let (g, gs) = glyph(app, b.mark());
                put(buf, r.x + 3, y, 1, g, gs);
                if b.lead && y + 1 < r.bottom() {
                    put(buf, r.x + 2, y + 1, 1, "★", t.amber);
                    app.hits.clicks.push((Rect::new(r.x + 2, y + 1, 1, 1), Click::LeadAgent));
                }
            }
            RosterLine::Label(_) => {}
        }
    }
}

/// A folded section in the compact strip: `▸`, its bot count and the unread glyph if it has unread.
fn compact_section(buf: &mut Buffer, row: Rect, bots: usize, unread: Option<(&str, Style)>) {
    let t = theme();
    let x = put(buf, row.x + 1, row.y, 1, "▸", t.dim);
    let x = put(buf, x, row.y, 2, &bots.to_string(), t.dim.add_modifier(Modifier::BOLD));
    if let Some((g, style)) = unread {
        put(buf, x, row.y, 1, g, style);
    }
}

/// The cursor's rows: a selection tint and a bar in the first cell of each.
fn mark_cursor(buf: &mut Buffer, row: Rect, bar: Style) {
    restyle(buf, row, theme().sel);
    for y in row.top()..row.bottom() {
        put(buf, row.x, y, 1, "▌", bar);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_section_name_gives_way_to_the_count_and_leaves_a_gap_before_the_badge() {
        let row = Rect::new(0, 0, 24, 1);
        let mut buf = Buffer::empty(row);
        section_row(&mut buf, row, None, &"n".repeat(40), 12, 7, true);
        let text = (0..row.width).map(|x| buf[(x, 0)].symbol()).collect::<String>();
        assert_eq!(text, " ▸ nnnnnnnnnn… (12)  7  ");
    }

    #[test]
    fn a_folded_section_is_one_cell_row_in_the_compact_strip() {
        let row = Rect::new(0, 0, 5, 1);
        for (bots, unread, text) in [(3, None, " ▸3  "), (12, Some(("●", Style::new())), " ▸12●")] {
            let mut buf = Buffer::empty(row);
            compact_section(&mut buf, row, bots, unread);
            let cells = (0..row.width).map(|x| buf[(x, 0)].symbol()).collect::<String>();
            assert_eq!(cells, text);
        }
    }
}
