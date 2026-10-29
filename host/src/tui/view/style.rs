//! The theme, and how bots and states look: colors, glyphs, avatars.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::sync::LazyLock;

use super::super::app::{App, Bot, Mark};

pub struct Theme {
    pub color: bool,
    pub text: Style,
    pub bold: Style,
    pub secondary: Style,
    pub dim: Style,
    pub line: Style,
    pub amber: Style,
    pub red: Style,
    pub green: Style,
    pub code: Style,
    pub code_block: Style,
    pub band: Style,
    pub sel: Style,
    pub panel: Style,
    pub btn: Style,
    pub btn_primary: Style,
    pub added: Style,
    pub removed: Style,
    pub on_color: Color,
    pub shade: Color,
}

pub(super) fn rgb(hex: u32) -> Color {
    Color::Rgb(((hex >> 16) & 0xFF) as u8, ((hex >> 8) & 0xFF) as u8, (hex & 0xFF) as u8)
}

pub fn theme() -> &'static Theme {
    use super::super::appearance::{self, Appearance};
    static LIGHT: LazyLock<Theme> = LazyLock::new(|| build_theme(true, true, true));
    static DARK: LazyLock<Theme> = LazyLock::new(|| build_theme(false, true, true));
    static SYSTEM_LIGHT: LazyLock<Theme> = LazyLock::new(|| build_theme(true, true, false));
    static SYSTEM_DARK: LazyLock<Theme> = LazyLock::new(|| build_theme(false, true, false));
    static MONO: LazyLock<Theme> = LazyLock::new(|| build_theme(false, false, false));
    if std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()) {
        return &MONO;
    }
    match appearance::current() {
        Appearance::Light => &LIGHT,
        Appearance::Dark => &DARK,
        Appearance::System if appearance::system_is_light() => &SYSTEM_LIGHT,
        Appearance::System => &SYSTEM_DARK,
    }
}

fn build_theme(light: bool, color: bool, explicit: bool) -> Theme {
    let s = if explicit { Style::default().bg(rgb(if light { 0xFFFFFF } else { 0x0A0A0A })) } else { Style::default() };
    if !color {
        let rev = s.add_modifier(Modifier::REVERSED);
        return Theme {
            color,
            text: s,
            bold: s.add_modifier(Modifier::BOLD),
            secondary: s,
            dim: s.add_modifier(Modifier::DIM),
            line: s.add_modifier(Modifier::DIM),
            amber: s.add_modifier(Modifier::BOLD),
            red: s.add_modifier(Modifier::BOLD),
            green: s,
            code: s.add_modifier(Modifier::BOLD),
            code_block: s,
            band: s,
            sel: rev,
            panel: s,
            btn: s,
            btn_primary: rev.add_modifier(Modifier::BOLD),
            added: s,
            removed: s.add_modifier(Modifier::DIM),
            on_color: Color::Reset,
            shade: Color::Reset,
        };
    }
    let (text, second, dim, line, band, sel, panel, btn, green, red, add_bg, rm_bg) = if light {
        (
            0x141414, 0x5F5F5F, 0x767676, 0xD6D6D6, 0xEFEFEF, 0xE2E2E2, 0xF7F7F7, 0xE4E4E4, 0x2E7D32, 0xC23A2B,
            0xE3F3E4, 0xF9E3E0,
        )
    } else {
        (
            0xF2F2F2, 0x9A9A9A, 0x929292, 0x333333, 0x1C1C1C, 0x262626, 0x141414, 0x2A2A2A, 0x8FD18B, 0xF0A7A7,
            0x14261A, 0x2A1616,
        )
    };
    let fg = |h| s.fg(rgb(h));
    Theme {
        color,
        text: if explicit { fg(text) } else { s },
        bold: (if explicit { fg(text) } else { s }).add_modifier(Modifier::BOLD),
        secondary: fg(second),
        dim: fg(dim),
        line: fg(line),
        amber: fg(if light { 0xB8700A } else { 0xF0A030 }),
        red: fg(red),
        green: fg(green),
        code: s.bg(rgb(band)),
        code_block: s.bg(rgb(band)),
        band: s.bg(rgb(band)),
        sel: s.bg(rgb(sel)),
        panel: s.bg(rgb(panel)),
        btn: s.bg(rgb(btn)),
        btn_primary: s.fg(rgb(if light { 0xFFFFFF } else { 0x0A0A0A })).bg(rgb(text)).add_modifier(Modifier::BOLD),
        added: s.fg(rgb(green)).bg(rgb(add_bg)),
        removed: s.fg(rgb(red)).bg(rgb(rm_bg)),
        on_color: rgb(0x0A0A0A),
        shade: rgb(if light { 0xB0B0B0 } else { 0x3A3A3A }),
    }
}

