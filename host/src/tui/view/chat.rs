//! The chat pane: header, transcript viewport and composer.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::super::app::{App, Click, Editor, Entry, Mark, SlashAction, Status};
use super::super::l10n::tr;
use super::super::md::{truncate, width as w};
use super::buffer::{fill, hline, put, put_line, rput, u};
use super::set_cursor;
use super::style::{avatar, rgb, spin, theme};
use super::time::elapsed;
use super::transcript::build_chat;

pub(super) fn chat(buf: &mut Buffer, r: Rect, app: &mut App, narrow: bool) {
    let t = theme();
    app.hits.chat = r;
    let Some(b) = app.bot().cloned() else {
        let msg = match &app.error {
            Some(e) if app.bots.is_empty() => e.as_str(),
            _ if app.bots.is_empty() && !app.online => "Connecting…",
            _ if app.bots.is_empty() => "n creates your first bot",
            _ => "Pick a bot",
        };
        put(buf, r.x + 2, r.y + 1, r.width.saturating_sub(2), msg, t.dim);
        return;
    };
    // Header.
    let mut x = r.x + 1;
    if narrow {
        fill(buf, Rect::new(r.x, r.y, r.width, 1), t.panel);
        x = put(buf, x, r.y, 2, "‹", t.bold) + 1;
    }
    put_line(buf, x, r.y, 2, &avatar(app, &b));
    x = put(buf, x + 3, r.y, r.width / 3, &b.name, t.bold) + 2;
    let (status, ss) = match b.mark() {
        Mark::Need => ("◆ needs you".to_owned(), t.amber),
        Mark::Work => (format!("{} working {}", spin(app), elapsed(b.started_at)), t.secondary),
        Mark::Error => ("× error".to_owned(), t.red),
        _ => (String::new(), t.dim),
    };
    let sx = rput(buf, r.right() - 1, r.y, &status, ss.add_modifier(Modifier::BOLD));
    let room = sx.saturating_sub(x + 2);
    if b.group {
        let names: Vec<String> = b.members.iter().map(|m| app.author_name(Some(m))).collect();
        put(buf, x, r.y, room, &truncate(&format!("group · {}", names.join(", ")), usize::from(room)), t.secondary);
    } else if !narrow {
        let mode = match b.mode.as_deref() {
            _ if b.auto => "approve all",
            None => "ask",
            Some(id) => app.mode_name(&b.backend, id),
        };
        let fast = match (b.fast_mode, b.fast_refusal().is_some()) {
            (true, true) => " · ⚡ refused",
            (true, false) => " · ⚡ fast",
            _ => "",
        };
        let cwd =
            truncate(&b.folder(&app.home), usize::from(room).saturating_sub(w(&b.backend) + w(mode) + w(fast) + 6));
        put(buf, x, r.y, room, &format!("{} · {cwd} · {mode}{fast}", b.backend), t.secondary);
    }
    let mut top = r.y + 1;
    if !narrow {
        hline(buf, r.x, top, r.width, t.line);
        top += 1;
    }
    if !app.online {
        let bar = Rect::new(r.x, top, r.width, 1);
        let s = if t.color { Style::default().fg(t.on_color).bg(rgb(0xF0A030)) } else { t.sel };
        fill(buf, bar, s);
        put(
            buf,
            r.x + 1,
            top,
            r.width - 2,
            &format!("{} Reconnecting to {}…", spin(app), if app.host.is_empty() { &app.url } else { &app.host }),
            s.add_modifier(Modifier::BOLD),
        );
        top += 1;
    }

    // Composer at the bottom.
    let draft = app.draft();
    let inner = usize::from(r.width.saturating_sub(4)).max(1);
    let (clines, cursor) = composer_lines(&draft, inner);
    let ch = u(clines.len().clamp(1, 8));
    // The message it replies to, then attached files: a row each above the text.
    let quote = app.replying_to().map(Entry::quoted);
    let qh = u16::from(quote.is_some());
    let files = app.draft_files();
    let fh = u16::from(!files.is_empty());
    let box_top = r.bottom().saturating_sub(ch + qh + fh + 2);
    let typing = app.typing && app.overlays.is_empty();
    let rule = if typing { t.text } else { t.line };
    hline(buf, r.x, box_top, r.width, rule);
    hline(buf, r.x, box_top + qh + fh + ch + 1, r.width, rule);
    if let Some(quote) = quote {
        let quote = super::roster::plain(&quote);
        let hint = if typing { "" } else { "esc cancels" };
        let room = r.width.saturating_sub(u(w(hint)) + 6);
        put(
            buf,
            r.x + 3,
            box_top + 1,
            room,
            &format!("↳ “{}”", truncate(&quote, usize::from(room).saturating_sub(3))),
            t.secondary,
        );
        rput(buf, r.right() - 1, box_top + 1, hint, t.dim);
    }
    if fh > 0 {
        let names: Vec<String> = files
            .iter()
            .map(|p| format!("▤ {}", p.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()))
            .collect();
        let hint = if draft.text.is_empty() { "⌫ removes" } else { "" };
        let row = box_top + qh + 1;
        put(buf, r.x + 3, row, r.width.saturating_sub(u(w(hint)) + 6), &names.join("  "), t.secondary);
        rput(buf, r.right() - 1, row, hint, t.dim);
    }
    let comp_top = box_top + qh + fh;
    put(buf, r.x + 1, comp_top + 1, 1, "›", if typing { t.bold } else { t.dim });
    let comp_rect = Rect::new(r.x, box_top, r.width, ch + qh + fh + 2);
    app.hits.clicks.push((comp_rect, Click::Composer));
    let first = clines.len().saturating_sub(usize::from(ch)).min(cursor.1.saturating_sub(usize::from(ch) - 1));
    // Where the first composer line's text ends, so the chip on its right never covers it.
    let mut used = clines.get(first).map_or(0, |l| w(l));
    if draft.text.is_empty() {
        // The apps' wording.
        let placeholder = if let Some(pick) = &app.pick {
            let artifact = app.lane(&b.id).iter().any(|e| &e.id == pick && e.artifact().is_some());
            if artifact {
                "j k pick · o open in the browser · f save as HTML · ↵ reply · 1–6 react · c copy · esc"
            } else {
                "j k pick · ↵ reply · 1–6 react · f save files · c copy · esc"
            }
            .to_owned()
        } else if app.reply.is_some() {
            tr("Reply…").into_owned()
        } else if b.group {
            tr("Ask everyone · @ to ask one bot").into_owned()
        } else {
            format!("Message {}…", b.name)
        };
        let placeholder = truncate(&placeholder, usize::from(r.width.saturating_sub(5)));
        put(buf, r.x + 3, comp_top + 1, r.width.saturating_sub(4), &placeholder, t.dim);
        used = w(&placeholder);
    } else {
        for (i, l) in clines.iter().skip(first).take(usize::from(ch)).enumerate() {
            put(buf, r.x + 3, comp_top + 1 + u(i), r.width.saturating_sub(4), l, t.text);
        }
    }
    // Only a turn in this chat holds a message back; in a group a new one ends the round instead.
    let here = b.works_here();
    let chip = if typing {
        match b.status {
            Status::Working if here && b.group => "goes in after this reply",
            Status::Working if here => "sends after this turn",
            Status::NeedsInput if here => "sends after the approval",
            _ => "",
        }
    } else if app.pick.is_some() {
        if draft.text.is_empty() { "" } else { "↵ replies to it · esc cancels" }
    } else if matches!(b.status, Status::Working | Status::NeedsInput) {
        "s stops"
    } else {
        "i to type · r reply"
    };
    if (draft.text.is_empty() || !typing) && used + w(chip) + 6 <= usize::from(r.width) {
        rput(buf, r.right() - 1, comp_top + 1, chip, t.dim);
    }
    if typing {
        let cy = comp_top + 1 + u(cursor.1.saturating_sub(first));
        set_cursor(Position::new(r.x + 3 + u(cursor.0), cy.min(comp_top + ch)));
    }

    // Transcript.
    let body = Rect::new(r.x, top + 1, r.width, box_top.saturating_sub(top + 1));
    app.chat_height = usize::from(body.height);
    let built = build_chat(app, &b, usize::from(body.width.saturating_sub(1)));
    let total = built.lines.len();
    let h = usize::from(body.height);
    let max = total.saturating_sub(h);
    // Keep the picked message on screen.
    let picked = app.pick.as_ref().and_then(|p| built.messages.iter().find(|m| &m.0 == p)).map(|m| (m.1, m.2));
    if let Some((from, to)) = picked {
        let start = total.saturating_sub(h + app.chat_scroll);
        if from < start {
            app.chat_scroll = total.saturating_sub(h + from);
        } else if to > start + h {
            app.chat_scroll = total.saturating_sub(to.max(h));
        }
    }
    app.chat_scroll = app.chat_scroll.min(max);
    app.chat_top = total > 0 && app.chat_scroll == max;
    let start = total.saturating_sub(h + app.chat_scroll);
    for (i, l) in built.lines.iter().skip(start).take(h).enumerate() {
        put_line(buf, body.x, body.y + u(i), body.width, l);
    }
    if let Some((from, to)) = picked {
        for li in from.max(start)..to.min(start + h) {
            put(buf, body.x, body.y + u(li - start), 1, "▌", t.amber);
        }
    }
    for (li, bx, bw, click) in built.buttons {
        if li >= start && li < start + h {
            let y = body.y + u(li - start);
            app.hits.clicks.push((Rect::new(body.x + bx, y, bw, 1), click));
        }
    }
    if app.chat_scroll > 0 {
        let s = format!(" ↓ {} more ", app.chat_scroll);
        rput(buf, r.right() - 1, box_top.saturating_sub(1), &s, t.btn);
    }
    if typing {
        command_menu(buf, body, app);
    }
}

