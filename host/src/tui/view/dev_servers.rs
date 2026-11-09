//! The dev servers sheet: a line per server, and one server's log; and their summary in the chat's header.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::super::app::{App, Bot, Click};
use super::super::l10n::tr;
use super::super::manage::{DevServer, DevServerSummary, DevServersSheet, LogsView, dev_server_summary};
use super::buffer::{centered, frame_box, hline, put, rput, u, window};
use super::network::wrapped;
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
    let selected = sheet.selected.min(servers.len().saturating_sub(1));
    let server = servers.get(selected);
    // Anywhere else its addresses stay on screen: a hint fades and cuts them off, and this device may reach only some.
    let addresses = server.filter(|_| !app.on_this_computer).map(DevServer::reachable).unwrap_or_default();
    let height = u(servers.len() + addresses.len()).saturating_add(7);
    let r = centered(area, WIDTH, height.min(area.height.saturating_sub(2)));
    let inner = frame_box(buf, r, t.text, t.panel, Some((&tr("Dev servers"), t.text)));
    if inner.height < 4 {
        return;
    }
    let address_lines = u(addresses.len()).min(inner.height - 4);
    let rows = usize::from(inner.height - 3 - address_lines);
    for (y, (i, server)) in (inner.y..).zip(servers.iter().enumerate().skip(window(selected, rows)).take(rows)) {
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
    let Some(server) = server else { return };
    let first_address = inner.bottom() - 3 - address_lines;
    for (y, address) in (first_address..inner.bottom() - 3).zip(&addresses) {
        put(buf, inner.x + 2, y, inner.width.saturating_sub(2), address, t.dim.patch(t.panel));
    }
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
    match (&logs.error, logs.text.as_deref()) {
        (Some(error), text) => {
            let mut y = body.y;
            wrapped(buf, body, &mut y, error, t.red);
            let below = Rect::new(body.x, y, body.width, body.bottom().saturating_sub(y));
            log_lines(buf, below, text.unwrap_or_default(), logs.scroll);
        }
        (None, None) => {
            put(buf, body.x, body.y, body.width, &tr("Loading…"), t.dim.patch(t.panel));
        }
        (None, Some("")) => {
            put(buf, body.x, body.y, body.width, &tr("No output yet"), t.dim.patch(t.panel));
        }
        (None, Some(text)) => log_lines(buf, body, text, logs.scroll),
    }
    hline(buf, inner.x, inner.bottom() - 2, inner.width, t.dim.patch(t.panel));
    put(buf, inner.x, inner.bottom() - 1, inner.width, &tr(" ↑↓ scroll · r refresh · esc back"), t.dim.patch(t.panel));
}

