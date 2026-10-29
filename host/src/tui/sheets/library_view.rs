//! A bot's Library tab: its artifacts and the files shared in its chat, newest first, under Today /
//! Yesterday / This week / Earlier.

use super::{App, Buffer, Rect, centered, footer, frame_box, name_of, page_tabs, put, row, rput, theme, u, window};
use crate::tui::l10n::tr;
use crate::tui::manage::{Library, LibraryItem, Page};
use crate::tui::md::truncate;
use crate::tui::view::{file_size, when};
use chrono::TimeZone as _;

pub fn draw(buf: &mut Buffer, area: Rect, app: &App, l: &Library) {
    let t = theme();
    let r = centered(area, 90, 28);
    let title = name_of(app, &l.bot);
    let inner = frame_box(buf, r, t.text, t.panel, Some((&title, t.text)));
    let inner = page_tabs(buf, inner, Page::Library);
    match l.items.as_deref() {
        None => {
            put(buf, inner.x, inner.y, inner.width, &tr("Loading…"), t.dim.patch(t.panel));
        }
        Some([]) => {
            put(buf, inner.x, inner.y, inner.width, &tr("Library is empty"), t.bold.patch(t.panel));
            let note = format!("Artifacts {title} makes and files shared in this chat show here.");
            put(buf, inner.x, inner.y + 1, inner.width, &tr(&note), t.secondary.patch(t.panel));
        }
        Some(items) => {
            let now = crate::store::now_ms();
            // Rows and the headings between them: `None` is a heading.
            let mut lines: Vec<(Option<usize>, &str)> = vec![];
            for (i, item) in items.iter().enumerate() {
                let heading = section(item.at(), now);
                if lines.last().is_none_or(|(_, h)| *h != heading) {
                    lines.push((None, heading));
                }
                lines.push((Some(i), heading));
            }
            let at = lines.iter().position(|(i, _)| *i == Some(l.cursor)).unwrap_or(0);
            let h = usize::from(inner.height.saturating_sub(3));
            let room = usize::from(inner.width.saturating_sub(10));
            for (y, (i, heading)) in (inner.y..).zip(lines.iter().skip(window(at, h)).take(h)) {
                let Some(i) = *i else {
                    put(buf, inner.x, y, inner.width, &tr(heading), t.dim.patch(t.panel));
                    continue;
                };
                let item = &items[i];
                let base = row(buf, inner, y, i == l.cursor);
                put(buf, inner.x, y, u(room) + 2, &truncate(&label(item), room + 2), t.text.patch(base));
                rput(buf, inner.right(), y, &when(item.at()), t.dim.patch(base));
            }
        }
    }
    footer(buf, inner, None, "↵ open an artifact in the browser · f save to Downloads · r rename · esc");
}

/// A row as the chat shows it: `▤ <title>` for an artifact, `▤ name · size` for a file.
fn label(item: &LibraryItem) -> String {
    match item {
        LibraryItem::Artifact(a) => format!("▤ {}", a.title),
        LibraryItem::File { file, .. } => format!("▤ {} · {}", file.name, file_size(file.size)),
        LibraryItem::Attachment { name, size, .. } => format!("▤ {name} · {}", file_size(*size)),
    }
}

/// Today / Yesterday / This week / Earlier: the local day it was published or shared, from `now`.
fn section(at: i64, now: i64) -> &'static str {
    let day = |ms: i64| chrono::Local.timestamp_millis_opt(ms).single().map(|t| t.date_naive());
    match day(now).zip(day(at)).map(|(today, d)| (today - d).num_days()) {
        Some(..=0) => "Today",
        Some(1) => "Yesterday",
        Some(2..=6) => "This week",
        _ => "Earlier",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_group_by_the_local_day() {
        let noon = |days_back: u64| {
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 24).expect("a date") - chrono::Days::new(days_back);
            let time = date.and_hms_opt(12, 0, 0).expect("noon");
            chrono::Local.from_local_datetime(&time).earliest().expect("noon exists").timestamp_millis()
        };
        let now = noon(0);
        assert_eq!(section(now - 3_600_000, now), "Today");
        assert_eq!(section(noon(1), now), "Yesterday");
        assert_eq!(section(noon(6), now), "This week");
        assert_eq!(section(noon(7), now), "Earlier");
        assert_eq!(section(0, now), "Earlier");
    }

    #[test]
    fn rows_read_like_the_chat() {
        let attachment = LibraryItem::Attachment { id: "u".into(), name: "a.png".into(), size: 2048, at: 0 };
        assert_eq!(label(&attachment), "▤ a.png · 2 KB");
        let file =
            crate::chat::files::SharedFile { id: "f".into(), name: "b.csv".into(), size: 9, sha256: String::new() };
        assert_eq!(label(&LibraryItem::File { entry: "e".into(), file, at: 0 }), "▤ b.csv · 9 B");
    }
}
