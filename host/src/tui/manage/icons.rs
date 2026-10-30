//! A terminal can choose a file or URL and color mode without pretending to render a bitmap.
use super::{Fields, Input, Submit};
use crate::tui::app::Editor;

pub fn icon_fields(target: String) -> Fields {
    Fields {
        title: "Custom icon".into(),
        note: "PNG/JPEG/WebP ≤5 MiB; SVG ≤256 KiB. Images stay still. Terminals keep the text avatar; apps show the image. Leave source empty to change only the color mode.".into(),
        choices: vec!["Original colors".into(), "Tint icon".into(), "Use default".into()],
        choice: 0,
        inputs: vec![Input {
            name: "source".into(), label: "Image".into(), secret: false, required: false,
            multiline: false, placeholder: "Local file path or https://…".into(), ed: Editor::default(),
        }],
        cursor: 0, error: None, saving: false, submit: Submit::Icon(target),
    }
}
