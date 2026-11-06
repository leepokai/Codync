//! Overlays over the dimmed screen, and their text fields.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};

use super::super::app::{App, Editor, Overlay, tilde};
use super::super::md::{truncate, width as w};
use super::buffer::{centered, dim_all, frame_box, put, restyle, rput, u};
use super::forms::{form_view, group_view};
use super::goto::goto;
use super::help::help;
use super::new_bot::new_bot_view;
use super::pair::pair;
use super::set_cursor;
use super::style::{rgb, theme};
use super::usage::usage;

pub(super) fn overlays(f: &mut Frame<'_>, app: &mut App) {
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

pub(super) fn overlay(buf: &mut Buffer, area: Rect, app: &mut App, top: &Overlay) {
    match top {
        Overlay::Goto(g) => {
            let items = app.goto_items(g);
            goto(buf, area, app, g, &items);
        }
        Overlay::Help(filter) => help(buf, area, filter),
        Overlay::LeadAgent(scroll) => super::lead_agent::lead_agent(buf, area, *scroll),
        Overlay::Confirm(c) => {
            let t = theme();
            let r = centered(area, 62, 8);
            let inner = frame_box(buf, r, t.red, t.panel, None);
            // The title already includes a bot or device name. Translating it
            // here would also translate a name that is itself a phrase.
            let detail = super::super::l10n::tr(&c.detail);
            let note = super::super::l10n::tr(&c.note);
            let button = super::super::l10n::tr(c.button);
            let cancel = super::super::l10n::tr("Cancel");
            put(buf, inner.x, inner.y, inner.width, &c.title, t.red.patch(t.panel).add_modifier(Modifier::BOLD));
            put(buf, inner.x, inner.y + 1, inner.width, detail.as_ref(), t.text.patch(t.panel));
            put(buf, inner.x, inner.y + 2, inner.width, note.as_ref(), t.secondary.patch(t.panel));
            let danger = if t.color {
                Style::default().fg(t.on_color).bg(rgb(0xF0A7A7)).add_modifier(Modifier::BOLD)
            } else {
                t.sel
            };
            let x = put(buf, inner.x, inner.y + 4, 30, &format!(" ↵ {button} "), danger) + 1;
            put(buf, x, inner.y + 4, 14, &format!(" esc {cancel} "), t.text.patch(t.btn).add_modifier(Modifier::BOLD));
        }
        Overlay::NewBot(n) => new_bot_view(buf, area, n),
        Overlay::Folder(p) => {
            let t = theme();
            let matches = App::folder_matches(p);
            let r = centered(area, 74, 22);
            let inner = frame_box(buf, r, t.text, t.panel, Some(("Folder", t.text)));
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
        Overlay::Usage(scroll) => usage(buf, area, app, *scroll),
        Overlay::Pair(code) => pair(buf, area, code),
        Overlay::Memory(m) => super::super::sheets::memory(buf, area, app, m),
        Overlay::BotChat(c) => super::bot_chat::bot_chat(buf, area, app, c),
        Overlay::Routines(l) => super::super::sheets::routines(buf, area, app, l),
        Overlay::Routine(f) => super::super::sheets::routine(buf, area, f),
        Overlay::Library(l) => super::super::sheets::library(buf, area, app, l),
        Overlay::Gear(g) => super::super::sheets::gear(buf, area, app, g),
        Overlay::Market(m) => super::super::sheets::market(buf, area, app, m),
        Overlay::Agent(a) => super::super::sheets::agent(buf, area, app, a),
        Overlay::Fields(f) => super::super::sheets::fields(buf, area, f),
        Overlay::Skill(s) => super::skill::skill(buf, area, app, s),
        Overlay::Analytics(s) => super::analytics::analytics(buf, area, s),
        Overlay::Network(n) => super::network::network(buf, area, n),
    }
}

/// A one-line text field. A search shows "/ " first, unless the caller drew its own prefix
/// (the folder path, with an empty placeholder).
pub(in crate::tui) fn field_line(buf: &mut Buffer, r: Rect, y: u16, e: &Editor, placeholder: &str, active: bool) {
    input_line(buf, r, y, e, placeholder, active, !placeholder.is_empty());
}

pub(in crate::tui) fn input_line(
    buf: &mut Buffer,
    r: Rect,
    y: u16,
    e: &Editor,
    placeholder: &str,
    active: bool,
    search: bool,
) {
    let t = theme();
    let x = if search { put(buf, r.x, y, 2, "/ ", t.dim.patch(t.panel)) } else { r.x };
    if e.text.is_empty() {
        if active {
            set_cursor(Position::new(x, y));
        }
        let hint = super::super::l10n::tr(placeholder);
        put(buf, x, y, r.right().saturating_sub(x), hint.as_ref(), t.dim.patch(t.panel));
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
