//! Drawing the `manage` sheets: memory, routines, marketplace, agent sign-in, fields.

// Drawing code reads best with x / y / w / t (theme) / r (rect).
#![allow(clippy::many_single_char_names, clippy::similar_names)]

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};

use crate::tui::l10n::tr;
use serde_json::Value;

mod library_view;
mod memory_view;
pub use library_view::draw as library;
pub use memory_view::draw as memory;

use super::app::{App, Editor};
use super::manage::{
    AgentSetup, Fields, Market, Page, RoutineField, RoutineForm, RoutineList, Row, SetupRow, Submit, TABS, Tab,
};
use super::md::truncate;
use super::view::{
    centered, chain_line, composer_lines, field_line, frame_box, hline, input_line, put, restyle, rput, set_cursor,
    theme, u, using_line, when_future,
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
        let shown = tr(e);
        put(
            buf,
            inner.x,
            by - 1,
            inner.width,
            &truncate(shown.as_ref(), usize::from(inner.width)),
            t.red.patch(t.panel),
        );
    }
    let keys = tr(keys);
    put(buf, inner.x, by, inner.width, keys.as_ref(), t.dim.patch(t.panel));
}

fn name_of(app: &App, bot: &str) -> String {
    app.bots.get(bot).map_or_else(String::new, |b| b.name.clone())
}

/// A bot's pages on the top line (Details, Memory, Settings) with this one marked; returns the
/// area under it.
pub fn page_tabs(buf: &mut Buffer, inner: Rect, current: Page) -> Rect {
    let t = theme();
    let mut x = inner.x;
    for page in Page::ALL {
        let style = if page == current { t.text.patch(t.sel) } else { t.secondary.patch(t.panel) };
        let tab = tr(page.label());
        x = put(buf, x, inner.y, inner.right().saturating_sub(x), &format!(" {tab} "), style) + 1;
    }
    rput(buf, inner.right(), inner.y, "⇧←→ page", t.dim.patch(t.panel));
    Rect::new(inner.x, inner.y + 2, inner.width, inner.height.saturating_sub(2))
}

