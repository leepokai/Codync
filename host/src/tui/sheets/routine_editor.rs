//! Drawing the routine editor; the host parses and validates all destination changes.

use super::{Buffer, Rect, centered, frame_box, input_line, multiline, put, rput, theme, tr};
use crate::tui::manage::{RoutineField, RoutineForm};
use ratatui::style::Modifier;

pub fn draw(buf: &mut Buffer, area: Rect, f: &RoutineForm) {
    let t = theme();
    let r = centered(area, 84, 20);
    if r.height < 20 || r.width < 60 {
        compact(buf, r, f);
        return;
    }
    let title = if f.id.is_some() { "Edit routine" } else { "New routine" };
    let inner = frame_box(buf, r, t.text, t.panel, Some((title, t.text)));
    let label = |on: bool| if on { t.bold } else { t.secondary }.patch(t.panel);
    let vx = inner.x + 9;
    let vw = inner.width.saturating_sub(9);
    let at = |y: u16| Rect::new(vx, y, vw, 1);
    put(buf, inner.x, inner.y, 9, tr("Name").as_ref(), label(f.field == RoutineField::Name));
    input_line(buf, at(inner.y), inner.y, &f.name, "Morning summary", f.field == RoutineField::Name, false);
    put(buf, inner.x, inner.y + 2, 9, "Do", label(f.field == RoutineField::Instruction));
    multiline(
        buf,
        Rect::new(vx, inner.y + 2, vw, 4),
        &f.instruction,
        "What the bot does each time",
        f.field == RoutineField::Instruction,
    );
    put(buf, inner.x, inner.y + 7, 9, "When", label(f.field == RoutineField::When));
    let placeholder = if f.original_text.is_empty() {
        "daily 09:00 · weekdays 09:00 · every 2h · 0 9 * * 1-5 · webhook"
    } else {
        &f.original_text
    };
    input_line(buf, at(inner.y + 7), inner.y + 7, &f.when, placeholder, f.field == RoutineField::When, false);
    put(
        buf,
        vx,
        inner.y + 8,
        vw,
        "or hourly :30 · weekly mon 09:00 · yearly mar 15 09:00 · 2026-10-01 09:00 (once)",
        t.dim.patch(t.panel),
    );
    put(buf, vx, inner.y + 9, vw, "every = from now on · webhook = when called", t.dim.patch(t.panel));
    let (zone_hint, zone_style) = match crate::tui::manage::complete_zone(&f.when.text, crate::store::now_ms()) {
        Some(done) => (format!("tab → {done}"), t.secondary),
        None => (format!("times in {}; end with a zone to change it", crate::routines::local_zone()), t.dim),
    };
    put(buf, vx, inner.y + 10, vw, &zone_hint, zone_style.patch(t.panel));
    put(buf, inner.x, inner.y + 12, 9, "Outputs", label(f.field == RoutineField::Outputs));
    multiline(
        buf,
        Rect::new(vx, inner.y + 12, vw, 2),
        &f.outputs,
        "whatsapp recipient@s.whatsapp.net from sender@s.whatsapp.net",
        f.field == RoutineField::Outputs,
    );
    put(
        buf,
        vx,
        inner.y + 14,
        vw,
        "Separate outputs with ; · quote titles · none clears outputs",
        t.dim.patch(t.panel),
    );
    let by = inner.bottom() - 1;
    if let Some(e) = &f.error {
        put(buf, inner.x, by - 1, inner.width, e, t.red.patch(t.panel));
    }
    let x = put(buf, inner.x, by, 14, if f.saving { " … Saving " } else { " ↵ Save " }, t.btn_primary) + 1;
    put(buf, x, by, 12, " esc Cancel ", t.text.patch(t.btn).add_modifier(Modifier::BOLD));
    rput(buf, inner.right(), by, "tab next field · ⇧↵ new line", t.dim.patch(t.panel));
}

fn compact(buf: &mut Buffer, area: Rect, form: &RoutineForm) {
    let t = theme();
    if area.width < 8 || area.height < 3 {
        return;
    }
    let inner = frame_box(buf, area, t.text, t.panel, Some(("Edit routine", t.text)));
    if inner.height < 5 {
        put(buf, inner.x, inner.y, inner.width, "Enlarge terminal · Esc closes", t.secondary);
        return;
    }
    let (label, editor, hint) = match form.field {
        RoutineField::Name => ("Name", &form.name, "Morning summary"),
        RoutineField::Instruction => ("Instruction", &form.instruction, "What the bot does each time"),
        RoutineField::When => ("When", &form.when, "daily 09:00"),
        RoutineField::Outputs => {
            ("Outputs", &form.outputs, "whatsapp recipient@s.whatsapp.net from sender@s.whatsapp.net")
        }
    };
    put(buf, inner.x, inner.y, inner.width, tr(label).as_ref(), t.bold);
    multiline(buf, Rect::new(inner.x, inner.y + 1, inner.width, inner.height.saturating_sub(3)), editor, hint, true);
    if let Some(error) = &form.error {
        put(buf, inner.x, inner.bottom() - 2, inner.width, error, t.red);
    }
    put(buf, inner.x, inner.bottom() - 1, inner.width, "Tab next · Enter save · Esc close", t.secondary);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::Editor;

    #[test]
    fn routine_editor_handles_small_terminal_buffers_with_every_field_selected() {
        let mut form = RoutineForm {
            bot: "bot".into(),
            id: None,
            name: Editor::default(),
            instruction: Editor::default(),
            when: Editor::default(),
            outputs: Editor::default(),
            original_outputs: Some(String::new()),
            original_output_values: serde_json::json!([]),
            original: serde_json::Value::Null,
            original_text: String::new(),
            field: RoutineField::Name,
            error: None,
            saving: false,
        };
        for field in [RoutineField::Name, RoutineField::Instruction, RoutineField::When, RoutineField::Outputs] {
            form.field = field;
            for (width, height) in [(0, 0), (8, 3), (30, 10), (80, 18), (100, 30)] {
                let area = Rect::new(0, 0, width, height);
                draw(&mut Buffer::empty(area), area, &form);
            }
        }
    }
}