pub fn bot_color(name: &str) -> Color {
    let hex = match name {
        "black" => 0x8A8A8A,
        "brown" => 0x936439,
        "red" => 0xFF263C,
        "orange" => 0xFF6700,
        "yellow" => 0xFF9800,
        "green" => 0x00C972,
        "cyan" => 0x00BCA6,
        "violet" => 0x9159FE,
        "magenta" => 0xFF309B,
        "gray" => 0x777777,
        _ => 0x1084FE,
    };
    rgb(hex)
}

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub(super) fn spin(app: &App) -> &'static str {
    SPINNER[usize::try_from(app.frame % 10).unwrap_or(0)]
}

pub fn glyph(app: &App, m: Mark) -> (&'static str, Style) {
    let t = theme();
    match m {
        Mark::Need => ("◆", t.amber),
        Mark::Work => (spin(app), t.secondary),
        Mark::Unread => ("●", t.bold),
        Mark::Idle => ("○", t.dim),
        Mark::Error => ("×", t.red),
    }
}

pub(super) fn avatar(app: &App, b: &Bot) -> Line<'static> {
    let t = theme();
    if b.group {
        // A group: its first two bots, like the apps' avatar stack.
        let mut v: Vec<Span<'static>> = b.members.iter().take(2).map(|m| person(app, m)).collect();
        v.resize(2, Span::raw(" "));
        return Line::from(v);
    }
    Line::from(if t.color {
        Span::styled("••", Style::default().fg(t.on_color).bg(bot_color(&b.color)).add_modifier(Modifier::BOLD))
    } else {
        Span::styled("[]", t.bold)
    })
}

/// One cell for a bot id (its initial on its color) or `"user"` (Y), as in a group's avatar stack.
pub(super) fn person(app: &App, id: &str) -> Span<'static> {
    let t = theme();
    if id == "user" {
        return Span::styled("Y", t.text.patch(t.btn).add_modifier(Modifier::BOLD));
    }
    let Some(b) = app.bots.get(id) else { return Span::styled("?", t.dim) };
    let initial: String = b.name.chars().next().map_or_else(|| "?".into(), |c| c.to_uppercase().collect());
    let style = if t.color {
        Style::default().fg(t.on_color).bg(bot_color(&b.color)).add_modifier(Modifier::BOLD)
    } else {
        t.bold
    };
    Span::styled(initial, style)
}

pub const FACE: [&str; 7] = [
    "⠀⣤⣿⣿⣿⣿⣿⣿⣿⣿⣿⣤⠀",
    "⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿",
    "⣿⣿⣿⣿⠀⣿⣿⣿⠀⣿⣿⣿⣿",
    "⣿⣿⣿⣿⣤⣿⣿⣿⣤⣿⣿⣿⣿",
    "⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿",
    "⠛⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⠛",
    "⠀⠀⠛⠛⠛⠛⠛⠛⠛⠛⠛⠀⠀",
];

pub(super) fn face_style(color: &str) -> Style {
    let t = theme();
    if t.color { Style::default().fg(bot_color(color)) } else { t.text }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_appearance_sets_both_ink_and_ground_while_system_keeps_terminal_colors() {
        let light = build_theme(true, true, true);
        let dark = build_theme(false, true, true);
        assert_eq!(light.text.fg, Some(rgb(0x141414)));
        assert_eq!(light.text.bg, Some(rgb(0xFFFFFF)));
        assert_eq!(dark.text.fg, Some(rgb(0xF2F2F2)));
        assert_eq!(dark.text.bg, Some(rgb(0x0A0A0A)));
        for light in [true, false] {
            assert_eq!(build_theme(light, true, false).text, Style::default());
        }
        let mono = build_theme(false, false, false);
        assert_eq!(mono.text, Style::default());
        assert_eq!(mono.btn_primary.fg, None);
        assert_eq!(mono.btn_primary.bg, None);
    }
}
