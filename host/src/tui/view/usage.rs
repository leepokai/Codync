//! The usage sheet.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use serde_json::Value;

use super::super::app::App;
use super::super::l10n::tr;
use super::super::md::truncate;
use super::buffer::{centered, frame_box, put, rput, u};
use super::filled;
use super::style::theme;
use super::time::{DAYS, MONTHS, clock, local, month_day, now_ms};

/// One line of the sheet's list.
enum Line<'a> {
    Provider(&'a str),
    Window(&'a Value),
    Gap,
}

/// Each provider's name, its windows and a gap.
fn lines(providers: &[Value]) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    for p in providers {
        lines.push(Line::Provider(p["name"].as_str().unwrap_or_default()));
        lines.extend(p["windows"].as_array().into_iter().flatten().map(Line::Window));
        lines.push(Line::Gap);
    }
    lines
}

/// The sheet on a screen `area`, and how many list lines fit in it.
fn sheet(area: Rect, rows: usize) -> (Rect, usize) {
    let r = centered(area, 78, u(rows + 5).max(8));
    // The frame, a blank line on top and the footer.
    (r, usize::from(r.height.saturating_sub(4)))
}

/// The agents this computer's chooser lists. A host older than the chooser sends none.
pub(in crate::tui) fn usage_services(app: &App) -> &[Value] {
    app.usage["services"].as_array().map_or(&[], Vec::as_slice)
}

/// How far the list scrolls on a screen `height` rows tall: more providers than fit scroll with ↑↓.
pub(in crate::tui) fn usage_scroll_max(app: &App, height: u16) -> usize {
    let providers = app.usage["providers"].as_array().cloned().unwrap_or_default();
    let rows = lines(&providers).len();
    let (_, fits) = sheet(Rect::new(0, 0, 80, height), rows);
    rows.saturating_sub(fits)
}

pub(super) fn usage(buf: &mut Buffer, area: Rect, app: &App, scroll: usize) {
    let t = theme();
    let providers = app.usage["providers"].as_array().cloned().unwrap_or_default();
    let lines = lines(&providers);
    let (r, fits) = sheet(area, lines.len());
    let inner = frame_box(buf, r, t.text, t.panel, Some((tr("Usage").as_ref(), t.text)));
    let top = inner.y + 1;
    if providers.is_empty() {
        put(buf, inner.x, top, inner.width, "No usage numbers yet.", t.secondary.patch(t.panel));
    }
    let scroll = scroll.min(lines.len().saturating_sub(fits));
    for (y, line) in (top..).zip(lines.iter().skip(scroll).take(fits)) {
        match line {
            Line::Provider(name) => {
                put(buf, inner.x, y, inner.width, name, t.bold.patch(t.panel));
            }
            Line::Window(win) => window(buf, inner, y, win),
            Line::Gap => {}
        }
    }
    let choose = if usage_services(app).is_empty() { "" } else { "s choose · " };
    let scrolls = if lines.len() > fits { "↑↓ scroll · " } else { "" };
    let footer = format!("Read from the agents on this computer. {choose}{scrolls}esc close");
    put(buf, inner.x, inner.bottom() - 1, inner.width, &footer, t.dim.patch(t.panel));
}

/// Which agents' limits show on this computer, and in what order.
pub(super) fn usage_chooser(buf: &mut Buffer, area: Rect, app: &App, row: usize) {
    let t = theme();
    let services = usage_services(app);
    let providers: &[Value] = app.usage["providers"].as_array().map_or(&[], Vec::as_slice);
    let r = centered(area, 60, u(services.len() + 5).max(8));
    let inner = frame_box(buf, r, t.text, t.panel, Some((tr("Show usage for").as_ref(), t.text)));
    let fits = usize::from(inner.height.saturating_sub(3));
    let skip = row.saturating_sub(fits.saturating_sub(1));
    for (i, (y, service)) in (inner.y + 1..).zip(services.iter().skip(skip).take(fits)).enumerate() {
        let shown = service["shown"].as_bool() == Some(true);
        let cursor = i + skip == row;
        let style = if cursor { t.bold } else { t.text }.patch(t.panel);
        let unread = shown && !providers.iter().any(|p| p["id"] == service["id"]);
        let name = service["name"].as_str().unwrap_or_default();
        let line = format!("{} [{}] {name}", if cursor { "›" } else { " " }, if shown { "x" } else { " " });
        let end = put(buf, inner.x, y, inner.width, &line, style);
        if unread {
            put(
                buf,
                end + 2,
                y,
                inner.right().saturating_sub(end + 2),
                tr("No reading yet").as_ref(),
                t.dim.patch(t.panel),
            );
        }
    }
    let note = tr("On by default for the agents your bots run.");
    put(buf, inner.x, inner.bottom() - 2, inner.width, note.as_ref(), t.dim.patch(t.panel));
    let keys = tr("↑↓ select · J K arrange · space toggle · esc close");
    put(buf, inner.x, inner.bottom() - 1, inner.width, keys.as_ref(), t.dim.patch(t.panel));
}

