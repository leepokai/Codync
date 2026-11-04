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
    /// Why a provider has no numbers: its agent is too old, or this account can't be read.
    Status(&'a str),
    Window(&'a Value),
    Gap,
}

/// Each provider's name, why it can't report, its windows and a gap.
fn lines(providers: &[Value]) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    for p in providers {
        lines.push(Line::Provider(p["name"].as_str().unwrap_or_default()));
        lines.extend(p["status"].as_str().map(Line::Status));
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
    rows.saturating_sub(fits.saturating_sub(usize::from(week_line(&app.week).is_some())))
}

pub(super) fn usage(buf: &mut Buffer, area: Rect, app: &App, scroll: usize) {
    let t = theme();
    let providers = app.usage["providers"].as_array().cloned().unwrap_or_default();
    let lines = lines(&providers);
    let (r, fits) = sheet(area, lines.len());
    // The week's line takes the row above the footer.
    let fits = fits.saturating_sub(usize::from(week_line(&app.week).is_some()));
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
            Line::Status(status) => {
                let message = if *status == "unsupported" {
                    "Update this agent to read its plan limits."
                } else {
                    "Plan limits are unavailable for this account."
                };
                put(buf, inner.x + 2, y, inner.width.saturating_sub(2), &tr(message), t.secondary.patch(t.panel));
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
    if let Some(line) = week_line(&app.week) {
        put(
            buf,
            inner.x,
            inner.bottom() - 2,
            inner.width,
            &truncate(&line, usize::from(inner.width)),
            t.secondary.patch(t.panel),
        );
    }
    put(buf, inner.x, inner.bottom() - 1, inner.width, footer, t.dim.patch(t.panel));
}

/// "Your week: …" from the host's `weekSummary`; nothing for a week without activity or a host that lacks it.
fn week_line(week: &Value) -> Option<String> {
    if week["activeBots"].as_i64().unwrap_or(0) == 0 {
        return None;
    }
    let n = |key: &str| week[key].as_i64().unwrap_or(0);
    Some(format!(
        "{}: {} {} · {} {} · {} {} · {}/{} {}",
        tr("Your week"),
        n("messages"),
        tr("messages"),
        n("replies"),
        tr("replies"),
        n("fileEdits"),
        tr("file edits"),
        n("approvalsAnswered"),
        n("approvalsAsked"),
        tr("approvals"),
    ))
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
    put(buf, x + 1, y, 5, &format!("{pct:>3.0}%"), t.text.patch(t.panel));
    let reset = win["resetsAt"]
        .as_i64()
        .map(|ms| format!("resets {}", when_future(ms)))
        .or_else(|| {
            // "Sep 26 at 11:59am (Asia/Taipei)": the zone is this computer's own.
            win["resetsText"].as_str().map(|s| truncate(s.split(" (").next().unwrap_or(s), 24))
        })
        .unwrap_or_default();
    let reset = crate::tui::l10n::reset_line(&reset);
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
    use super::{usage_scroll_max, week_line};
    use serde_json::{Value, json};

    #[test]
    fn the_week_line_counts_and_hides_for_an_empty_week() {
        assert_eq!(week_line(&Value::Null), None);
        assert_eq!(week_line(&json!({"activeBots": 0, "messages": 5})), None);
        let week = json!({"activeBots": 2, "messages": 12, "replies": 11, "fileEdits": 3, "approvalsAnswered": 3, "approvalsAsked": 4});
        let line = week_line(&week).expect("a week with activity");
        assert!(line.contains("12 messages") && line.contains("3/4 approvals"), "{line}");
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
}
