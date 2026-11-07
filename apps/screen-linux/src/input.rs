//! Input events (see `host/src/screen/mod.rs`), injected through the `RemoteDesktop` portal or,
//! without one, through XTest (`x11.rs`).

use crate::portal::Portal;
use anyhow::{Result, anyhow};
use ashpd::desktop::remote_desktop::{Axis, KeyState};
use serde_json::Value;
use std::future::Future;
use std::time::Duration;

/// Linux input event codes (`linux/input-event-codes.h`).
pub const BTN_LEFT: i32 = 0x110;
pub const BTN_RIGHT: i32 = 0x111;
pub const BTN_MIDDLE: i32 = 0x112;

/// What a backend injects. Coordinates are logical pixels of the event's display; buttons are
/// Linux codes; keys are X keysyms.
pub trait Inject {
    fn motion(&self, x: f64, y: f64) -> impl Future<Output = Result<()>> + Send;
    fn button(&self, button: i32, pressed: bool) -> impl Future<Output = Result<()>> + Send;
    /// Press or release a key that stays held (modifiers).
    fn key(&self, sym: i32, pressed: bool) -> impl Future<Output = Result<()>> + Send;
    /// Fails if any of these held keys can't be pressed, before a shortcut presses some of them.
    fn check_keys(&self, _syms: &[i32]) -> impl Future<Output = Result<()>> + Send {
        async { Ok(()) }
    }
    /// Press and release one key.
    fn tap(&self, sym: i32) -> impl Future<Output = Result<()>> + Send;
    /// Smooth scroll in pixels.
    fn axis(&self, dx: f64, dy: f64) -> impl Future<Output = Result<()>> + Send;
    fn axis_discrete(&self, axis: Axis, steps: i32) -> impl Future<Output = Result<()>> + Send;
}

/// The portal's input for one stream.
pub struct PortalInput<'a> {
    pub portal: &'a Portal,
    pub node: u32,
}

fn key_state(pressed: bool) -> KeyState {
    if pressed {
        KeyState::Pressed
    } else {
        KeyState::Released
    }
}

impl Inject for PortalInput<'_> {
    async fn motion(&self, x: f64, y: f64) -> Result<()> {
        let (rd, session) = (self.portal.remote(), self.portal.session());
        Ok(rd
            .notify_pointer_motion_absolute(session, self.node, x, y, Default::default())
            .await?)
    }

    async fn button(&self, button: i32, pressed: bool) -> Result<()> {
        let (rd, session) = (self.portal.remote(), self.portal.session());
        Ok(rd
            .notify_pointer_button(session, button, key_state(pressed), Default::default())
            .await?)
    }

    async fn key(&self, sym: i32, pressed: bool) -> Result<()> {
        let (rd, session) = (self.portal.remote(), self.portal.session());
        Ok(rd
            .notify_keyboard_keysym(session, sym, key_state(pressed), Default::default())
            .await?)
    }

    async fn tap(&self, sym: i32) -> Result<()> {
        self.key(sym, true).await?;
        self.key(sym, false).await
    }

    async fn axis(&self, dx: f64, dy: f64) -> Result<()> {
        let (rd, session) = (self.portal.remote(), self.portal.session());
        Ok(rd
            .notify_pointer_axis(
                session,
                dx,
                dy,
                ashpd::desktop::remote_desktop::NotifyPointerAxisOptions::default()
                    .set_finish(true),
            )
            .await?)
    }

    async fn axis_discrete(&self, axis: Axis, steps: i32) -> Result<()> {
        let (rd, session) = (self.portal.remote(), self.portal.session());
        Ok(rd
            .notify_pointer_axis_discrete(session, axis, steps, Default::default())
            .await?)
    }
}

