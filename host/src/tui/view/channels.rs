//! The Channels sheet: Telegram bots and the Slack connection.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::super::app::ChannelsSheet;
use super::super::app::channels::{bot_lines, pairing_instruction, slack_text};
use super::super::l10n::tr;
use super::super::md::truncate;
use super::buffer::{centered, frame_box, put, restyle, u};
use super::style::theme;

pub(super) fn channels(buf: &mut Buffer, area: Rect, s: &ChannelsSheet) {
    let t = theme();
    let rows = s.bots.len() * 2 + 9;
    let r = centered(area, 86, u(rows.clamp(12, usize::from(area.height.saturating_sub(2)).max(12))));
    let title = tr("Channels");
    let inner = frame_box(buf, r, t.text, t.panel, Some((title.as_ref(), t.text)));
    if inner.height < 6 {
        return;
    }
    let width = usize::from(inner.width);
    let now = crate::store::now_ms();
    let mut y = inner.y + 1;
    put(buf, inner.x, y, inner.width, &tr("Telegram"), t.secondary.patch(t.panel));
    y += 1;
    if s.bots.is_empty() {
        put(
            buf,
            inner.x,
            y,
            inner.width,
            &tr("No Telegram bots yet. Add one in the desktop app."),
            t.dim.patch(t.panel),
        );
        y += 1;
    }
    for (i, bot) in s.bots.iter().enumerate() {
        let selected = i == s.sel;
        if selected {
            restyle(buf, Rect::new(inner.x.saturating_sub(1), y, inner.width + 2, 2), t.sel);
        }
        let base = if selected { t.sel } else { t.panel };
        let [first, second] = bot_lines(bot, now);
        put(buf, inner.x, y, inner.width, &truncate(&first, width), t.text.patch(base));
        put(buf, inner.x, y + 1, inner.width, &truncate(&second, width), t.secondary.patch(base));
        y += 2;
    }
    if let Some((name, code)) = &s.code {
        let username =
            s.bots.iter().find(|b| b["name"] == name.as_str()).and_then(|b| b["username"].as_str()).unwrap_or_default();
        put(
            buf,
            inner.x,
            y,
            inner.width,
            &truncate(&pairing_instruction(code, username), width),
            t.text.patch(t.panel),
        );
        y += 1;
    }
    y += 1;
    put(
        buf,
        inner.x,
        y,
        inner.width,
        &truncate(&format!("Slack: {}", tr(&slack_text(s.slack.as_ref()))), width),
        t.secondary.patch(t.panel),
    );
    put(
        buf,
        inner.x,
        inner.bottom() - 1,
        inner.width,
        &tr("↑↓ pick  p pair someone  r refresh  esc close"),
        t.dim.patch(t.panel),
    );
}
