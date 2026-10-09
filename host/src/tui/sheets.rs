//! Drawing the `manage` sheets: memory, routines, marketplace, agent sign-in, fields.

// Drawing code reads best with x / y / w / t (theme) / r (rect).
#![allow(clippy::many_single_char_names, clippy::similar_names)]

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use serde_json::Value;

mod memory_view;
pub use memory_view::draw as memory;

use super::app::{App, Editor};
use super::manage::{
    AgentSetup, Fields, Market, RoutineField, RoutineForm, RoutineList, Row, SetupRow, Submit, TABS, Tab,
};
use super::md::truncate;
use super::view::{
    centered, composer_lines, field_line, frame_box, hline, input_line, put, restyle, rput, set_cursor, theme, u,
    when_future,
};

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or_default()
}

/// Rows `off..` of a list that fit `h` lines with `cursor` on screen.
fn window(cursor: usize, h: usize) -> usize {
    cursor.saturating_sub(h.saturating_sub(1))
}

/// A selectable row's background, highlighted when `sel`.
fn row(buf: &mut Buffer, inner: Rect, y: u16, sel: bool) -> Style {
    let t = theme();
    if sel {
        restyle(buf, Rect::new(inner.x - 1, y, inner.width + 2, 1), t.sel);
        t.sel
    } else {
        t.panel
    }
}

fn footer(buf: &mut Buffer, inner: Rect, error: Option<&str>, keys: &str) {
    let t = theme();
    let by = inner.bottom() - 1;
    if let Some(e) = error {
        put(buf, inner.x, by - 1, inner.width, &truncate(e, usize::from(inner.width)), t.red.patch(t.panel));
    }
    put(buf, inner.x, by, inner.width, keys, t.dim.patch(t.panel));
}

fn name_of(app: &App, bot: &str) -> String {
    app.bots.get(bot).map_or_else(String::new, |b| b.name.clone())
}

