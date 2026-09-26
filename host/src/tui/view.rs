//! Drawing. Black and white: color comes only from the bots; amber means
//! "needs you" and red means an error. Every state also has its own glyph.

// Colors are written as #RRGGBB, like the apps' Theme.swift.
#![allow(clippy::unreadable_literal)]
// Drawing code reads best with x / y / w / t (theme) / r (rect).
#![allow(clippy::many_single_char_names, clippy::similar_names)]

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::collections::{BTreeSet, HashMap};
use std::sync::LazyLock;

use super::app::{
    ACTIONS, App, Bot, COLORS, Click, Editor, Entry, FIELDS, FILTERS, Field, Focus, Form, GotoItem, GroupForm, Kind,
    MAX_MEMBERS, Mark, Overlay, SHAPES, Status, TraceMode, Width, tilde,
};
use super::md::{self, truncate, width as w};

pub struct Theme {
    pub color: bool,
    pub text: Style,
    pub bold: Style,
    pub secondary: Style,
    pub dim: Style,
    pub line: Style,
    pub amber: Style,
    pub red: Style,
    pub green: Style,
    pub code: Style,
    pub code_block: Style,
    pub band: Style,
    pub sel: Style,
    pub panel: Style,
    pub btn: Style,
    pub btn_primary: Style,
    pub added: Style,
    pub removed: Style,
    pub on_color: Color,
    pub shade: Color,
}

fn rgb(hex: u32) -> Color {
    Color::Rgb(((hex >> 16) & 0xFF) as u8, ((hex >> 8) & 0xFF) as u8, (hex & 0xFF) as u8)
}

pub fn theme() -> &'static Theme {
    static T: LazyLock<Theme> = LazyLock::new(|| {
        let color = std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty());
        // COLORFGBG="15;0" is a dark background; "0;15" a light one. CODYNC_THEME wins.
        let light = match std::env::var("CODYNC_THEME").as_deref() {
            Ok("light") => true,
            Ok("dark") => false,
            _ => std::env::var("COLORFGBG")
                .ok()
                .and_then(|v| v.rsplit(';').next().and_then(|b| b.parse::<u8>().ok()))
                .is_some_and(|bg| bg == 7 || bg == 15),
        };
        let s = Style::default();
        if !color {
            let rev = s.add_modifier(Modifier::REVERSED);
            return Theme {
                color,
                text: s,
                bold: s.add_modifier(Modifier::BOLD),
                secondary: s,
                dim: s.add_modifier(Modifier::DIM),
                line: s.add_modifier(Modifier::DIM),
                amber: s.add_modifier(Modifier::BOLD),
                red: s.add_modifier(Modifier::BOLD),
                green: s,
                code: s.add_modifier(Modifier::BOLD),
                code_block: s,
                band: s,
                sel: rev,
                panel: s,
                btn: s,
                btn_primary: rev.add_modifier(Modifier::BOLD),
                added: s,
                removed: s.add_modifier(Modifier::DIM),
                on_color: Color::Reset,
                shade: Color::Reset,
            };
        }
        let (text, second, dim, line, band, sel, panel, btn, green, red, add_bg, rm_bg) = if light {
            (
                0x141414, 0x5F5F5F, 0x8E8E8E, 0xD6D6D6, 0xEFEFEF, 0xE2E2E2, 0xF7F7F7, 0xE4E4E4, 0x2E7D32, 0xC23A2B,
                0xE3F3E4, 0xF9E3E0,
            )
        } else {
            (
                0xF2F2F2, 0x9A9A9A, 0x6E6E6E, 0x333333, 0x1C1C1C, 0x262626, 0x141414, 0x2A2A2A, 0x8FD18B, 0xF0A7A7,
                0x14261A, 0x2A1616,
            )
        };
        let fg = |h| s.fg(rgb(h));
        Theme {
            color,
            text: s,
            bold: s.add_modifier(Modifier::BOLD),
            secondary: fg(second),
            dim: fg(dim),
            line: fg(line),
            amber: fg(if light { 0xB8700A } else { 0xF0A030 }),
            red: fg(red),
            green: fg(green),
            code: s.bg(rgb(band)),
            code_block: s.bg(rgb(band)),
            band: s.bg(rgb(band)),
            sel: s.bg(rgb(sel)),
            panel: s.bg(rgb(panel)),
            btn: s.bg(rgb(btn)),
            btn_primary: s.fg(rgb(if light { 0xFFFFFF } else { 0x0A0A0A })).bg(rgb(text)).add_modifier(Modifier::BOLD),
            added: s.fg(rgb(green)).bg(rgb(add_bg)),
            removed: s.fg(rgb(red)).bg(rgb(rm_bg)),
            on_color: rgb(0x0A0A0A),
            shade: rgb(if light { 0xB0B0B0 } else { 0x3A3A3A }),
        }
    });
    &T
}

pub fn bot_color(name: &str) -> Color {
    let hex = match name {
        "black" => 0x8A8A8A,
        "brown" => 0x936439,
        "red" => 0xFF263C,
        "orange" => 0xFF6700,
        "yellow" => 0xFF9800,
        "green" => 0x00C972,
        "cyan" => 0x00BCA6,
        "violet" => 0x9159FE,
        "magenta" => 0xFF309B,
        "gray" => 0x777777,
        _ => 0x1084FE,
    };
    rgb(hex)
}

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn spin(app: &App) -> &'static str {
    SPINNER[usize::try_from(app.frame % 10).unwrap_or(0)]
}

pub fn glyph(app: &App, m: Mark) -> (&'static str, Style) {
    let t = theme();
    match m {
        Mark::Need => ("◆", t.amber),
        Mark::Work => (spin(app), t.secondary),
        Mark::Unread => ("●", t.bold),
        Mark::Idle => ("○", t.dim),
        Mark::Error => ("×", t.red),
    }
}

fn avatar(b: &Bot) -> Span<'static> {
    let t = theme();
    if b.group {
        // A group: # and how many bots are in it.
        return Span::styled(format!("#{}", b.members.len().min(9)), t.bold.patch(t.btn));
    }
    if t.color {
        Span::styled("••", Style::default().fg(t.on_color).bg(bot_color(&b.color)).add_modifier(Modifier::BOLD))
    } else {
        Span::styled("[]", t.bold)
    }
}

pub const FACE: [&str; 7] = [
    "⠀⣤⣿⣿⣿⣿⣿⣿⣿⣿⣿⣤⠀",
    "⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿",
    "⣿⣿⣿⣿⠀⣿⣿⣿⠀⣿⣿⣿⣿",
    "⣿⣿⣿⣿⣤⣿⣿⣿⣤⣿⣿⣿⣿",
    "⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿",
    "⠛⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⠛",
    "⠀⠀⠛⠛⠛⠛⠛⠛⠛⠛⠛⠀⠀",
];

fn face_style(color: &str) -> Style {
    let t = theme();
    if t.color { Style::default().fg(bot_color(color)) } else { t.text }
}

// ---------- time ----------

fn now_ms() -> i64 {
    crate::store::now_ms()
}

/// Local UTC offset in seconds, read once from `date +%z` (no tz database needed).
fn utc_offset() -> i64 {
    static OFF: LazyLock<i64> = LazyLock::new(|| {
        let out = std::process::Command::new("date").arg("+%z").output().ok();
        let s = out.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).unwrap_or_default();
        if s.len() != 5 {
            return 0;
        }
        let sign = if s.starts_with('-') { -1 } else { 1 };
        let h: i64 = s[1..3].parse().unwrap_or(0);
        let m: i64 = s[3..5].parse().unwrap_or(0);
        sign * (h * 3600 + m * 60)
    });
    *OFF
}

/// (days since epoch, seconds into the local day)
fn local(ms: i64) -> (i64, i64) {
    let s = ms.div_euclid(1000) + utc_offset();
    (s.div_euclid(86_400), s.rem_euclid(86_400))
}

fn month_day(days: i64) -> (usize, i64) {
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (usize::try_from(m - 1).unwrap_or(0), d)
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];

pub fn clock(ms: i64) -> String {
    let (_, secs) = local(ms);
    format!("{:02}:{:02}", secs / 3600, secs % 3600 / 60)
}

/// Roster time: 12:31 today, Tue this week, Sep 19 before.
pub fn when(ms: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    let (day, _) = local(ms);
    let (today, _) = local(now_ms());
    if now_ms() - ms < 60_000 {
        "now".into()
    } else if day == today {
        clock(ms)
    } else if today - day < 7 {
        DAYS[usize::try_from(day.rem_euclid(7)).unwrap_or(0)].into()
    } else {
        let (m, d) = month_day(day);
        format!("{} {d}", MONTHS[m])
    }
}

pub fn elapsed(start: Option<i64>) -> String {
    let Some(start) = start else { return String::new() };
    let s = ((now_ms() - start) / 1000).max(0);
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m{:02}s", s / 60, s % 60)
    } else {
        format!("{}h{:02}m", s / 3600, s % 3600 / 60)
    }
}

// ---------- buffer helpers ----------

fn put(buf: &mut Buffer, x: u16, y: u16, max: u16, s: &str, style: Style) -> u16 {
    if max == 0 || y >= buf.area.bottom() {
        return x;
    }
    let (end, _) = buf.set_stringn(x, y, s, usize::from(max), style);
    end
}

fn put_line(buf: &mut Buffer, x: u16, y: u16, max: u16, line: &Line<'_>) -> u16 {
    if y >= buf.area.bottom() {
        return x;
    }
    let (end, _) = buf.set_line(x, y, line, max);
    end
}

fn rput(buf: &mut Buffer, right: u16, y: u16, s: &str, style: Style) -> u16 {
    let width = u16::try_from(w(s)).unwrap_or(0);
    let x = right.saturating_sub(width);
    put(buf, x, y, width, s, style);
    x
}

