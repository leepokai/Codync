//! Input through XTest: what the `RemoteDesktop` portal does, on a plain X display. Runs on a
//! blocking thread (X requests and drag pauses block).

use super::X11;
use super::keymap::{Bound, HOLD, Keymap, Stroke};
use crate::backend::Display;
use crate::input::{Button, DRAG_STEP, Event, Point, Press, ScrollUnit, char_keysym, drag_path};
use anyhow::{Context, Result};
use std::sync::{Arc, PoisonError};
use std::time::Instant;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, ConnectionExt as _, KEY_PRESS_EVENT,
    KEY_RELEASE_EVENT, MOTION_NOTIFY_EVENT,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{CURRENT_TIME, NONE};

/// Scroll pixels per wheel click: about what one click scrolls in Linux browsers (Chrome: 53).
const PIXELS_PER_CLICK: f64 = 50.0;
/// Wheel clicks one event sends at most.
const MAX_CLICKS: f64 = 50.0;
const SHIFT_L: u32 = 0xffe1;

/// What input keeps between events.
#[derive(Default)]
pub(super) struct State {
    /// Pixel scroll that doesn't add up to a wheel click yet, per axis.
    x: f64,
    y: f64,
    spares: Bound,
}

impl X11 {
    pub fn perform(&self, d: &Display, event: &Event) -> Result<()> {
        let mut state = self.input.lock().unwrap_or_else(PoisonError::into_inner);
        match event {
            Event::Move(at) => self.move_to(d, *at)?,
            Event::Down(Press {
                at,
                button,
                modifiers,
            }) => {
                self.move_to(d, *at)?;
                self.keys(KEY_PRESS_EVENT, &self.modifiers(modifiers)?)?;
                self.fake(BUTTON_PRESS_EVENT, number(*button))?;
            }
            Event::Up(Press {
                at,
                button,
                modifiers,
            }) => {
                self.move_to(d, *at)?;
                self.fake(BUTTON_RELEASE_EVENT, number(*button))?;
                self.keys(KEY_RELEASE_EVENT, &self.modifiers(modifiers)?)?;
            }
            Event::Click {
                at,
                button,
                count,
                modifiers,
            } => {
                let held = self.modifiers(modifiers)?;
                self.move_to(d, *at)?;
                self.keys(KEY_PRESS_EVENT, &held)?;
                for _ in 0..*count {
                    self.fake(BUTTON_PRESS_EVENT, number(*button))?;
                    self.fake(BUTTON_RELEASE_EVENT, number(*button))?;
                }
                self.keys(KEY_RELEASE_EVENT, &held)?;
            }
            Event::Drag { from, to } => {
                self.move_to(d, *from)?;
                self.fake(BUTTON_PRESS_EVENT, number(Button::Left))?;
                for point in drag_path(*from, *to) {
                    self.conn.flush()?;
                    std::thread::sleep(DRAG_STEP);
                    self.move_to(d, point)?;
                }
                self.fake(BUTTON_RELEASE_EVENT, number(Button::Left))?;
            }
            Event::Scroll { at, dx, dy, unit } => {
                self.move_to(d, *at)?;
                let vertical = clicks(&mut state.y, *dy, *unit);
                let horizontal = clicks(&mut state.x, *dx, *unit);
                // Wheel buttons: 4 up, 5 down, 6 left, 7 right.
                for (n, back, forward) in [(vertical, 4, 5), (horizontal, 6, 7)] {
                    let button = if n < 0 { back } else { forward };
                    for _ in 0..n.unsigned_abs() {
                        self.fake(BUTTON_PRESS_EVENT, button)?;
                        self.fake(BUTTON_RELEASE_EVENT, button)?;
                    }
                }
            }
            Event::Text(text) => {
                let mut map = self.keymap()?;
                text.chars().try_for_each(|c| {
                    let sym = char_keysym(c).cast_unsigned();
                    self.type_sym(&mut map, &mut state.spares, sym, &[])
                })?;
            }
            Event::Key { sym, modifiers } => {
                let mut map = self.keymap()?;
                let held = modifier_codes(&map, modifiers)?;
                self.type_sym(&mut map, &mut state.spares, sym.cast_unsigned(), &held)?;
            }
        }
        self.conn.sync()?;
        Ok(())
    }

    fn fake(&self, kind: u8, detail: u8) -> Result<()> {
        // Unchecked: an X error arrives as an event, which the watcher logs.
        drop(
            self.conn
                .xtest_fake_input(kind, detail, CURRENT_TIME, NONE, 0, 0, 0)?,
        );
        Ok(())
    }

    fn keys(&self, kind: u8, codes: &[u8]) -> Result<()> {
        if kind == KEY_RELEASE_EVENT {
            codes.iter().rev().try_for_each(|&c| self.fake(kind, c))
        } else {
            codes.iter().try_for_each(|&c| self.fake(kind, c))
        }
    }

    fn move_to(&self, d: &Display, (x, y): Point) -> Result<()> {
        let (x, y) = (pixel(x, d.width), pixel(y, d.height));
        drop(self.conn.xtest_fake_input(
            MOTION_NOTIFY_EVENT,
            0,
            CURRENT_TIME,
            self.root,
            x,
            y,
            0,
        )?);
        Ok(())
    }