/// The `/` list over the bottom of the transcript: the commands matching what's typed, the
/// highlighted one kept in view.
fn command_menu(buf: &mut Buffer, body: Rect, app: &App) {
    let t = theme();
    let commands = app.slash_commands();
    let sel = app.slash_selected(commands.len());
    let n = u(commands.len().min(5)).min(body.height);
    let start = sel.saturating_sub(usize::from(n).saturating_sub(1));
    for (row, (i, c)) in commands.iter().enumerate().skip(start).take(usize::from(n)).enumerate() {
        let y = body.bottom() - n + u(row);
        let bg = if i == sel { t.sel } else { t.panel };
        fill(buf, Rect::new(body.x, y, body.width, 1), bg);
        let name = if i == sel { t.bold } else { t.text };
        let x = put(buf, body.x + 3, y, 26, &truncate(&format!("/{}", c.name), 26), name.patch(bg)) + 2;
        let hint = match (i == sel, c.action) {
            (false, _) => "",
            (true, SlashAction::Skill) => "tab fills in",
            (true, _) => "↵ runs",
        };
        let rx = rput(buf, body.right() - 1, y, hint, t.dim.patch(bg));
        let room = usize::from(rx.saturating_sub(x + 1));
        put(buf, x, y, u(room), &truncate(&c.about(), room), t.secondary.patch(bg));
    }
}