fn fill(buf: &mut Buffer, r: Rect, style: Style) {
    for y in r.top()..r.bottom() {
        for x in r.left()..r.right() {
            if let Some(c) = buf.cell_mut(Position::new(x, y)) {
                c.set_symbol(" ");
                c.set_style(style);
            }
        }
    }
}

fn restyle(buf: &mut Buffer, r: Rect, style: Style) {
    buf.set_style(r, style);
}

fn hline(buf: &mut Buffer, x: u16, y: u16, width: u16, style: Style) {
    put(buf, x, y, width, &"─".repeat(usize::from(width)), style);
}

fn vline(buf: &mut Buffer, x: u16, y: u16, h: u16, style: Style) {
    for yy in y..y + h {
        put(buf, x, yy, 1, "│", style);
    }
}

fn frame_box(buf: &mut Buffer, r: Rect, border: Style, bg: Style, title: Option<(&str, Style)>) -> Rect {
    fill(buf, r, bg);
    if r.width < 2 || r.height < 2 {
        return r;
    }
    let inner = usize::from(r.width - 2);
    let b = border.patch(bg);
    put(buf, r.x, r.y, r.width, &format!("╭{}╮", "─".repeat(inner)), b);
    for y in r.y + 1..r.bottom() - 1 {
        put(buf, r.x, y, 1, "│", b);
        put(buf, r.right() - 1, y, 1, "│", b);
    }
    put(buf, r.x, r.bottom() - 1, r.width, &format!("╰{}╯", "─".repeat(inner)), b);
    if let Some((t, s)) = title {
        put(buf, r.x + 2, r.y, r.width.saturating_sub(4), &format!(" {t} "), s.patch(bg).add_modifier(Modifier::BOLD));
    }
    Rect::new(r.x + 2, r.y + 1, r.width.saturating_sub(4), r.height.saturating_sub(2))
}

fn dim_all(buf: &mut Buffer) {
    let shade = theme().shade;
    for c in &mut buf.content {
        c.set_style(Style::default().fg(shade).bg(Color::Reset).remove_modifier(Modifier::all()));
    }
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let wd = width.min(area.width.saturating_sub(2));
    let h = height.min(area.height.saturating_sub(2));
    Rect::new(area.x + (area.width - wd) / 2, area.y + (area.height - h) / 3, wd, h)
}

