//! XTest input for one display: pointer, buttons, wheel, and keys by keysym, binding a spare
//! keycode for symbols the keymap lacks.

use super::keymap::{Keymap, XK_SHIFT_L};
use super::{X_TIMEOUT, X11, x_button};
use crate::input::Inject;
use crate::portal::Display;
use anyhow::{Result, anyhow};
use ashpd::desktop::remote_desktop::Axis;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use x11rb::protocol::xproto::{
    BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, KEY_PRESS_EVENT, KEY_RELEASE_EVENT,
    MOTION_NOTIFY_EVENT,
};

/// Wheel clicks one event may send; a bogus delta must not flood the X server.
pub(super) const MAX_WHEEL_CLICKS: u32 = 50;
/// Pixels of smooth scroll per wheel click. The phone sends 1.5× its pan distance.
const PIXELS_PER_WHEEL_STEP: f64 = 40.0;
/// Time for clients to read a temporary key mapping before and after it is used.
pub(super) const REMAP_SETTLE: Duration = Duration::from_millis(20);

/// XTest input for one display.
pub struct X11Input<'a> {
    pub x11: &'a X11,
    pub display: &'a Display,
}

impl Inject for X11Input<'_> {
    async fn motion(&self, x: f64, y: f64) -> Result<()> {
        let (rx, ry) = to_root(self.display, x, y);
        self.x11
            .call(move |x| {
                x.fake(MOTION_NOTIFY_EVENT, 0, rx, ry)?;
                x.flush()
            })
            .await
    }

    async fn button(&self, button: i32, pressed: bool) -> Result<()> {
        let kind = if pressed {
            BUTTON_PRESS_EVENT
        } else {
            BUTTON_RELEASE_EVENT
        };
        let button = x_button(button);
        self.x11
            .call(move |x| {
                x.fake(kind, button, 0, 0)?;
                x.flush()
            })
            .await
    }

    async fn key(&self, sym: i32, pressed: bool) -> Result<()> {
        let sym = u32::try_from(sym)?;
        let kind = if pressed {
            KEY_PRESS_EVENT
        } else {
            KEY_RELEASE_EVENT
        };
        self.x11
            .call(move |x| {
                let (code, _) = x
                    .keymap()?
                    .find(sym)
                    .ok_or_else(|| anyhow!("key {sym:#x} isn't on this keyboard"))?;
                x.fake(kind, code, 0, 0)?;
                x.flush()
            })
            .await
    }

    async fn check_keys(&self, syms: &[i32]) -> Result<()> {
        let syms = syms
            .iter()
            .map(|&s| u32::try_from(s))
            .collect::<Result<Vec<_>, _>>()?;
        let map = self.x11.call(X11::keymap).await?;
        match syms.into_iter().find(|&s| map.find(s).is_none()) {
            Some(sym) => Err(anyhow!("key {sym:#x} isn't on this keyboard")),
            None => Ok(()),
        }
    }

    async fn tap(&self, sym: i32) -> Result<()> {
        let x11 = self.x11;
        let sym = u32::try_from(sym)?;
        let taps = tokio::time::timeout(X_TIMEOUT, x11.0.keys.clone().lock_owned())
            .await
            .map_err(|_| anyhow!("an earlier key on the X display is still pending"))?;
        let map = x11.call(X11::keymap).await?;
        if let Some((code, shifted)) = map.find(sym) {
            let shift = shifted
                .then(|| map.find(XK_SHIFT_L).map(|(c, _)| c))
                .flatten();
            let codes: Vec<u8> = shift.into_iter().chain([code]).collect();
            return x11.call(move |x| x.press_release(&codes)).await;
        }
        // Not on the keyboard (accents, emoji): bind it to an unused keycode for one tap.
        let code = map
            .spare()
            .ok_or_else(|| anyhow!("no free keycode to type {sym:#x}"))?;
        let (row, per) = (map.bound_row(sym), map.per);
        // Restores the empty row even if this future is dropped mid-tap.
        let mut unbind = Unbind {
            x11: x11.clone(),
            code,
            map,
            taps: Some(taps),
            dropped: Arc::default(),
        };
        let dropped = unbind.dropped.clone();
        x11.call(move |x| {
            let _remap = x.remap();
            // Dropped before this ran: the clearing may already be done, so don't bind after it.
            if dropped.load(Ordering::SeqCst) {
                return Ok(());
            }
            x.bind(code, &row, per)
        })
        .await?;
        tokio::time::sleep(REMAP_SETTLE).await;
        x11.call(move |x| {
            x.press_release(&[code])?;
            x.sync()
        })
        .await?;
        tokio::time::sleep(REMAP_SETTLE).await;
        unbind.now().await
    }

    async fn axis(&self, dx: f64, dy: f64) -> Result<()> {
        let (sx, sy) = {
            let mut rest = self
                .x11
                .0
                .scroll
                .lock()
                .map_err(|_| anyhow!("scroll state poisoned"))?;
            (wheel_steps(&mut rest.0, dx), wheel_steps(&mut rest.1, dy))
        };
        self.axis_discrete(Axis::Vertical, sy).await?;
        self.axis_discrete(Axis::Horizontal, sx).await
    }

    async fn axis_discrete(&self, axis: Axis, steps: i32) -> Result<()> {
        if steps == 0 {
            return Ok(());
        }
        let (button, count) = wheel_button(axis, steps);
        self.x11
            .call(move |x| {
                for _ in 0..count {
                    x.fake(BUTTON_PRESS_EVENT, button, 0, 0)?;
                    x.fake(BUTTON_RELEASE_EVENT, button, 0, 0)?;
                }
                x.flush()
            })
            .await
    }
}

