//! A bot's memory: about you, this bot's own, its dreams and Claude Code's own memory where it is kept on.

use super::{App, Buffer, Rect, centered, footer, frame_box, name_of, page_tabs, put, row, s, theme, u, window};
use crate::tui::app::OwnMemory;
use crate::tui::l10n::tr;
use crate::tui::manage::{MemoryMode, MemorySheet, Page, Scroll, Section};
use crate::tui::md::{truncate, wrap};
use crate::tui::view::when;
use ratatui::text::Span;
use ratatui::widgets::{Paragraph, Widget};
use serde_json::Value;

pub fn draw(buf: &mut Buffer, area: Rect, app: &App, memory: &MemorySheet) {
    let palette = theme();
    let rect = centered(area, 100, 32);
    let title = name_of(app, &memory.bot);
    let inner = frame_box(buf, rect, palette.text, palette.panel, Some((&title, palette.text)));
    let inner = page_tabs(buf, inner, Page::Memory);
    tabs(buf, inner, memory);
    let body = Rect::new(inner.x, inner.y + 2, inner.width, inner.height.saturating_sub(5));
    let keys = match &memory.mode {
        MemoryMode::File { detail, .. } => {
            note(buf, body, detail.as_ref(), &memory.scroll);
            "↑↓ scroll · Esc back"
        }
        MemoryMode::Browse => match memory.section {
            Section::Notes => {
                notes(buf, body, memory);
                "←→ section · ↑↓ move · ↵ open · Esc close · the bot writes notes"
            }
            Section::Yours | Section::Bot => {
                entries(buf, body, app, memory);
                "←→ section · ↑↓ move · s shared · Esc close · tell the bot to change it"
            }
            Section::Dreams => {
                dreams(buf, body, memory);
                "←→ section · ↑↓ move · r refresh · Esc close · read-only"
            }
            Section::Harness => {
                harness(buf, body, memory);
                "←→ section · ↑↓ scroll · r refresh · m turn off · Esc close · read-only"
            }
        },
    };
    footer(buf, inner, memory.error.as_deref(), keys);
}

fn tabs(buf: &mut Buffer, inner: Rect, memory: &MemorySheet) {
    let palette = theme();
    let mut x = inner.x;
    for section in memory.sections() {
        let mut label = tr(section.label()).into_owned();
        let layer = match section {
            Section::Yours => Some("yours"),
            Section::Bot => Some("bot"),
            _ => None,
        };
        if let (Some(layer), Some(view)) = (layer, &memory.view) {
            label = format!("{label} {}/{}", view[layer]["used"], view[layer]["cap"]);
        }
        let style = if section == memory.section {
            palette.text.patch(palette.sel)
        } else {
            palette.secondary.patch(palette.panel)
        };
        x = put(buf, x, inner.y, inner.right().saturating_sub(x), &format!(" {label} "), style) + 1;
    }
    if memory.busy {
        put(buf, x, inner.y, inner.right().saturating_sub(x), &tr("Loading…"), palette.dim.patch(palette.panel));
    }
}

fn entries(buf: &mut Buffer, body: Rect, app: &App, memory: &MemorySheet) {
    let palette = theme();
    let Some(view) = &memory.view else { return };
    let bot = app.bots.get(&memory.bot);
    let harness = app.harness_name(&memory.bot);
    let note = match (memory.section, view["shared"] == true, bot.and_then(|b| b.harness_memory)) {
        (Section::Yours, true, _) => tr("Shared by your bots · s stops this bot using it").into_owned(),
        (Section::Yours, false, _) => tr("This bot doesn't use what your bots share · s turns it on").into_owned(),
        (_, _, Some(OwnMemory::Switch(true))) => {
            tr("What this bot learned about you · {harness}'s own memory on · m turns it off")
                .replace("{harness}", &harness)
        }
        (_, _, Some(OwnMemory::Switch(false))) => {
            tr("What this bot learned about you · {harness}'s own memory off · m turns it on")
                .replace("{harness}", &harness)
        }
        (_, _, Some(OwnMemory::Shared)) => {
            tr("What this bot learned about you · {harness}'s own memory: shared by every {harness} bot")
                .replace("{harness}", &harness)
        }
        (_, _, None) => tr("What this bot learned about you").into_owned(),
    };
    let top = caption(buf, body, &note);
    let rows = memory.rows();
    if rows.is_empty() {
        let empty = tr(if memory.section == Section::Yours {
            "Nothing shared yet. Facts every bot should know (your name, time zone, people) go here."
        } else {
            "Nothing yet. What this bot learns about you shows up here."
        });
        put(buf, body.x, body.y + top, body.width, &empty, palette.secondary);
        return;
    }
    let height = usize::from(body.height.saturating_sub(top)) / 2;
    for (position, (index, e)) in rows.iter().enumerate().skip(window(memory.cursor, height)).take(height).enumerate() {
        let y = body.y + top + u(position * 2);
        let base = row(buf, body, y, index == memory.cursor);
        let archived = e["state"] == "archived";
        let text = format!("{}{}", if e["pinned"] == true { "● " } else { "" }, s(e, "text"));
        let style = if archived { palette.dim } else { palette.text };
        put(buf, body.x, y, body.width, &text, style.patch(base));
        let detail = format!(
            "{} · {}{}",
            author(app, e),
            when(e["updatedAt"].as_i64().unwrap_or(0)),
            if archived { " · archived" } else { "" }
        );
        put(buf, body.x, y + 1, body.width, &detail, palette.dim.patch(palette.panel));
    }
}

