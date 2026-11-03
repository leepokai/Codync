//! The phone's touches and keys on the real desktop, through `SendInput` and the clipboard.

use crate::displays::Monitor;
use crate::input::{Button, Input, Key, is_extended, vk};
use anyhow::{Result, bail};
use std::time::Duration;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

/// Pixels the phone's pixel-unit scrolling needs for one wheel notch.
const PIXELS_PER_NOTCH: f64 = 40.0;
const WHEEL_DELTA: f64 = 120.0;
const CF_UNICODETEXT: u32 = 13;

/// Applies one event to `monitor`; blocks for the length of a drag.
pub fn perform(monitor: &Monitor, input: &Input) -> Result<()> {
    match input {
        Input::Move { x, y } => move_to(monitor, *x, *y),
        Input::Down { x, y, button } => {
            move_to(monitor, *x, *y)?;
            send(&[mouse(button_flag(*button, true), 0, 0, 0)])
        }
        Input::Up { x, y, button } => {
            move_to(monitor, *x, *y)?;
            send(&[mouse(button_flag(*button, false), 0, 0, 0)])
        }
        Input::Click {
            x,
            y,
            button,
            count,
            modifiers,
        } => {
            move_to(monitor, *x, *y)?;
            hold(modifiers, true)?;
            for _ in 0..*count {
                send(&[
                    mouse(button_flag(*button, true), 0, 0, 0),
                    mouse(button_flag(*button, false), 0, 0, 0),
                ])?;
            }
            hold(modifiers, false)
        }
        Input::Drag { x, y, to_x, to_y } => {
            move_to(monitor, *x, *y)?;
            send(&[mouse(button_flag(Button::Left, true), 0, 0, 0)])?;
            for step in 1..=20 {
                std::thread::sleep(Duration::from_millis(15));
                let t = f64::from(step) / 20.0;
                move_to(monitor, x + (to_x - x) * t, y + (to_y - y) * t)?;
            }
            send(&[mouse(button_flag(Button::Left, false), 0, 0, 0)])
        }
        Input::Scroll {
            x,
            y,
            dx,
            dy,
            pixels,
        } => {
            move_to(monitor, *x, *y)?;
            let notches = |v: f64| if *pixels { v / PIXELS_PER_NOTCH } else { v };
            // Windows counts a wheel turned away from the user as positive; the protocol counts down as positive.
            let vertical = wheel(-notches(*dy));
            let horizontal = wheel(notches(*dx));
            if vertical != 0 {
                send(&[mouse(MOUSEEVENTF_WHEEL, 0, 0, vertical)])?;
            }
            if horizontal != 0 {
                send(&[mouse(MOUSEEVENTF_HWHEEL, 0, 0, horizontal)])?;
            }
            Ok(())
        }
        Input::Text(text) => type_text(text),
        Input::Key { key, modifiers } => press(key, modifiers),
        Input::Clipboard(text) => set_clipboard(text),
    }
}

#[expect(clippy::cast_possible_truncation, reason = "a few wheel notches")]
fn wheel(notches: f64) -> i32 {
    (notches * WHEEL_DELTA).round() as i32
}

