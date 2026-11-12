//! Scrollable shared intent fields, keeping keyboard focus in view.
use super::{
    Buffer, Editor, Fields, Modifier, Rect, Submit, centered, frame_box, hline, input_line, multiline, put, rput,
    theme, tr, truncate, u,
};

pub fn fields(buf: &mut Buffer, area: Rect, f: &Fields) {
    let t = theme();
    let tall = f.inputs.iter().any(|i| i.multiline);
    let h = if tall { 20 } else { u(f.inputs.len() * 2 + usize::from(f.choices.len() > 1) * 2 + 8) };
    let r = centered(area, 84, h);
    let title = tr(&f.title);
    let note = tr(&f.note);
    let inner = frame_box(buf, r, t.text, t.panel, Some((title.as_ref(), t.text)));
    if inner.height < 8 || inner.width < 8 {
        return;
    }
    let mut y = inner.y;
    put(buf, inner.x, y, inner.width, &truncate(note.as_ref(), usize::from(inner.width)), t.dim.patch(t.panel));
    y += 2;
    let label_w = 18u16.min(inner.width / 2);
    let vx = inner.x + label_w;
    let vw = inner.width.saturating_sub(label_w);
    let row_i = usize::from(f.choices.len() > 1);
    let visible = usize::from(inner.height.saturating_sub(6) / 2).max(1);
    let offset = f.cursor.saturating_sub(visible.saturating_sub(1));
    if f.choices.len() > 1 && offset == 0 {
        let on = f.cursor == 0;
        let runs = tr("Runs");
        put(buf, inner.x, y, label_w, runs.as_ref(), if on { t.bold } else { t.secondary }.patch(t.panel));
        let base = if on { t.sel } else { t.panel };
        put(buf, vx, y, vw, &format!("‹ {} ›", f.choices.get(f.choice).map_or("", String::as_str)), t.text.patch(base));
        y += 2;
    }
    for (i, input) in f.inputs.iter().enumerate() {
        if row_i + i < offset {
            continue;
        }
        if y >= inner.bottom().saturating_sub(4) {
            break;
        }
        let on = f.cursor == row_i + i;
        let star = if input.required { " *" } else { "" };
        let label = tr(&input.label);
        put(
            buf,
            inner.x,
            y,
            label_w - 1,
            &truncate(&format!("{label}{star}"), usize::from(label_w - 1)),
            if on { t.bold } else { t.secondary }.patch(t.panel),
        );
        if input.multiline {
            multiline(
                buf,
                Rect::new(vx, y, vw, inner.bottom().saturating_sub(y + 3)),
                &input.ed,
                &input.placeholder,
                on,
            );
            break;
        }
        let shown = if input.secret {
            Editor {
                text: "•".repeat(input.ed.text.chars().count()),
                cursor: "•".len() * input.ed.text[..input.ed.cursor].chars().count(),
            }
        } else {
            input.ed.clone()
        };
        input_line(buf, Rect::new(vx, y, vw, 1), y, &shown, &input.placeholder, on, false);
        y += 2;
    }
    let by = inner.bottom() - 1;
    if let Some(e) = &f.error {
        put(buf, inner.x, by - 1, inner.width, e, t.red.patch(t.panel));
    }
    hline(buf, inner.x, by - 2, inner.width, t.line.patch(t.panel));
    let x = put(
        buf,
        inner.x,
        by,
        14,
        if f.saving {
            " … Saving "
        } else if matches!(f.submit, Submit::Connection(_)) {
            " ↵ Continue "
        } else if matches!(f.submit, Submit::Connector(_) | Submit::App(_)) {
            " ↵ Add "
        } else {
            " ↵ Save "
        },
        t.btn_primary,
    ) + 1;
    put(buf, x, by, 12, " esc Cancel ", t.text.patch(t.btn).add_modifier(Modifier::BOLD));
    let hint = if matches!(f.submit, Submit::Connection(_)) {
        "tab next · esc close"
    } else if tall {
        "⇧↵ new line · paste works"
    } else {
        "tab next field"
    };
    rput(buf, inner.right(), by, hint, t.dim.patch(t.panel));
}
