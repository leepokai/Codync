//! The keys sheet.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;

use super::super::app::Editor;
use super::buffer::{centered, frame_box, put, u};
use super::overlay::field_line;
use super::style::theme;

const HELP: [(&str, &str, &str); 55] = [
    ("MOVE", "j k  ↑ ↓", "next / previous bot"),
    ("MOVE", "[ ]", "previous / next bot"),
    ("MOVE", "1…9", "jump to bot 1–9"),
    ("MOVE", "!", "next bot that needs you"),
    ("MOVE", "u", "next unread bot"),
    ("MOVE", "^k", "go to…"),
    ("MOVE", "tab  h l", "roster ⇄ chat ⇄ trace"),
    ("MOVE", "^d ^u", "half page down / up"),
    ("MOVE", "g G", "top / bottom"),
    ("CHAT", "i  ↵", "type a message"),
    ("CHAT", "y", "allow once"),
    ("CHAT", "a", "always allow"),
    ("CHAT", "n", "reject"),
    ("CHAT", "N", "reject and say why"),
    ("CHAT", "1…9", "pick an approval option"),
    ("CHAT", "s", "stop the bot"),
    ("CHAT", "o", "last turn's steps"),
    ("CHAT", "c", "copy the last reply"),
    ("CHAT", "C", "open connection / login request"),
    ("CHAT", "r", "reply in a thread"),
    ("CHAT", "v", "pick a message"),
    ("CHAT", "j k  ↵", "picking: move / open"),
    ("CHAT", "1…6", "picking: react"),
    ("CHAT", "f", "picking: save its files"),
    ("CHAT", "↵", "picking a bot message: open its conversation"),
    ("CHAT", "esc", "close the thread"),
    ("TYPE", "↵", "send"),
    ("TYPE", "⇧↵ alt↵ ^j", "new line"),
    ("TYPE", "esc", "back to NAV"),
    ("TYPE", "^c", "clear the message"),
    ("TYPE", "^a ^e", "line start / end"),
    ("TYPE", "alt b / f", "word left / right"),
    ("TYPE", "^w ^u", "delete word / line"),
    ("TYPE", "@name", "group: ask just that bot"),
    ("BOTS", "n", "new bot"),
    ("BOTS", "m", "new group chat"),
    ("BOTS", "e", "edit bot or group"),
    ("BOTS", "p", "pin / unpin"),
    ("BOTS", "J K", "move bot down / up"),
    ("BOTS", "S", "new session"),
    ("BOTS", "M", "memory"),
    ("BOTS", "R", "routines"),
    ("BOTS", "A", "marketplace, sign-in"),
    ("BOTS", "x", "delete bot or group"),
    ("VIEW", "t", "trace pane"),
    ("VIEW", "T", "trace full screen"),
    ("VIEW", "{ }", "previous / next turn"),
    ("VIEW", "b", "compact roster"),
    ("VIEW", "U", "usage"),
    ("VIEW", "P", "pair a phone"),
    ("VIEW", "^k share", "share usage analytics on / off"),
    ("VIEW", "esc", "close / go back"),
    ("VIEW", "q", "quit; bots keep going"),
    ("MOUSE", "click", "open, press a button"),
    ("MOUSE", "wheel", "scroll under pointer"),
];

pub(super) fn help(buf: &mut Buffer, area: Rect, filter: &Editor) {
    let t = theme();
    // Two columns fit everything from 42 rows up; shorter windows get a third.
    let r = centered(area, if area.height >= 42 { 80 } else { 120 }, 40);
    let inner = frame_box(buf, r, t.text, t.panel, Some(("Keys", t.text)));
    field_line(buf, inner, inner.y, filter, "filter actions and keys", true);
    let q = filter.text.to_lowercase();
    let items: Vec<&(&str, &str, &str)> = HELP
        .iter()
        .filter(|(_, k, d)| q.is_empty() || d.to_lowercase().contains(&q) || k.to_lowercase().contains(&q))
        .collect();
    // As many 38-wide columns as fit; what doesn't fit is counted in the footer.
    let cols = (inner.width / 38).max(1);
    let col_w = inner.width / cols;
    let rows = usize::from(inner.height.saturating_sub(3));
    let total = items.len();
    let (mut col, mut y, mut section, mut shown) = (0u16, inner.y + 2, "", 0usize);
    for (sec, k, d) in items {
        let need = if *sec == section { 1 } else { 2 + u16::from(!section.is_empty()) };
        if y + need > inner.y + 2 + u(rows) {
            if col + 1 == cols {
                break;
            }
            col += 1;
            y = inner.y + 2;
            section = "";
        }
        let x = inner.x + col * col_w;
        if *sec != section {
            if !section.is_empty() {
                y += 1;
            }
            put(buf, x, y, col_w, sec, t.dim.patch(t.panel).add_modifier(Modifier::BOLD));
            y += 1;
            section = sec;
        }
        put(buf, x, y, 11, k, t.bold.patch(t.panel));
        put(buf, x + 12, y, col_w.saturating_sub(13), d, t.secondary.patch(t.panel));
        y += 1;
        shown += 1;
    }
    let foot =
        if shown < total { format!("{} more: type to filter · esc close", total - shown) } else { "esc close".into() };
    put(buf, inner.x, inner.bottom() - 1, inner.width, &foot, t.dim.patch(t.panel));
}
