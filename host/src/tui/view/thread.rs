//! A message's thread (its lines, a composer and its title) and the chat's thread list.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

use super::super::app::{App, Click};
use super::super::l10n::tr;
use super::super::manage::{ThreadSheet, ThreadsSheet};
use super::super::md::truncate;
use super::buffer::{centered, frame_box, hline, put, put_line, restyle, u};
use super::set_cursor;
use super::style::theme;
use super::time::when;
use super::transcript::{Built, entry_lines, turn_info};

/// The open thread: what it branched from, its lines and a composer.
pub(super) fn thread(buf: &mut Buffer, area: Rect, app: &mut App, s: &ThreadSheet) {
    let t = theme();
    let r = centered(area, 84, area.height.saturating_sub(4));
    let name = if s.title.is_empty() { tr("Thread").into_owned() } else { s.title.clone() };
    let title = if s.archived { format!("{name} · {}", tr("archived")) } else { name };
    let inner = frame_box(buf, r, t.text, t.panel, Some((title.as_str(), t.text)));
    if inner.height < 6 {
        return;
    }
    let width = usize::from(inner.width);
    let text_width = inner.width.saturating_sub(3);
    let mut top = inner.y;
    if let Some(branch) = &s.branch {
        let quote = truncate(&super::roster::plain(branch), width.saturating_sub(20));
        let text = format!(" ↳ {} “{quote}”", tr("Branched from"));
        put(buf, inner.x, top, inner.width, &text, t.dim.patch(t.panel));
        top += 1;
    }
    // The composer and its rule sit above the footer.
    let (clines, cursor) = super::composer_lines(&s.composer, width.saturating_sub(3).max(1));
    let ch = u(clines.len().clamp(1, 4));
    let footer = inner.bottom().saturating_sub(1);
    let rule = footer.saturating_sub(ch + 1);
    hline(buf, inner.x, rule, inner.width, t.line);
    let body = Rect::new(inner.x, top, inner.width, rule.saturating_sub(top).max(1));
    let built = match (s.entries.as_deref(), app.bots.get(s.bot.as_str())) {
        (Some(entries), Some(b)) => {
            let refs: Vec<_> = entries.iter().collect();
            let info = turn_info(&refs);
            let mut out = Built { lines: vec![], buttons: vec![], messages: vec![] };
            for e in &refs {
                entry_lines(&mut out, app, b, e, &info, width);
            }
            Some(out)
        }
        _ => None,
    };
    let placeholder = if s.entries.is_some() { tr("No replies yet") } else { tr("Loading…") };
    let lines = match &built {
        Some(b) if !b.lines.is_empty() => b.lines.clone(),
        _ => vec![Line::from(Span::styled(format!(" {placeholder}"), t.dim))],
    };
    let h = usize::from(body.height);
    let max = lines.len().saturating_sub(h);
    s.max_scroll.set(max);
    s.page.set(h);
    let start = lines.len().saturating_sub(h + s.scroll.min(max));
    for (i, l) in lines.iter().skip(start).take(h).enumerate() {
        put_line(buf, body.x, body.y + u(i), body.width, l);
    }
    // The composer.
    let comp_top = rule + 1;
    let first = cursor.1.saturating_sub(usize::from(ch) - 1);
    put(buf, inner.x + 1, comp_top, 1, "›", t.bold.patch(t.panel));
    for (i, line) in clines.iter().skip(first).take(usize::from(ch)).enumerate() {
        put(buf, inner.x + 3, comp_top + u(i), text_width, line, t.text.patch(t.panel));
    }
    if s.composer.text.is_empty() {
        put(buf, inner.x + 3, comp_top, text_width, &tr("Reply in the thread"), t.dim.patch(t.panel));
    }
    if s.sending {
        let y = comp_top + ch.saturating_sub(1);
        put(buf, inner.x + 3, y, text_width, &tr("Sending…"), t.dim.patch(t.panel));
    }
    let comp = Rect::new(inner.x, rule, inner.width, ch + 1);
    app.hits.clicks.push((comp, Click::Composer));
    set_cursor(Position::new(inner.x + 3 + u(cursor.0), comp_top + u(cursor.1 - first)));
    // Footer: the rename field, or the thread's keys.
    if let Some(r) = &s.renaming {
        let label = format!(" {}:", tr("Name"));
        put(buf, inner.x, footer, 6, &label, t.secondary.patch(t.panel));
        super::overlay::field_line(
            buf,
            Rect::new(inner.x + 7, footer, inner.width.saturating_sub(24), 1),
            footer,
            r,
            "Thread title",
            true,
        );
        let keys = format!(" ↵ {} · esc", tr("save"));
        put(buf, inner.right().saturating_sub(17), footer, 17, &keys, t.dim.patch(t.panel));
    } else {
        let archive = if s.archived { tr("unarchive") } else { tr("archive") };
        let keys = format!(
            " ↵ {} · ⇧↵ {} · r {} · a {archive} · d {} · esc",
            tr("send"),
            tr("new line"),
            tr("rename"),
            tr("delete")
        );
        put(buf, inner.x, footer, inner.width, &truncate(&keys, width), t.dim.patch(t.panel));
    }
    if let Some(e) = &s.error {
        let y = footer.saturating_sub(1);
        put(buf, inner.x, y, inner.width, &truncate(&tr(e), width), t.red.patch(t.panel));
    }
}