    fn keymap(&self) -> Result<Keymap> {
        let setup = self.conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let reply = self
            .conn
            .get_keyboard_mapping(min, (max - min).saturating_add(1))?
            .reply()?;
        Ok(Keymap::new(min, reply.keysyms_per_keycode, reply.keysyms))
    }

    fn modifiers(&self, modifiers: &[i32]) -> Result<Vec<u8>> {
        if modifiers.is_empty() {
            return Ok(vec![]);
        }
        modifier_codes(&self.keymap()?, modifiers)
    }

    /// Presses and releases the key for `sym` while `held` keys are down.
    fn type_sym(&self, map: &mut Keymap, spares: &mut Bound, sym: u32, held: &[u8]) -> Result<()> {
        let stroke = map
            .stroke(sym, spares, Instant::now())
            .with_context(|| format!("no key on this X display can type keysym {sym:#x}"))?;
        let (code, shift) = match stroke {
            Stroke::Mapped { code, shift } => (code, shift),
            Stroke::Spare { code, syms, ready } => {
                let wait = ready.saturating_duration_since(Instant::now());
                if !wait.is_zero() {
                    self.conn.flush()?;
                    std::thread::sleep(wait);
                }
                drop(
                    self.conn
                        .change_keyboard_mapping(1, code, map.per(), &syms)?,
                );
                // The binding is in place before its key goes down.
                self.conn.sync()?;
                (code, false)
            }
        };
        let mut keys = held.to_vec();
        if shift {
            let (shift_code, _) = map.find(SHIFT_L).context("the keyboard has no Shift key")?;
            if !keys.contains(&shift_code) {
                keys.push(shift_code);
            }
        }
        self.keys(KEY_PRESS_EVENT, &keys)?;
        self.fake(KEY_PRESS_EVENT, code)?;
        self.fake(KEY_RELEASE_EVENT, code)?;
        self.keys(KEY_RELEASE_EVENT, &keys)
    }

    /// Clears spare keys once apps have read them, so the map ends up as it was. ponytail: a
    /// binding younger than `HOLD` when the helper exits stays; clear on exit if a restarted
    /// helper ever runs short of spare keys.
    pub(super) fn sweep_spares(self: &Arc<Self>) {
        let x = self.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(HOLD);
                if let Err(error) = x.clear_spares() {
                    tracing::debug!(error = format!("{error:#}"), "can't clear spare keys");
                }
            }
        });
    }

    fn clear_spares(&self) -> Result<()> {
        let mut state = self.input.lock().unwrap_or_else(PoisonError::into_inner);
        let codes = state.spares.expired(Instant::now());
        if codes.is_empty() {
            return Ok(());
        }
        let per = self.keymap()?.per();
        let none = vec![0; usize::from(per)];
        for code in codes {
            drop(self.conn.change_keyboard_mapping(1, code, per, &none)?);
        }
        self.conn.flush()?;
        Ok(())
    }
}

fn number(button: Button) -> u8 {
    match button {
        Button::Left => 1,
        Button::Middle => 2,
        Button::Right => 3,
    }
}

fn modifier_codes(map: &Keymap, modifiers: &[i32]) -> Result<Vec<u8>> {
    modifiers
        .iter()
        .map(|&m| {
            map.find(m.cast_unsigned())
                .map(|(code, _)| code)
                .with_context(|| format!("the keyboard has no key for modifier {m:#x}"))
        })
        .collect()
}

/// A display point as a root window pixel, kept on the screen.
#[expect(
    clippy::cast_possible_truncation,
    reason = "X screens are under 32768 pixels"
)]
fn pixel(v: f64, size: f64) -> i16 {
    v.clamp(0.0, (size - 1.0).max(0.0)).round() as i16
}

/// Wheel clicks for a scroll; pixel scroll that doesn't make a whole click carries over.
#[expect(clippy::cast_possible_truncation, reason = "clamped to MAX_CLICKS")]
fn clicks(carry: &mut f64, delta: f64, unit: ScrollUnit) -> i32 {
    if !delta.is_finite() {
        return 0;
    }
    let whole = match unit {
        ScrollUnit::Line => delta.round(),
        ScrollUnit::Pixel => {
            *carry += delta / PIXELS_PER_CLICK;
            let whole = carry.trunc();
            *carry -= whole;
            whole
        }
    };
    whole.clamp(-MAX_CLICKS, MAX_CLICKS) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_scroll_adds_up_to_wheel_clicks() {
        let mut carry = 0.0;
        assert_eq!(clicks(&mut carry, 125.0, ScrollUnit::Pixel), 2);
        assert_eq!(clicks(&mut carry, 25.0, ScrollUnit::Pixel), 1);
        assert_eq!(clicks(&mut carry, -25.0, ScrollUnit::Pixel), 0);
        assert_eq!(clicks(&mut carry, -75.0, ScrollUnit::Pixel), -2);
        assert_eq!(clicks(&mut carry, f64::NAN, ScrollUnit::Pixel), 0);
        assert_eq!(clicks(&mut carry, 50.0, ScrollUnit::Pixel), 1);
        assert_eq!(clicks(&mut carry, 2.6, ScrollUnit::Line), 3);
        assert_eq!(clicks(&mut carry, -1e9, ScrollUnit::Line), -50);
    }

    #[test]
    fn points_stay_on_the_screen() {
        assert_eq!(pixel(-5.0, 1280.0), 0);
        assert_eq!(pixel(639.6, 1280.0), 640);
        assert_eq!(pixel(5000.0, 1280.0), 1279);
    }
}
