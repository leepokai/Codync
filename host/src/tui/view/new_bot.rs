//! The new bot dialog: the bot's face in its color, a name, then color and shape.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::super::app::{COLORS, NewBot, NewBotField, SHAPES};
use super::super::l10n::tr;
use super::super::md::width as w;
use super::buffer::{centered, frame_box, put, rput, u};
use super::overlay::input_line;
use super::style::{FACE, bot_color, face_style, theme};

/// "Color " + "‹ " + the swatches (2 cells and a gap each) + "›".
const LOOKS_W: usize = 6 + 2 + COLORS.len() * 3 + 1;

pub(super) fn new_bot_view(buf: &mut Buffer, area: Rect, n: &NewBot) {
    let t = theme();
    let r = centered(area, 56, 18);
    let inner = frame_box(buf, r, t.text, t.panel, Some((tr("New bot").as_ref(), t.text)));
    let mid = |width: usize| inner.x + inner.width.saturating_sub(u(width)) / 2;
    for (y, row) in (inner.y + 1..).zip(FACE) {
        put(buf, mid(w(row)), y, inner.width, row, face_style(COLORS[n.color]).patch(t.panel));
    }
    // Left empty, the host names the bot after its first conversations.
    let placeholder = "Name your bot";
    let shown = if n.name.text.is_empty() { placeholder } else { n.name.text.as_str() };
    let name_w = u(w(shown) + 1).min(inner.width);
    let y = inner.y + 9;
    let field = Rect::new(mid(usize::from(name_w)), y, name_w, 1);
    input_line(buf, field, y, &n.name, placeholder, n.field == NewBotField::Name, false);
    // ‹ › mark the row the arrows change.
    let label = |f: NewBotField| if n.field == f { t.bold } else { t.secondary }.patch(t.panel);
    let on_color = n.field == NewBotField::Color;
    let x0 = mid(LOOKS_W);
    let y = inner.y + 11;
    let mut x = put(buf, x0, y, 6, tr("Color ").as_ref(), label(NewBotField::Color));
    x = put(buf, x, y, 2, if on_color { "‹ " } else { "  " }, label(NewBotField::Color));
    for (ci, c) in COLORS.iter().enumerate() {
        let s = if t.color { Style::default().bg(bot_color(c)) } else { t.text };
        let mark = if ci == n.color { "◆ " } else { "  " };
        x = put(buf, x, y, 2, mark, s.fg(t.on_color)) + 1;
    }
    put(buf, x, y, 1, if on_color { "›" } else { " " }, label(NewBotField::Color));
    let shape = SHAPES[n.shape];
    let shape = if n.field == NewBotField::Shape { format!("‹ {shape} ›") } else { format!("  {shape}") };
    let x = put(buf, x0, y + 1, 6, "Shape ", label(NewBotField::Shape));
    put(buf, x, y + 1, u(w(&shape)), &shape, label(NewBotField::Shape));
    let by = inner.bottom() - 1;
    if let Some(e) = &n.error {
        put(buf, inner.x, by - 1, inner.width, e, t.red.patch(t.panel));
    }
    let create = if n.saving { " … Saving " } else { " ↵ Create " };
    let x = put(buf, inner.x, by, 14, create, t.btn_primary) + 1;
    put(buf, x, by, 12, " esc Cancel ", t.text.patch(t.btn).add_modifier(Modifier::BOLD));
    let hint = if n.field == NewBotField::Name { "tab color and shape" } else { "←→ change · tab next" };
    rput(buf, inner.right(), by, hint, t.dim.patch(t.panel));
}