fn author(app: &App, e: &Value) -> String {
    match s(e, "author") {
        "user" => "You wrote it".into(),
        "bot" => format!("{} remembered it", name_of(app, s(e, "authorBot"))),
        "dream" => "Dreaming".into(),
        _ => "Moved from the old memory".into(),
    }
}

/// What dreaming did, read-only, under what it does (the other apps' info sheet, short).
fn dreams(buf: &mut Buffer, body: Rect, memory: &MemorySheet) {
    let palette = theme();
    let about = "Between chats, and once memory is nearly full, the bot tidies its memory from what you typed, away \
                 from the chat: + added, − taken out or moved word for word into a note, which keeps one line per \
                 topic pointing to it. Pasted text and its replies never count. A record: to change a memory, tell \
                 the bot.";
    let top = caption(buf, body, about);
    let rows = memory.rows();
    if rows.is_empty() {
        let empty = tr("No dreams yet.");
        put(buf, body.x, body.y + top, body.width, &empty, palette.secondary);
        return;
    }
    let height = usize::from(body.height.saturating_sub(top)) / 2;
    for (position, (index, d)) in rows.iter().enumerate().skip(window(memory.cursor, height)).take(height).enumerate() {
        let y = body.y + top + u(position * 2);
        let base = row(buf, body, y, index == memory.cursor);
        let count = |k: &str| d[k].as_array().map_or(0, Vec::len);
        let head = format!(
            "{} · added {} · taken out {}{}",
            when(d["at"].as_i64().unwrap_or(0)),
            count("added"),
            count("archived"),
            if d["undone"] == true { " · undone" } else { "" }
        );
        put(buf, body.x, y, body.width, &head, palette.text.patch(base));
        let mut x = body.x;
        for (sign, key, style) in [("+", "added", palette.green), ("−", "archived", palette.red)] {
            for c in d[key].as_array().into_iter().flatten() {
                let change = format!("{sign} {}  ", s(c, "text"));
                x = put(buf, x, y + 1, body.right().saturating_sub(x), &change, style.patch(palette.panel));
            }
        }
        for f in d["files"].as_array().into_iter().flatten() {
            let note = format!("Added to {}  ", s(f, "name"));
            x = put(buf, x, y + 1, body.right().saturating_sub(x), &note, palette.secondary.patch(palette.panel));
        }
    }
}

/// `text` wrapped whole at the top of `body`, muted; returns how far down what goes under it starts.
fn caption(buf: &mut Buffer, body: Rect, text: &str) -> u16 {
    let palette = theme();
    let muted = palette.secondary.patch(palette.panel);
    let lines = wrap(&[Span::styled(text.to_owned(), muted)], usize::from(body.width), &[], &[]);
    let top = u(lines.len()).min(body.height) + 1;
    Paragraph::new(lines).style(muted).render(Rect::new(body.x, body.y, body.width, top - 1), buf);
    top
}

/// Both layers' long-form notes: name, where it lives, description, size and date, marked past a cap.
fn notes(buf: &mut Buffer, body: Rect, memory: &MemorySheet) {
    let palette = theme();
    let Some(view) = &memory.view else { return };
    let caption = tr("Longer notes, one topic each. The bot reads one when its topic comes up.");
    put(buf, body.x, body.y, body.width, &caption, palette.secondary.patch(palette.panel));
    let rows = memory.rows();
    if rows.is_empty() {
        put(buf, body.x, body.y + 2, body.width, "No notes yet.", palette.secondary);
        return;
    }
    let cap = view["files"]["cap"].as_u64().unwrap_or(2_000);
    let height = usize::from(body.height.saturating_sub(2)) / 2;
    for (position, (index, f)) in rows.iter().enumerate().skip(window(memory.cursor, height)).take(height).enumerate() {
        let y = body.y + 2 + u(position * 2);
        let base = row(buf, body, y, index == memory.cursor);
        let layer = if f["shared"] == true { "shared" } else { "this bot" };
        put(buf, body.x, y, body.width, &format!("{} · {layer}", s(f, "name")), palette.text.patch(base));
        let tokens = f["tokens"].as_u64().unwrap_or(0);
        let over = if tokens > cap { format!(", over the {cap} cap") } else { String::new() };
        let detail =
            format!("{} · {tokens} tokens{over} · {}", s(f, "description"), when(f["updatedAt"].as_i64().unwrap_or(0)));
        put(buf, body.x, y + 1, body.width, &detail, palette.dim.patch(palette.panel));
    }
}

