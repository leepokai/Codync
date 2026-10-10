//! The read-only conversation sheet between two bots.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use super::super::app::{App, ConversationRow, Outcome, conversation, rows};
use super::super::manage::BotChatSheet;
use super::super::md::{self, width as w};
use super::buffer::{centered, frame_box, hline, put, put_line, u};
use super::chat::with_bg;
use super::style::{person, theme};
use super::time::when;
use super::transcript::name_style;

fn name_and_color(app: &App, id: &str) -> (String, String) {
    app.bots
        .get(id)
        .map_or_else(|| ("A deleted bot".to_owned(), "gray".to_owned()), |b| (b.name.clone(), b.color.clone()))
}

pub(super) fn bot_chat(buf: &mut Buffer, area: Rect, app: &App, c: &BotChatSheet) {
    let t = theme();
    let r = centered(area, 84, area.height.saturating_sub(4));
    let inner = frame_box(buf, r, t.text, t.panel, None);
    if inner.height < 5 {
        return;
    }
    let mut pill = vec![Span::raw(" ")];
    for (i, id) in [&c.bot, &c.peer].into_iter().enumerate() {
        let (name, color) = name_and_color(app, id);
        if i == 1 {
            pill.push(Span::styled(" ⇄ ", t.dim));
        }
        pill.extend([person(app, id), Span::raw(" "), Span::styled(name, name_style(&color))]);
    }
    put_line(buf, inner.x, inner.y, inner.width, &Line::from(pill));
    hline(buf, inner.x, inner.y + 1, inner.width, t.dim.patch(t.panel));
    put(buf, inner.x, inner.bottom() - 1, inner.width, " ↑↓ scroll · esc close", t.dim.patch(t.panel));

    let body = Rect::new(inner.x, inner.y + 2, inner.width, inner.height - 3);
    let lines = body_lines(app, c, usize::from(body.width));
    let h = usize::from(body.height);
    let max = lines.len().saturating_sub(h);
    c.max_scroll.set(max);
    c.page.set(h);
    let start = lines.len().saturating_sub(h + c.scroll.min(max));
    for (i, l) in lines.iter().skip(start).take(h).enumerate() {
        put_line(buf, body.x, body.y + u(i), body.width, l);
    }
}

fn body_lines(app: &App, c: &BotChatSheet, width: usize) -> Vec<Line<'static>> {
    let t = theme();
    let Some(fetched) = &c.fetched else {
        return vec![Line::from(Span::styled(" Loading…", t.dim))];
    };
    let live = app.entries.get(&c.bot);
    let exchanges = conversation(&c.bot, fetched, live, &c.peer);
    if exchanges.is_empty() {
        return vec![Line::from(Span::styled(" No messages yet.", t.dim))];
    }
    let mut out: Vec<Line<'static>> = vec![];
    for row in rows(&exchanges) {
        match row {
            ConversationRow::Separator(at) => {
                if !out.is_empty() {
                    out.push(Line::default());
                }
                let label = format!(" {} ", when(at));
                let side = width.saturating_sub(w(&label) + 4);
                out.push(Line::from(Span::styled(format!(" ───{label}{}", "─".repeat(side)), t.dim)));
            }
            ConversationRow::Message { author, text, shows_author } => {
                if shows_author {
                    out.push(Line::default());
                    let (name, color) = name_and_color(app, &author);
                    out.push(Line::from(vec![
                        Span::raw(" "),
                        person(app, &author),
                        Span::raw(" "),
                        Span::styled(name, name_style(&color)),
                    ]));
                }
                out.extend(with_bg(md::render(&text, width, 1), width, t.band));
            }
            ConversationRow::Status { outcome, awaiting } => {
                out.push(Line::default());
                if let Outcome::Failed(detail) = outcome {
                    let detail = if detail.is_empty() { "Didn't complete.".to_owned() } else { detail };
                    out.extend(md::wrap(&[Span::styled(detail, t.red)], width, &[Span::raw(" ")], &[Span::raw(" ")]));
                } else {
                    let (name, _) = name_and_color(app, &awaiting);
                    out.push(Line::from(Span::styled(format!(" Waiting for {name}…"), t.dim)));
                }
            }
        }
    }
    out
}
