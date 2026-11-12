//! Input events through the `RemoteDesktop` portal.

use crate::backend::Display;
use crate::input::{Button, DRAG_STEP, Event, Point, Press, ScrollUnit, drag_path};
use crate::portal::Portal;
use anyhow::Result;
use ashpd::desktop::remote_desktop::{Axis, KeyState, NotifyPointerAxisOptions};

/// Linux input event codes (`linux/input-event-codes.h`).
const BTN_LEFT: i32 = 0x110;
const BTN_RIGHT: i32 = 0x111;
const BTN_MIDDLE: i32 = 0x112;

fn code(button: Button) -> i32 {
    match button {
        Button::Left => BTN_LEFT,
        Button::Right => BTN_RIGHT,
        Button::Middle => BTN_MIDDLE,
    }
}

pub async fn perform(portal: &Portal, d: &Display, event: &Event) -> Result<()> {
    let rd = portal.remote();
    let session = portal.session();
    let node = portal.node(d)?;
    let move_to =
        |(x, y): Point| rd.notify_pointer_motion_absolute(session, node, x, y, Default::default());
    let press = |button: Button, state: KeyState| {
        rd.notify_pointer_button(session, code(button), state, Default::default())
    };
    let key = |sym: i32, state: KeyState| {
        rd.notify_keyboard_keysym(session, sym, state, Default::default())
    };
    let hold = |modifiers: &[i32], state: KeyState| {
        let modifiers = modifiers.to_vec();
        async move {
            for m in modifiers {
                key(m, state).await?;
            }
            anyhow::Ok(())
        }
    };
    match event {
        Event::Move(at) => move_to(*at).await?,
        Event::Down(Press {
            at,
            button,
            modifiers,
        }) => {
            move_to(*at).await?;
            hold(modifiers, KeyState::Pressed).await?;
            press(*button, KeyState::Pressed).await?;
        }
        Event::Up(Press {
            at,
            button,
            modifiers,
        }) => {
            move_to(*at).await?;
            press(*button, KeyState::Released).await?;
            hold(modifiers, KeyState::Released).await?;
        }
        Event::Click {
            at,
            button,
            count,
            modifiers,
        } => {
            move_to(*at).await?;
            hold(modifiers, KeyState::Pressed).await?;
            for _ in 0..*count {
                press(*button, KeyState::Pressed).await?;
                press(*button, KeyState::Released).await?;
            }
            hold(modifiers, KeyState::Released).await?;
        }
        Event::Drag { from, to } => {
            move_to(*from).await?;
            press(Button::Left, KeyState::Pressed).await?;
            for point in drag_path(*from, *to) {
                tokio::time::sleep(DRAG_STEP).await;
                move_to(point).await?;
            }
            press(Button::Left, KeyState::Released).await?;
        }
        Event::Scroll { at, dx, dy, unit } => {
            move_to(*at).await?;
            match unit {
                ScrollUnit::Pixel => {
                    rd.notify_pointer_axis(
                        session,
                        *dx,
                        *dy,
                        NotifyPointerAxisOptions::default().set_finish(true),
                    )
                    .await?;
                }
                ScrollUnit::Line => {
                    #[expect(clippy::cast_possible_truncation, reason = "a few scroll lines")]
                    for (axis, steps) in [
                        (Axis::Vertical, dy.round() as i32),
                        (Axis::Horizontal, dx.round() as i32),
                    ] {
                        if steps != 0 {
                            rd.notify_pointer_axis_discrete(
                                session,
                                axis,
                                steps,
                                Default::default(),
                            )
                            .await?;
                        }
                    }
                }
            }
        }
        Event::Text(text) => {
            for c in text.chars() {
                let sym = crate::input::char_keysym(c);
                key(sym, KeyState::Pressed).await?;
                key(sym, KeyState::Released).await?;
            }
        }
        Event::Key { sym, modifiers } => {
            hold(modifiers, KeyState::Pressed).await?;
            key(*sym, KeyState::Pressed).await?;
            key(*sym, KeyState::Released).await?;
            hold(modifiers, KeyState::Released).await?;
        }
    }
    Ok(())
}
