//! The dev servers sheet: a line per server, and one server's log; and their summary in the chat's header.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::super::app::{App, Bot, Click};
use super::super::l10n::tr;
use super::super::manage::{DevServerSummary, DevServersSheet, LogsView, dev_server_summary};
use super::buffer::{centered, frame_box, hline, put, rput};
use super::style::theme;
use crate::dev_servers::Status;

const WIDTH: u16 = 84;

/// The port in quiet text, left of the status; a click opens the servers. Returns where the header's free room ends.
pub(super) fn in_header(buf: &mut Buffer, r: Rect, end: u16, app: &mut App, b: &Bot) -> u16 {
    let t = theme();
    let (text, style) = match dev_server_summary(&b.dev_servers) {
        None => return end,
        Some(DevServerSummary::Running(text)) => (text, t.dim),
        Some(DevServerSummary::Starting) => (tr("Starting").into_owned(), t.dim),
        Some(DevServerSummary::Failed) => (format!("× {}", tr("Failed to start")), t.red),
    };
    let right = if end < r.right() - 1 { end.saturating_sub(2) } else { end };
    let start = rput(buf, right, r.y, &text, style);
    app.hits.clicks.push((Rect::new(start, r.y, right.saturating_sub(start), 1), Click::DevServers));
    start
}

pub(super) fn dev_servers(buf: &mut Buffer, area: Rect, app: &App, sheet: &DevServersSheet) {
    match &sheet.logs {
        Some(logs) => log_view(buf, area, logs),
        None => list(buf, area, app, sheet),
    }
}

fn list(buf: &mut Buffer, area: Rect, app: &App, sheet: &DevServersSheet) {
    let t = theme();
    let servers = app.bots.get(&sheet.bot).map(|bot| bot.dev_servers.as_slice()).unwrap_or_default();
    let height = u16::try_from(servers.len()).unwrap_or(u16::MAX).saturating_add(7);
    let r = centered(area, WIDTH, height.min(area.height.saturating_sub(2)));
    let inner = frame_box(buf, r, t.text, t.panel, Some((&tr("Dev servers"), t.text)));
    if inner.height < 4 {
        return;
    }
    let selected = sheet.selected.min(servers.len().saturating_sub(1));
    let rows = usize::from(inner.height.saturating_sub(3));
    for (i, server) in servers.iter().take(rows).enumerate() {
        let y = inner.y + u16::try_from(i).unwrap_or(0);
        let chosen = i == selected;
        let row = if chosen { t.sel.patch(t.panel) } else { t.panel };
        for x in inner.x..inner.right() {
            buf[(x, y)].set_style(row);
        }
        put(buf, inner.x, y, 2, if chosen { "▸" } else { " " }, t.text.patch(row));
        put(buf, inner.x + 2, y, 18, &server.name, t.text.patch(row));
        let style = if server.status == Status::Failed { t.red } else { t.dim };
        put(buf, inner.x + 21, y, 20, &tr(&server.value()), style.patch(row));
        put(buf, inner.x + 42, y, inner.width.saturating_sub(42), &server.command, t.dim.patch(row));
    }
    let Some(server) = servers.get(selected) else { return };
    put(buf, inner.x + 2, inner.bottom() - 3, inner.width.saturating_sub(2), &server.note(), t.dim.patch(t.panel));
    hline(buf, inner.x, inner.bottom() - 2, inner.width, t.dim.patch(t.panel));
    let keys: Vec<String> = server.key_hints().into_iter().map(|key| tr(key).into_owned()).collect();
    put(buf, inner.x, inner.bottom() - 1, inner.width, &format!(" {}", keys.join(" · ")), t.dim.patch(t.panel));
}

fn log_view(buf: &mut Buffer, area: Rect, logs: &LogsView) {
    let t = theme();
    let r = centered(area, WIDTH, area.height.saturating_sub(4));
    let inner = frame_box(buf, r, t.text, t.panel, Some((&logs.name, t.text)));
    if inner.height < 4 {
        return;
    }
    put(buf, inner.x, inner.y, inner.width, &logs.detail, t.dim.patch(t.panel));
    hline(buf, inner.x, inner.y + 1, inner.width, t.dim.patch(t.panel));
    let body = Rect::new(inner.x, inner.y + 2, inner.width, inner.height - 4);
    match &logs.text {
        None => {
            put(buf, body.x, body.y, body.width, &tr("Loading…"), t.dim.patch(t.panel));
        }
        Some(text) if text.is_empty() => {
            put(buf, body.x, body.y, body.width, &tr("No output yet"), t.dim.patch(t.panel));
        }
        Some(text) => {
            let lines: Vec<&str> = text.lines().collect();
            let height = usize::from(body.height);
            let end = lines.len().saturating_sub(logs.scroll).max(height.min(lines.len()));
            let start = end.saturating_sub(height);
            for (i, line) in lines[start..end].iter().enumerate() {
                let y = body.y + u16::try_from(i).unwrap_or(0);
                put(buf, body.x, y, body.width, line, t.text.patch(t.panel));
            }
        }
    }
    hline(buf, inner.x, inner.bottom() - 2, inner.width, t.dim.patch(t.panel));
    put(buf, inner.x, inner.bottom() - 1, inner.width, &tr(" ↑↓ scroll · r refresh · esc back"), t.dim.patch(t.panel));
}
