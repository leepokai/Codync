//! The new/edit bot and group chat forms.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};

use super::super::app::{App, COLORS, Editor, FIELDS, Field, Form, GroupField, GroupForm, SHAPES, tilde};
use super::super::md::{truncate, width as w};
use super::buffer::{centered, frame_box, put, put_line, restyle, rput, u};
use super::chat::composer_lines;
use super::overlay::input_line;
use super::set_cursor;
use super::style::{FACE, avatar, bot_color, face_style, theme};

pub(super) fn form_view(buf: &mut Buffer, area: Rect, app: &App, f: &Form) {
    let t = theme();
    let r = centered(area, 88, 30);
    let title = match &f.bot_id {
        Some(_) => format!("Edit {}", f.name.text),
        None => "New bot · 2/2 name and looks".into(),
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
            Field::Folder => "Workspace",
            Field::Approvals => "Approvals",
            Field::Notify => "Notify",
            Field::Computer => "Use computer",
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
            Field::Name => {
                // A new bot may stay unnamed: the host names it after its first conversations.
                let placeholder = if f.bot_id.is_none() { "Named after a few chats" } else { "Name" };
                text_field(buf, &f.name, placeholder, y);
            }
            Field::Instructions => {
                text_field(buf, &f.instructions, "e.g. Reviews PRs. Never pushes without asking.", y);
            }
            Field::Model => {
                text_field(buf, &f.model, "default", y);
                let hint = match app.models.get(&f.backend) {
                    Some(None) => "loading models…".to_owned(),
                    Some(Some(list)) if !list.is_empty() => {
                        let id = f.model.text.trim();
                        list.iter()
                            .find(|(m, _)| m == id)
                            .map_or_else(|| format!("‹ › {} models", list.len()), |(_, name)| format!("‹ {name} ›"))
                    }
                    _ => String::new(),
                };
                if !hint.is_empty() && w(&f.model.text) + w(&hint) + 2 < usize::from(vw) {
                    rput(buf, inner.right(), y, &hint, t.dim.patch(base));
                }
            }
            Field::Agent => {
                put(buf, vx, y, vw, &format!("‹ {agent_name} ›"), t.text.patch(base));
            }
            Field::Folder => {
                put(
                    buf,
                    vx,
                    y,
                    vw,
                    &format!(
                        "{}  ▸",
                        truncate(
                            &if f.cwd.is_empty() { "Personal workspace".to_owned() } else { tilde(&f.cwd, &app.home) },
                            usize::from(vw.saturating_sub(4))
                        )
                    ),
                    t.text.patch(base),
                );
            }
            Field::Approvals => {
                let s = if f.auto { "○ Ask   ◉ Auto (approve every request)" } else { "◉ Ask   ○ Auto" };
                put(buf, vx, y, vw, s, t.text.patch(base));
            }
            Field::Computer => {
                let s = if f.computer { "◉ On   ○ Off" } else { "○ On   ◉ Off" };
                let status = super::computer_access::status(&app.screen, app.online);
                put(buf, vx, y, vw, &format!("{s}  · {status}"), t.text.patch(base));
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
    } else if f.current() == Field::Computer {
        put(
            buf,
            inner.x,
            by - 1,
            inner.width,
            "Set up on the computer: Codync → Settings → Computer access",
            t.dim.patch(t.panel),
        );
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
        Field::Folder => "↵ project folder · backspace personal · tab next field",
        Field::Connectors | Field::Skills => "←→ move · space toggle · tab next",
        Field::Agent => "←→ change · ↵ install / sign in · tab next",
        Field::Model => "←→ pick a model · or type its id",
        Field::Color | Field::Shape | Field::Approvals | Field::Notify | Field::Computer => {
            "←→ change · tab next field"
        }
        Field::Instructions => "⇧↵ new line · tab next field",
        Field::Name => "tab next field · ^s save",
    };
    rput(buf, inner.right(), by, hint, t.dim.patch(t.panel));
}

pub(super) fn group_view(buf: &mut Buffer, area: Rect, app: &App, g: &GroupForm) {
    let t = theme();
    let bots = app.group_candidates(&g.members);
    let r = centered(area, 74, u(bots.len() + 12).max(16));
    let title = if g.group_id.is_some() { "Group chat" } else { "New group chat" };
    let inner = frame_box(buf, r, t.text, t.panel, Some((title, t.text)));
    let label = |on: bool| if on { t.bold } else { t.secondary }.patch(t.panel);
    let on_bots = g.field == GroupField::Bots;
    put(buf, inner.x, inner.y, 8, "Name", label(g.field == GroupField::Name));
    let default = app.group_default_name(&g.members);
    let placeholder = if default.is_empty() { "Name" } else { default.as_str() };
    input_line(
        buf,
        Rect::new(inner.x + 8, inner.y, inner.width.saturating_sub(8), 1),
        inner.y,
        &g.name,
        placeholder,
        g.field == GroupField::Name,
        false,
    );
    put(buf, inner.x, inner.y + 2, 8, "About", label(g.field == GroupField::About));
    input_line(
        buf,
        Rect::new(inner.x + 8, inner.y + 2, inner.width.saturating_sub(8), 1),
        inner.y + 2,
        &g.about,
        "What this group works on (optional)",
        g.field == GroupField::About,
        false,
    );
    put(buf, inner.x, inner.y + 4, inner.width, &format!("Bots · {}", g.members.len()), label(on_bots));
    let list_h = usize::from(inner.height.saturating_sub(9));
    let off = g.cursor.saturating_sub(list_h.saturating_sub(1));
    if bots.is_empty() {
        put(buf, inner.x, inner.y + 5, inner.width, "No bots yet. n creates one.", t.dim.patch(t.panel));
    }
    for (y, (i, b)) in (inner.y + 5..).zip(bots.iter().enumerate().skip(off).take(list_h)) {
        let sel = on_bots && i == g.cursor;
        let base = if sel { t.sel } else { t.panel };
        if sel {
            restyle(buf, Rect::new(inner.x - 1, y, inner.width + 2, 1), t.sel);
        }
        let on = g.members.contains(&b.id);
        put(buf, inner.x, y, 2, if on { "☑" } else { "☐" }, if on { t.bold } else { t.dim }.patch(base));
        put_line(buf, inner.x + 2, y, 2, &avatar(app, b));
        put(buf, inner.x + 5, y, inner.width / 2, &b.name, if sel { t.bold } else { t.text }.patch(base));
        let folder = if b.managed_workspace {
            "personal".to_owned()
        } else {
            std::path::Path::new(&b.cwd).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()
        };
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
        if on_bots { "↑↓ move · space pick · tab name" } else { "tab next field" },
        t.dim.patch(t.panel),
    );
}
