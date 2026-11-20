//! Input events (see `host/src/screen/mod.rs`), parsed once for every backend, and the X
//! keysyms both backends type with.

use crate::backend::{Backend, Display};
use anyhow::{Result, anyhow, bail};
use serde_json::Value;
use std::time::Duration;

/// A position in display points.
pub type Point = (f64, f64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollUnit {
    Line,
    Pixel,
}

/// A button pressed or let go at a point, with modifiers (X keysyms).
#[derive(Clone, Debug, PartialEq)]
pub struct Press {
    pub at: Point,
    pub button: Button,
    pub modifiers: Vec<i32>,
}

/// One input event. `modifiers` are X keysyms, held around the action.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Move(Point),
    /// A button pressed with its modifiers until the matching `Up` (the desktop's real mouse, touch drags).
    Down(Press),
    /// Lets go of the button and of the modifiers its `Down` pressed (`Helper::input` puts those in).
    Up(Press),
    Click {
        at: Point,
        button: Button,
        count: u64,
        modifiers: Vec<i32>,
    },
    Drag {
        from: Point,
        to: Point,
    },
    Scroll {
        at: Point,
        dx: f64,
        dy: f64,
        unit: ScrollUnit,
    },
    Text(String),
    Key {
        sym: i32,
        modifiers: Vec<i32>,
    },
}

impl Event {
    pub fn parse(e: &Value) -> Result<Self> {
        let num = |k: &str| e[k].as_f64().ok_or_else(|| anyhow!("{k} is required"));
        let at = || anyhow::Ok((num("x")?, num("y")?));
        let button = match e["button"].as_str() {
            Some("right") => Button::Right,
            Some("middle") => Button::Middle,
            _ => Button::Left,
        };
        let modifiers = || {
            e["modifiers"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|m| modifier(m.as_str()?))
                .collect()
        };
        Ok(match e["type"].as_str().unwrap_or_default() {
            "move" => Self::Move(at()?),
            "down" => Self::Down(Press {
                at: at()?,
                button,
                modifiers: modifiers(),
            }),
            "up" => Self::Up(Press {
                at: at()?,
                button,
                modifiers: modifiers(),
            }),
            "click" => Self::Click {
                at: at()?,
                button,
                count: e["count"].as_u64().unwrap_or(1).clamp(1, 3),
                modifiers: modifiers(),
            },
            "drag" => Self::Drag {
                from: at()?,
                to: (num("toX")?, num("toY")?),
            },
            "scroll" => Self::Scroll {
                at: at()?,
                dx: e["dx"].as_f64().unwrap_or(0.0),
                dy: e["dy"].as_f64().unwrap_or(0.0),
                unit: if e["units"] == "pixel" {
                    ScrollUnit::Pixel
                } else {
                    ScrollUnit::Line
                },
            },
            "text" => Self::Text(e["text"].as_str().unwrap_or_default().to_owned()),
            "key" => {
                let name = e["key"].as_str().unwrap_or_default();
                Self::Key {
                    sym: keysym(name).ok_or_else(|| anyhow!("unknown key {name}"))?,
                    modifiers: modifiers(),
                }
            }
            other => bail!("unknown input {other}"),
        })
    }
}

/// Remembers a `Down` (on its display) in `held`, and gives its `Up` the modifiers that `Down`
/// pressed in place of its own.
pub fn track_held(held: &mut Option<(Display, Press)>, d: &Display, event: Event) -> Event {
    match event {
        Event::Down(press) => {
            *held = Some((d.clone(), press.clone()));
            Event::Down(press)
        }
        Event::Up(press) => Event::Up(Press {
            modifiers: held
                .take()
                .map(|(_, down)| down.modifiers)
                .unwrap_or_default(),
            ..press
        }),
        other => other,
    }
}

/// Pause between a drag's steps, so apps see the motion.
pub const DRAG_STEP: Duration = Duration::from_millis(15);

/// The points a drag passes through after its start, `DRAG_STEP` apart.
pub fn drag_path(from: Point, to: Point) -> impl Iterator<Item = Point> {
    (1..=20).map(move |i| {
        let t = f64::from(i) / 20.0;
        (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t)
    })
}

fn modifier(name: &str) -> Option<i32> {
    Some(match name {
        "cmd" => 0xffeb,    // Super_L
        "option" => 0xffe9, // Alt_L
        "ctrl" => 0xffe3,   // Control_L
        "shift" => 0xffe1,  // Shift_L
        _ => return None,
    })
}