/// The end of `text` that fits `body`, `scroll` lines up from the last.
fn log_lines(buf: &mut Buffer, body: Rect, text: &str, scroll: usize) {
    let t = theme();
    let lines: Vec<&str> = text.lines().collect();
    let height = usize::from(body.height);
    let end = lines.len().saturating_sub(scroll).max(height.min(lines.len()));
    let start = end.saturating_sub(height);
    for (y, line) in (body.y..).zip(&lines[start..end]) {
        put(buf, body.x, y, body.width, line, t.text.patch(t.panel));
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::tui::app::{Overlay, test_app};
    use crate::tui::manage::Reply;

    /// What `draw` leaves in `area`, line by line.
    fn screen(area: Rect, draw: impl FnOnce(&mut Buffer)) -> Vec<String> {
        let mut buf = Buffer::empty(area);
        draw(&mut buf);
        (0..area.height).map(|y| (0..area.width).map(|x| buf[(x, y)].symbol()).collect()).collect()
    }

    #[tokio::test]
    async fn elsewhere_the_sheet_lists_every_address_of_the_selected_server_whole() {
        let addresses = ["http://100.64.0.1:5173", "http://mac.tailnet-1234.ts.net:5173", "http://192.168.1.20:5173"];
        let server = json!({"id": "a", "name": "web", "command": "npm run dev", "status": "running", "managed": true,
            "ports": [{"port": 5173, "local": "http://localhost:5173", "remote": addresses}]});
        let mut app = test_app();
        let bot = json!({"id": "b", "name": "Bot", "devServers": [server]});
        app.bots.insert("b".into(), Bot::parse(&bot).expect("a bot with servers parses"));
        let sheet = DevServersSheet { bot: "b".into(), selected: 0, logs: None };
        let area = Rect::new(0, 0, 100, 24);
        let shown = |app: &App| screen(area, |buf| list(buf, area, app, &sheet));

        app.on_this_computer = false;
        let lines = shown(&app);
        let at = |address: &str| lines.iter().position(|line| line.contains(address));
        let rows: Vec<_> = addresses.iter().map(|address| at(address).expect("the whole address shows")).collect();
        assert_eq!(rows, [rows[0], rows[0] + 1, rows[0] + 2], "one line each");
        assert!(at("npm run dev") < Some(rows[0]), "under the servers");

        app.on_this_computer = true;
        assert!(!shown(&app).concat().contains("100.64.0.1"), "a browser opens here");
    }

    #[tokio::test]
    async fn the_selected_server_stays_on_screen_when_more_servers_exist_than_fit() {
        let servers: Vec<_> = (0..8)
            .map(|i| {
                json!({"id": format!("s{i}"), "name": format!("server-{i}"), "command": "x", "status": "stopped",
                "ports": [], "managed": true})
            })
            .collect();
        let mut app = test_app();
        let bot = json!({"id": "b", "name": "Bot", "devServers": servers});
        app.bots.insert("b".into(), Bot::parse(&bot).expect("a bot with servers parses"));
        let area = Rect::new(0, 0, 100, 10);
        let shown = |selected| {
            let sheet = DevServersSheet { bot: "b".into(), selected, logs: None };
            screen(area, |buf| list(buf, area, &app, &sheet)).concat()
        };

        assert!(shown(7).contains("▸ server-7"), "{}", shown(7));
        assert!(shown(0).contains("▸ server-0"), "{}", shown(0));
        assert!(!shown(0).contains("server-7"), "only what fits");
    }

    #[tokio::test]
    async fn a_failed_read_says_why_and_keeps_what_an_earlier_one_read() {
        let mut app = test_app();
        let view = LogsView {
            id: "a".into(),
            name: "web".into(),
            detail: "npm run dev".into(),
            text: None,
            error: None,
            scroll: 0,
        };
        app.overlays.push(Overlay::DevServers(DevServersSheet { bot: "b".into(), selected: 0, logs: Some(view) }));
        let area = Rect::new(0, 0, 100, 24);
        let reply = |app: &mut App, read: Result<&str, &str>| {
            let read = read.map(|text| json!({"text": text})).map_err(str::to_owned);
            app.on_sheet_reply(Reply::DevServerLogs("a".into()), read);
            let Some(Overlay::DevServers(DevServersSheet { logs: Some(logs), .. })) = app.overlays.last() else {
                panic!("the log is open")
            };
            screen(area, |buf| log_view(buf, area, logs)).concat()
        };

        let failed = reply(&mut app, Err("reading a.log: denied"));
        assert!(failed.contains("reading a.log: denied"), "{failed}");
        assert!(!failed.contains("No output yet") && !failed.contains("Loading"), "{failed}");

        let read = reply(&mut app, Ok("first line"));
        assert!(read.contains("first line") && !read.contains("denied"), "{read}");

        let failed = reply(&mut app, Err("reading a.log: denied"));
        assert!(failed.contains("denied") && failed.contains("first line"), "{failed}");

        let empty = reply(&mut app, Ok(""));
        assert!(empty.contains("No output yet") && !empty.contains("denied"), "{empty}");
    }
}