/// Wraps the draft; returns display lines and the cursor's (column, line).
pub(in crate::tui) fn composer_lines(e: &Editor, width: usize) -> (Vec<String>, (usize, usize)) {
    let mut lines = vec![String::new()];
    let mut col = 0;
    let mut cur = (0, 0);
    for (i, c) in e.text.char_indices() {
        if i == e.cursor {
            cur = (col, lines.len() - 1);
        }
        if c == '\n' {
            lines.push(String::new());
            col = 0;
            continue;
        }
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if col + cw > width {
            lines.push(String::new());
            col = 0;
            if i == e.cursor {
                cur = (0, lines.len() - 1);
            }
        }
        if let Some(l) = lines.last_mut() {
            l.push(c);
        }
        col += cw;
    }
    if e.cursor >= e.text.len() {
        cur = (col, lines.len() - 1);
        if col >= width {
            lines.push(String::new());
            cur = (0, lines.len() - 1);
        }
    }
    (lines, cur)
}

pub(super) fn with_bg(lines: Vec<Line<'static>>, width: usize, bg: Style) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .map(|l| {
            let used: usize = l.spans.iter().map(|s| w(&s.content)).sum();
            let mut spans: Vec<Span<'static>> =
                l.spans.into_iter().map(|s| Span::styled(s.content, s.style.patch(bg))).collect();
            spans.push(Span::styled(" ".repeat(width.saturating_sub(used)), bg));
            Line::from(spans)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composer_cursor_tracks_wraps_and_newlines() {
        let (l, c) = composer_lines(&Editor::with("ab\ncd"), 10);
        assert_eq!(l, ["ab", "cd"]);
        assert_eq!(c, (2, 1));
        let (l, c) = composer_lines(&Editor::with("abcdef"), 3);
        assert_eq!(l, ["abc", "def", ""]);
        assert_eq!(c, (0, 2));
    }
}
