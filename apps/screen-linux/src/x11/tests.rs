#![allow(clippy::unwrap_used)]

use super::input::*;
use super::*;
use crate::input::{BTN_LEFT, Inject};
use ashpd::desktop::remote_desktop::Axis;

fn display() -> Display {
    Display {
        id: 0,
        node: 0,
        x: 0,
        y: 0,
        width: 1920.0,
        height: 1080.0,
    }
}

#[test]
fn points_map_to_root_pixels_inside_the_screen() {
    let d = display();
    assert_eq!(to_root(&d, 100.4, 200.6), (100, 201));
    assert_eq!(to_root(&d, -5.0, 5000.0), (0, 1079));
    assert_eq!(to_root(&d, 1920.0, 1080.0), (1919, 1079));
    let offset = Display {
        x: 1920,
        ..display()
    };
    assert_eq!(to_root(&offset, 10.0, 0.0), (1930, 0));
}

#[test]
fn buttons_and_wheel_follow_x_numbering() {
    assert_eq!(x_button(BTN_LEFT), 1);
    assert_eq!(x_button(BTN_MIDDLE), 2);
    assert_eq!(x_button(BTN_RIGHT), 3);
    assert_eq!(wheel_button(Axis::Vertical, 3), (5, 3));
    assert_eq!(wheel_button(Axis::Vertical, -2), (4, 2));
    assert_eq!(wheel_button(Axis::Horizontal, 1), (7, 1));
    assert_eq!(wheel_button(Axis::Horizontal, -1), (6, 1));
    assert_eq!(
        wheel_button(Axis::Vertical, i32::MAX),
        (5, MAX_WHEEL_CLICKS)
    );
    assert_eq!(
        wheel_button(Axis::Vertical, i32::MIN),
        (4, MAX_WHEEL_CLICKS)
    );
}

#[test]
fn smooth_scroll_accumulates_into_wheel_clicks() {
    let mut rest = 0.0;
    assert_eq!(wheel_steps(&mut rest, 15.0), 0);
    assert_eq!(wheel_steps(&mut rest, 30.0), 1);
    assert!((rest - 5.0).abs() < 1e-9);
    assert_eq!(wheel_steps(&mut rest, -125.0), -3);
    assert!(rest.abs() < 1e-9);
}

/// A private Xvfb, stopped on drop.
struct Xvfb(std::process::Child, String);

impl Xvfb {
    /// `None` (and a note in the test output) where Xvfb isn't installed.
    fn start() -> Option<Self> {
        use std::io::BufRead as _;
        let child = std::process::Command::new("Xvfb")
            .args([
                "-displayfd",
                "1",
                "-nolisten",
                "tcp",
                "-screen",
                "0",
                "800x600x24",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn();
        let Ok(mut child) = child else {
            eprintln!("skipped: Xvfb isn't installed");
            return None;
        };
        let mut number = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut number)
            .unwrap();
        Some(Self(child, format!(":{}", number.trim())))
    }
}

impl Drop for Xvfb {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn xtest_moves_the_pointer_and_types_symbols_missing_from_the_keymap() {
    let Some(server) = Xvfb::start() else { return };
    let x11 = X11::connect(server.1.clone()).await.unwrap();
    let screen = x11.display(None).await.unwrap();
    assert_eq!((screen.width, screen.height), (800.0, 600.0));
    let input = X11Input {
        x11: &x11,
        display: &screen,
    };
    input.motion(5000.0, 120.0).await.unwrap();
    let pointer = x11
        .call(|x| Ok(x.0.conn.query_pointer(x.0.root)?.reply()?))
        .await
        .unwrap();
    assert_eq!((pointer.root_x, pointer.root_y), (799, 120));

    // é isn't on Xvfb's US keymap: it is bound to a spare keycode for the tap, then unbound.
    let before = x11.call(X11::keymap).await.unwrap();
    assert_eq!(before.find(0xe9), None);
    input.tap(0xe9).await.unwrap();
    let after = x11.call(X11::keymap).await.unwrap();
    assert_eq!(after.find(0xe9), None);
    assert_eq!(after.syms, before.syms);

    // Cancelled mid-tap, after the binding and before the clearing (the tap waits twice
    // REMAP_SETTLE around the press), the spare keycode is still cleared, and the next tap
    // waits for that.
    let cancelled =
        tokio::time::timeout(REMAP_SETTLE + Duration::from_millis(5), input.tap(0x20ac)).await;
    assert!(cancelled.is_err(), "the tap should still be settling");
    input.tap(0xe9).await.unwrap();
    let after_cancel = x11.call(X11::keymap).await.unwrap();
    assert_eq!(after_cancel.syms, before.syms);

    // A shortcut whose modifier the keymap lacks fails before pressing anything.
    assert!(input.check_keys(&[0xffe3]).await.is_ok());
    assert!(input.check_keys(&[0xffe3, 0x0100_4e2d]).await.is_err());

    drop(server);
    assert!(x11.displays().await.is_empty());
    assert!(
        !x11.is_active(),
        "a vanished X server makes the backend inactive"
    );
}
