//! The bot settings form and the new/edit group chat form.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};

use super::super::app::{App, COLORS, Editor, FIELDS, Field, Form, GroupField, GroupForm, SHAPES, tilde};
use super::super::l10n::tr;
use super::super::manage::Page;
use super::super::md::{truncate, width as w};
use super::super::sheets::page_tabs;
use super::buffer::{centered, frame_box, put, put_line, restyle, rput, u};
use super::chat::composer_lines;
use super::overlay::input_line;
use super::set_cursor;
use super::style::{FACE, avatar, bot_color, face_style, theme};
use super::time::{MONTHS, local, month_day};

/// "created by Lead · Oct 20" for a bot another bot created (`chat::manage`).
fn created_by(app: &App, id: &str) -> Option<String> {
    let bot = app.bots.get(id)?;
    let creator = app.bots.get(bot.created_by.as_deref()?).map_or("a bot", |b| b.name.as_str());
    let (month, day) = month_day(local(bot.created_at).0);
    Some(format!("created by {creator} · {} {day}", MONTHS[month]))
}

pub(super) fn form_view(buf: &mut Buffer, area: Rect, app: &App, f: &Form) {
    let t = theme();
    let r = centered(area, 88, 42);
    let title = app.bots.get(&f.bot_id).map_or_else(|| f.name.text.clone(), |b| b.name.clone());
    let inner = frame_box(buf, r, t.text, t.panel, Some((&title, t.text)));
    let inner = page_tabs(buf, inner, Page::Settings);
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
    let agent_name = app.agent_name(&f.backend);
    for (i, field) in FIELDS.iter().enumerate() {
        let active = i == f.field;
        let base = if active { t.sel } else { t.panel };
        let lab = match field {
            Field::Name => "Name",
            Field::Instructions => "Instructions",
            Field::Agent => "Agent",
            Field::Model => "Model",
            Field::ModelOptions => "Options",
            Field::Fast => "Fast mode",
            Field::Fallbacks => "Fallbacks",
            Field::SelfFallbacks => "Bot edits",
            Field::Folder => "Workspace",
            Field::Approvals => "Approvals",
            Field::Notify => "Notify",
            Field::Computer => "Use computer",
            Field::Team => "Manage team",
            Field::Network => "Network",
            Field::Assistants => "Assistants",
            Field::Color => "Color",
            Field::Shape => "Shape",
            Field::Connectors => "Connectors",
            Field::Skills => "Skills",
        };
        let label = tr(lab);
        put(buf, fx, y, label_w, &label, if active { t.bold } else { t.secondary }.patch(t.panel));
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
                text_field(buf, &f.name, tr("Name").as_ref(), y);
                if let Some(by) = created_by(app, &f.bot_id)
                    && w(&f.name.text) + w(&by) + 2 < usize::from(vw)
                {
                    rput(buf, inner.right(), y, &by, t.dim.patch(base));
                }
            }
            Field::Instructions => {
                let hint = tr("e.g. Reviews PRs. Never pushes without asking.");
                text_field(buf, &f.instructions, &hint, y);
            }
            Field::Model => {
                text_field(buf, &f.model, "default", y);
                let hint = match app.models.get(&f.backend) {
                    Some(None) => "loading models…".to_owned(),
                    Some(Some(list)) if !list.is_empty() => {
                        let id = f.model.text.trim();
                        list.iter()
                            .find(|m| m.id == id)
                            .map_or_else(|| format!("‹ › {} models", list.len()), |m| format!("‹ {} ›", m.name))
                    }
                    _ => String::new(),
                };
                if !hint.is_empty() && w(&f.model.text) + w(&hint) + 2 < usize::from(vw) {
                    rput(buf, inner.right(), y, &hint, t.dim.patch(base));
                }
            }
            Field::Fast => {
                let s = if f.fast { "◉ On   ○ Off" } else { "○ On   ◉ Off" };
                // Known once the agent's model list loads, or once the bot's own sessions say.
                let saved = app.bots.get(&f.bot_id).filter(|b| b.backend == f.backend);
                let offered = match app.models.get(&f.backend) {
                    _ if saved.is_some_and(super::super::app::Bot::fast_shown) => "",
                    Some(Some(_)) if !app.fast_mode.contains(&f.backend) => "  · not offered by this agent",
                    _ => "",
                };
                put(buf, vx, y, vw, &format!("{s}{offered}"), t.text.patch(base));
            }
            Field::ModelOptions => {
                let offered = app.model_settings(&f.backend, f.model.text.trim());
                if offered.is_empty() {
                    let error = app
                        .models
                        .get(&f.backend)
                        .and_then(Option::as_ref)
                        .and_then(|models| models.iter().find(|m| m.id == f.model.text.trim()))
                        .and_then(|model| model.options_error.as_deref());
                    put(buf, vx, y, vw, error.unwrap_or("none for this model"), t.dim.patch(base));
                }
                let mut x = vx;
                for (j, o) in offered.iter().enumerate() {
                    let picked = f.model_options().and_then(|p| p.get(&o.id));
                    let chosen = picked.or(o.default.as_ref());
                    let name = chosen
                        .map(|v| o.values.iter().find(|(id, _)| id == v).map_or(v.as_str(), |(_, name)| name.as_str()));
                    let value = match (picked, name) {
                        (None, Some(name)) => format!("default · {name}"),
                        (_, Some(name)) => name.to_owned(),
                        _ => "default".to_owned(),
                    };
                    let item = format!("{} ‹{value}›", o.name);
                    let s = if active && j == f.sub { t.bold.add_modifier(Modifier::UNDERLINED) } else { t.text };
                    if x + u(w(&item)) >= inner.right() {
                        put(buf, x, y, 2, "…", t.dim.patch(base));
                        break;
                    }
                    x = put(buf, x, y, u(w(&item)), &item, s.patch(base)) + 2;
                }
            }
            Field::Agent => {
                put(buf, vx, y, vw, &format!("‹ {agent_name} ›"), t.text.patch(base));
                if let Some(note) = app.bots.get(&f.bot_id).and_then(|b| super::fallbacks::rest_note(b, &f.backend)) {
                    rput(buf, inner.right(), y, &note, t.amber.patch(base));
                }
            }
            Field::Fallbacks => {
                let bot = app.bots.get(&f.bot_id);
                let mut items: Vec<String> =
                    f.fallbacks.iter().enumerate().map(|(j, x)| super::fallbacks::step_item(app, bot, j, x)).collect();
                if f.fallbacks.len() < crate::agent::fallback::MAX_FALLBACKS {
                    items.push("+ add".into());
                }
                let mut x = vx;
                for (j, item) in items.iter().enumerate() {
                    let s = if active && j == f.sub { t.bold.add_modifier(Modifier::UNDERLINED) } else { t.text };
                    if x + u(w(item)) >= inner.right() {
                        put(buf, x, y, 2, "…", t.dim.patch(base));
                        break;
                    }
                    x = put(buf, x, y, u(w(item)), item, s.patch(base)) + 2;
                }
            }
            Field::SelfFallbacks => {
                let s = if f.self_fallbacks { "◉ On   ○ Off" } else { "○ On   ◉ Off" };
                put(buf, vx, y, vw, s, t.text.patch(base));
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
                            &if f.cwd.is_empty() {
                                tr("Personal workspace").into_owned()
                            } else {
                                tilde(&f.cwd, &app.home)
                            },
                            usize::from(vw.saturating_sub(4))
                        )
                    ),
                    t.text.patch(base),
                );
            }
            Field::Approvals => {
                let label = app.approval(f).label;
                put(buf, vx, y, vw, &format!("‹ {} ›", tr(&label)), t.text.patch(base));
            }
            Field::Computer => {
                let s = if f.computer { "◉ On   ○ Off" } else { "○ On   ◉ Off" };
                let status = super::computer_access::status(&app.screen, app.online);
                put(buf, vx, y, vw, &format!("{s}  · {status}"), t.text.patch(base));
            }
            Field::Team => {
                let s = match f.manage_team {
                    Some(true) => "◉ On   ○ Off",
                    Some(false) => "○ On   ◉ Off",
                    None => "Needs a newer Codync host",
                };
                put(buf, vx, y, vw, s, t.text.patch(base));
            }
            Field::Network | Field::Assistants => {
                let (on, applies, off_here) = if *field == Field::Network {
                    (f.offered_to_network, app.network["joined"] == true, "this computer isn't in a network")
                } else {
                    (f.reachable_by_assistants, app.network["assistants"] == true, "cloud assistants are off here")
                };
                let s = match on {
                    Some(true) => "◉ On   ○ Off",
                    Some(false) => "○ On   ◉ Off",
                    None => "Needs a newer Codync host",
                };
                let s = if on.is_some() && !applies { format!("{s}  · {}", tr(off_here)) } else { s.into() };
                put(buf, vx, y, vw, &s, t.text.patch(base));
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
    } else if f.current() == Field::Fast {
        let refusal = app.bots.get(&f.bot_id).and_then(super::super::app::Bot::fast_refusal);
        let (line, style) = match refusal {
            Some(why) => (why, t.amber),
            None => {
                (tr("Faster replies for more usage, on models that offer it (Claude Code, Codex)").into_owned(), t.dim)
            }
        };
        put(buf, inner.x, by - 1, inner.width, &line, style.patch(t.panel));
    } else if f.current() == Field::Computer {
        put(
            buf,
            inner.x,
            by - 1,
            inner.width,
            &tr("Set up on the computer: Codync → Settings → Computer access"),
            t.dim.patch(t.panel),
        );
    } else if f.current() == Field::Fallbacks {
        let line = tr("If {agent} runs out of usage or can't answer, the bot carries on with the next one. Chat, memory and workspace carry over.")
            .replace("{agent}", &agent_name);
        put(buf, inner.x, by - 1, inner.width, &line, t.dim.patch(t.panel));
    } else if f.current() == Field::SelfFallbacks {
        let line = tr("Let the bot change its fallbacks: it can set this list itself, for example when you ask it to.");
        put(buf, inner.x, by - 1, inner.width, &line, t.dim.patch(t.panel));
    } else if f.current() == Field::Approvals {
        let choice = app.approval(f);
        let line = [choice.note.unwrap_or_default(), tr(choice.detail).into_owned()].join(" ");
        put(buf, inner.x, by - 1, inner.width, line.trim(), t.dim.patch(t.panel));
    } else if f.current() == Field::Team {
        let line =
            "Let this bot create bots and change any bot's setup, its own included. The bots it creates can too.";
        put(buf, inner.x, by - 1, inner.width, line, t.dim.patch(t.panel));
    } else if f.current() == Field::Network {
        let line = tr("Offered to the network: bots on your other computers can see and message this bot.");
        put(buf, inner.x, by - 1, inner.width, &line, t.dim.patch(t.panel));
    } else if f.current() == Field::Assistants {
        let line =
            tr("Reachable by cloud assistants: ChatGPT or Claude in your Codync account can see and message this bot.");
        put(buf, inner.x, by - 1, inner.width, &line, t.dim.patch(t.panel));
    }
    let save = if f.saving {
        " … Saving "
    } else if app.custom_icons {
        " ↵ Save · ^O Icon "
    } else {
        " ↵ Save "
    };
    let x = put(buf, inner.x, by, 14, save, t.btn_primary) + 1;
    put(buf, x, by, 12, " esc Cancel ", t.text.patch(t.btn).add_modifier(Modifier::BOLD));
    let hint = match f.current() {
        Field::Folder => "↵ project folder · backspace personal · tab next field",
        Field::Connectors => "←→ move · space toggle · tab next",
        Field::ModelOptions => "←→ move · space change · tab next",
        Field::Skills => "←→ move · space toggle · i details · tab next",
        Field::Agent => "←→ change · ↵ install / sign in · tab next",
        Field::Model => "←→ pick a model · or type its id",
        Field::Fallbacks => "←→ pick · space add / model · a agent · [ ] move · x remove",
        Field::Fast
        | Field::SelfFallbacks
        | Field::Color
        | Field::Shape
        | Field::Approvals
        | Field::Notify
        | Field::Computer
        | Field::Team
        | Field::Network
        | Field::Assistants => "←→ change · tab next field",
        Field::Instructions => "⇧↵ new line · tab next field",
        Field::Name => "tab next field · ^s save",
    };
    rput(buf, inner.right(), by, hint, t.dim.patch(t.panel));
}

