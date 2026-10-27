//! Context meter on the composer's right edge, with W / click for its last reported counts.
use ratatui::{buffer::Buffer, layout::Rect, text::Span};

use super::super::app::{App, Bot, Click};
use super::super::{l10n::tr, md};
use super::buffer::{frame_box, put, put_line, rput, u};
use super::style::theme;
use crate::chat::context_usage::Level;

pub(super) fn meter(buf: &mut Buffer, area: Rect, app: &mut App, bot: &Bot) {
    let t = theme();
    let (text, ink) = match &bot.context_usage {
        Some(usage) => {
            let ink = match usage.level {
                Level::Normal => t.secondary,
                Level::Warning => t.amber,
                Level::Danger => t.red,
            };
            (format!(" ○ {}% · W ", usage.percent), ink)
        }
        None => (" ○ – · W ".into(), t.dim),
    };
    let x = rput(buf, area.right().saturating_sub(1), area.y, &text, ink);
    app.hits.clicks.push((Rect::new(x, area.y, u(md::width(&text)), 1), Click::ContextUsage));
}

pub(super) fn detail(buf: &mut Buffer, area: Rect, app: &App) {
    let t = theme();
    let width = area.width.saturating_sub(4).min(38);
    let r = Rect::new(area.right().saturating_sub(width + 2), area.bottom().saturating_sub(10), width, 7);
    let inner = frame_box(buf, r, t.secondary, t.panel, Some((tr("Context window").as_ref(), t.bold)));
    let usage = app.bot().and_then(|b| b.context_usage.as_ref());
    let text = match usage {
        Some(u) => format!(
            "{}%\n{}",
            u.percent,
            tr("{used} / {max} tokens")
                .replace("{used}", &u.used_tokens.to_string())
                .replace("{max}", &u.max_tokens.to_string())
        ),
        None => tr("The agent has not reported this session's context usage yet.").into_owned(),
    };
    let lines = text
        .lines()
        .flat_map(|line| md::wrap(&[Span::styled(line.to_owned(), t.text)], usize::from(inner.width), &[], &[]));
    for (offset, line) in lines.take(usize::from(inner.height.saturating_sub(1))).enumerate() {
        put_line(buf, inner.x, inner.y + u(offset), inner.width, &line);
    }
    put(buf, inner.x, inner.bottom().saturating_sub(1), inner.width, tr("esc close").as_ref(), t.dim);
}
