//! Input events (see `host/src/screen.rs`) through the `RemoteDesktop` portal.

use crate::portal::{Display, Portal};
use anyhow::{Result, anyhow};
use ashpd::desktop::remote_desktop::{Axis, KeyState};
use serde_json::Value;
use std::time::Duration;

/// Linux input event codes (`linux/input-event-codes.h`).
const BTN_LEFT: i32 = 0x110;
const BTN_RIGHT: i32 = 0x111;
const BTN_MIDDLE: i32 = 0x112;

pub async fn perform(portal: &Portal, display: &Display, event: &Value) -> Result<()> {
    let (d, e) = (display, event);
    let rd = portal.remote();
    let session = portal.session();
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
    let move_to = |x: f64, y: f64| {
        rd.notify_pointer_motion_absolute(session, d.node, x, y, Default::default())
    };
    let hold = |down: bool| {
        let modifiers = modifiers.clone();
        async move {
            let state = if down {
                KeyState::Pressed
            } else {
                KeyState::Released
            };
            for m in modifiers {
                rd.notify_keyboard_keysym(session, m, state, Default::default())
                    .await?;
            }
            anyhow::Ok(())
        }
    };
    match e["type"].as_str().unwrap_or_default() {
        "move" => {
            let (px, py) = point()?;
            move_to(px, py).await?;
        }
        "down" | "up" => {
            let (px, py) = point()?;
            move_to(px, py).await?;
            let state = if e["type"] == "down" {
                KeyState::Pressed
            } else {
                KeyState::Released
            };
            rd.notify_pointer_button(session, button, state, Default::default())
                .await?;
        }
        "click" => {
            let (px, py) = point()?;
            move_to(px, py).await?;
            hold(true).await?;
            let count = e["count"].as_u64().unwrap_or(1).clamp(1, 3);
            for _ in 0..count {
                rd.notify_pointer_button(session, button, KeyState::Pressed, Default::default())
                    .await?;
                rd.notify_pointer_button(session, button, KeyState::Released, Default::default())
                    .await?;
            }
            hold(false).await?;
        }
        "drag" => {
            let (px, py) = point()?;
            let (tx, ty) = (
                num("toX").ok_or_else(|| anyhow!("toX is required"))?,
                num("toY").ok_or_else(|| anyhow!("toY is required"))?,
            );
            move_to(px, py).await?;
            rd.notify_pointer_button(session, BTN_LEFT, KeyState::Pressed, Default::default())
                .await?;
            for i in 1..=20 {
                tokio::time::sleep(Duration::from_millis(15)).await;
                let t = f64::from(i) / 20.0;
                move_to(px + (tx - px) * t, py + (ty - py) * t).await?;
            }
            rd.notify_pointer_button(session, BTN_LEFT, KeyState::Released, Default::default())
                .await?;
        }
        "scroll" => {
            let (px, py) = point()?;
            move_to(px, py).await?;
            let (dx, dy) = (num("dx").unwrap_or(0.0), num("dy").unwrap_or(0.0));
            if e["units"] == "pixel" {
                rd.notify_pointer_axis(
                    session,
                    dx,
                    dy,
                    ashpd::desktop::remote_desktop::NotifyPointerAxisOptions::default()
                        .set_finish(true),
                )
                .await?;
            } else {
                #[expect(clippy::cast_possible_truncation, reason = "a few scroll lines")]
                for (axis, steps) in [
                    (Axis::Vertical, dy.round() as i32),
                    (Axis::Horizontal, dx.round() as i32),
                ] {
                    if steps != 0 {
                        rd.notify_pointer_axis_discrete(session, axis, steps, Default::default())
                            .await?;
                    }
                }
            }
        }
        "text" => {
            for c in e["text"].as_str().unwrap_or_default().chars() {
                let sym = char_keysym(c);
                rd.notify_keyboard_keysym(session, sym, KeyState::Pressed, Default::default())
                    .await?;
                rd.notify_keyboard_keysym(session, sym, KeyState::Released, Default::default())
                    .await?;
            }
        }
        "key" => {
            let name = e["key"].as_str().unwrap_or_default();
            let sym = keysym(name).ok_or_else(|| anyhow!("unknown key {name}"))?;
            hold(true).await?;
            rd.notify_keyboard_keysym(session, sym, KeyState::Pressed, Default::default())
                .await?;
            rd.notify_keyboard_keysym(session, sym, KeyState::Released, Default::default())
                .await?;
            hold(false).await?;
        }
        other => return Err(anyhow!("unknown input {other}")),
    }
    Ok(())
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
}
