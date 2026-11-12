use super::{Buffer, Rect, centered, footer, frame_box, put, row, s, theme, tr, window};
use crate::tui::manage::n8n::{N8nSheet, SHOWN_WORKFLOWS, run_label};

/// `2026-11-01 10:00` from n8n's ISO time; nothing for a run that has no start.
fn started(run: &serde_json::Value) -> String {
    s(run, "startedAt").get(..16).unwrap_or_default().replace('T', " ")
}

pub fn draw(buf: &mut Buffer, area: Rect, sheet: &N8nSheet) {
    let palette = theme();
    let inner = frame_box(buf, centered(area, 100, 32), palette.text, palette.panel, Some(("n8n", palette.text)));
    if inner.height < 8 {
        return;
    }
    let state = if sheet.loaded() {
        tr(sheet.state_label()).replace("{name}", "n8n")
    } else {
        tr("Checking {name}…").replace("{name}", "n8n")
    };
    put(buf, inner.x, inner.y, inner.width, &state, palette.text);
    let detail = match s(&sheet.status, "error") {
        "" => String::new(),
        error => error.to_owned(),
    };
    put(buf, inner.x, inner.y + 1, inner.width, &detail, palette.secondary);

    let body = Rect::new(inner.x, inner.y + 3, inner.width, inner.height.saturating_sub(5));
    if sheet.running() {
        let rows = usize::from(body.height).saturating_sub(sheet.runs.len().min(4) + 2);
        if sheet.workflows.is_empty() {
            let text = if sheet.busy {
                tr("Checking {name}…").replace("{name}", "n8n")
            } else {
                tr("No workflows yet.").into_owned()
            };
            put(buf, body.x, body.y, body.width, &text, palette.secondary);
        }
        let shown = sheet.workflows.len().min(SHOWN_WORKFLOWS);
        let offset = window(sheet.cursor, rows);
        for (line, index) in (offset..shown).take(rows).enumerate() {
            let workflow = &sheet.workflows[index];
            let y = body.y + u16::try_from(line).unwrap_or(0);
            let style = row(buf, body, y, index == sheet.cursor);
            let mark = if workflow["active"] == true { "[x]" } else { "[ ]" };
            let name = if s(workflow, "name").is_empty() { s(workflow, "id") } else { s(workflow, "name") };
            put(buf, body.x, y, body.width, &format!("{mark} {name}"), palette.text.patch(style));
        }
        let hidden = sheet.workflows.len().saturating_sub(SHOWN_WORKFLOWS);
        if hidden > 0 || sheet.more {
            let count = if hidden > 0 { hidden.to_string() } else { "+".to_owned() };
            let y = body.y + u16::try_from(rows).unwrap_or(0);
            put(buf, body.x, y, body.width, &tr("{count} more in n8n").replace("{count}", &count), palette.secondary);
        }
        if !sheet.runs.is_empty() {
            let top = body.y + u16::try_from(rows).unwrap_or(0) + 1;
            put(buf, body.x, top, body.width, &tr("Latest runs"), palette.secondary);
            for (line, run) in sheet.runs.iter().take(4).enumerate() {
                let name = sheet
                    .workflows
                    .iter()
                    .find(|w| w["id"] == run["workflowId"])
                    .map_or_else(|| s(run, "workflowId"), |w| s(w, "name"));
                let text = format!("{name} · {} · {}", tr(run_label(s(run, "status"))), started(run));
                put(buf, body.x, top + 1 + u16::try_from(line).unwrap_or(0), body.width, &text, palette.text);
            }
        }
    } else if sheet.loaded() {
        put(buf, body.x, body.y, body.width, &tr("Start n8n to see its workflows and runs."), palette.secondary);
    }
    let keys = if sheet.running() {
        "↑↓ select · Enter publish or unpublish · s stop · r refresh · Esc close"
    } else if sheet.active() {
        "s stop · r refresh · Esc close"
    } else {
        "s start · r refresh · Esc close"
    };
    footer(buf, inner, sheet.error.as_deref().or(sheet.busy.then_some("Waiting for host…")), keys);
}