fn u(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

// ---------- layout ----------

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

fn set_cursor(p: Position) {
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
    }
    if let Some((h, _)) = &app.hint {
        put(buf, x, r.y, r.right().saturating_sub(x), h, t.amber.patch(t.panel));
    }
    let hint = if insert { "↵ send · esc nav" } else { "? keys" };
    let mut right = rput(buf, r.right() - 1, r.y, hint, t.dim.patch(t.panel)).saturating_sub(3);
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
        rput(buf, right + 1, r.y, &name, t.dim.patch(t.panel));
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

// ---------- roster ----------

fn roster(buf: &mut Buffer, r: Rect, app: &mut App, narrow: bool) {
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
    rput(buf, r.right() - 1, r.y, &format!("{} bots", bots.len()), t.dim);
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
    // Rows: an optional section label, then 2 lines per bot and a gap.
    let any_pinned = bots.iter().any(|b| b.pinned);
    let mut rows: Vec<(u16, Option<&'static str>, Option<usize>)> = vec![];
    let mut y = 0u16;
    for (i, b) in bots.iter().enumerate() {
        if any_pinned && (i == 0 || (bots[i - 1].pinned && !b.pinned)) {
            rows.push((y, Some(if b.pinned { "PINNED" } else { "BOTS" }), None));
            y += 1;
        }
        rows.push((y, None, Some(i)));
        y += 3;
    }
    let sel_idx = app.selected.as_ref().and_then(|s| bots.iter().position(|b| &b.id == s));
    let view_h = bottom.saturating_sub(top);
    let sel_y = sel_idx.and_then(|i| rows.iter().find(|r| r.2 == Some(i)).map(|r| r.0)).unwrap_or(0);
    let offset = (sel_y + 3).saturating_sub(view_h);
    for (ry, label, idx) in rows {
        let Some(yy) = (top + ry).checked_sub(offset) else { continue };
        if ry < offset || yy + 1 >= bottom {
            continue;
        }
        if let Some(l) = label {
            put(buf, x0 + 1, yy, wd, l, t.dim.add_modifier(Modifier::BOLD));
            continue;
        }
        let Some(i) = idx else { continue };
        let b = &bots[i];
        let selected = Some(i) == sel_idx;
        let row = Rect::new(x0, yy, wd, 2);
        app.hits.clicks.push((row, Click::Bot(b.id.clone())));
        if selected {
            restyle(buf, row, t.sel);
            let bar = if focused { t.bold } else { t.secondary };
            put(buf, x0, yy, 1, "▌", bar);
            put(buf, x0, yy + 1, 1, "▌", bar);
        }
        put_line(buf, x0 + 2, yy, 2, &Line::from(avatar(b)));
        let (g, gs) = glyph(app, b.mark());
        let time = if b.status == Status::Working { elapsed(b.started_at) } else { when(b.last_at) };
        let tx = rput(buf, r.right() - 1, yy, &time, if b.unread > 0 { t.bold } else { t.dim });
        let gx = tx.saturating_sub(2);
        put(buf, gx, yy, 1, g, gs);
        put(buf, x0 + 5, yy, gx.saturating_sub(x0 + 6), &b.name, t.bold);
        let (pv, ps) = preview(b);
        let badge = if b.unread > 0 { format!(" {} ", b.unread) } else { String::new() };
        let room = usize::from(wd.saturating_sub(7)).saturating_sub(w(&badge) + usize::from(!badge.is_empty()));
        put(buf, x0 + 5, yy + 1, u(room), &truncate(&pv, room), ps);
        if !badge.is_empty() {
            rput(buf, r.right() - 1, yy + 1, &badge, t.btn_primary);
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

/// A one-line preview without Markdown punctuation.
fn plain(s: &str) -> String {
    let line = s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
    line.trim_start_matches(['#', '>', '-', '*', ' ']).replace("**", "").replace('`', "")
}

fn preview(b: &Bot) -> (String, Style) {
    let (text, style) = preview_raw(b);
    (plain(&text), style)
}

fn preview_raw(b: &Bot) -> (String, Style) {
    let t = theme();
    match b.mark() {
        Mark::Need => (if b.activity.is_empty() { "Needs your approval".into() } else { b.activity.clone() }, t.amber),
        Mark::Work => (if b.activity.is_empty() { "Working…".into() } else { b.activity.clone() }, t.secondary),
        Mark::Error => {
            (if b.last_message.is_empty() { "Something went wrong".into() } else { b.last_message.clone() }, t.red)
        }
        Mark::Unread => (b.last_message.clone(), t.text),
        Mark::Idle if b.last_message.is_empty() => ("No messages yet".into(), t.dim),
        Mark::Idle => (b.last_message.clone(), t.secondary),
    }
}

fn compact_roster(buf: &mut Buffer, r: Rect, app: &mut App) {
    let t = theme();
    app.hits.roster = r;
    let bots: Vec<Bot> = app.roster().into_iter().cloned().collect();
    let sel = app.selected.clone();
    let sel_idx = sel.as_ref().and_then(|s| bots.iter().position(|b| &b.id == s)).unwrap_or(0);
    let per = 2u16;
    let view = r.height.saturating_sub(1) / per;
    let offset = (u(sel_idx) + 1).saturating_sub(view);
    for (i, b) in bots.iter().enumerate().skip(usize::from(offset)) {
        let y = r.y + 1 + (u(i) - offset) * per;
        if y >= r.bottom() {
            break;
        }
        let row = Rect::new(r.x, y, r.width, 1);
        app.hits.clicks.push((row, Click::Bot(b.id.clone())));
        if sel.as_deref() == Some(b.id.as_str()) {
            restyle(buf, row, t.sel);
            put(buf, r.x, y, 1, "▌", if app.focus == Focus::Roster { t.bold } else { t.secondary });
        }
        put_line(buf, r.x + 1, y, 2, &Line::from(avatar(b)));
        let (g, gs) = glyph(app, b.mark());
        put(buf, r.x + 3, y, 1, g, gs);
    }
}

// ---------- chat ----------

struct Built {
    lines: Vec<Line<'static>>,
    /// (line index, x offset, width, click)
    buttons: Vec<(usize, u16, u16, Click)>,
    /// Each message's (entry id, first line, end line), for picking one to reply to.
    messages: Vec<(String, usize, usize)>,
}

fn chat(buf: &mut Buffer, r: Rect, app: &mut App, narrow: bool) {
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
    put_line(buf, x, r.y, 2, &Line::from(avatar(&b)));
    let title = if app.thread.is_some() { format!("Thread · {}", b.name) } else { b.name.clone() };
    x = put(buf, x + 3, r.y, r.width / 3, &title, t.bold) + 2;
    let (status, ss) = match b.mark() {
        Mark::Need => ("◆ needs you".to_owned(), t.amber),
        Mark::Work => (format!("{} working {}", spin(app), elapsed(b.started_at)), t.secondary),
        Mark::Error => ("× error".to_owned(), t.red),
        _ => (String::new(), t.dim),
    };
    let sx = rput(buf, r.right() - 1, r.y, &status, ss.add_modifier(Modifier::BOLD));
    if app.thread.is_some() {
        put(buf, x, r.y, sx.saturating_sub(x + 2), "esc back to the chat", t.dim);
    } else if b.group {
        let names: Vec<String> = b.members.iter().map(|m| app.author_name(Some(m))).collect();
        put(buf, x, r.y, sx.saturating_sub(x + 2), &format!("group · {}", names.join(", ")), t.secondary);
    } else if !narrow {
        let meta = format!("{} · {} · {}", b.backend, tilde(&b.cwd, &app.home), if b.auto { "auto" } else { "ask" });
        put(buf, x, r.y, sx.saturating_sub(x + 2), &meta, t.secondary);
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
    let comp_top = r.bottom().saturating_sub(ch + 2);
    let typing = app.typing && app.overlays.is_empty();
    let rule = if typing { t.text } else { t.line };
    hline(buf, r.x, comp_top, r.width, rule);
    hline(buf, r.x, comp_top + ch + 1, r.width, rule);
    put(buf, r.x + 1, comp_top + 1, 1, "›", if typing { t.bold } else { t.dim });
    let comp_rect = Rect::new(r.x, comp_top, r.width, ch + 2);
    app.hits.clicks.push((comp_rect, Click::Composer));
    let first = clines.len().saturating_sub(usize::from(ch)).min(cursor.1.saturating_sub(usize::from(ch) - 1));
    if draft.text.is_empty() {
        let placeholder =
            if app.thread.is_some() { "Reply in thread…".to_owned() } else { format!("Message {}…", b.name) };
        put(buf, r.x + 3, comp_top + 1, r.width.saturating_sub(4), &placeholder, t.dim);
    } else {
        for (i, l) in clines.iter().skip(first).take(usize::from(ch)).enumerate() {
            put(buf, r.x + 3, comp_top + 1 + u(i), r.width.saturating_sub(4), l, t.text);
        }
    }
    let chip = if typing {
        match b.status {
            Status::Working => "sends after this turn",
            Status::NeedsInput => "sends after the approval",
            _ => "",
        }
    } else if app.pick.is_some() {
        "↵ opens its thread · esc cancels"
    } else if matches!(b.status, Status::Working) {
        "s stops"
    } else {
        "i to type"
    };
    if draft.text.is_empty() || !typing {
        rput(buf, r.right() - 1, comp_top + 1, chip, t.dim);
    }
    if typing {
        let cy = comp_top + 1 + u(cursor.1.saturating_sub(first));
        set_cursor(Position::new(r.x + 3 + u(cursor.0), cy.min(comp_top + ch)));
    }

    // Transcript.
    let body = Rect::new(r.x, top + 1, r.width, comp_top.saturating_sub(top + 1));
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
        rput(buf, r.right() - 1, comp_top.saturating_sub(1), &s, t.btn);
    }
}

/// Wraps the draft; returns display lines and the cursor's (column, line).
fn composer_lines(e: &Editor, width: usize) -> (Vec<String>, (usize, usize)) {
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

fn with_bg(lines: Vec<Line<'static>>, width: usize, bg: Style) -> Vec<Line<'static>> {
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

struct TurnInfo {
    steps: usize,
    files: BTreeSet<String>,
    has_final: bool,
}

fn turn_info(entries: &[&Entry]) -> HashMap<i64, TurnInfo> {
    let mut m: HashMap<i64, TurnInfo> = HashMap::new();
    for e in entries {
        let ti = m.entry(e.turn).or_insert_with(|| TurnInfo { steps: 0, files: BTreeSet::new(), has_final: false });
        match e.kind {
            Kind::Tool => {
                ti.steps += 1;
                for d in e.data["diffs"].as_array().into_iter().flatten() {
                    if let Some(p) = d["path"].as_str() {
                        ti.files.insert(p.to_owned());
                    }
                }
            }
            Kind::Agent if e.is_final() => ti.has_final = true,
            _ => {}
        }
    }
    m
}

fn gap(out: &mut Built) {
    if !out.lines.is_empty() {
        out.lines.push(Line::default());
    }
}

fn name_style(color: &str) -> Style {
    let t = theme();
    if t.color { Style::default().fg(bot_color(color)).add_modifier(Modifier::BOLD) } else { t.bold }
}

/// "3 replies", "1 reply".
fn replies(n: i64) -> String {
    if n == 1 { "1 reply".into() } else { format!("{n} replies") }
}

fn build_chat(app: &App, b: &Bot, width: usize) -> Built {
    let t = theme();
    let mut out = Built { lines: vec![], buttons: vec![], messages: vec![] };
    let thread = app.thread.as_deref();
    let entries = app.lane(&b.id);
    let info = turn_info(&entries);
    if entries.is_empty() && thread.is_none() {
        intro(&mut out, app, b, width);
        return out;
    }
    if thread.is_none() && !app.history_complete(&b.id) {
        out.lines.push(Line::from(Span::styled(" ↑ older messages load when you scroll up", t.dim)));
    }
    if let Some(root) = thread {
        // The message the thread is on, then its replies.
        if let Some(e) = app.entries.get(&b.id).and_then(|m| m.values().find(|e| e.id == root)) {
            entry_lines(&mut out, app, b, e, &HashMap::new(), false, width);
        }
        let n = entries.iter().filter(|e| e.is_message()).count();
        let label = if n == 0 {
            " No replies yet ".to_owned()
        } else {
            format!(" {} ", replies(i64::try_from(n).unwrap_or(0)))
        };
        gap(&mut out);
        let side = width.saturating_sub(w(&label) + 4);
        out.lines.push(Line::from(Span::styled(format!(" ───{label}{}", "─".repeat(side)), t.dim)));
    }
    for e in &entries {
        entry_lines(&mut out, app, b, e, &info, thread.is_none(), width);
    }
    // A turn in flight: who's on it and what they're doing.
    let last_turn = entries.iter().map(|e| e.turn).max().unwrap_or(0);
    // A group's members each reply in the room turn, so a reply doesn't end it.
    let in_flight = b.group || info.get(&last_turn).is_none_or(|ti| !ti.has_final);
    if b.status == Status::Working && b.works_in(thread) && in_flight {
        gap(&mut out);
        out.lines.push(Line::from(Span::styled(format!(" {}", b.name), name_style(&b.color))));
        let act = if b.activity.is_empty() { "Working…".to_owned() } else { b.activity.clone() };
        out.lines.push(Line::from(vec![
            Span::styled(format!(" {} ", spin(app)), t.secondary),
            Span::styled(truncate(&act, width.saturating_sub(4)), t.secondary),
        ]));
        let steps = info.get(&last_turn).map_or(0, |ti| ti.steps);
        if steps > 0 {
            out.lines.push(Line::from(Span::styled(
                format!("   {steps} step{} so far · t shows them live", if steps == 1 { "" } else { "s" }),
                t.dim,
            )));
        }
    } else if thread.is_none()
        && b.working_thread.is_some()
        && b.working_chat.as_deref().unwrap_or(&b.id) == b.id
        && matches!(b.status, Status::Working | Status::NeedsInput)
    {
        gap(&mut out);
        let line = if b.status == Status::NeedsInput {
            Span::styled(" ◆ Needs you in a thread · ! opens it", t.amber)
        } else {
            Span::styled(format!(" {} Working in a thread", spin(app)), t.secondary)
        };
        out.lines.push(Line::from(line));
    }
    out
}

/// An empty chat: who this is and how it works.
fn intro(out: &mut Built, app: &App, b: &Bot, width: usize) {
    let t = theme();
    out.lines.push(Line::default());
    if !b.group {
        for row in FACE {
            out.lines.push(Line::from(vec![Span::raw("  "), Span::styled(row, face_style(&b.color))]));
        }
        out.lines.push(Line::default());
    }
    out.lines.push(Line::from(vec![Span::raw("  "), Span::styled(b.name.clone(), t.bold)]));
    let (meta, hello) = if b.group {
        let names: Vec<String> = b.members.iter().map(|m| app.author_name(Some(m))).collect();
        (
            format!("group · {}", names.join(", ")),
            "Everyone answers in turn unless you @mention someone. Each bot works in its own folder with its own tools."
                .to_owned(),
        )
    } else {
        (
            format!("{} · {}", b.backend, tilde(&b.cwd, &app.home)),
            format!("Say what you need. {} works in the background and pings you when it's done or needs you.", b.name),
        )
    };
    out.lines.push(Line::from(vec![Span::raw("  "), Span::styled(meta, t.secondary)]));
    out.lines.push(Line::default());
    out.lines.extend(md::wrap(&[Span::styled(hello, t.dim)], width, &[Span::raw("  ")], &[Span::raw("  ")]));
}

/// One chat entry. `main`: the main chat, where a message shows its thread's replies line.
fn entry_lines(
    out: &mut Built,
    app: &App,
    b: &Bot,
    e: &Entry,
    info: &HashMap<i64, TurnInfo>,
    main: bool,
    width: usize,
) {
    let t = theme();
    // In a group each reply is its author's; elsewhere the bot's own.
    let author = if b.group { app.bots.get(e.data["author"].as_str().unwrap_or_default()) } else { Some(b) };
    match e.kind {
        Kind::User => {
            gap(out);
            let from = out.lines.len();
            let mut head = vec![
                Span::styled(" you", t.secondary.add_modifier(Modifier::BOLD)),
                Span::styled(format!(" {}", clock(e.created_at)), t.dim),
            ];
            match e.data["status"].as_str() {
                Some("queued") => head.push(Span::styled(" · queued", t.dim)),
                Some("cancelled") => head.push(Span::styled(" · not sent", t.red)),
                _ => {}
            }
            out.lines.push(Line::from(head));
            let body = md::wrap(
                &[Span::styled(e.text().to_owned(), t.text)],
                width.saturating_sub(1),
                &[Span::raw(" › ")],
                &[Span::raw("   ")],
            );
            out.lines.extend(with_bg(body, width, t.band));
            out.messages.push((e.id.clone(), from, out.lines.len()));
            if main {
                thread_line(out, e);
            }
        }
        Kind::Agent if e.is_final() => {
            gap(out);
            let from = out.lines.len();
            let (name, color) = match author {
                Some(a) => (a.name.clone(), a.color.as_str()),
                None => ("A deleted bot".to_owned(), "gray"),
            };
            out.lines.push(Line::from(vec![
                Span::styled(format!(" {name}"), name_style(color)),
                Span::styled(format!(" {}", clock(e.created_at)), t.dim),
            ]));
            out.lines.extend(md::render(e.text(), width, 1));
            out.messages.push((e.id.clone(), from, out.lines.len()));
            if let Some(ti) = info.get(&e.turn).filter(|ti| ti.steps > 0) {
                let files = if ti.files.is_empty() {
                    String::new()
                } else {
                    format!(" · {} file{} changed", ti.files.len(), if ti.files.len() == 1 { "" } else { "s" })
                };
                out.lines.push(Line::from(Span::styled(
                    format!("   ↳ {} step{}{files} · o opens them", ti.steps, if ti.steps == 1 { "" } else { "s" }),
                    t.dim,
                )));
            }
            if main {
                thread_line(out, e);
            }
        }
        Kind::Permission => {
            gap(out);
            if e.pending() {
                // A card in a group is its asking bot's.
                permission_card(out, e, author.unwrap_or(b), width, &app.home);
            } else {
                decided_line(out, e);
            }
        }
        Kind::Notice => {
            let text = e.text().to_owned();
            gap(out);
            match e.data["style"].as_str() {
                Some("divider") => {
                    let label = format!(" {text} · {} ", clock(e.created_at));
                    let side = width.saturating_sub(w(&label) + 4);
                    out.lines.push(Line::from(Span::styled(format!(" ───{label}{}", "─".repeat(side)), t.dim)));
                }
                Some("error") => out.lines.extend(md::wrap(
                    &[Span::styled(text, t.red)],
                    width,
                    &[Span::styled(" × ", t.red)],
                    &[Span::raw("   ")],
                )),
                _ => out.lines.extend(md::wrap(
                    &[Span::styled(text, t.dim)],
                    width,
                    &[Span::styled(" · ", t.dim)],
                    &[Span::raw("   ")],
                )),
            }
        }
        _ => {}
    }
}

/// Under a message with a thread: how many replies, how recently (click opens it).
fn thread_line(out: &mut Built, e: &Entry) {
    let Some(text) = thread_summary(&e.data["thread"]) else { return };
    out.buttons.push((out.lines.len(), 3, u(w(&text)), Click::Thread(e.id.clone())));
    out.lines.push(Line::from(vec![Span::raw("   "), Span::styled(text, theme().secondary)]));
}

/// "↳ 3 replies · 12:31" from the host's `data.thread`, or nothing without replies.
fn thread_summary(v: &serde_json::Value) -> Option<String> {
    let n = v["count"].as_i64().filter(|&n| n > 0)?;
    let at = when(v["lastAt"].as_i64().unwrap_or(0));
    Some(if at.is_empty() { format!("↳ {}", replies(n)) } else { format!("↳ {} · {at}", replies(n)) })
}

/// Keys printed on approval buttons: y / a / n for the usual kinds, digits otherwise.
fn option_keys(opts: &[(String, String, String)]) -> Vec<String> {
    let mut used = vec![];
    opts.iter()
        .enumerate()
        .map(|(i, (_, _, kind))| {
            let k = match kind.as_str() {
                "allow_once" => "y",
                "allow_always" => "a",
                "reject_once" | "reject_always" => "n",
                _ => "",
            };
            let k = if k.is_empty() || used.contains(&k) { (i + 1).to_string() } else { k.to_owned() };
            if k.len() == 1 && k.chars().all(char::is_alphabetic) {
                used.push(match k.as_str() {
                    "y" => "y",
                    "a" => "a",
                    _ => "n",
                });
            }
            k
        })
        .collect()
}

fn permission_card(out: &mut Built, e: &Entry, b: &Bot, width: usize, home: &str) {
    let t = theme();
    let bw = width.saturating_sub(1).max(20);
    let inner = bw.saturating_sub(4);
    let title = e.data["title"].as_str().unwrap_or("Use a tool");
    let head = format!(" ◆ {} wants to: {} ", b.name, truncate(title, inner.saturating_sub(w(&b.name) + 14)));
    let side = bw.saturating_sub(w(&head) + 3);
    out.lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled("╭─", t.amber),
        Span::styled(head, t.amber.add_modifier(Modifier::BOLD)),
        Span::styled(format!("{}╮", "─".repeat(side)), t.amber),
    ]));
    let row = |content: Vec<Span<'static>>| -> Line<'static> {
        let used: usize = content.iter().map(|s| w(&s.content)).sum();
        let mut spans = vec![Span::raw(" "), Span::styled("│ ", t.amber)];
        spans.extend(content);
        spans.push(Span::raw(" ".repeat(inner.saturating_sub(used))));
        spans.push(Span::styled(" │", t.amber));
        Line::from(spans)
    };
    if let Some(cmd) = e.data["command"].as_str().filter(|c| !c.is_empty()) {
        for (i, l) in md::wrap(&[Span::styled(cmd.to_owned(), t.bold)], inner.saturating_sub(2), &[], &[])
            .into_iter()
            .take(6)
            .enumerate()
        {
            let mut c = vec![Span::styled(if i == 0 { "$ " } else { "  " }, t.dim)];
            c.extend(l.spans);
            out.lines.push(row(c));
        }
        let cwd = tilde(e.data["cwd"].as_str().unwrap_or(&b.cwd), home);
        out.lines.push(row(vec![Span::styled(format!("  in {}", truncate(&cwd, inner.saturating_sub(5))), t.dim)]));
    }
    for d in e.data["diffs"].as_array().into_iter().flatten().take(3) {
        let path = tilde(d["path"].as_str().unwrap_or_default(), home);
        let stat = format!("+{} −{}", d["added"].as_u64().unwrap_or(0), d["removed"].as_u64().unwrap_or(0));
        let p = truncate(&path, inner.saturating_sub(w(&stat) + 2));
        let pad = inner.saturating_sub(w(&p) + w(&stat));
        out.lines.push(row(vec![Span::styled(p, t.bold), Span::raw(" ".repeat(pad)), Span::styled(stat, t.secondary)]));
        let patch = d["patch"].as_str().unwrap_or_default();
        let n = patch.lines().count();
        for l in patch.lines().take(6) {
            let s = if l.starts_with('+') {
                t.added
            } else if l.starts_with('-') {
                t.removed
            } else {
                t.secondary
            };
            let body = truncate(l, inner);
            let padded = format!("{body}{}", " ".repeat(inner.saturating_sub(w(&body))));
            out.lines.push(row(vec![Span::styled(padded, s)]));
        }
        if n > 6 {
            out.lines.push(row(vec![Span::styled(format!("… {} more lines · t shows the whole diff", n - 6), t.dim)]));
        }
    }
    if let Some(detail) = e.data["detail"].as_str().filter(|d| !d.trim().is_empty() && e.data["command"].is_null()) {
        for l in detail.lines().take(4) {
            out.lines.push(row(vec![Span::styled(truncate(l, inner), t.secondary)]));
        }
    }
    out.lines.push(row(vec![]));
    // Buttons, wrapping onto more lines when narrow.
    let opts = e.options();
    let keys = option_keys(&opts);
    let mut line: Vec<Span<'static>> = vec![];
    let mut used = 0usize;
    let mut pending: Vec<(u16, u16, Click)> = vec![];
    let flush =
        |out: &mut Built, line: &mut Vec<Span<'static>>, used: &mut usize, pending: &mut Vec<(u16, u16, Click)>| {
            let li = out.lines.len();
            out.lines.push(row(std::mem::take(line)));
            for (x, bw, c) in pending.drain(..) {
                out.buttons.push((li, x, bw, c));
            }
            *used = 0;
        };
    for (i, (id, name, kind)) in opts.iter().enumerate() {
        let key = format!(" {} ", keys[i]);
        let label = format!("{name} ");
        let bw_ = w(&key) + w(&label);
        if used > 0 && used + bw_ + 1 > inner {
            flush(out, &mut line, &mut used, &mut pending);
        }
        let style = if i == 0 {
            t.btn_primary
        } else if kind.starts_with("reject") {
            t.red.patch(t.btn)
        } else {
            t.text.patch(t.btn)
        };
        if used > 0 {
            line.push(Span::raw(" "));
            used += 1;
        }
        pending.push((u(3 + used), u(bw_), Click::Option { entry: e.id.clone(), option: id.clone() }));
        line.push(Span::styled(key, style.add_modifier(Modifier::BOLD)));
        line.push(Span::styled(label, style));
        used += bw_;
    }
    if !line.is_empty() {
        let hint = "N reject + say why";
        if used + w(hint) + 3 <= inner {
            line.push(Span::raw(" ".repeat(inner - used - w(hint))));
            line.push(Span::styled(hint, t.dim));
        }
        flush(out, &mut line, &mut used, &mut pending);
    }
    out.lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled(format!("╰{}╯", "─".repeat(bw.saturating_sub(2))), t.amber),
    ]));
}

fn decided_line(out: &mut Built, e: &Entry) {
    let t = theme();
    let title = e.data["title"].as_str().unwrap_or("Use a tool");
    let sel = e.data["selected"].as_str();
    let chosen = sel.and_then(|s| e.options().into_iter().find(|o| o.0 == s));
    let (g, gs, what) = match (e.data["status"].as_str(), chosen) {
        (Some("answered"), Some((_, name, kind))) if kind.starts_with("allow") => ("✓", t.green, name),
        (Some("answered"), Some((_, name, _))) => ("×", t.red, name),
        (Some("expired"), _) => ("·", t.dim, "Expired".to_owned()),
        _ => ("·", t.dim, "Cancelled".to_owned()),
    };
    out.lines.push(Line::from(vec![
        Span::styled(format!(" {g} "), gs),
        Span::styled(what, t.secondary),
        Span::styled(format!(" · {title}"), t.dim),
        Span::styled(format!(" · {}", clock(e.created_at)), t.dim),
    ]));
}

// ---------- trace ----------

fn trace(buf: &mut Buffer, r: Rect, app: &mut App) {
    let t = theme();
    app.hits.trace = r;
    let full = app.trace == TraceMode::Full;
    let Some(b) = app.bot().cloned() else { return };
    let latest = app.latest_turn(&b.id).unwrap_or(0);
    let turn = app.trace_turn.unwrap_or(latest);
    let live = turn == latest && b.status == Status::Working;
    let mut x = put(
        buf,
        r.x + 1,
        r.y,
        6,
        "TRACE",
        if app.focus == Focus::Trace { t.bold } else { t.secondary.add_modifier(Modifier::BOLD) },
    );
    x = put(
        buf,
        x + 1,
        r.y,
        r.width.saturating_sub(20),
        &format!("· {} · turn {turn}{}", b.name, if live { " · live" } else { "" }),
        t.dim,
    );
    let _ = x;
    rput(buf, r.right() - 1, r.y, if full { "{ } turns · esc close" } else { "{ } turns · T full" }, t.dim);
    hline(buf, r.x, r.y + 1, r.width, t.line);
    let body = Rect::new(r.x, r.y + 2, r.width, r.height.saturating_sub(2));
    let lines = build_trace(app, &b, turn, usize::from(body.width.saturating_sub(1)), full);
    let h = usize::from(body.height);
    let max = lines.len().saturating_sub(h);
    if live && app.focus != Focus::Trace {
        app.trace_scroll = max; // follow along while it works
    }
    app.trace_scroll = app.trace_scroll.min(max);
    for (i, l) in lines.iter().skip(app.trace_scroll).take(h).enumerate() {
        put_line(buf, body.x, body.y + u(i), body.width, l);
    }
    if lines.is_empty() {
        put(buf, body.x + 2, body.y, body.width.saturating_sub(2), "Nothing happened in this turn yet.", t.dim);
    }
}

fn tool_label(kind: &str) -> &str {
    match kind {
        "execute" => "bash",
        "other" => "tool",
        k => k,
    }
}

fn build_trace(app: &App, b: &Bot, turn: i64, width: usize, full: bool) -> Vec<Line<'static>> {
    let t = theme();
    let mut out: Vec<Line<'static>> = vec![];
    let Some(map) = app.entries.get(&b.id) else { return out };
    let label = |s: &str| Span::styled(format!("{s:<8} "), t.secondary);
    let text_rest = |width: usize| width.saturating_sub(13);
    let (diff_max, out_max) = if full { (400, 400) } else { (14, 6) };
    for e in map.values().filter(|e| e.turn == turn) {
        match e.kind {
            Kind::User => out.push(Line::from(vec![
                Span::styled(" › ", t.dim),
                label("you"),
                Span::styled(truncate(e.text(), text_rest(width)), t.text),
            ])),
            Kind::Thought | Kind::Agent => {
                let (g, lab) = if e.kind == Kind::Thought {
                    ("✱", "thought")
                } else if e.is_final() {
                    ("●", "reply")
                } else {
                    ("✱", "says")
                };
                let s = if e.kind == Kind::Thought { t.dim } else { t.text };
                if full {
                    let first = [Span::styled(format!(" {g} "), t.dim), label(lab)];
                    let rest = [Span::raw(" ".repeat(12))];
                    out.extend(md::wrap(&[Span::styled(e.text().trim().to_owned(), s)], width, &first, &rest));
                } else {
                    out.push(Line::from(vec![
                        Span::styled(format!(" {g} "), t.dim),
                        label(lab),
                        Span::styled(truncate(e.text().trim(), text_rest(width)), s),
                    ]));
                }
            }
            Kind::Tool => {
                let (g, gs) = match e.data["status"].as_str() {
                    Some("completed") => ("✓".to_owned(), t.green),
                    Some("failed") => ("×".to_owned(), t.red),
                    _ => (spin(app).to_owned(), t.secondary),
                };
                let kind = tool_label(e.data["toolKind"].as_str().unwrap_or("tool")).to_owned();
                let title = e.data["title"].as_str().unwrap_or("Tool");
                out.push(Line::from(vec![
                    Span::styled(format!(" {g} "), gs),
                    label(&kind),
                    Span::styled(truncate(title, text_rest(width)), t.text),
                ]));
                for d in e.data["diffs"].as_array().into_iter().flatten() {
                    let stat = format!("+{} −{}", d["added"].as_u64().unwrap_or(0), d["removed"].as_u64().unwrap_or(0));
                    let path = tilde(d["path"].as_str().unwrap_or_default(), &app.home);
                    out.push(Line::from(vec![
                        Span::raw("   "),
                        Span::styled(truncate(&path, width.saturating_sub(12)), t.bold),
                        Span::styled(format!("  {stat}"), t.secondary),
                    ]));
                    let start = d["startLine"].as_u64().unwrap_or(1);
                    out.push(Line::from(Span::styled(format!("   @@ line {start} @@"), t.dim)));
                    let patch = d["patch"].as_str().unwrap_or_default();
                    let n = patch.lines().count();
                    for l in patch.lines().take(diff_max) {
                        let s = if l.starts_with('+') {
                            t.added
                        } else if l.starts_with('-') {
                            t.removed
                        } else {
                            t.secondary
                        };
                        let body = truncate(l, width.saturating_sub(4));
                        let pad = width.saturating_sub(4 + w(&body));
                        out.push(Line::from(vec![
                            Span::raw("   "),
                            Span::styled(format!("{body}{}", " ".repeat(pad)), s),
                        ]));
                    }
                    if n > diff_max {
                        out.push(Line::from(Span::styled(
                            format!("   … {} more lines · T shows everything", n - diff_max),
                            t.dim,
                        )));
                    }
                }
                let output = e.data["output"].as_str().unwrap_or_default().trim_end();
                if !output.is_empty() {
                    let lines: Vec<&str> = output.lines().collect();
                    let skip = lines.len().saturating_sub(out_max);
                    if skip > 0 {
                        out.push(Line::from(Span::styled(format!("           … {skip} earlier lines"), t.dim)));
                    }
                    for l in &lines[skip..] {
                        out.push(Line::from(vec![
                            Span::raw("           "),
                            Span::styled(truncate(l, width.saturating_sub(12)), t.dim),
                        ]));
                    }
                }
            }
            Kind::Plan => {
                let items = e.data["entries"].as_array().cloned().unwrap_or_default();
                let done = items.iter().filter(|i| i["status"] == "completed").count();
                out.push(Line::from(vec![
                    Span::styled(" ☐ ", t.secondary),
                    label("plan"),
                    Span::styled(format!("{done}/{}", items.len()), t.text),
                ]));
                for i in &items {
                    let (g, s) = match i["status"].as_str() {
                        Some("completed") => ("✓", t.green),
                        Some("in_progress") => ("▸", t.bold),
                        _ => ("☐", t.dim),
                    };
                    out.push(Line::from(vec![
                        Span::raw("   "),
                        Span::styled(format!("{g} "), s),
                        Span::styled(
                            truncate(i["content"].as_str().unwrap_or_default(), width.saturating_sub(6)),
                            t.secondary,
                        ),
                    ]));
                }
            }
            Kind::Permission => {
                let st = e.data["status"].as_str().unwrap_or_default();
                out.push(Line::from(vec![
                    Span::styled(" ◆ ", t.amber),
                    label("asked"),
                    Span::styled(
                        truncate(e.data["title"].as_str().unwrap_or_default(), text_rest(width).saturating_sub(12)),
                        t.text,
                    ),
                    Span::styled(format!("  {st}"), t.dim),
                ]));
            }
            Kind::Notice => out.push(Line::from(vec![
                Span::styled(" · ", t.dim),
                Span::styled(truncate(e.text(), width.saturating_sub(4)), t.dim),
            ])),
            Kind::Other => {}
        }
    }
    out
}