fn button_flag(button: Button, down: bool) -> MOUSE_EVENT_FLAGS {
    match (button, down) {
        (Button::Left, true) => MOUSEEVENTF_LEFTDOWN,
        (Button::Left, false) => MOUSEEVENTF_LEFTUP,
        (Button::Right, true) => MOUSEEVENTF_RIGHTDOWN,
        (Button::Right, false) => MOUSEEVENTF_RIGHTUP,
        (Button::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
        (Button::Middle, false) => MOUSEEVENTF_MIDDLEUP,
    }
}

fn mouse(flags: MOUSE_EVENT_FLAGS, dx: i32, dy: i32, data: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data.cast_unsigned(),
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn key_event(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send(inputs: &[INPUT]) -> Result<()> {
    // SAFETY: `inputs` is a valid slice of fully initialised INPUT structs.
    let sent = unsafe { SendInput(inputs, i32::try_from(size_of::<INPUT>())?) };
    if usize::try_from(sent)? != inputs.len() {
        bail!(
            "Windows refused the input (a window running as administrator, or the secure desktop, is in front)"
        );
    }
    Ok(())
}

/// Moves the pointer to `x`,`y` pixels on `monitor`.
fn move_to(monitor: &Monitor, x: f64, y: f64) -> Result<()> {
    // SAFETY: plain metrics queries.
    let (left, top, width, height) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    let (ax, ay) = absolute(monitor, x, y, (left, top, width, height));
    send(&[mouse(
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        ax,
        ay,
        0,
    )])
}

/// `SendInput`'s 0..=65535 coordinates over the virtual desktop `(left, top, width, height)`.
#[expect(clippy::cast_possible_truncation, reason = "clamped to 0..=65535")]
fn absolute(
    monitor: &Monitor,
    x: f64,
    y: f64,
    virtual_desktop: (i32, i32, i32, i32),
) -> (i32, i32) {
    let (left, top, width, height) = virtual_desktop;
    let px = (x.clamp(0.0, f64::from(monitor.width) - 1.0) + f64::from(monitor.left - left))
        / f64::from((width - 1).max(1));
    let py = (y.clamp(0.0, f64::from(monitor.height) - 1.0) + f64::from(monitor.top - top))
        / f64::from((height - 1).max(1));
    (
        (px * 65535.0).round().clamp(0.0, 65535.0) as i32,
        (py * 65535.0).round().clamp(0.0, 65535.0) as i32,
    )
}

fn hold(modifiers: &[u16], down: bool) -> Result<()> {
    for &m in modifiers {
        let flags = if down {
            KEYBD_EVENT_FLAGS(0)
        } else {
            KEYEVENTF_KEYUP
        };
        send(&[key_event(m, 0, flags | extended(m))])?;
    }
    Ok(())
}

fn extended(vk: u16) -> KEYBD_EVENT_FLAGS {
    if is_extended(vk) {
        KEYEVENTF_EXTENDEDKEY
    } else {
        KEYBD_EVENT_FLAGS(0)
    }
}

fn press(key: &Key, modifiers: &[u16]) -> Result<()> {
    let (vk, layout_modifiers) = match key {
        Key::Virtual(vk) => (*vk, Vec::new()),
        Key::Char(c) => {
            let mut utf16 = [0u16; 2];
            let unit = c.encode_utf16(&mut utf16);
            // SAFETY: layout lookup for a single UTF-16 unit.
            let scanned = if unit.len() == 1 {
                unsafe { VkKeyScanW(unit[0]) }
            } else {
                -1
            };
            if modifiers.is_empty() || scanned == -1 {
                // No shortcut: type the character itself, whatever the layout.
                return type_text(&c.to_string());
            }
            let scanned = scanned.cast_unsigned();
            let mut extra = Vec::new();
            for (bit, modifier) in [(1, vk::SHIFT), (2, vk::CONTROL), (4, vk::MENU)] {
                if scanned >> 8 & bit != 0 && !modifiers.contains(&modifier) {
                    extra.push(modifier);
                }
            }
            (scanned & 0xff, extra)
        }
    };
    hold(modifiers, true)?;
    hold(&layout_modifiers, true)?;
    let down = send(&[key_event(vk, 0, extended(vk))]);
    let up = send(&[key_event(vk, 0, extended(vk) | KEYEVENTF_KEYUP)]);
    hold(&layout_modifiers, false)?;
    hold(modifiers, false)?;
    down.and(up)
}

fn type_text(text: &str) -> Result<()> {
    for unit in text.encode_utf16() {
        let flags = KEYEVENTF_UNICODE;
        send(&[
            key_event(0, unit, flags),
            key_event(0, unit, flags | KEYEVENTF_KEYUP),
        ])?;
    }
    Ok(())
}

fn set_clipboard(text: &str) -> Result<()> {
    let units: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = units.len() * size_of::<u16>();
    // SAFETY: the global block is filled before ownership passes to the clipboard, and freed if that fails.
    unsafe {
        OpenClipboard(None)?;
        let result = (|| -> Result<()> {
            EmptyClipboard()?;
            let block: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, bytes)?;
            let locked = GlobalLock(block).cast::<u16>();
            if locked.is_null() {
                let _ = GlobalFree(Some(block));
                bail!("no memory for the clipboard");
            }
            std::ptr::copy_nonoverlapping(units.as_ptr(), locked, units.len());
            let _ = GlobalUnlock(block);
            if let Err(error) = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(block.0))) {
                let _ = GlobalFree(Some(block));
                return Err(error.into());
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(left: i32, top: i32, width: u32, height: u32) -> Monitor {
        Monitor {
            id: 1,
            name: String::new(),
            adapter: 0,
            output: 0,
            left,
            top,
            width,
            height,
        }
    }

    #[test]
    fn maps_monitor_pixels_onto_the_virtual_desktop() {
        // Two 1920x1080 monitors side by side; the second starts at x = 1920.
        let desktop = (0, 0, 3840, 1080);
        assert_eq!(
            absolute(&monitor(0, 0, 1920, 1080), 0.0, 0.0, desktop),
            (0, 0)
        );
        assert_eq!(
            absolute(&monitor(1920, 0, 1920, 1080), 1919.0, 1079.0, desktop),
            (65535, 65535)
        );
        // A monitor left of the primary has a negative origin; the virtual desktop starts there.
        assert_eq!(
            absolute(
                &monitor(-1920, 0, 1920, 1080),
                0.0,
                0.0,
                (-1920, 0, 3840, 1080)
            ),
            (0, 0)
        );
        // Points beyond the edge stay on the monitor.
        assert_eq!(
            absolute(&monitor(0, 0, 100, 100), 500.0, -5.0, (0, 0, 100, 100)),
            (65535, 0)
        );
    }
}
