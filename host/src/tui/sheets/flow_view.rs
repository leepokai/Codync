//! A routine's flow: the chain from the routine's bot, each bot with its latest step's status.

use super::{App, Buffer, Rect, centered, footer, frame_box, multiline, put, row, rput, theme, u, window};
use crate::tui::l10n::tr;
use crate::tui::manage::{FlowSheet, FlowStage, Target};
use crate::tui::md::truncate;
use serde_json::Value;

pub fn draw(buf: &mut Buffer, area: Rect, app: &App, f: &FlowSheet) {
    let t = theme();
    let r = centered(area, 90, 24);
    let title = format!("{} · {}", tr("Flow"), f.name);
    let inner = frame_box(buf, r, t.text, t.panel, Some((&title, t.text)));
    let name = |id: &str| app.bots.get(id).map_or_else(|| id.to_owned(), |b| b.name.clone());
    if f.canvas.is_none() {
        put(buf, inner.x, inner.y, inner.width, &tr("Loading…"), t.dim.patch(t.panel));
        footer(buf, inner, None, "esc");
        return;
    }
    let rows = f.rows();
    let height = usize::from(inner.height.saturating_sub(6));
    for (y, (i, line)) in (inner.y..).zip(rows.iter().enumerate().skip(window(f.cursor, height)).take(height)) {
        let base = row(buf, inner, y, i == f.cursor);
        let indent = line.depth * 2;
        let arrow = if line.depth == 0 { "" } else { "→ " };
        let label = format!("{}{arrow}{}", " ".repeat(indent), name(&line.bot));
        put(
            buf,
            inner.x,
            y,
            u(usize::from(inner.width) / 2),
            &truncate(&label, usize::from(inner.width) / 2),
            t.bold.patch(base),
        );
        if let Some(link) = &line.link {
            let hint = link["instruction"].as_str().unwrap_or_default();
            let room = usize::from(inner.width / 2).saturating_sub(14);
            put(buf, inner.x + inner.width / 2, y, u(room), &truncate(hint, room), t.secondary.patch(base));
        }
        let (glyph, status, tone) = status_of(line.step.as_ref());
        let style = match tone {
            Tone::Active => t.amber,
            Tone::Done => t.text,
            Tone::Bad => t.red,
            Tone::Quiet => t.dim,
        };
        rput(buf, inner.right(), y, &format!("{glyph} {status}"), style.patch(base));
    }
    if rows.len() <= 1 {
        let y = inner.y + 2;
        put(
            buf,
            inner.x,
            y,
            inner.width,
            &tr("Nothing is handed on yet. a connects a bot."),
            t.secondary.patch(t.panel),
        );
    }
    let (hint, hint_y) = (stage_hint(app, f), inner.bottom().saturating_sub(5));
    match &f.stage {
        FlowStage::Browse => {}
        FlowStage::Pick { cursor } => {
            let from = f.rows().get(f.cursor).map_or_else(|| f.bot.clone(), |r| r.bot.clone());
            put(buf, inner.x, hint_y, inner.width, &hint, t.bold.patch(t.panel));
            for (y, (i, (_, bot))) in (hint_y + 1..).zip(app.flow_candidates(f, &from).iter().enumerate().take(3)) {
                let base = row(buf, inner, y, i == *cursor);
                put(buf, inner.x, y, inner.width, bot, t.text.patch(base));
            }
        }
        FlowStage::Instruction { text, .. } => {
            put(buf, inner.x, hint_y, inner.width, &hint, t.bold.patch(t.panel));
            multiline(buf, Rect::new(inner.x, hint_y + 1, inner.width, 3), text, "What it does with the reply", true);
        }
    }
    let keys = match f.stage {
        FlowStage::Browse => "a connect a bot · e instruction · d disconnect · t test · esc",
        FlowStage::Pick { .. } => "↑↓ choose · ↵ next · esc",
        FlowStage::Instruction { .. } => "↵ save · esc",
    };
    footer(buf, inner, None, keys);
}

fn stage_hint(app: &App, f: &FlowSheet) -> String {
    let name = |id: &str| app.bots.get(id).map_or_else(|| id.to_owned(), |b| b.name.clone());
    match &f.stage {
        FlowStage::Browse => String::new(),
        FlowStage::Pick { .. } => tr("Hand the reply to:").into_owned(),
        FlowStage::Instruction { target: Target::New { to, .. }, .. } => format!("{} {}", tr("Ask"), name(to)),
        FlowStage::Instruction { target: Target::Link(_), .. } => tr("New instruction").into_owned(),
    }
}

#[derive(Clone, Copy)]
enum Tone {
    Active,
    Done,
    Bad,
    Quiet,
}

/// The glyph, label and tone of a step's status; a bot with no step has not run yet.
fn status_of(step: Option<&Value>) -> (&'static str, String, Tone) {
    let error = step.and_then(|s| s["error"].as_str());
    match step.and_then(|s| s["status"].as_str()) {
        Some("running") => ("▶", tr("running").into_owned(), Tone::Active),
        Some("succeeded") => ("✓", tr("succeeded").into_owned(), Tone::Done),
        Some("failed") => ("✗", error.map_or_else(|| tr("failed").into_owned(), |e| truncate(e, 40)), Tone::Bad),
        Some("skipped") => ("–", tr("skipped").into_owned(), Tone::Quiet),
        Some("cancelled") => ("■", tr("cancelled").into_owned(), Tone::Quiet),
        _ => ("·", tr("not run").into_owned(), Tone::Quiet),
    }
}