// ---------- overlays ----------

fn overlays(f: &mut Frame<'_>, app: &mut App) {
    if app.overlays.is_empty() {
        return;
    }
    let area = f.area();
    let buf = f.buffer_mut();
    dim_all(buf);
    app.hits.clicks.clear();
    // Only the top overlay is live; draw it alone over the dimmed screen.
    let ovs = std::mem::take(&mut app.overlays);
    if let Some(top) = ovs.last() {
        overlay(buf, area, app, top);
    }
    app.overlays = ovs;
}

fn overlay(buf: &mut Buffer, area: Rect, app: &mut App, top: &Overlay) {
    match top {
        Overlay::Goto(g) => {
            let items = app.goto_items(g);
            goto(buf, area, app, g, &items);
        }
        Overlay::Help(filter) => help(buf, area, filter),
        Overlay::Confirm(c) => {
            let t = theme();
            let r = centered(area, 62, 8);
            let inner = frame_box(buf, r, t.red, t.panel, None);
            put(buf, inner.x, inner.y, inner.width, &c.title, t.red.patch(t.panel).add_modifier(Modifier::BOLD));
            put(buf, inner.x, inner.y + 1, inner.width, &c.detail, t.text.patch(t.panel));
            put(buf, inner.x, inner.y + 2, inner.width, &c.note, t.secondary.patch(t.panel));
            let danger = if t.color {
                Style::default().fg(t.on_color).bg(rgb(0xF0A7A7)).add_modifier(Modifier::BOLD)
            } else {
                t.sel
            };
            let x = put(buf, inner.x, inner.y + 4, 30, &format!(" ↵ {} ", c.button), danger) + 1;
            put(buf, x, inner.y + 4, 14, " esc Cancel ", t.text.patch(t.btn).add_modifier(Modifier::BOLD));
        }
        Overlay::Agents(a) => {
            let t = theme();
            let choices: Vec<serde_json::Value> = app.agent_choices(&a.query.text).into_iter().cloned().collect();
            let h = u(choices.len() + 8).min(area.height.saturating_sub(2)).max(10);
            let r = centered(area, 74, h);
            let inner = frame_box(buf, r, t.text, t.panel, Some(("New bot · 1/3 agent", t.text)));
            field_line(buf, inner, inner.y, &a.query, "type to filter", true);
            let list_h = usize::from(inner.height.saturating_sub(4));
            let off = a.cursor.saturating_sub(list_h.saturating_sub(1));
            if choices.is_empty() {
                put(
                    buf,
                    inner.x,
                    inner.y + 2,
                    inner.width,
                    if app.backends.is_empty() { "Loading agents…" } else { "No agent matches" },
                    t.dim.patch(t.panel),
                );
            }
            for (y, (i, c)) in (inner.y + 2..).zip(choices.iter().enumerate().skip(off).take(list_h)) {
                let row = Rect::new(inner.x - 1, y, inner.width + 2, 1);
                let sel = i == a.cursor;
                if sel {
                    restyle(buf, row, t.sel);
                }
                let installed = c["installed"].as_bool().unwrap_or(false);
                let base = if sel { t.sel } else { t.panel };
                put(
                    buf,
                    inner.x,
                    y,
                    2,
                    if installed { "✓" } else { "↓" },
                    if installed { t.green } else { t.dim }.patch(base),
                );
                put(
                    buf,
                    inner.x + 2,
                    y,
                    inner.width / 2,
                    c["name"].as_str().unwrap_or_default(),
                    if sel { t.bold } else { t.text }.patch(base),
                );
                let how = if installed { "on this computer".to_owned() } else { "fetched on first use".to_owned() };
                rput(buf, inner.right(), y, &how, t.dim.patch(base));
            }
            put(
                buf,
                inner.x,
                inner.bottom() - 1,
                inner.width,
                "↵ next: folder   ↑↓ move   esc cancel",
                t.dim.patch(t.panel),
            );
        }
        Overlay::Folder(p) => {
            let t = theme();
            let matches = App::folder_matches(p);
            let r = centered(area, 74, 22);
            let title = if p.new_bot.is_some() { "New bot · 2/3 folder" } else { "Folder" };
            let inner = frame_box(buf, r, t.text, t.panel, Some((title, t.text)));
            let shown = tilde(&p.path, &app.home);
            let path = format!("{}/", shown.trim_end_matches('/'));
            let x = put(
                buf,
                inner.x,
                inner.y,
                inner.width / 2,
                &truncate(&path, usize::from(inner.width / 2)),
                t.secondary.patch(t.panel),
            );
            field_line(buf, Rect::new(x, inner.y, inner.right().saturating_sub(x), 1), inner.y, &p.query, "", true);
            let list_h = usize::from(inner.height.saturating_sub(5));
            let off = p.cursor.saturating_sub(list_h.saturating_sub(1));
            let mut y = inner.y + 2;
            if let Some(e) = &p.error {
                put(buf, inner.x, y, inner.width, e, t.red.patch(t.panel));
                y += 1;
            }
            for (i, d) in matches.iter().enumerate().skip(off).take(list_h) {
                let sel = i == p.cursor;
                let base = if sel { t.sel } else { t.panel };
                if sel {
                    restyle(buf, Rect::new(inner.x - 1, y, inner.width + 2, 1), t.sel);
                }
                put(buf, inner.x, y, 2, "▸", t.secondary.patch(base));
                put(
                    buf,
                    inner.x + 2,
                    y,
                    inner.width - 10,
                    &format!("{}/", d.name),
                    if sel { t.bold } else { t.text }.patch(base),
                );
                if d.git {
                    rput(buf, inner.right(), y, "git", t.dim.patch(base));
                }
                y += 1;
            }
            let chosen = matches.get(p.cursor).map_or(shown.clone(), |d| tilde(&d.path, &app.home));
            put(
                buf,
                inner.x,
                inner.bottom() - 2,
                inner.width,
                &format!("↵ use {}", truncate(&chosen, usize::from(inner.width.saturating_sub(8)))),
                t.text.patch(t.panel),
            );
            put(
                buf,
                inner.x,
                inner.bottom() - 1,
                inner.width,
                "→ open  ← up  . use this folder  esc back",
                t.dim.patch(t.panel),
            );
        }
        Overlay::Form(form) => form_view(buf, area, app, form),
        Overlay::Group(g) => group_view(buf, area, app, g),
        Overlay::Usage => usage(buf, area, app),
        Overlay::Pair(url) => pair(buf, area, url.as_deref()),
    }
}

