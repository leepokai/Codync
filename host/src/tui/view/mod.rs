//! Drawing. Black and white: color comes only from the bots; amber means
//! "needs you" and red means an error. Every state also has its own glyph.

// Colors are written as #RRGGBB, like the apps' Theme.swift.
#![allow(clippy::unreadable_literal)]
// Drawing code reads best with x / y / w / t (theme) / r (rect).
#![allow(clippy::many_single_char_names, clippy::similar_names)]

mod bot_chat;
mod buffer;
mod chat;
mod computer_access;
mod consent;
mod forms;
mod goto;
mod help;
mod overlay;
mod pair;
mod permission;
mod roster;
mod style;
mod time;
mod trace;
mod transcript;
mod usage;

pub(super) use buffer::{centered, frame_box, hline, put, restyle, rput, u};
pub(super) use chat::composer_lines;
pub(super) use overlay::{field_line, input_line};
pub(super) use style::theme;
pub(super) use usage::when_future;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};

use super::app::{App, Click, Focus, TraceMode, Width};
use super::md::{truncate, width as w};

use buffer::{fill, vline};
use chat::chat;
use overlay::overlays;
use roster::{compact_roster, roster};
use style::rgb;
use trace::trace;

pub fn draw(f: &mut Frame<'_>, app: &mut App) {
    let area = f.area();
    app.hits = super::app::Hits::default();
    app.width = match area.width {
        0..80 => Width::Narrow,
        80..100 => Width::Mid,
        100..150 => Width::Wide,
        _ => Width::Huge,
    };
    let buf = f.buffer_mut();
    if area.width < 40 || area.height < 12 {
        let c = app.counts();
        put(buf, 0, 0, area.width, "Make the window bigger", theme().bold);
        put(buf, 0, 1, area.width, &format!("◆{} ⠹{} ●{} ×{}", c[0], c[1], c[2], c[3]), theme().secondary);
        return;
    }
    let body = Rect::new(area.x, area.y, area.width, area.height - 1);
    status_line(buf, Rect::new(area.x, area.bottom() - 1, area.width, 1), app);

    if app.width == Width::Narrow {
        if app.chat_page && app.selected.is_some() {
            if app.trace == TraceMode::Full {
                trace(buf, body, app);
            } else {
                chat(buf, body, app, true);
            }
        } else {
            app.chat_page = false;
            if app.focus != Focus::Roster {
                app.focus = Focus::Roster;
            }
            roster(buf, body, app, true);
        }
    } else {
        let compact =
            app.compact || app.width == Width::Mid || (app.width == Width::Wide && app.trace == TraceMode::Pane);
        let rw: u16 = if compact { 5 } else { 32 };
        let r = Rect::new(body.x, body.y, rw, body.height);
        if compact {
            compact_roster(buf, r, app);
        } else {
            roster(buf, r, app, false);
        }
        vline(buf, body.x + rw, body.y, body.height, theme().line);
        let main = Rect::new(body.x + rw + 1, body.y, body.width - rw - 1, body.height);
        match app.trace {
            TraceMode::Full => trace(buf, main, app),
            TraceMode::Pane => {
                let cw = main.width * 55 / 100;
                chat(buf, Rect::new(main.x, main.y, cw, main.height), app, false);
                vline(buf, main.x + cw, main.y, main.height, theme().line);
                trace(buf, Rect::new(main.x + cw + 1, main.y, main.width - cw - 1, main.height), app);
            }
            TraceMode::Off => chat(buf, main, app, false),
        }
    }
    overlays(f, app);
    let buf = f.buffer_mut();
    toasts(buf, area, app);
    if let Some(pos) = CURSOR.with(std::cell::Cell::take) {
        f.set_cursor_position(pos);
    }
}

thread_local! {
    static CURSOR: std::cell::Cell<Option<Position>> = const { std::cell::Cell::new(None) };
}

pub(super) fn set_cursor(p: Position) {
    CURSOR.with(|c| c.set(Some(p)));
}