/// One limit: its label, a bar with the percent and when it resets; or its note.
fn window(buf: &mut Buffer, inner: Rect, y: u16, win: &Value) {
    let t = theme();
    let label = truncate(win["label"].as_str().unwrap_or_default(), 18);
    put(buf, inner.x + 2, y, 18, &label, t.secondary.patch(t.panel));
    if let Some(text) = win["text"].as_str().filter(|text| !text.is_empty()) {
        put(buf, inner.x + 21, y, inner.width.saturating_sub(23), &truncate(text, 40), t.text.patch(t.panel));
        return;
    }
    let pct = win["percent"].as_f64().unwrap_or(0.0).clamp(0.0, 100.0);
    let on = filled(pct, 16);
    // At most 16 columns each, and never past the sheet's edge on a narrow terminal.
    let room_from = |x: u16| inner.right().saturating_sub(x).min(16);
    let mut x = put(
        buf,
        inner.x + 21,
        y,
        room_from(inner.x + 21),
        &"▰".repeat(on.min(16)),
        if pct >= 80.0 { t.amber } else { t.text }.patch(t.panel),
    );
    x = put(buf, x, y, room_from(x), &"▱".repeat(16 - on.min(16)), t.line.patch(t.panel));
    // Said as used: a bare percent reads as what's left.
    let used = tr("{percent}% used").replace("{percent}", &format!("{pct:.0}"));
    let end = put(buf, x + 1, y, room_from(x + 1), &used, t.text.patch(t.panel));
    let reset = win["resetsAt"]
        .as_i64()
        .map(|ms| format!("resets {}", when_future(ms)))
        .or_else(|| {
            // "Sep 26 at 11:59am (Asia/Taipei)": the zone is this computer's own.
            win["resetsText"].as_str().map(|s| truncate(s.split(" (").next().unwrap_or(s), 24))
        })
        .unwrap_or_default();
    // Only in what the reading leaves, so a longer translation never runs into it.
    let room = usize::from(inner.right().saturating_sub(end + 1));
    if room == 0 {
        // A lone "…" would land on the reading.
        return;
    }
    let reset = truncate(&crate::tui::l10n::reset_line(&reset), room);
    rput(buf, inner.right(), y, &reset, t.dim.patch(t.panel));
}

pub(in crate::tui) fn when_future(ms: i64) -> String {
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

#[cfg(test)]
mod tests {
    use super::super::super::app::App;
    use super::super::super::net::Client;
    use super::usage_scroll_max;
    use serde_json::json;

    #[tokio::test]
    async fn the_chooser_marks_what_shows_and_what_has_no_reading() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(Client::new("http://127.0.0.1:1", None), tx, String::new());
        app.usage = json!({
            "providers": [{"id": "claude", "name": "Claude", "windows": []}],
            "services": [
                {"id": "claude", "name": "Claude", "shown": true},
                {"id": "antigravity", "name": "Google Antigravity", "shown": true},
                {"id": "cursor", "name": "Cursor", "shown": false},
            ],
        });
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        super::usage_chooser(&mut buf, area, &app, 2);
        let text: String = buf.content().iter().map(ratatui::buffer::Cell::symbol).collect();
        assert!(text.contains("  [x] Claude"), "{text}");
        assert!(text.contains("[x] Google Antigravity  No reading yet"), "{text}");
        assert!(text.contains("› [ ] Cursor"), "{text}");
    }

    #[tokio::test]
    async fn five_providers_scroll_on_a_small_terminal() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(Client::new("http://127.0.0.1:1", None), tx, String::new());
        let windows = json!([{"label": "5h", "percent": 10}, {"label": "Weekly", "percent": 20}, {"label": "Opus", "percent": 30}]);
        let providers: Vec<_> = (0..5).map(|i| json!({"name": format!("P{i}"), "windows": windows})).collect();
        app.usage = json!({ "providers": providers });
        // 25 lines (each name, three windows, a gap); a 24-row screen shows 18 of them.
        assert_eq!(usage_scroll_max(&app, 24), 7);
        assert_eq!(usage_scroll_max(&app, 60), 0);
    }

    #[test]
    fn a_narrow_sheet_keeps_the_reading_inside_its_edge() {
        use ratatui::{buffer::Buffer, layout::Rect};
        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 1));
        // The bar ends at column 37 and "42% used" needs 8 more: a 44-column sheet cuts the reading.
        let inner = Rect::new(0, 0, 44, 1);
        let win = json!({"label": "5h", "percent": 42, "resetsText": "Sep 26 at 11:59am"});
        super::window(&mut buf, inner, 0, &win);
        let row: String = (0..60).map(|x| buf[(x, 0)].symbol().to_owned()).collect();
        assert!(row[row.char_indices().nth(38).unwrap().0..].starts_with("42% us"), "{row}");
        assert!(!row.contains('…'), "no reset squeezed onto the reading: {row}");
        assert!(row.chars().skip(44).all(|c| c == ' '), "nothing past the edge: {row}");
    }
}