fn field_line(buf: &mut Buffer, r: Rect, y: u16, e: &Editor, placeholder: &str, active: bool) {
    let t = theme();
    // An empty placeholder means the caller drew its own prefix (the folder path).
    let x = if placeholder.is_empty() { r.x } else { put(buf, r.x, y, 2, "/ ", t.dim.patch(t.panel)) };
    if e.text.is_empty() {
        if active {
            set_cursor(Position::new(x, y));
        }
        put(buf, x, y, r.right().saturating_sub(x), placeholder, t.dim.patch(t.panel));
    } else {
        let room = usize::from(r.right().saturating_sub(x + 1));
        let text = e.text.replace('\n', " ");
        let before = w(&e.text[..e.cursor]);
        let skip = before.saturating_sub(room);
        let shown: String = text.chars().skip(skip).collect();
        put(buf, x, y, u(room), &shown, t.bold.patch(t.panel));
        if active {
            set_cursor(Position::new(x + u(before - skip), y));
        }
    }
}

fn goto(buf: &mut Buffer, area: Rect, app: &mut App, g: &super::app::Goto, items: &[GotoItem]) {
    let t = theme();
    let r = centered(area, 80, 24);
    let inner = frame_box(buf, r, t.text, t.panel, Some(("Go to", t.text)));
    let nbots = items.iter().filter(|i| matches!(i, GotoItem::Bot(_))).count();
    rput(buf, inner.right(), inner.y, &format!("{nbots} of {}", app.bots.len()), t.dim.patch(t.panel));
    field_line(
        buf,
        Rect::new(inner.x, inner.y, inner.width.saturating_sub(10), 1),
        inner.y,
        &g.query,
        "search bots",
        true,
    );
    // Filter chips.
    let counts = app.counts();
    let mut x = inner.x;
    for (i, (label, mark)) in FILTERS.iter().enumerate() {
        let n = match mark {
            Some(Mark::Need) => format!(" {}", counts[0]),
            Some(Mark::Work) => format!(" {}", counts[1]),
            Some(Mark::Unread) => format!(" {}", counts[2]),
            Some(Mark::Error) => format!(" {}", counts[3]),
            _ => String::new(),
        };
        let chip = format!(" {label}{n} ");
        let s = if i == g.filter { t.btn_primary } else { t.secondary.patch(t.btn) };
        let start = x;
        x = put(buf, x, inner.y + 2, inner.right().saturating_sub(x), &chip, s) + 1;
        app.hits.clicks.push((Rect::new(start, inner.y + 2, x - start, 1), Click::Filter(i)));
    }
    let list_top = inner.y + 4;
    let list_h = usize::from(inner.bottom().saturating_sub(list_top + 3));
    let off = g.cursor.saturating_sub(list_h.saturating_sub(1));
    let mut y = list_top;
    let mut shown_actions = false;
    for (i, item) in items.iter().enumerate().skip(off).take(list_h) {
        let sel = i == g.cursor;
        let base = if sel { t.sel } else { t.panel };
        match item {
            GotoItem::Bot(id) => {
                let Some(b) = app.bots.get(id) else { continue };
                if sel {
                    restyle(buf, Rect::new(inner.x - 1, y, inner.width + 2, 1), t.sel);
                }
                app.hits.clicks.push((Rect::new(inner.x - 1, y, inner.width + 2, 1), Click::Bot(id.clone())));
                let (gl, gs) = glyph(app, b.mark());
                put(buf, inner.x, y, 1, gl, gs.patch(base));
                put_line(buf, inner.x + 2, y, 2, &Line::from(avatar(b)));
                put(buf, inner.x + 5, y, 10, &b.name, t.bold.patch(base));
                put(buf, inner.x + 16, y, 10, if b.group { "group" } else { &b.backend }, t.secondary.patch(base));
                let cwd =
                    if b.group { format!("{} bots", b.members.len()) } else { truncate(&tilde(&b.cwd, &app.home), 18) };
                put(buf, inner.x + 27, y, 19, &cwd, t.secondary.patch(base));
                let (pv, ps) = preview(b);
                let room = inner.width.saturating_sub(47);
                put(buf, inner.x + 47, y, room, &truncate(&pv, usize::from(room)), ps.patch(base));
            }
            GotoItem::Action(a) => {
                if !shown_actions {
                    shown_actions = true;
                    if y > list_top {
                        y += 1;
                    }
                    put(buf, inner.x, y, inner.width, "ACTIONS", t.dim.patch(t.panel).add_modifier(Modifier::BOLD));
                    y += 1;
                }
                if sel {
                    restyle(buf, Rect::new(inner.x - 1, y, inner.width + 2, 1), t.sel);
                }
                if let Some((label, key, _)) = ACTIONS.iter().find(|x| x.2 == *a) {
                    put(buf, inner.x + 2, y, inner.width, label, if sel { t.bold } else { t.secondary }.patch(base));
                    rput(buf, inner.right(), y, key, t.dim.patch(base));
                }
            }
        }
        y += 1;
        if y >= inner.bottom().saturating_sub(3) {
            break;
        }
    }
    if items.is_empty() {
        put(buf, inner.x, list_top, inner.width, "Nothing matches", t.dim.patch(t.panel));
    }
    hline(buf, inner.x, inner.bottom() - 3, inner.width, t.line.patch(t.panel));
    if let Some(GotoItem::Bot(id)) = items.get(g.cursor)
        && let Some(b) = app.bots.get(id)
    {
        let meta = if b.group {
            let names: Vec<String> = b.members.iter().map(|m| app.author_name(Some(m))).collect();
            format!("{} · group · {}", b.name, names.join(", "))
        } else {
            format!("{} · {} · {}", b.name, b.backend, tilde(&b.cwd, &app.home))
        };
        put(buf, inner.x, inner.bottom() - 2, inner.width, &meta, t.secondary.patch(t.panel));
    }
    put(
        buf,
        inner.x,
        inner.bottom() - 1,
        inner.width,
        "↵ open   tab filter   ↑↓ move   esc close",
        t.dim.patch(t.panel),
    );
}