pub(super) fn group_view(buf: &mut Buffer, area: Rect, app: &App, g: &GroupForm) {
    let t = theme();
    let bots = app.group_candidates(&g.members);
    // A new group gets its About afterwards, in the editor.
    let about = g.group_id.is_some();
    let top = if about { 4 } else { 2 };
    let r = centered(area, 74, u(bots.len() + 8).saturating_add(top).max(16));
    let title = tr(if about { "Group chat" } else { "New group chat" });
    let inner = frame_box(buf, r, t.text, t.panel, Some((title.as_ref(), t.text)));
    let label = |on: bool| if on { t.bold } else { t.secondary }.patch(t.panel);
    let on_bots = g.field == GroupField::Bots;
    put(buf, inner.x, inner.y, 8, tr("Name").as_ref(), label(g.field == GroupField::Name));
    let default = app.group_default_name(&g.members);
    let fallback = tr("Name your group");
    let placeholder = if default.is_empty() { fallback.as_ref() } else { default.as_str() };
    input_line(
        buf,
        Rect::new(inner.x + 8, inner.y, inner.width.saturating_sub(8), 1),
        inner.y,
        &g.name,
        placeholder,
        g.field == GroupField::Name,
        false,
    );
    if about {
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
    }
    put(buf, inner.x, inner.y + top, inner.width, &format!("Bots · {}", g.members.len()), label(on_bots));
    let list_h = usize::from(inner.height.saturating_sub(top + 5));
    let off = g.cursor.saturating_sub(list_h.saturating_sub(1));
    if bots.is_empty() {
        put(
            buf,
            inner.x,
            inner.y + top + 1,
            inner.width,
            tr("No bots yet. Create a bot first.").as_ref(),
            t.dim.patch(t.panel),
        );
    }
    for (y, (i, b)) in (inner.y + top + 1..).zip(bots.iter().enumerate().skip(off).take(list_h)) {
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
            tr("Everyone answers in turn unless you @mention someone.").as_ref(),
            t.dim.patch(t.panel),
        );
    }
    let save = if g.saving {
        " … Saving "
    } else if about {
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