/// X keysym for a key name from the shared protocol, or a single character.
pub fn keysym(name: &str) -> Option<i32> {
    let named = match name {
        "return" => 0xff0d,
        "tab" => 0xff09,
        "space" => 0x20,
        "escape" => 0xff1b,
        "delete" => 0xff08, // BackSpace: the protocol names keys like a Mac keyboard
        "forwardDelete" => 0xffff,
        "left" => 0xff51,
        "up" => 0xff52,
        "right" => 0xff53,
        "down" => 0xff54,
        "home" => 0xff50,
        "end" => 0xff57,
        "pageUp" => 0xff55,
        "pageDown" => 0xff56,
        _ => {
            if let Some(n) = name
                .strip_prefix('f')
                .and_then(|n| n.parse::<i32>().ok())
                .filter(|n| (1..=12).contains(n))
            {
                return Some(0xffbe + n - 1);
            }
            let mut chars = name.chars();
            return match (chars.next(), chars.next()) {
                (Some(c), None) => Some(char_keysym(c)),
                _ => None,
            };
        }
    };
    Some(named)
}

/// Latin-1 keysyms are the code point; everything else is the Unicode keysym range.
pub fn char_keysym(c: char) -> i32 {
    let cp = u32::from(c);
    let sym = match cp {
        0x0a | 0x0d => 0xff0d,
        0x09 => 0xff09,
        0x20..=0x7e | 0xa0..=0xff => cp,
        _ => 0x0100_0000 + cp,
    };
    i32::try_from(sym).unwrap_or(0)
}

/// Opens an app by desktop-file id or name, else runs it as a command.
pub async fn open_app(backend: &Backend, name: &str) -> Result<()> {
    let id = name.trim().trim_end_matches(".desktop");
    let candidates = [
        id.to_owned(),
        id.to_lowercase(),
        id.to_lowercase().replace(' ', "-"),
    ];
    for c in &candidates {
        let ok = backend
            .command("gtk-launch")
            .arg(c)
            .status()
            .await
            .is_ok_and(|s| s.success());
        if ok {
            return Ok(());
        }
    }
    backend
        .command(&id.to_lowercase())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| anyhow!("Couldn't find an app named {name}."))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use serde_json::json;

    #[test]
    fn keysyms() {
        assert_eq!(keysym("return"), Some(0xff0d));
        assert_eq!(keysym("f5"), Some(0xffc2));
        assert_eq!(keysym("a"), Some(0x61));
        assert_eq!(keysym("banana"), None);
        assert_eq!(char_keysym('é'), 0xe9);
        assert_eq!(char_keysym('中'), 0x0100_4e2d);
    }

    #[test]
    fn events_parse_with_their_modifiers() {
        let down = json!({"type": "down", "x": 1, "y": 2, "button": "right", "count": 2, "modifiers": ["shift", "bogus"]});
        assert_eq!(
            Event::parse(&down).unwrap(),
            Event::Down(Press {
                at: (1.0, 2.0),
                button: Button::Right,
                modifiers: vec![0xffe1],
            })
        );
        let click = json!({"type": "click", "x": 1, "y": 2, "count": 9});
        assert!(matches!(
            Event::parse(&click).unwrap(),
            Event::Click { count: 3, .. }
        ));
        let scroll = json!({"type": "scroll", "x": 0, "y": 0, "dy": 30, "units": "pixel"});
        assert!(matches!(
            Event::parse(&scroll).unwrap(),
            Event::Scroll {
                unit: ScrollUnit::Pixel,
                ..
            }
        ));
        assert!(Event::parse(&json!({"type": "move", "x": 1})).is_err());
        assert!(Event::parse(&json!({"type": "key", "key": "banana"})).is_err());
    }

    #[test]
    fn an_up_lets_go_of_what_its_down_pressed() {
        let d = Display {
            id: 0,
            x: 0,
            y: 0,
            width: 10.0,
            height: 10.0,
        };
        let press = |modifiers: Vec<i32>| Press {
            at: (1.0, 1.0),
            button: Button::Left,
            modifiers,
        };
        let mut held = None;
        let down = track_held(&mut held, &d, Event::Down(press(vec![0xffe3])));
        assert_eq!(down, Event::Down(press(vec![0xffe3])));
        // Ctrl was let go before the button: the up carries none, and still lets go of Ctrl.
        let up = track_held(&mut held, &d, Event::Up(press(vec![])));
        assert_eq!(up, Event::Up(press(vec![0xffe3])));
        assert!(held.is_none());
        let up = track_held(&mut held, &d, Event::Up(press(vec![0xffe1])));
        assert_eq!(up, Event::Up(press(vec![])), "nothing held, nothing let go");
    }
}