/// One note, to read.
fn note(buf: &mut Buffer, body: Rect, detail: Option<&Value>, scroll: &Scroll) {
    let Some(detail) = detail else {
        let palette = theme();
        put(buf, body.x, body.y, body.width, tr("Loading…").as_ref(), palette.dim.patch(palette.panel));
        return;
    };
    read(buf, body, s(detail, "path"), detail["content"].as_str().unwrap_or("Deleted."), scroll);
}

fn harness(buf: &mut Buffer, body: Rect, memory: &MemorySheet) {
    let Some(project) = memory.view.as_ref().map(|v| &v["projectMemory"]) else { return };
    let text = s(project, "text");
    read(buf, body, s(project, "path"), text, &memory.scroll);
    if text.is_empty() {
        let empty = "Nothing yet. Claude Code's notes show up here once it saves some.";
        put(buf, body.x, body.y + 2, body.width, empty, theme().secondary);
    }
}

/// `path` over its `text`, scrolled by wrapped row; the draw tells `scroll` how far it goes and a page's height.
fn read(buf: &mut Buffer, body: Rect, path: &str, text: &str, scroll: &Scroll) {
    let palette = theme();
    put(buf, body.x, body.y, body.width, &truncate(path, usize::from(body.width)), palette.dim.patch(palette.panel));
    let rect = Rect::new(body.x, body.y + 2, body.width, body.height.saturating_sub(2));
    let lines = wrap(&[Span::raw(text.to_owned())], usize::from(rect.width), &[], &[]);
    let height = usize::from(rect.height);
    scroll.max.set(lines.len().saturating_sub(height));
    scroll.page.set(height);
    Paragraph::new(lines)
        .scroll((u(scroll.pos.min(scroll.max.get())), 0))
        .style(palette.text.patch(palette.panel))
        .render(rect, buf);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_scrolls_to_its_last_wrapped_row() {
        let area = Rect::new(0, 0, 20, 6);
        let detail = serde_json::json!({"path": "p", "content": format!("{}end", "word ".repeat(30))});
        let mut scroll = Scroll::default();
        note(&mut Buffer::empty(area), area, Some(&detail), &scroll);
        assert_eq!(scroll.max.get(), 4, "one line, 8 rows wrapped, 4 shown under the path");
        scroll.pos = scroll.max.get();
        let mut buf = Buffer::empty(area);
        note(&mut buf, area, Some(&detail), &scroll);
        let last = (0..area.width).map(|x| buf[(x, area.bottom() - 1)].symbol()).collect::<String>();
        assert!(last.trim_end().ends_with("end"), "{last}");
    }

    #[test]
    fn dreams_paragraph_wraps_whole_and_the_log_starts_under_it() {
        // 80 and 60 columns leave the sheet's body 74 and 54 wide.
        for (width, rows) in [(74, 5), (54, 6)] {
            let area = Rect::new(0, 0, width, 12);
            let mut buf = Buffer::empty(area);
            let memory = MemorySheet {
                bot: String::new(),
                view: None,
                section: Section::Dreams,
                cursor: 0,
                scroll: Scroll::default(),
                mode: MemoryMode::Browse,
                error: None,
                busy: false,
            };
            dreams(&mut buf, area, &memory);
            let line = |y: u16| (0..width).map(|x| buf[(x, y)].symbol()).collect::<String>();
            assert!(line(rows - 1).trim_end().ends_with("tell the bot."), "{width}: {}", line(rows - 1));
            assert_eq!(line(rows + 1).trim_end(), "No dreams yet.", "{width}");
        }
    }

    #[test]
    fn a_shared_harness_note_wraps_whole_and_the_entries_start_under_it() {
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(crate::tui::net::Client::new("http://127.0.0.1:1", None), tx, String::new());
        let bot = serde_json::json!({"id": "b", "name": "B", "backend": "hermes", "harnessMemoryShared": true});
        app.on_msg(crate::tui::app::Msg::Event(serde_json::json!({"type": "bot", "bot": bot})));
        // An 80-column terminal leaves the sheet's body 74 wide.
        let area = Rect::new(0, 0, 74, 12);
        let mut buf = Buffer::empty(area);
        let memory = MemorySheet {
            bot: "b".into(),
            view: Some(serde_json::json!({"bot": {"entries": []}})),
            section: Section::Bot,
            cursor: 0,
            scroll: Scroll::default(),
            mode: MemoryMode::Browse,
            error: None,
            busy: false,
        };
        entries(&mut buf, area, &app, &memory);
        let line = |y: u16| (0..area.width).map(|x| buf[(x, y)].symbol()).collect::<String>();
        let note = format!("{} {}", line(0).trim_end(), line(1).trim_end());
        assert!(note.ends_with("shared by every hermes bot"), "{note}");
        assert_eq!(line(3).trim_end(), "Nothing yet. What this bot learns about you shows up here.");
    }
}