pub fn routines(buf: &mut Buffer, area: Rect, app: &App, l: &RoutineList) {
    let t = theme();
    let r = centered(area, 90, 24);
    let title = format!("{}'s routines", name_of(app, &l.bot));
    let inner = frame_box(buf, r, t.text, t.panel, Some((&title, t.text)));
    let list_h = usize::from(inner.height.saturating_sub(3)) / 2;
    match &l.items {
        None => {
            put(buf, inner.x, inner.y + 1, inner.width, "Loading…", t.dim.patch(t.panel));
        }
        Some(v) if v.is_empty() => {
            put(
                buf,
                inner.x,
                inner.y + 1,
                inner.width,
                "No routines. c asks the bot for one; n sets one up here.",
                t.secondary.patch(t.panel),
            );
        }
        Some(items) => {
            let mut y = inner.y + 1;
            for (i, it) in items.iter().enumerate().skip(window(l.cursor, list_h)).take(list_h) {
                let base = row(buf, inner, y, i == l.cursor);
                let on = it["enabled"] == true;
                put(buf, inner.x, y, 2, if on { "●" } else { "○" }, if on { t.bold } else { t.dim }.patch(base));
                put(buf, inner.x + 2, y, inner.width / 2, s(it, "name"), t.bold.patch(base));
                let next = it["nextRunAt"].as_i64().map(|ms| format!("next {}", when_future(ms)));
                rput(
                    buf,
                    inner.right(),
                    y,
                    &next.unwrap_or_else(|| if on { String::new() } else { "off".into() }),
                    t.dim.patch(base),
                );
                let when: Vec<&str> =
                    it["triggerDescriptions"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
                let (line, style) = match it["lastError"].as_str() {
                    Some(e) => (format!("{} · {e}", when.join(" · ")), t.red),
                    None => (when.join(" · "), t.secondary),
                };
                let room = usize::from(inner.width.saturating_sub(2));
                put(buf, inner.x + 2, y + 1, u(room), &truncate(&line, room), style.patch(t.panel));
                y += 2;
            }
        }
    }
    footer(
        buf,
        inner,
        None,
        "n new · c ask the bot · ↵ edit · space on/off · r run now · w copy webhook · W new key · x delete · esc",
    );
}

pub fn routine(buf: &mut Buffer, area: Rect, f: &RoutineForm) {
    let t = theme();
    let r = centered(area, 84, 14);
    let title = if f.id.is_some() { "Edit routine" } else { "New routine" };
    let inner = frame_box(buf, r, t.text, t.panel, Some((title, t.text)));
    let label = |on: bool| if on { t.bold } else { t.secondary }.patch(t.panel);
    let vx = inner.x + 9;
    let vw = inner.width.saturating_sub(9);
    let at = |y: u16| Rect::new(vx, y, vw, 1);
    put(buf, inner.x, inner.y, 9, "Name", label(f.field == RoutineField::Name));
    input_line(buf, at(inner.y), inner.y, &f.name, "Morning summary", f.field == RoutineField::Name, false);
    put(buf, inner.x, inner.y + 2, 9, "Do", label(f.field == RoutineField::Instruction));
    multiline(
        buf,
        Rect::new(vx, inner.y + 2, vw, 4),
        &f.instruction,
        "What the bot does each time",
        f.field == RoutineField::Instruction,
    );
    put(buf, inner.x, inner.y + 7, 9, "When", label(f.field == RoutineField::When));
    let placeholder = if f.original_text.is_empty() {
        "0 9 * * 1-5  ·  every 2h  ·  2026-10-01 09:00  ·  webhook"
    } else {
        &f.original_text
    };
    input_line(buf, at(inner.y + 7), inner.y + 7, &f.when, placeholder, f.field == RoutineField::When, false);
    put(
        buf,
        vx,
        inner.y + 8,
        vw,
        &format!("cron runs in {}; every = from now on; webhook = when called", super::manage::local_zone()),
        t.dim.patch(t.panel),
    );
    let by = inner.bottom() - 1;
    if let Some(e) = &f.error {
        put(buf, inner.x, by - 1, inner.width, e, t.red.patch(t.panel));
    }
    let x = put(buf, inner.x, by, 14, if f.saving { " … Saving " } else { " ↵ Save " }, t.btn_primary) + 1;
    put(buf, x, by, 12, " esc Cancel ", t.text.patch(t.btn).add_modifier(Modifier::BOLD));
    rput(buf, inner.right(), by, "tab next field · ⇧↵ new line", t.dim.patch(t.panel));
}

/// A text field over several lines, following its cursor.
fn multiline(buf: &mut Buffer, r: Rect, e: &Editor, placeholder: &str, active: bool) {
    let t = theme();
    if e.text.is_empty() {
        put(buf, r.x, r.y, r.width, placeholder, t.dim.patch(t.panel));
        if active {
            set_cursor(Position::new(r.x, r.y));
        }
        return;
    }
    let (lines, cur) = composer_lines(e, usize::from(r.width.saturating_sub(1)).max(1));
    let first = cur.1.saturating_sub(usize::from(r.height).saturating_sub(1));
    for (j, l) in lines.iter().skip(first).take(usize::from(r.height)).enumerate() {
        put(buf, r.x, r.y + u(j), r.width, l, t.text.patch(t.panel));
    }
    if active {
        set_cursor(Position::new(r.x + u(cur.0), r.y + u(cur.1 - first)));
    }
}

pub fn market(buf: &mut Buffer, area: Rect, app: &App, m: &Market) {
    let t = theme();
    let r = centered(area, 96, 30);
    let title = format!("Marketplace · {}", app.host);
    let inner = frame_box(buf, r, t.text, t.panel, Some((&title, t.text)));
    let mut x = inner.x;
    for (tab, label) in TABS {
        let style = if tab == m.tab { t.btn_primary } else { t.secondary.patch(t.btn) };
        x = put(buf, x, inner.y, 20, &format!(" {label} "), style) + 1;
    }
    let search = Rect::new(x + 2, inner.y, inner.right().saturating_sub(x + 2), 1);
    let placeholder = match m.tab {
        Tab::Connectors => "search the MCP registry",
        _ => "filter",
    };
    field_line(buf, search, inner.y, &m.query, placeholder, true);
    let rows = app.market_rows(m);
    let list_h = usize::from(inner.height.saturating_sub(5));
    let mut y = inner.y + 2;
    if App::market_stale(m) {
        put(buf, inner.x, y, inner.width, "↵ searches the registry", t.secondary.patch(t.panel));
        y += 1;
    }
    for (i, item) in rows.iter().enumerate().skip(window(m.cursor, list_h)).take(list_h) {
        let base = row(buf, inner, y, i == m.cursor);
        let (mark, name, detail, right): (&str, String, String, String) = match item {
            Row::Agent(b) => {
                let status = match (b["installed"] == true, b["signedIn"].as_bool()) {
                    (_, Some(true)) => "signed in",
                    (true, Some(false)) => "not signed in",
                    (true, None) => "installed",
                    (false, _) if b["curated"] == true => "not installed",
                    (false, _) => "sets up on first use",
                };
                let mark = if b["signedIn"] == true || b["installed"] == true { "✓" } else { "↓" };
                (mark, s(b, "name").into(), s(b, "description").into(), status.into())
            }
            Row::Installed(c) => {
                let right = match s(c, "auth") {
                    "signedOut" => "↵ sign in",
                    _ if c["kind"] == "composio" => "Composio",
                    _ => "installed",
                };
                let detail = c["command"].as_str().or(c["url"].as_str()).unwrap_or(s(c, "description"));
                ("✓", s(c, "name").into(), detail.into(), right.into())
            }
            Row::Custom => ("+", "Custom connector".into(), "Paste any MCP server's config".into(), String::new()),
            Row::Registry(c) => ("↓", s(c, "title").into(), s(c, "description").into(), String::new()),
            Row::More => ("…", "Load more connectors".into(), String::new(), String::new()),
            Row::OwnSkill(v) => ("✓", s(v, "name").into(), s(v, "description").into(), "installed".into()),
            Row::Skill(v) => ("↓", s(v, "name").into(), s(v, "description").into(), String::new()),
        };
        let ok = mark == "✓";
        put(buf, inner.x, y, 2, mark, if ok { t.green } else { t.dim }.patch(base));
        let nx = put(buf, inner.x + 2, y, 26, &truncate(&name, 26), t.bold.patch(base)) + 2;
        let rx = rput(buf, inner.right(), y, &right, t.dim.patch(base));
        let room = usize::from(rx.saturating_sub(nx + 1));
        put(buf, nx, y, u(room), &truncate(&detail.replace('\n', " "), room), t.secondary.patch(base));
        y += 1;
    }
    if rows.is_empty() {
        let msg = if m.loading { "Loading…" } else { "Nothing matches" };
        put(buf, inner.x, y, inner.width, msg, t.dim.patch(t.panel));
    }
    let keys = match m.tab {
        Tab::Agents => "tab switch · ↵ install / sign in · ^r refresh · esc close",
        Tab::Connectors => "tab switch · ↵ add / sign in · ^x remove · ^r refresh · esc",
        Tab::Skills => "tab switch · ↵ install · ^x remove · esc close",
    };
    let loading = m.loading && m.tab == Tab::Connectors && !rows.is_empty();
    footer(buf, inner, m.error.as_deref().or(loading.then_some("Loading the registry…")), keys);
}

pub fn agent(buf: &mut Buffer, area: Rect, app: &App, a: &AgentSetup) {
    let t = theme();
    let b = app.backends.iter().find(|b| b["id"] == a.backend.as_str());
    let name = b.map_or(a.backend.as_str(), |b| s(b, "name"));
    let rows = app.agent_rows(a);
    let r = centered(area, 80, u(rows.len() * 2 + 11).max(14));
    let inner = frame_box(buf, r, t.text, t.panel, Some((name, t.text)));
    let mut y = inner.y;
    if let Some(d) = b.map(|b| s(b, "description")).filter(|d| !d.is_empty()) {
        put(buf, inner.x, y, inner.width, &truncate(d, usize::from(inner.width)), t.secondary.patch(t.panel));
        y += 2;
    }
    let signed_in =
        a.auth.as_ref().map_or_else(|| b.and_then(|b| b["signedIn"].as_bool()), |v| v["signedIn"].as_bool());
    let status = if a.busy {
        if a.auth.is_none() {
            format!("Checking with {name}… the first check can download it.")
        } else {
            "Working…".into()
        }
    } else {
        match signed_in {
            Some(true) => "Signed in.".into(),
            Some(false) => "Not signed in yet.".into(),
            None => format!("Couldn't tell. Skip this if you've already signed in on {}.", app.host),
        }
    };
    let style = if signed_in == Some(true) { t.green } else { t.text };
    put(buf, inner.x, y, inner.width, &status, style.patch(t.panel).add_modifier(Modifier::BOLD));
    y += 1;
    if signed_in != Some(true)
        && let Some(d) = a.auth.as_ref().and_then(|v| v["detail"].as_str()).filter(|d| !d.is_empty())
    {
        put(buf, inner.x, y, inner.width, &truncate(d, usize::from(inner.width)), t.dim.patch(t.panel));
        y += 1;
    }
    y += 1;
    for (i, rw) in rows.iter().enumerate() {
        if y + 1 >= inner.bottom().saturating_sub(2) {
            break;
        }
        let base = row(buf, inner, y, i == a.cursor);
        let (icon, title, detail) = match rw {
            SetupRow::Install => ("↓", "Install".to_owned(), format!("Runs the official installer on {}.", app.host)),
            SetupRow::Login => ("›_", "Sign in".to_owned(), format!("In a terminal on {}.", app.host)),
            SetupRow::Method(m) => {
                let (icon, fallback) = match s(m, "kind") {
                    "terminal" => ("›_", format!("In a terminal on {}.", app.host)),
                    "envVar" => ("⚿", format!("Saved on {} only.", app.host)),
                    _ => ("◎", format!("Opens a browser on {}.", app.host)),
                };
                let d = m["description"].as_str().map_or(fallback, str::to_owned);
                (icon, s(m, "name").to_owned(), d)
            }
            SetupRow::Check => ("↻", "Check again".to_owned(), String::new()),
        };
        put(buf, inner.x, y, 3, icon, t.secondary.patch(base));
        let nx = put(buf, inner.x + 3, y, 30, &title, t.bold.patch(base)) + 2;
        put(
            buf,
            nx,
            y,
            inner.right().saturating_sub(nx),
            &truncate(&detail, usize::from(inner.right().saturating_sub(nx))),
            t.secondary.patch(base),
        );
        y += 1;
    }
    footer(buf, inner, a.error.as_deref(), "↑↓ move · ↵ run · esc back");
}

pub fn fields(buf: &mut Buffer, area: Rect, f: &Fields) {
    let t = theme();
    let tall = f.inputs.iter().any(|i| i.multiline);
    let h = if tall { 20 } else { u(f.inputs.len() * 2 + usize::from(f.choices.len() > 1) * 2 + 8) };
    let r = centered(area, 84, h);
    let inner = frame_box(buf, r, t.text, t.panel, Some((&f.title, t.text)));
    let mut y = inner.y;
    put(buf, inner.x, y, inner.width, &truncate(&f.note, usize::from(inner.width)), t.dim.patch(t.panel));
    y += 2;
    let label_w = 18u16;
    let vx = inner.x + label_w;
    let vw = inner.width.saturating_sub(label_w);
    let mut row_i = 0;
    if f.choices.len() > 1 {
        let on = f.cursor == 0;
        put(buf, inner.x, y, label_w, "Runs", if on { t.bold } else { t.secondary }.patch(t.panel));
        let base = if on { t.sel } else { t.panel };
        put(buf, vx, y, vw, &format!("‹ {} ›", f.choices[f.choice]), t.text.patch(base));
        y += 2;
        row_i = 1;
    }
    for (i, input) in f.inputs.iter().enumerate() {
        let on = f.cursor == row_i + i;
        let star = if input.required { " *" } else { "" };
        put(
            buf,
            inner.x,
            y,
            label_w - 1,
            &truncate(&format!("{}{star}", input.label), usize::from(label_w - 1)),
            if on { t.bold } else { t.secondary }.patch(t.panel),
        );
        if input.multiline {
            multiline(
                buf,
                Rect::new(vx, y, vw, inner.bottom().saturating_sub(y + 3)),
                &input.ed,
                &input.placeholder,
                on,
            );
            break;
        }
        let shown = if input.secret {
            Editor {
                text: "•".repeat(input.ed.text.chars().count()),
                cursor: "•".len() * input.ed.text[..input.ed.cursor].chars().count(),
            }
        } else {
            input.ed.clone()
        };
        input_line(buf, Rect::new(vx, y, vw, 1), y, &shown, &input.placeholder, on, false);
        y += 2;
    }
    let by = inner.bottom() - 1;
    if let Some(e) = &f.error {
        put(buf, inner.x, by - 1, inner.width, e, t.red.patch(t.panel));
    }
    hline(buf, inner.x, by - 2, inner.width, t.line.patch(t.panel));
    let x = put(
        buf,
        inner.x,
        by,
        14,
        if f.saving {
            " … Saving "
        } else if matches!(f.submit, Submit::Connection(_)) {
            " ↵ Continue "
        } else {
            " ↵ Save "
        },
        t.btn_primary,
    ) + 1;
    put(buf, x, by, 12, " esc Cancel ", t.text.patch(t.btn).add_modifier(Modifier::BOLD));
    let hint = if matches!(f.submit, Submit::Connection(_)) {
        "^x cancel request · tab next · esc close"
    } else if tall {
        "⇧↵ new line · paste works"
    } else {
        "tab next field"
    };
    rput(buf, inner.right(), by, hint, t.dim.patch(t.panel));
}
