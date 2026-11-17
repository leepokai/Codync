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
    // The label column holds "Color" in any language, and at least "Shape ".
    let color = tr("Color");
    let label_w = u((w(&color) + 1).max(6));
    // Label, "‹ ", the swatches (2 cells and a gap each), then "›".
    let x0 = mid(usize::from(label_w) + 2 + COLORS.len() * 3 + 1);
    let y = inner.y + 11;
    put(buf, x0, y, label_w, &color, label(NewBotField::Color));
    let mut x = x0 + label_w;
    x = put(buf, x, y, 2, if on_color { "‹ " } else { "  " }, label(NewBotField::Color));
    for (ci, c) in COLORS.iter().enumerate() {
        let s = if t.color { Style::default().bg(bot_color(c)) } else { t.text };
        let mark = if ci == n.color { "◆ " } else { "  " };
        x = put(buf, x, y, 2, mark, s.fg(t.on_color)) + 1;
    }
    put(buf, x, y, 1, if on_color { "›" } else { " " }, label(NewBotField::Color));
    let shape = SHAPES[n.shape];
    let shape = if n.field == NewBotField::Shape { format!("‹ {shape} ›") } else { format!("  {shape}") };
    put(buf, x0, y + 1, label_w, "Shape", label(NewBotField::Shape));
    let x = x0 + label_w;
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

#[cfg(test)]
mod tests {
    use super::super::super::app::Editor;
    use super::super::super::l10n::set_language;
    use super::*;
    use crate::LockExt;

    /// Keeps this module's tests from switching the shared language under each other.
    static LANGUAGE: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// The color row's cells in a 60x20 terminal, the new bot dialog on its color field.
    fn color_row(language: &str) -> Vec<String> {
        let area = Rect::new(0, 0, 60, 20);
        let mut buf = Buffer::empty(area);
        let n = NewBot {
            name: Editor::default(),
            color: 0,
            shape: 0,
            field: NewBotField::Color,
            error: None,
            saving: false,
        };
        let _serial = LANGUAGE.locked();
        let previous = set_language(language);
        new_bot_view(&mut buf, area, &n);
        set_language(previous);
        let y = centered(area, 56, 18).y + 12;
        (area.left()..area.right()).map(|x| buf[(x, y)].symbol().to_owned()).collect()
    }

    fn arrow_x(language: &str) -> Option<usize> {
        color_row(language).iter().position(|cell| cell == "‹")
    }

    #[test]
    fn the_color_row_keeps_its_column_in_every_language() {
        assert_eq!(arrow_x("en"), Some(15));
        for language in ["es", "de", "ja", "zh-Hant"] {
            assert_eq!(arrow_x(language), Some(15), "{language}");
        }
    }

    #[test]
    fn a_long_translation_widens_the_label_column() {
        assert!(color_row("fr").concat().contains("Couleur ‹"));
    }
}
