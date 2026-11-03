//! The phone's input events (see `host/src/screen/mod.rs`) decoded into Windows actions.

use anyhow::{Result, anyhow, bail};
use serde_json::Value;

/// Windows virtual-key codes (`winuser.h`) the protocol names.
pub mod vk {
    pub const BACK: u16 = 0x08;
    pub const TAB: u16 = 0x09;
    pub const RETURN: u16 = 0x0D;
    pub const SHIFT: u16 = 0x10;
    pub const CONTROL: u16 = 0x11;
    pub const MENU: u16 = 0x12;
    pub const ESCAPE: u16 = 0x1B;
    pub const SPACE: u16 = 0x20;
    pub const PRIOR: u16 = 0x21;
    pub const NEXT: u16 = 0x22;
    pub const END: u16 = 0x23;
    pub const HOME: u16 = 0x24;
    pub const LEFT: u16 = 0x25;
    pub const UP: u16 = 0x26;
    pub const RIGHT: u16 = 0x27;
    pub const DOWN: u16 = 0x28;
    pub const DELETE: u16 = 0x2E;
    pub const LWIN: u16 = 0x5B;
    pub const F1: u16 = 0x70;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}

/// A key to press: a named key, or a character the keyboard layout turns into one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    Virtual(u16),
    Char(char),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Move {
        x: f64,
        y: f64,
    },
    Down {
        x: f64,
        y: f64,
        button: Button,
    },
    Up {
        x: f64,
        y: f64,
        button: Button,
    },
    Click {
        x: f64,
        y: f64,
        button: Button,
        count: u32,
        modifiers: Vec<u16>,
    },
    Drag {
        x: f64,
        y: f64,
        to_x: f64,
        to_y: f64,
    },
    Scroll {
        x: f64,
        y: f64,
        dx: f64,
        dy: f64,
        pixels: bool,
    },
    Text(String),
    Key {
        key: Key,
        modifiers: Vec<u16>,
    },
    Clipboard(String),
}

impl Input {
    pub fn parse(event: &Value) -> Result<Self> {
        let num = |key: &str| event[key].as_f64();
        let point = || -> Result<(f64, f64)> {
            Ok((
                num("x").ok_or_else(|| anyhow!("x is required"))?,
                num("y").ok_or_else(|| anyhow!("y is required"))?,
            ))
        };
        let button = match event["button"].as_str() {
            Some("right") => Button::Right,
            Some("middle") => Button::Middle,
            _ => Button::Left,
        };
        let modifiers = event["modifiers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| modifier(m.as_str()?))
            .collect();
        Ok(match event["type"].as_str().unwrap_or_default() {
            "move" => {
                let (x, y) = point()?;
                Self::Move { x, y }
            }
            "down" | "up" => {
                let (x, y) = point()?;
                if event["type"] == "down" {
                    Self::Down { x, y, button }
                } else {
                    Self::Up { x, y, button }
                }
            }
            "click" => {
                let (x, y) = point()?;
                let count =
                    u32::try_from(event["count"].as_u64().unwrap_or(1).clamp(1, 3)).unwrap_or(1);
                Self::Click {
                    x,
                    y,
                    button,
                    count,
                    modifiers,
                }
            }
            "drag" => {
                let (x, y) = point()?;
                Self::Drag {
                    x,
                    y,
                    to_x: num("toX").ok_or_else(|| anyhow!("toX is required"))?,
                    to_y: num("toY").ok_or_else(|| anyhow!("toY is required"))?,
                }
            }
            "scroll" => {
                let (x, y) = point()?;
                Self::Scroll {
                    x,
                    y,
                    dx: num("dx").unwrap_or(0.0),
                    dy: num("dy").unwrap_or(0.0),
                    pixels: event["units"] == "pixel",
                }
            }
            "text" => Self::Text(event["text"].as_str().unwrap_or_default().to_owned()),
            "key" => {
                let name = event["key"].as_str().unwrap_or_default();
                Self::Key {
                    key: key(name).ok_or_else(|| anyhow!("unknown key {name}"))?,
                    modifiers,
                }
            }
            "clipboard" => Self::Clipboard(event["text"].as_str().unwrap_or_default().to_owned()),
            other => bail!("unknown input {other}"),
        })
    }
}

fn modifier(name: &str) -> Option<u16> {
    Some(match name {
        "cmd" => vk::LWIN,
        "option" => vk::MENU,
        "ctrl" => vk::CONTROL,
        "shift" => vk::SHIFT,
        _ => return None,
    })
}

/// The protocol names keys like a Mac keyboard: `delete` is Backspace.
pub fn key(name: &str) -> Option<Key> {
    let named = match name {
        "return" => vk::RETURN,
        "tab" => vk::TAB,
        "space" => vk::SPACE,
        "escape" => vk::ESCAPE,
        "delete" => vk::BACK,
        "forwardDelete" => vk::DELETE,
        "left" => vk::LEFT,
        "up" => vk::UP,
        "right" => vk::RIGHT,
        "down" => vk::DOWN,
        "home" => vk::HOME,
        "end" => vk::END,
        "pageUp" => vk::PRIOR,
        "pageDown" => vk::NEXT,
        _ => {
            if let Some(n) = name
                .strip_prefix('f')
                .and_then(|n| n.parse::<u16>().ok())
                .filter(|n| (1..=12).contains(n))
            {
                return Some(Key::Virtual(vk::F1 + n - 1));
            }
            let mut chars = name.chars();
            return match (chars.next(), chars.next()) {
                (Some(c), None) => Some(Key::Char(c)),
                _ => None,
            };
        }
    };
    Some(Key::Virtual(named))
}

/// Keys that need the extended-key flag to be the arrow/navigation key rather than the numpad's.
pub fn is_extended(vk: u16) -> bool {
    matches!(
        vk,
        self::vk::PRIOR..=self::vk::DOWN | self::vk::DELETE | self::vk::LWIN
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn names_map_to_virtual_keys() {
        assert_eq!(key("return"), Some(Key::Virtual(vk::RETURN)));
        assert_eq!(key("delete"), Some(Key::Virtual(vk::BACK)));
        assert_eq!(key("f5"), Some(Key::Virtual(0x74)));
        assert_eq!(key("a"), Some(Key::Char('a')));
        assert_eq!(key("banana"), None);
        assert!(is_extended(vk::LEFT) && is_extended(vk::DELETE) && !is_extended(vk::RETURN));
    }

    #[test]
    fn decodes_events() {
        let click = Input::parse(&json!({"type": "click", "x": 10, "y": 20.5, "button": "right", "count": 9, "modifiers": ["cmd", "bogus", "shift"]})).unwrap();
        assert_eq!(
            click,
            Input::Click {
                x: 10.0,
                y: 20.5,
                button: Button::Right,
                count: 3,
                modifiers: vec![vk::LWIN, vk::SHIFT]
            }
        );
        assert_eq!(
            Input::parse(&json!({"type": "text", "text": "hi"})).unwrap(),
            Input::Text("hi".into())
        );
        assert!(Input::parse(&json!({"type": "move", "x": 1})).is_err());
        assert!(Input::parse(&json!({"type": "key", "key": "nope"})).is_err());
        assert!(Input::parse(&json!({"type": "wiggle"})).is_err());
        let scroll =
            Input::parse(&json!({"type": "scroll", "x": 0, "y": 0, "dy": -3, "units": "pixel"}))
                .unwrap();
        assert_eq!(
            scroll,
            Input::Scroll {
                x: 0.0,
                y: 0.0,
                dx: 0.0,
                dy: -3.0,
                pixels: true
            }
        );
    }
}