const HELP: [(&str, &str, &str); 44] = [
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
    ("CHAT", "r", "reply in thread: pick a message"),
    ("CHAT", "j k  ↵", "picking: move / open its thread"),
    ("CHAT", "esc", "close the thread"),
    ("TYPE", "↵", "send"),
    ("TYPE", "⇧↵ alt↵ ^j", "new line"),
    ("TYPE", "esc", "back to NAV"),
    ("TYPE", "^c", "clear the message"),
    ("TYPE", "^a ^e", "line start / end"),
    ("TYPE", "alt b / f", "word left / right"),
    ("TYPE", "^w ^u", "delete word / line"),
    ("BOTS", "n", "new bot"),
    ("BOTS", "m", "new group chat"),
    ("BOTS", "e", "edit bot or group"),
    ("BOTS", "p", "pin / unpin"),
    ("BOTS", "S", "new session"),
    ("BOTS", "x", "delete bot"),
    ("VIEW", "t", "trace pane"),
    ("VIEW", "T", "trace full screen"),
    ("VIEW", "{ }", "previous / next turn"),
    ("VIEW", "b", "compact roster"),
    ("VIEW", "U", "usage"),
    ("VIEW", "P", "pair a phone"),
    ("VIEW", "esc", "close / go back"),
    ("VIEW", "q", "quit; bots keep going"),
    ("MOUSE", "click", "open bot or thread, press button"),
    ("MOUSE", "wheel", "scroll under pointer"),
];

