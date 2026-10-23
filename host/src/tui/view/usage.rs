//! The usage sheet.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::super::app::App;
use super::super::l10n::tr;
use super::super::md::truncate;
use super::buffer::{centered, frame_box, put, rput, u};
use super::filled;
use super::style::theme;
use super::time::{DAYS, MONTHS, clock, local, month_day, now_ms};

pub(super) fn usage(buf: &mut Buffer, area: Rect, app: &App) {
    let t = theme();
    let providers = app.usage["providers"].as_array().cloned().unwrap_or_default();
    let rows: usize = providers.iter().map(|p| 2 + p["windows"].as_array().map_or(0, Vec::len)).sum();
    let r = centered(area, 78, u(rows + 5).max(8));
    let inner = frame_box(buf, r, t.text, t.panel, Some((tr("Usage").as_ref(), t.text)));
    let mut y = inner.y + 1;
    if providers.is_empty() {
        put(buf, inner.x, y, inner.width, "No usage numbers yet.", t.secondary.patch(t.panel));
    }
    for p in &providers {
        put(buf, inner.x, y, inner.width, p["name"].as_str().unwrap_or_default(), t.bold.patch(t.panel));
        y += 1;
        for win in p["windows"].as_array().into_iter().flatten() {
            let pct = win["percent"].as_f64().unwrap_or(0.0).clamp(0.0, 100.0);
            let on = filled(pct, 16);
            put(
                buf,
                inner.x + 2,
                y,
                18,
                &truncate(win["label"].as_str().unwrap_or_default(), 18),
                t.secondary.patch(t.panel),
            );
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
            y += 1;
        }
        y += 1;
    }
    put(buf, inner.x, inner.bottom() - 1, inner.width, "From your local installs. esc close", t.dim.patch(t.panel));
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