/// Clears a temporarily bound keycode. Dropped mid-tap, it clears it on a blocking thread and
/// keeps the next tap waiting until then.
struct Unbind {
    x11: X11,
    code: u8,
    map: Keymap,
    taps: Option<tokio::sync::OwnedMutexGuard<()>>,
    /// Set when the tap is dropped, so a binding still on its way is skipped.
    dropped: Arc<AtomicBool>,
}

impl Unbind {
    async fn now(&mut self) -> Result<()> {
        // The lock goes with the clearing, so even if this future is dropped while it runs, the
        // next tap starts after it.
        let taps = self.taps.take();
        let (code, row, per) = (self.code, self.map.empty_row(), self.map.per);
        self.x11
            .call(move |x| {
                let _remap = x.remap();
                let cleared = x.bind(code, &row, per);
                drop(taps);
                cleared
            })
            .await
    }
}

impl Drop for Unbind {
    fn drop(&mut self) {
        let Some(taps) = self.taps.take() else { return };
        self.dropped.store(true, Ordering::SeqCst);
        let (x11, code, row, per) = (
            self.x11.clone(),
            self.code,
            self.map.empty_row(),
            self.map.per,
        );
        let clear = move || {
            let _remap = x11.remap();
            if x11.bind(code, &row, per).is_err() {
                tracing::debug!(code, "couldn't clear a temporary key binding");
            }
            drop(taps);
        };
        match tokio::runtime::Handle::try_current() {
            Ok(rt) => drop(rt.spawn_blocking(clear)),
            Err(_) => clear(),
        }
    }
}

/// Logical display point to root-window pixel, clamped to the display.
#[expect(
    clippy::cast_possible_truncation,
    reason = "clamped to the X screen size"
)]
pub(super) fn to_root(d: &Display, x: f64, y: f64) -> (i16, i16) {
    let clamp = |v: f64, origin: i32, size: f64| {
        (f64::from(origin) + v.clamp(0.0, (size - 1.0).max(0.0)))
            .round()
            .clamp(0.0, f64::from(i16::MAX)) as i16
    };
    (clamp(x, d.x, d.width), clamp(y, d.y, d.height))
}

/// X wheel button and click count; positive steps scroll down or right, as on the portal.
pub(super) fn wheel_button(axis: Axis, steps: i32) -> (u8, u32) {
    let button = match (axis, steps > 0) {
        (Axis::Vertical, false) => 4,
        (Axis::Vertical, true) => 5,
        (Axis::Horizontal, false) => 6,
        (Axis::Horizontal, true) => 7,
    };
    (button, steps.unsigned_abs().min(MAX_WHEEL_CLICKS))
}

/// Whole wheel clicks in `rest + delta` pixels; keeps the remainder in `rest`.
#[expect(clippy::cast_possible_truncation, reason = "a few clicks per event")]
pub(super) fn wheel_steps(rest: &mut f64, delta: f64) -> i32 {
    let total = *rest + delta;
    let steps = (total / PIXELS_PER_WHEEL_STEP).trunc();
    *rest = total - steps * PIXELS_PER_WHEEL_STEP;
    steps as i32
}