fn help(buf: &mut Buffer, area: Rect, filter: &Editor) {
    let t = theme();
    let r = centered(area, 80, 34);
    let inner = frame_box(buf, r, t.text, t.panel, Some(("Keys", t.text)));
    field_line(buf, inner, inner.y, filter, "filter actions and keys", true);
    let q = filter.text.to_lowercase();
    let items: Vec<&(&str, &str, &str)> = HELP
        .iter()
        .filter(|(_, k, d)| q.is_empty() || d.to_lowercase().contains(&q) || k.to_lowercase().contains(&q))
        .collect();
    let col_w = inner.width / 2;
    let rows = usize::from(inner.height.saturating_sub(3));
    let (mut col, mut y, mut section) = (0u16, inner.y + 2, "");
    for (sec, k, d) in items {
        let need = if *sec == section { 1 } else { 2 + u16::from(!section.is_empty()) };
        if y + need > inner.y + 2 + u(rows) {
            if col == 1 {
                break;
            }
            col = 1;
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
    }
    put(buf, inner.x, inner.bottom() - 1, inner.width, "esc close", t.dim.patch(t.panel));
}

fn form_view(buf: &mut Buffer, area: Rect, app: &App, f: &Form) {
    let t = theme();
    let r = centered(area, 88, 30);
    let title = match &f.bot_id {
        Some(_) => format!("Edit {}", f.name.text),
        None => "New bot · 3/3 name and looks".into(),
    };
    let inner = frame_box(buf, r, t.text, t.panel, Some((&title, t.text)));
    let wide = inner.width >= 70;
    let fx = if wide { inner.x + 18 } else { inner.x };
    if wide {
        for (i, row) in FACE.iter().enumerate() {
            put(buf, inner.x + 1, inner.y + 1 + u(i), 13, row, face_style(COLORS[f.color]).patch(t.panel));
        }
        put(buf, inner.x + 1, inner.y + 9, 16, COLORS[f.color], t.secondary.patch(t.panel));
        put(buf, inner.x + 1, inner.y + 10, 16, SHAPES[f.shape], t.dim.patch(t.panel));
    }
    let label_w = 13u16;
    let vx = fx + label_w;
    let vw = inner.right().saturating_sub(vx);
    let mut y = inner.y + 1;
    let agent_name = app
        .backends
        .iter()
        .find(|b| b["id"].as_str() == Some(f.backend.as_str()))
        .and_then(|b| b["name"].as_str())
        .unwrap_or(&f.backend)
        .to_owned();
    for (i, field) in FIELDS.iter().enumerate() {
        let active = i == f.field;
        let base = if active { t.sel } else { t.panel };
        let lab = match field {
            Field::Name => "Name",
            Field::Instructions => "Instructions",
            Field::Agent => "Agent",
            Field::Model => "Model",
            Field::Folder => "Folder",
            Field::Approvals => "Approvals",
            Field::Notify => "Notify",
            Field::Color => "Color",
            Field::Shape => "Shape",
            Field::Connectors => "Connectors",
            Field::Skills => "Skills",
        };
        put(buf, fx, y, label_w, lab, if active { t.bold } else { t.secondary }.patch(t.panel));
        let rows = if *field == Field::Instructions { 3 } else { 1 };
        if active {
            restyle(buf, Rect::new(vx.saturating_sub(1), y, vw + 1, rows), t.sel);
        }
        let text_field = |buf: &mut Buffer, e: &Editor, placeholder: &str, y: u16| {
            if e.text.is_empty() {
                put(buf, vx, y, vw, placeholder, t.dim.patch(base));
                if active {
                    set_cursor(Position::new(vx, y));
                }
            } else {
                let (lines, cur) = composer_lines(e, usize::from(vw.saturating_sub(1)).max(1));
                let first = cur.1.saturating_sub(usize::from(rows) - 1);
                for (j, l) in lines.iter().skip(first).take(usize::from(rows)).enumerate() {
                    put(buf, vx, y + u(j), vw, l, t.text.patch(base));
                }
                if active {
                    set_cursor(Position::new(vx + u(cur.0), y + u(cur.1 - first)));
                }
            }
        };
        match field {
            Field::Name => text_field(buf, &f.name, "Name", y),
            Field::Instructions => {
                text_field(buf, &f.instructions, "e.g. Reviews PRs. Never pushes without asking.", y);
            }
            Field::Model => text_field(buf, &f.model, "default", y),
            Field::Agent => {
                put(buf, vx, y, vw, &format!("‹ {agent_name} ›"), t.text.patch(base));
            }
            Field::Folder => {
                put(
                    buf,
                    vx,
                    y,
                    vw,
                    &format!("{}  ▸", truncate(&tilde(&f.cwd, &app.home), usize::from(vw.saturating_sub(4)))),
                    t.text.patch(base),
                );
            }
            Field::Approvals => {
                let s = if f.auto { "○ Ask   ◉ Auto (approve every request)" } else { "◉ Ask   ○ Auto" };
                put(buf, vx, y, vw, s, t.text.patch(base));
            }
            Field::Notify => {
                let s = if f.notify { "◉ Done and needs-you   ○ Off" } else { "○ Done and needs-you   ◉ Off" };
                put(buf, vx, y, vw, s, t.text.patch(base));
            }
            Field::Color => {
                let mut x = vx;
                for (ci, c) in COLORS.iter().enumerate() {
                    let s = if t.color { Style::default().bg(bot_color(c)) } else { t.text };
                    let mark = if ci == f.color { "◆ " } else { "  " };
                    x = put(buf, x, y, 2, mark, s.fg(t.on_color)) + 1;
                }
            }
            Field::Shape => {
                put(buf, vx, y, vw, &format!("‹ {} ›", SHAPES[f.shape]), t.text.patch(base));
            }
            Field::Connectors | Field::Skills => {
                let list = if *field == Field::Connectors { &f.connectors } else { &f.skills };
                if list.is_empty() {
                    put(buf, vx, y, vw, "none installed", t.dim.patch(base));
                } else {
                    let mut x = vx;
                    for (j, tg) in list.iter().enumerate() {
                        let item = format!("{} {}", if tg.on { "☑" } else { "☐" }, tg.name);
                        let s = if active && j == f.sub { t.bold.add_modifier(Modifier::UNDERLINED) } else { t.text };
                        if x + u(w(&item)) >= inner.right() {
                            put(buf, x, y, 2, "…", t.dim.patch(base));
                            break;
                        }
                        x = put(buf, x, y, u(w(&item)), &item, s.patch(base)) + 2;
                    }
                }
            }
        }
        y += rows + 1;
        if y >= inner.bottom().saturating_sub(2) {
            break;
        }
    }
    let by = inner.bottom() - 1;
    if let Some(e) = &f.error {
        put(buf, inner.x, by - 1, inner.width, e, t.red.patch(t.panel));
    }
    let save = if f.saving {
        " … Saving "
    } else if f.bot_id.is_some() {
        " ↵ Save "
    } else {
        " ↵ Create "
    };
    let x = put(buf, inner.x, by, 14, save, t.btn_primary) + 1;
    put(buf, x, by, 12, " esc Cancel ", t.text.patch(t.btn).add_modifier(Modifier::BOLD));
    let hint = match f.current() {
        Field::Folder => "↵ browse · tab next field",
        Field::Connectors | Field::Skills => "←→ move · space toggle · tab next",
        Field::Agent | Field::Color | Field::Shape | Field::Approvals | Field::Notify => "←→ change · tab next field",
        Field::Instructions => "⇧↵ new line · tab next field",
        _ => "tab next field · ^s save",
    };
    rput(buf, inner.right(), by, hint, t.dim.patch(t.panel));
}

fn group_view(buf: &mut Buffer, area: Rect, app: &App, g: &GroupForm) {
    let t = theme();
    let bots = app.group_candidates(&g.members);
    let r = centered(area, 74, u(bots.len() + 10).max(14));
    let title = if g.group_id.is_some() { "Group chat" } else { "New group chat" };
    let inner = frame_box(buf, r, t.text, t.panel, Some((title, t.text)));
    let label = |on: bool| if on { t.bold } else { t.secondary }.patch(t.panel);
    put(buf, inner.x, inner.y, 8, "Name", label(g.on_name));
    let default = app.group_default_name(&g.members);
    let placeholder = if default.is_empty() { "Name" } else { default.as_str() };
    field_line(
        buf,
        Rect::new(inner.x + 8, inner.y, inner.width.saturating_sub(8), 1),
        inner.y,
        &g.name,
        placeholder,
        g.on_name,
    );
    put(
        buf,
        inner.x,
        inner.y + 2,
        inner.width,
        &format!("Bots · {} of {MAX_MEMBERS}", g.members.len()),
        label(!g.on_name),
    );
    let list_h = usize::from(inner.height.saturating_sub(7));
    let off = g.cursor.saturating_sub(list_h.saturating_sub(1));
    if bots.is_empty() {
        put(buf, inner.x, inner.y + 3, inner.width, "No bots yet. n creates one.", t.dim.patch(t.panel));
    }
    for (y, (i, b)) in (inner.y + 3..).zip(bots.iter().enumerate().skip(off).take(list_h)) {
        let sel = !g.on_name && i == g.cursor;
        let base = if sel { t.sel } else { t.panel };
        if sel {
            restyle(buf, Rect::new(inner.x - 1, y, inner.width + 2, 1), t.sel);
        }
        let on = g.members.contains(&b.id);
        put(buf, inner.x, y, 2, if on { "☑" } else { "☐" }, if on { t.bold } else { t.dim }.patch(base));
        put_line(buf, inner.x + 2, y, 2, &Line::from(avatar(b)));
        put(buf, inner.x + 5, y, inner.width / 2, &b.name, if sel { t.bold } else { t.text }.patch(base));
        let folder =
            std::path::Path::new(&b.cwd).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
        rput(buf, inner.right(), y, &truncate(&folder, usize::from(inner.width / 3)), t.dim.patch(base));
    }
    let by = inner.bottom() - 1;
    if let Some(e) = &g.error {
        put(buf, inner.x, by - 1, inner.width, e, t.red.patch(t.panel));
    } else {
        put(
            buf,
            inner.x,
            by - 1,
            inner.width,
            "Everyone answers in turn unless you @mention someone.",
            t.dim.patch(t.panel),
        );
    }
    let save = if g.saving {
        " … Saving "
    } else if g.group_id.is_some() {
        " ↵ Save "
    } else {
        " ↵ Create "
    };
    let x = put(buf, inner.x, by, 14, save, t.btn_primary) + 1;
    put(buf, x, by, 12, " esc Cancel ", t.text.patch(t.btn).add_modifier(Modifier::BOLD));
    rput(
        buf,
        inner.right(),
        by,
        if g.on_name { "tab bots" } else { "↑↓ move · space pick · tab name" },
        t.dim.patch(t.panel),
    );
}

fn usage(buf: &mut Buffer, area: Rect, app: &App) {
    let t = theme();
    let providers = app.usage["providers"].as_array().cloned().unwrap_or_default();
    let rows: usize = providers.iter().map(|p| 2 + p["windows"].as_array().map_or(0, Vec::len)).sum();
    let r = centered(area, 78, u(rows + 5).max(8));
    let inner = frame_box(buf, r, t.text, t.panel, Some(("Usage", t.text)));
    let mut y = inner.y + 1;
    if providers.is_empty() {
        put(buf, inner.x, y, inner.width, "No usage numbers yet.", t.secondary.patch(t.panel));
    }
    for p in &providers {
        put(buf, inner.x, y, inner.width, p["name"].as_str().unwrap_or_default(), t.bold.patch(t.panel));
        y += 1;
        for win in p["windows"].as_array().into_iter().flatten() {
            let pct = win["percent"].as_f64().unwrap_or(0.0).clamp(0.0, 100.0);
            let on = filled(pct, 16);
            put(
                buf,
                inner.x + 2,
                y,
                18,
                &truncate(win["label"].as_str().unwrap_or_default(), 18),
                t.secondary.patch(t.panel),
            );
            let mut x = put(
                buf,
                inner.x + 21,
                y,
                16,
                &"▰".repeat(on.min(16)),
                if pct >= 80.0 { t.amber } else { t.text }.patch(t.panel),
            );
            x = put(buf, x, y, 16, &"▱".repeat(16 - on.min(16)), t.line.patch(t.panel));
            put(buf, x + 1, y, 5, &format!("{pct:>3.0}%"), t.text.patch(t.panel));
            let reset = win["resetsAt"]
                .as_i64()
                .map(|ms| format!("resets {}", when_future(ms)))
                .or_else(|| {
                    // "Sep 26 at 11:59am (Asia/Taipei)": the zone is this computer's own.
                    win["resetsText"].as_str().map(|s| truncate(s.split(" (").next().unwrap_or(s), 24))
                })
                .unwrap_or_default();
            rput(buf, inner.right(), y, &reset, t.dim.patch(t.panel));
            y += 1;
        }
        y += 1;
    }
    put(buf, inner.x, inner.bottom() - 1, inner.width, "From your local installs. esc close", t.dim.patch(t.panel));
}

fn when_future(ms: i64) -> String {
    let (day, _) = local(ms);
    let (today, _) = local(now_ms());
    if day == today {
        clock(ms)
    } else if day - today < 7 {
        format!("{} {}", DAYS[usize::try_from(day.rem_euclid(7)).unwrap_or(0)], clock(ms))
    } else {
        let (m, d) = month_day(day);
        format!("{} {d}", MONTHS[m])
    }
}

fn pair(buf: &mut Buffer, area: Rect, url: Option<&str>) {
    let t = theme();
    let Some(url) = url else {
        let r = centered(area, 40, 5);
        let inner = frame_box(buf, r, t.text, t.panel, Some(("Pair a phone", t.text)));
        put(buf, inner.x, inner.y + 1, inner.width, "Getting the pairing code…", t.secondary.patch(t.panel));
        return;
    };
    let qr = qrcode::QrCode::new(url.as_bytes())
        .map(|c| c.render::<qrcode::render::unicode::Dense1x2>().quiet_zone(true).build())
        .unwrap_or_default();
    let lines: Vec<&str> = qr.lines().collect();
    let qw = lines.first().map_or(0, |l| w(l));
    let fits = u(lines.len() + 6) <= area.height && u(qw + 4) <= area.width;
    let (bw, bh) = if fits { (u(qw.max(50) + 4), u(lines.len() + 6)) } else { (70, 8) };
    let r = centered(area, bw, bh);
    let inner = frame_box(buf, r, t.text, t.panel, Some(("Pair a phone", t.text)));
    let mut y = inner.y;
    if fits {
        // The QR needs light modules on dark ones regardless of the terminal theme.
        let qs = Style::default().fg(Color::White).bg(Color::Black);
        let x = inner.x + inner.width.saturating_sub(u(qw)) / 2;
        for l in &lines {
            put(buf, x, y, u(qw), l, qs);
            y += 1;
        }
    } else {
        put(buf, inner.x, y, inner.width, "Make the window taller to show the QR code.", t.secondary.patch(t.panel));
        y += 1;
    }
    put(
        buf,
        inner.x,
        y,
        inner.width,
        "Scan with the Codync iPhone app, or open on the phone:",
        t.secondary.patch(t.panel),
    );
    put(buf, inner.x, y + 1, inner.width, &truncate(url, usize::from(inner.width)), t.dim.patch(t.panel));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(month_day(0), (0, 1)); // 1970-01-01
        assert_eq!(month_day(20_454), (0, 1)); // 2026-01-01
        assert_eq!(month_day(20_721), (8, 25)); // 2026-09-25
        assert_eq!(DAYS[usize::try_from(20_721_i64.rem_euclid(7)).unwrap_or(0)], "Fri");
    }

    #[test]
    fn composer_cursor_tracks_wraps_and_newlines() {
        let (l, c) = composer_lines(&Editor::with("ab\ncd"), 10);
        assert_eq!(l, ["ab", "cd"]);
        assert_eq!(c, (2, 1));
        let (l, c) = composer_lines(&Editor::with("abcdef"), 3);
        assert_eq!(l, ["abc", "def", ""]);
        assert_eq!(c, (0, 2));
    }

    #[test]
    fn thread_summary_counts_replies() {
        assert_eq!(thread_summary(&serde_json::json!({"count": 0})), None);
        assert_eq!(thread_summary(&serde_json::Value::Null), None);
        assert_eq!(thread_summary(&serde_json::json!({"count": 1})).as_deref(), Some("↳ 1 reply"));
        assert_eq!(thread_summary(&serde_json::json!({"count": 3, "lastAt": 0})).as_deref(), Some("↳ 3 replies"));
    }

    #[test]
    fn option_keys_use_letters_then_digits() {
        let o = |k: &str| (String::new(), String::new(), k.to_owned());
        let keys = option_keys(&[o("allow_once"), o("allow_always"), o("reject_once"), o("reject_always")]);
        assert_eq!(keys, ["y", "a", "n", "4"]);
    }
}