/// The chat's threads, newest first.
pub(super) fn threads(buf: &mut Buffer, area: Rect, app: &App, s: &ThreadsSheet) {
    let t = theme();
    let r = centered(area, 90, area.height.saturating_sub(4));
    let title =
        if s.archived { format!("{} · {}", tr("Threads"), tr("archived")) } else { tr("Threads").into_owned() };
    let inner = frame_box(buf, r, t.text, t.panel, Some((title.as_str(), t.text)));
    if inner.height < 6 {
        return;
    }
    let width = usize::from(inner.width);
    let heading = if s.archived { tr("Archived") } else { tr("Newest first") };
    put(buf, inner.x, inner.y + 1, inner.width, &truncate(&heading, width), t.secondary.patch(t.panel));
    let mut y = inner.y + 3;
    match &s.items {
        None => {
            put(buf, inner.x, y, inner.width, &tr("Loading…"), t.dim.patch(t.panel));
        }
        Some(items) if items.is_empty() => {
            let empty = if s.archived { tr("Nothing here yet.") } else { tr("No threads yet.") };
            put(buf, inner.x, y, inner.width, &empty, t.dim.patch(t.panel));
        }
        Some(items) => {
            let room = usize::from(inner.bottom().saturating_sub(y)).saturating_sub(2) / 2;
            let room = room.max(1);
            let off = s.cursor.saturating_sub(room - 1);
            for (i, item) in items.iter().enumerate().skip(off).take(room) {
                if y + 1 >= inner.bottom() {
                    break;
                }
                let selected = i == s.cursor;
                if selected {
                    restyle(buf, Rect::new(inner.x.saturating_sub(1), y, inner.width + 2, 2), t.sel);
                }
                let base = if selected { t.sel } else { t.panel };
                let name = item["title"].as_str().filter(|s| !s.is_empty()).unwrap_or("Thread");
                let unread = item["unread"].as_i64().unwrap_or(0);
                let mut head = vec![Span::raw(" ")];
                if unread > 0 {
                    head.push(Span::styled("• ", t.amber));
                }
                head.push(Span::styled(name.to_owned(), t.bold.patch(base)));
                put_line(buf, inner.x, y, inner.width, &Line::from(head));
                let branch = item["branchText"].as_str().filter(|s| !s.is_empty());
                let last = item["lastText"].as_str().filter(|s| !s.is_empty());
                let author = item["lastAuthor"].as_str().and_then(|id| app.bots.get(id)).map(|b| b.name.as_str());
                let detail = if let Some(branch) = branch {
                    format!("   {} “{}”", tr("Branched from"), super::roster::plain(branch))
                } else if let (Some(author), Some(last)) = (author, last) {
                    format!("   {author}: {}", super::roster::plain(last))
                } else {
                    format!("   {}", tr("No replies yet"))
                };
                put(buf, inner.x, y + 1, inner.width, &truncate(&detail, width), t.dim.patch(base));
                let right = inner.right().saturating_sub(12);
                if unread > 0 {
                    let new = format!("{unread} {}", tr("new"));
                    put(buf, right, y, 12, &new, t.amber.patch(base).add_modifier(Modifier::BOLD));
                } else if let Some(at) = item["updatedAt"].as_i64().filter(|a| *a > 0) {
                    put(buf, right, y, 12, &when(at), t.dim.patch(base));
                }
                y += 2;
            }
        }
    }
    let keys = format!(
        " ↑↓ {} · ↵ {} · r {} · x {} · d {} · a {} · esc",
        tr("pick"),
        tr("open"),
        tr("rename"),
        tr("archive"),
        tr("delete"),
        tr("archived")
    );
    let by = inner.bottom().saturating_sub(1);
    put(buf, inner.x, by, inner.width, &truncate(&keys, width), t.dim.patch(t.panel));
}