fn status_line(buf: &mut Buffer, r: Rect, app: &mut App) {
    let t = theme();
    fill(buf, r, t.panel);
    let insert = app.typing && app.overlays.is_empty();
    let pill = if insert { " INSERT " } else { " NAV " };
    let pill_style = if insert {
        Style::default().fg(t.on_color).bg(rgb(0xF0A030)).add_modifier(Modifier::BOLD)
    } else {
        t.btn_primary
    };
    let pill_style = if t.color { pill_style } else { t.sel };
    let mut x = put(buf, r.x, r.y, r.width, pill, pill_style) + 1;
    let host = if app.host.is_empty() { "codync" } else { &app.host };
    x = put(buf, x, r.y, 24, host, t.bold.patch(t.panel)) + 2;
    let c = app.counts();
    let marks = [("◆", t.amber, 1), ("⠹", t.secondary, 2), ("●", t.bold, 3), ("×", t.red, 4)];
    for (i, (g, s, filter)) in marks.into_iter().enumerate() {
        if c[i] == 0 {
            continue;
        }
        let start = x;
        x = put(buf, x, r.y, 2, g, s.patch(t.panel)) + 1;
        x = put(buf, x, r.y, 4, &c[i].to_string(), t.secondary.patch(t.panel)) + 2;
        app.hits.clicks.push((Rect::new(start, r.y, x - start, 1), Click::Filter(filter)));
    }
    if !app.online {
        x = put(buf, x, r.y, 20, "offline", t.amber.patch(t.panel)) + 2;
    } else if app.mismatch.is_some() {
        x = put(buf, x, r.y, 20, "needs update", t.red.patch(t.panel)) + 2;
    }
    let hint = if insert { "↵ send · esc nav" } else { "? keys" };
    let mut right = rput(buf, r.right() - 1, r.y, hint, t.dim.patch(t.panel)).saturating_sub(3);
    let mut edge = right;
    // Usage: the first provider that has numbers, narrowest windows first.
    if r.width >= 100
        && let Some(p) = app.usage["providers"]
            .as_array()
            .and_then(|ps| ps.iter().find(|p| p["windows"].as_array().is_some_and(|w| !w.is_empty())))
    {
        let wins = p["windows"].as_array().cloned().unwrap_or_default();
        for win in wins.iter().take(2).rev() {
            let pct = win["percent"].as_f64().unwrap_or(0.0).clamp(0.0, 100.0);
            let label = short_window(win["label"].as_str().unwrap_or_default());
            let seg = format!("{label} ▰▰▰▰▰▰▰▰ {pct:.0}%");
            let sx = right.saturating_sub(u(w(&seg)));
            let on = filled(pct, 8);
            let mut px = put(buf, sx, r.y, 8, &format!("{label} "), t.dim.patch(t.panel));
            let bar_style = if pct >= 80.0 { t.amber } else { t.text };
            px = put(buf, px, r.y, 8, &"▰".repeat(on), bar_style.patch(t.panel));
            px = put(buf, px, r.y, 8, &"▱".repeat(8 - on.min(8)), t.line.patch(t.panel));
            put(buf, px, r.y, 6, &format!(" {pct:.0}%"), t.secondary.patch(t.panel));
            right = sx.saturating_sub(3);
        }
        let name = p["name"].as_str().unwrap_or_default().to_lowercase();
        edge = rput(buf, right + 1, r.y, &name, t.dim.patch(t.panel)).saturating_sub(2);
    }
    // A flash gets the room between the host and the usage, cut short to fit.
    // Otherwise what to update stays in view until it's done.
    let standing = app.mismatch.as_ref().filter(|_| app.online).map(|m| super::app::mismatch_text(m, &app.host));
    if let Some(h) = app.hint.as_ref().map(|(h, _)| h.clone()).or(standing) {
        let room = edge.saturating_sub(x);
        put(buf, x, r.y, room, &truncate(&h, usize::from(room)), t.amber.patch(t.panel));
    }
}

/// How many of `n` bar cells a percentage fills (rounded).
fn filled(pct: f64, n: u32) -> usize {
    (0..n).filter(|&i| (f64::from(i) + 0.5) * 100.0 / f64::from(n) < pct).count()
}

fn short_window(label: &str) -> String {
    let l = label.to_lowercase();
    if l.contains('5') && l.contains('h') {
        "5h".into()
    } else if l.contains("week") || l.contains('7') {
        "wk".into()
    } else {
        truncate(label, 6)
    }
}

fn toasts(buf: &mut Buffer, area: Rect, app: &App) {
    let t = theme();
    let mut y = area.y + 1;
    for toast in &app.toasts {
        let text = format!("{}  {}", if toast.need { "◆" } else { "●" }, toast.text);
        let hint = if toast.need { "! jump" } else { "u jump" };
        let bw = u(w(&text) + w(hint) + 7).min(area.width.saturating_sub(2));
        let r = Rect::new(area.right().saturating_sub(bw + 1), y, bw, 3);
        let inner = frame_box(buf, r, if toast.need { t.amber } else { t.line }, t.panel, None);
        put(buf, inner.x, inner.y, inner.width, &text, t.bold.patch(t.panel));
        if toast.need {
            put(buf, inner.x, inner.y, 1, "◆", t.amber.patch(t.panel));
        }
        rput(buf, inner.right(), inner.y, hint, t.dim.patch(t.panel));
        y += 3;
    }
}
