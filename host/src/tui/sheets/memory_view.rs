//! Memory browsing, editing and portable backups.

use super::{App, Buffer, Rect, centered, footer, frame_box, input_line, name_of, put, row, s, theme, u, window};
use crate::tui::manage::{MemoryMode, MemorySheet};
use ratatui::widgets::{Paragraph, Widget, Wrap};

pub fn draw(buf: &mut Buffer, area: Rect, app: &App, memory: &MemorySheet) {
    let palette = theme();
    let rect = centered(area, 100, 30);
    let title = format!("{} · Memory", name_of(app, &memory.bot));
    let inner = frame_box(buf, rect, palette.text, palette.panel, Some((&title, palette.text)));
    let body = Rect::new(inner.x, inner.y + 1, inner.width, inner.height.saturating_sub(4));
    let keys = match &memory.mode {
        MemoryMode::Browse => {
            browse(buf, body, memory);
            "↑↓ move · / search · f filter · n add · ↵ edit · p pin · h history · v review · x forget"
        }
        MemoryMode::Search(editor) => {
            input_line(buf, body, body.y, editor, "Search memories", true, false);
            "Enter search · Esc cancel"
        }
        MemoryMode::Edit(form) => {
            for (index, label) in
                ["Title", "Memory", "Scope: personal / project / global", "Category", "Topic (optional)"]
                    .iter()
                    .enumerate()
            {
                let y = body.y + u(index * 3);
                put(buf, body.x, y, body.width, label, palette.secondary.patch(palette.panel));
                input_line(buf, body, y + 1, &form.fields[index], "", form.focus == index, false);
            }
            "Tab next field · Shift+Enter newline · Ctrl+S save · Esc cancel"
        }
        MemoryMode::Import(editor) => {
            put(
                buf,
                body.x,
                body.y,
                body.width,
                "Paste Engram JSON. Import merges; newer changes win. Backup saved first.",
                palette.secondary,
            );
            input_line(buf, body, body.y + 2, editor, "JSON backup", true, false);
            "Ctrl+S import · Esc cancel"
        }
        MemoryMode::Detail { text, scroll, .. } | MemoryMode::Export { text, scroll } => {
            Paragraph::new(text.as_str())
                .wrap(Wrap { trim: false })
                .scroll((u(*scroll), 0))
                .style(palette.text.patch(palette.panel))
                .render(body, buf);
            "↑↓ scroll · c copy · o older versions · Esc back"
        }
    };
    if matches!(memory.mode, MemoryMode::Browse) {
        put(
            buf,
            inner.x,
            inner.bottom().saturating_sub(3),
            inner.width,
            "[ ] pages · r refresh · E export · I import · D clear · Esc close",
            palette.dim.patch(palette.panel),
        );
    }
    footer(buf, inner, memory.error.as_deref(), keys);
}

fn browse(buf: &mut Buffer, body: Rect, memory: &MemorySheet) {
    let palette = theme();
    let filter = ["All", "About you", "Pinned", "Needs review"][memory.filter];
    let heading = format!(
        "{filter} · {} memories · {}{}",
        memory.total,
        memory.query,
        if memory.busy { " · Loading…" } else { "" }
    );
    put(buf, body.x, body.y, body.width, &heading, palette.secondary.patch(palette.panel));
    let Some(facts) = &memory.facts else {
        return;
    };
    if facts.is_empty() {
        put(
            buf,
            body.x,
            body.y + 2,
            body.width,
            "No matching memories. Important details are remembered as you chat.",
            palette.secondary,
        );
        return;
    }
    let height = usize::from(body.height.saturating_sub(2)) / 2;
    for (position, (index, fact)) in
        facts.iter().enumerate().skip(window(memory.cursor, height)).take(height).enumerate()
    {
        let y = body.y + 2 + u(position * 2);
        let base = row(buf, body, y, index == memory.cursor);
        let title = format!(
            "{}{} · {} revisions",
            if fact["pinned"] == true { "● " } else { "" },
            s(fact, "title"),
            fact["revisionCount"]
        );
        put(buf, body.x, y, body.width, &title, palette.text.patch(base));
        put(buf, body.x, y + 1, body.width, s(fact, "content"), palette.secondary.patch(palette.panel));
    }
}
