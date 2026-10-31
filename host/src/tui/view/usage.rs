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
    let footer = if lines.len() > fits {
        "Read from the agents on this computer. ↑↓ scroll · esc close"
    } else {
        "Read from the agents on this computer. esc close"
    };
    put(buf, inner.x, inner.bottom() - 1, inner.width, footer, t.dim.patch(t.panel));
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
    let mut x = put(
        buf,
        inner.x + 21,
        y,
        16,
        &"▰".repeat(on.min(16)),
        if pct >= 80.0 { t.amber } else { t.text }.patch(t.panel),
    );
    x = put(buf, x, y, 16, &"▱".repeat(16 - on.min(16)), t.line.patch(t.panel));
    // Said as used: a bare percent reads as what's left.
    let used = tr("{percent}% used").replace("{percent}", &format!("{pct:.0}"));
    let end = put(buf, x + 1, y, 16, &used, t.text.patch(t.panel));
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
}