pub async fn perform(input: &impl Inject, event: &Value) -> Result<()> {
    let e = event;
    let num = |k: &str| e[k].as_f64();
    let point = || -> Result<(f64, f64)> {
        Ok((
            num("x").ok_or_else(|| anyhow!("x is required"))?,
            num("y").ok_or_else(|| anyhow!("y is required"))?,
        ))
    };
    let button = match e["button"].as_str() {
        Some("right") => BTN_RIGHT,
        Some("middle") => BTN_MIDDLE,
        _ => BTN_LEFT,
    };
    let modifiers: Vec<i32> = e["modifiers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| modifier(m.as_str()?))
        .collect();
    if !modifiers.is_empty() {
        input.check_keys(&modifiers).await?;
    }
    let hold = |down: bool| {
        let modifiers = modifiers.clone();
        async move {
            for m in modifiers {
                input.key(m, down).await?;
            }
            anyhow::Ok(())
        }
    };
    match e["type"].as_str().unwrap_or_default() {
        "move" => {
            let (px, py) = point()?;
            input.motion(px, py).await?;
        }
        "down" | "up" => {
            let (px, py) = point()?;
            input.motion(px, py).await?;
            input.button(button, e["type"] == "down").await?;
        }
        "click" => {
            let (px, py) = point()?;
            input.motion(px, py).await?;
            hold(true).await?;
            let count = e["count"].as_u64().unwrap_or(1).clamp(1, 3);
            for _ in 0..count {
                input.button(button, true).await?;
                input.button(button, false).await?;
            }
            hold(false).await?;
        }
        "drag" => {
            let (px, py) = point()?;
            let (tx, ty) = (
                num("toX").ok_or_else(|| anyhow!("toX is required"))?,
                num("toY").ok_or_else(|| anyhow!("toY is required"))?,
            );
            input.motion(px, py).await?;
            input.button(BTN_LEFT, true).await?;
            for i in 1..=20 {
                tokio::time::sleep(Duration::from_millis(15)).await;
                let t = f64::from(i) / 20.0;
                input.motion(px + (tx - px) * t, py + (ty - py) * t).await?;
            }
            input.button(BTN_LEFT, false).await?;
        }
        "scroll" => {
            let (px, py) = point()?;
            input.motion(px, py).await?;
            let (dx, dy) = (num("dx").unwrap_or(0.0), num("dy").unwrap_or(0.0));
            if e["units"] == "pixel" {
                input.axis(dx, dy).await?;
            } else {
                #[expect(clippy::cast_possible_truncation, reason = "a few scroll lines")]
                for (axis, steps) in [
                    (Axis::Vertical, dy.round() as i32),
                    (Axis::Horizontal, dx.round() as i32),
                ] {
                    if steps != 0 {
                        input.axis_discrete(axis, steps).await?;
                    }
                }
            }
        }
        "text" => {
            for c in e["text"].as_str().unwrap_or_default().chars() {
                input.tap(char_keysym(c)).await?;
            }
        }
        "key" => {
            let name = e["key"].as_str().unwrap_or_default();
            let sym = keysym(name).ok_or_else(|| anyhow!("unknown key {name}"))?;
            hold(true).await?;
            input.tap(sym).await?;
            hold(false).await?;
        }
        other => return Err(anyhow!("unknown input {other}")),
    }
    Ok(())
}

const SUPER_L: i32 = 0xffeb;
const ALT_L: i32 = 0xffe9;
const CONTROL_L: i32 = 0xffe3;
const SHIFT_L: i32 = 0xffe1;
/// Every key `modifiers` can hold.
pub const MODIFIER_KEYSYMS: [i32; 4] = [SUPER_L, ALT_L, CONTROL_L, SHIFT_L];

fn modifier(name: &str) -> Option<i32> {
    Some(match name {
        "cmd" => SUPER_L,
        "option" => ALT_L,
        "ctrl" => CONTROL_L,
        "shift" => SHIFT_L,
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
pub async fn open_app(name: &str) -> Result<()> {
    let id = name.trim().trim_end_matches(".desktop");
    let candidates = [
        id.to_owned(),
        id.to_lowercase(),
        id.to_lowercase().replace(' ', "-"),
    ];
    for c in &candidates {
        let ok = tokio::process::Command::new("gtk-launch")
            .arg(c)
            .status()
            .await
            .is_ok_and(|s| s.success());
        if ok {
            return Ok(());
        }
    }
    tokio::process::Command::new(id.to_lowercase())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| anyhow!("Couldn't find an app named {name}."))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keysyms() {
        assert_eq!(keysym("return"), Some(0xff0d));
        assert_eq!(keysym("f5"), Some(0xffc2));
        assert_eq!(keysym("a"), Some(0x61));
        assert_eq!(keysym("banana"), None);
        assert_eq!(char_keysym('é'), 0xe9);
        assert_eq!(char_keysym('中'), 0x0100_4e2d);
    }

    /// Records what `perform` injects: the backends only translate these calls. Keys in the
    /// second field are missing from the keyboard.
    #[derive(Default)]
    struct Record(std::sync::Mutex<Vec<String>>, Vec<i32>);

    impl Record {
        fn push(&self, call: String) {
            self.0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(call);
        }
    }

    #[allow(
        clippy::unused_async_trait_impl,
        reason = "a synchronous fake of an async trait"
    )]
    impl Inject for Record {
        async fn motion(&self, x: f64, y: f64) -> Result<()> {
            self.push(format!("move {x},{y}"));
            Ok(())
        }
        async fn button(&self, button: i32, pressed: bool) -> Result<()> {
            self.push(format!("button {button:#x} {pressed}"));
            Ok(())
        }
        async fn key(&self, sym: i32, pressed: bool) -> Result<()> {
            self.push(format!("key {sym:#x} {pressed}"));
            Ok(())
        }
        async fn check_keys(&self, syms: &[i32]) -> Result<()> {
            match syms.iter().find(|s| self.1.contains(s)) {
                Some(sym) => Err(anyhow!("key {sym:#x} isn't on this keyboard")),
                None => Ok(()),
            }
        }
        async fn tap(&self, sym: i32) -> Result<()> {
            self.push(format!("tap {sym:#x}"));
            Ok(())
        }
        async fn axis(&self, dx: f64, dy: f64) -> Result<()> {
            self.push(format!("axis {dx},{dy}"));
            Ok(())
        }
        async fn axis_discrete(&self, axis: Axis, steps: i32) -> Result<()> {
            self.push(format!("discrete {axis:?} {steps}"));
            Ok(())
        }
    }

    async fn calls(event: Value) -> Vec<String> {
        let record = Record::default();
        perform(&record, &event).await.expect("valid event");
        record
            .0
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[tokio::test]
    async fn events_become_the_same_injections_on_every_backend() {
        let click = serde_json::json!({"type": "click", "x": 10, "y": 20, "button": "right", "count": 2, "modifiers": ["ctrl"]});
        assert_eq!(
            calls(click).await,
            [
                "move 10,20",
                "key 0xffe3 true",
                "button 0x111 true",
                "button 0x111 false",
                "button 0x111 true",
                "button 0x111 false",
                "key 0xffe3 false",
            ]
        );
        let lines = serde_json::json!({"type": "scroll", "x": 1, "y": 2, "dy": 2, "dx": -1});
        assert_eq!(
            calls(lines).await,
            ["move 1,2", "discrete Vertical 2", "discrete Horizontal -1"]
        );
        let pixels =
            serde_json::json!({"type": "scroll", "x": 1, "y": 2, "dy": -90, "units": "pixel"});
        assert_eq!(calls(pixels).await, ["move 1,2", "axis 0,-90"]);
        let text = serde_json::json!({"type": "text", "text": "aé"});
        assert_eq!(calls(text).await, ["tap 0x61", "tap 0xe9"]);
        let key = serde_json::json!({"type": "key", "key": "return", "modifiers": ["shift", "fn"]});
        assert_eq!(
            calls(key).await,
            ["key 0xffe1 true", "tap 0xff0d", "key 0xffe1 false"]
        );
    }

    #[tokio::test]
    async fn a_shortcut_with_a_missing_modifier_presses_nothing() {
        let record = Record(std::sync::Mutex::default(), vec![0xffeb]);
        let shortcut = serde_json::json!({"type": "key", "key": "l", "modifiers": ["ctrl", "cmd"]});
        assert!(perform(&record, &shortcut).await.is_err());
        let click = serde_json::json!({"type": "click", "x": 1, "y": 2, "modifiers": ["cmd"]});
        assert!(perform(&record, &click).await.is_err());
        let pressed = record
            .0
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(pressed, Vec::<String>::new());
    }
}