pub fn routines(buf: &mut Buffer, area: Rect, app: &App, l: &RoutineList) {
    let t = theme();
    let r = centered(area, 90, 28);
    let title = name_of(app, &l.bot);
    let inner = frame_box(buf, r, t.text, t.panel, Some((&title, t.text)));
    let mut inner = page_tabs(buf, inner, Page::Details);
    // What changes often: the agent, its model and the instructions (`i` edits them in Settings).
    if let Some(b) = app.bots.get(&l.bot).filter(|b| !b.group) {
        let agent = app
            .backends
            .iter()
            .find(|x| x["id"].as_str() == Some(b.backend.as_str()))
            .map_or(&*b.backend, |x| s(x, "name"));
        let model = b.model.as_deref().unwrap_or("default model");
        let fast = if b.fast_mode { " · fast" } else { "" };
        put(buf, inner.x, inner.y, inner.width, &format!("{agent} · {model}{fast}"), t.text.patch(t.panel));
        rput(buf, inner.right(), inner.y, "i edits", t.dim.patch(t.panel));
        let mut line = inner.y + 1;
        // The agent it's on when its own can't answer, and the chain.
        if let Some(using) = using_line(app, b) {
            put(buf, inner.x, line, inner.width, &using, t.amber.patch(t.panel));
            rput(buf, inner.right(), line, "B switches back", t.dim.patch(t.panel));
            line += 1;
        }
        if let Some(chain) = chain_line(app, b) {
            let room = usize::from(inner.width);
            put(buf, inner.x, line, inner.width, &truncate(&chain, room), t.secondary.patch(t.panel));
            line += 1;
        }
        let first = b.description.lines().find(|l| !l.trim().is_empty()).unwrap_or("No standing instructions");
        let room = usize::from(inner.width);
        put(buf, inner.x, line, inner.width, &truncate(first.trim(), room), t.secondary.patch(t.panel));
        let used = line + 2 - inner.y;
        inner = Rect::new(inner.x, inner.y + used, inner.width, inner.height.saturating_sub(used));
    }
    if !app.screen.is_null() {
        let on = app.bots.get(&l.bot).is_some_and(|b| b.computer);
        put(buf, inner.x, inner.y, inner.width, &tr("Use the computer"), t.text.patch(t.panel));
        let x = rput(buf, inner.right(), inner.y, "  u switches", t.dim.patch(t.panel));
        rput(buf, x, inner.y, if on { "◉ On" } else { "○ Off" }, if on { t.text } else { t.secondary }.patch(t.panel));
        inner = Rect::new(inner.x, inner.y + 2, inner.width, inner.height.saturating_sub(2));
    }
    put(buf, inner.x, inner.y, inner.width, &tr("Routines"), t.bold.patch(t.panel));
    let list_h = usize::from(inner.height.saturating_sub(3)) / 2;
    match &l.items {
        None => {
            put(buf, inner.x, inner.y + 1, inner.width, &tr("Loading…"), t.dim.patch(t.panel));
        }
        Some(v) if v.is_empty() => {
            put(
                buf,
                inner.x,
                inner.y + 1,
                inner.width,
                &tr("No routines. c asks the bot for one; n sets one up here."),
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
                // A retry still waiting comes before the schedule's next run.
                let retry = l
                    .runs
                    .iter()
                    .find(|r| r["routineId"] == it["id"] && r["status"] == "pending")
                    .and_then(|r| r["notBefore"].as_i64());
                let next = retry
                    .map(|ms| format!("retry {}", when_future(ms)))
                    .or_else(|| it["nextRunAt"].as_i64().map(|ms| format!("next {}", when_future(ms))));
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
    let r = centered(area, 84, 15);
    let title = if f.id.is_some() { "Edit routine" } else { "New routine" };
    let inner = frame_box(buf, r, t.text, t.panel, Some((title, t.text)));
    let label = |on: bool| if on { t.bold } else { t.secondary }.patch(t.panel);
    let vx = inner.x + 9;
    let vw = inner.width.saturating_sub(9);
    let at = |y: u16| Rect::new(vx, y, vw, 1);
    put(buf, inner.x, inner.y, 9, tr("Name").as_ref(), label(f.field == RoutineField::Name));
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
        "daily 09:00 · weekdays 09:00 · every 2h · 0 9 * * 1-5 · webhook"
    } else {
        &f.original_text
    };
    input_line(buf, at(inner.y + 7), inner.y + 7, &f.when, placeholder, f.field == RoutineField::When, false);
    put(buf, vx, inner.y + 8, vw, "or hourly :30 · weekly mon 09:00 · 2026-10-01 09:00 (once)", t.dim.patch(t.panel));
    put(buf, vx, inner.y + 9, vw, "every = from now on · webhook = when called", t.dim.patch(t.panel));
    let (zone_hint, zone_style) = match super::manage::complete_zone(&f.when.text, crate::store::now_ms()) {
        Some(done) => (format!("tab → {done}"), t.secondary),
        None => (format!("times in {}; end with a zone to change it", super::manage::local_zone()), t.dim),
    };
    put(buf, vx, inner.y + 10, vw, &zone_hint, zone_style.patch(t.panel));
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
    let title = format!("{} · {}", tr("Extensions"), app.host);
    let inner = frame_box(buf, r, t.text, t.panel, Some((&title, t.text)));
    let mut x = inner.x;
    for (tab, label) in TABS {
        let style = if tab == m.tab { t.btn_primary } else { t.secondary.patch(t.btn) };
        let tab_label = tr(label);
        x = put(buf, x, inner.y, 20, &format!(" {tab_label} "), style) + 1;
    }
    let search = Rect::new(x + 2, inner.y, inner.right().saturating_sub(x + 2), 1);
    let placeholder = match m.tab {
        Tab::Connectors => tr("search the MCP registry"),
        _ => tr("filter"),
    };
    field_line(buf, search, inner.y, &m.query, &placeholder, true);
    let rows = app.market_rows(m);
    let list_h = usize::from(inner.height.saturating_sub(5));
    let mut y = inner.y + 2;
    if App::market_stale(m) {
        put(buf, inner.x, y, inner.width, &tr("↵ searches the registry"), t.secondary.patch(t.panel));
        y += 1;
    }
    for (i, item) in rows.iter().enumerate().skip(window(m.cursor, list_h)).take(list_h) {
        let base = row(buf, inner, y, i == m.cursor);
        let (mark, name, detail, right): (&str, String, String, String) = match item {
            Row::Node => (
                "+",
                tr("Add Node.js").into_owned(),
                tr("Some agents need it to run. Codync adds a copy just for them.").into_owned(),
                String::new(),
            ),
            Row::Agent(b) => {
                let status = match (b["installed"] == true, b["signedIn"].as_bool()) {
                    (false, _) if b["curated"] == true => "not installed",
                    _ if b["node"] == "missing" => "needs Node.js",
                    (_, Some(true)) => "signed in",
                    (true, Some(false)) => "not signed in",
                    (true, None) => "installed",
                    (false, _) => "sets up on first use",
                };
                let status = tr(status);
                let mark = if b["signedIn"] == true || b["installed"] == true { "✓" } else { "↓" };
                (mark, s(b, "name").into(), s(b, "description").into(), status.into_owned())
            }
            Row::Installed(c) => {
                let right = match s(c, "auth") {
                    "signedOut" => tr("↵ sign in"),
                    _ if c["kind"] == "composio" => std::borrow::Cow::Borrowed("Composio"),
                    _ => tr("installed"),
                };
                let detail = c["command"].as_str().or(c["url"].as_str()).unwrap_or(s(c, "description"));
                ("✓", s(c, "name").into(), detail.into(), right.into_owned())
            }
            Row::Custom => (
                "+",
                tr("Custom connector").into_owned(),
                tr("Paste any MCP server's config").into_owned(),
                String::new(),
            ),
            Row::Registry(c) => ("↓", s(c, "title").into(), s(c, "description").into(), String::new()),
            Row::More => ("…", tr("Load more connectors").into_owned(), String::new(), String::new()),
            Row::OwnSkill(v) => ("✓", s(v, "name").into(), s(v, "description").into(), tr("installed").into_owned()),
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
        let msg = if m.loading { tr("Loading…") } else { tr("Nothing matches") };
        put(buf, inner.x, y, inner.width, &msg, t.dim.patch(t.panel));
    }
    let keys = match m.tab {
        Tab::Agents => tr("tab switch · ↵ install / sign in · ^r refresh · esc close"),
        Tab::Connectors => tr("tab switch · ↵ add / sign in · ^x remove · ^r refresh · esc"),
        Tab::Skills => tr("tab switch · ↵ details · ^x remove · esc close"),
    };
    let loading = m.loading && m.tab == Tab::Connectors && !rows.is_empty();
    let registry = tr("Loading the registry…");
    footer(buf, inner, m.error.as_deref().or(loading.then_some(registry.as_ref())), &keys);
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
        let shown = tr(d);
        put(
            buf,
            inner.x,
            y,
            inner.width,
            &truncate(shown.as_ref(), usize::from(inner.width)),
            t.secondary.patch(t.panel),
        );
        y += 2;
    }
    let signed_in =
        a.auth.as_ref().map_or_else(|| b.and_then(|b| b["signedIn"].as_bool()), |v| v["signedIn"].as_bool());
    let status = if a.busy {
        if a.auth.is_none() {
            format!("Checking with {name}… The first check can download it.")
        } else {
            tr("Working…").into_owned()
        }
    } else if signed_in != Some(true) && b.is_some_and(|b| b["node"] == "missing") {
        "Add Node.js first.".into()
    } else {
        match signed_in {
            Some(true) => "Signed in.".into(),
            Some(false) => "Not signed in yet.".into(),
            None => format!("Couldn't tell. Skip this if you've already signed in on {}.", app.host),
        }
    };
    let style = if signed_in == Some(true) { t.green } else { t.text };
    let status = tr(&status);
    put(buf, inner.x, y, inner.width, status.as_ref(), style.patch(t.panel).add_modifier(Modifier::BOLD));
    y += 1;
    if signed_in != Some(true)
        && let Some(d) = a.auth.as_ref().and_then(|v| v["detail"].as_str()).filter(|d| !d.is_empty())
    {
        let shown = tr(d);
        put(buf, inner.x, y, inner.width, &truncate(shown.as_ref(), usize::from(inner.width)), t.dim.patch(t.panel));
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
            SetupRow::Node => {
                ("↓", "Add Node.js".to_owned(), format!("{name} runs on Node.js. Codync adds its own copy."))
            }
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
        let title = tr(&title);
        let detail = tr(&detail);
        put(buf, inner.x, y, 3, icon, t.secondary.patch(base));
        let nx = put(buf, inner.x + 3, y, 30, title.as_ref(), t.bold.patch(base)) + 2;
        put(
            buf,
            nx,
            y,
            inner.right().saturating_sub(nx),
            &truncate(detail.as_ref(), usize::from(inner.right().saturating_sub(nx))),
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
    let title = tr(&f.title);
    let note = tr(&f.note);
    let inner = frame_box(buf, r, t.text, t.panel, Some((title.as_ref(), t.text)));
    let mut y = inner.y;
    put(buf, inner.x, y, inner.width, &truncate(note.as_ref(), usize::from(inner.width)), t.dim.patch(t.panel));
    y += 2;
    let label_w = 18u16;
    let vx = inner.x + label_w;
    let vw = inner.width.saturating_sub(label_w);
    let mut row_i = 0;
    if f.choices.len() > 1 {
        let on = f.cursor == 0;
        let runs = tr("Runs");
        put(buf, inner.x, y, label_w, runs.as_ref(), if on { t.bold } else { t.secondary }.patch(t.panel));
        let base = if on { t.sel } else { t.panel };
        put(buf, vx, y, vw, &format!("‹ {} ›", f.choices[f.choice]), t.text.patch(base));
        y += 2;
        row_i = 1;
    }
    for (i, input) in f.inputs.iter().enumerate() {
        let on = f.cursor == row_i + i;
        let star = if input.required { " *" } else { "" };
        let label = tr(&input.label);
        put(
            buf,
            inner.x,
            y,
            label_w - 1,
            &truncate(&format!("{label}{star}"), usize::from(label_w - 1)),
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
        "tab next · esc close"
    } else if tall {
        "⇧↵ new line · paste works"
    } else {
        "tab next field"
    };
    rput(buf, inner.right(), by, hint, t.dim.patch(t.panel));
}
