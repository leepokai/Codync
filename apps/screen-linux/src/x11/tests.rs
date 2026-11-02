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

#[tokio::test]
async fn caps_lock_doesnt_change_the_typed_letters() {
    use x11rb::protocol::Event;
    use x11rb::protocol::xproto::{
        CreateWindowAux, EventMask, InputFocus, KeyButMask, WindowClass,
    };
    let Some(server) = Xvfb::start() else { return };
    let x11 = X11::connect(server.1.clone()).await.unwrap();
    let screen = x11.display(None).await.unwrap();
    // Another client's focused window, which gets the key events.
    let (reader, _) = x11rb::connect(Some(&server.1)).unwrap();
    let window = reader.generate_id().unwrap();
    let root = reader.setup().roots[0].root;
    let aux = CreateWindowAux::new().event_mask(EventMask::KEY_PRESS);
    reader
        .create_window(
            0,
            window,
            root,
            0,
            0,
            10,
            10,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &aux,
        )
        .unwrap();
    reader.map_window(window).unwrap();
    reader
        .set_input_focus(InputFocus::PARENT, window, x11rb::CURRENT_TIME)
        .unwrap();
    reader.get_input_focus().unwrap().reply().unwrap();
    x11.call(|x| x.lock_keyboard(true, Group::M1))
        .await
        .unwrap();

    let input = X11Input {
        x11: &x11,
        display: &screen,
    };
    // `a` is on the keymap; `é` goes through the spare keycode.
    input.tap(0x61).await.unwrap();
    input.tap(0xe9).await.unwrap();
    // Clients apply the Lock in an event's state: with it, both would come out in capitals.
    let mut states = Vec::new();
    let deadline = std::time::Instant::now() + X_TIMEOUT;
    while states.len() < 2 && std::time::Instant::now() < deadline {
        match reader.poll_for_event().unwrap() {
            Some(Event::KeyPress(press)) => states.push(press.state),
            Some(_) => {}
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    assert_eq!(states.len(), 2, "both keys reached the focused window");
    assert!(states.iter().all(|s| !s.contains(KeyButMask::LOCK)));
    // Caps Lock is on again for whoever types at the desktop.
    let locked = x11
        .call(|x| {
            Ok(x.0
                .conn
                .xkb_get_state(xkb::ID::USE_CORE_KBD.into())?
                .reply()?)
        })
        .await
        .unwrap()
        .locked_mods;
    assert!(locked.contains(ModMask::LOCK));
}

#[tokio::test]
async fn a_streams_input_never_reaches_the_display_shared_after_it() {
    use crate::backend::Backend;
    use tokio::sync::Mutex as AsyncMutex;
    let Some(server) = Xvfb::start() else { return };
    let first = Backend::X11(X11::connect(server.1.clone()).await.unwrap());
    let helper = crate::Helper {
        portal: Arc::new(AsyncMutex::new(Some(first.clone()))),
        last_status: AsyncMutex::new(None),
        sessions: crate::sessions::Sessions::default(),
        out: tokio::sync::mpsc::unbounded_channel().0,
        rt: tokio::runtime::Handle::current(),
    };
    assert!(helper.input_backend(&first).await.is_ok());
    let paste = helper.x11_display(&first).await.unwrap();
    assert_eq!(paste.as_deref(), Some(server.1.as_str()));
    // A reconnection replaced the backend, here with the same display: what the ended stream
    // still sends is refused.
    let next = Backend::X11(X11::connect(server.1.clone()).await.unwrap());
    *helper.portal.lock().await = Some(next.clone());
    assert!(helper.input_backend(&first).await.is_err());
    assert!(helper.x11_display(&first).await.is_err());
    assert!(helper.input_backend(&next).await.is_ok());
}

#[tokio::test]
async fn paste_goes_to_the_shared_x_display() {
    if std::process::Command::new("xclip")
        .arg("-version")
        .output()
        .is_err()
    {
        eprintln!("skipped: xclip isn't installed");
        return;
    }
    let Some(server) = Xvfb::start() else { return };
    // The private Xvfb is never the runner's `DISPLAY`, like a display the helper found itself.
    crate::clipboard::set("pasted é", Some(&server.1))
        .await
        .unwrap();
    let read = std::process::Command::new("xclip")
        .args(["-display", &server.1, "-selection", "clipboard", "-o"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&read.stdout), "pasted é");
}

#[tokio::test]
async fn a_frozen_x_server_doesnt_keep_connection_attempts_open() {
    use std::io::Read as _;
    // Accepts connections and never answers, like an X server that froze. Over TCP, so no real
    // display's socket is involved: `127.0.0.1:N` is port 6000 + N.
    let (listener, number) = (100..200)
        .find_map(|n| {
            Some((
                std::net::TcpListener::bind(("127.0.0.1", 6000 + n)).ok()?,
                n,
            ))
        })
        .unwrap();
    let (closed, hangups) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let closed = closed.clone();
            std::thread::spawn(move || {
                // Gives up after a while, so a stuck attempt fails this test instead of hanging it.
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                if (&stream).read_to_end(&mut Vec::new()).is_ok() {
                    let _ = closed.send(());
                }
            });
        }
    });
    let name = format!("127.0.0.1:{number}");
    let error = X11::connect(name.clone()).await.err().unwrap();
    assert!(format!("{error:#}").contains("didn't answer"), "{error:#}");
    hangups
        .recv_timeout(Duration::from_secs(1))
        .expect("the timed-out attempt hung up its connection");
    // Its slot is free again: as many attempts as the limit allows still get through.
    let attempts = (0..MAX_HUNG_CONNECTS).map(|_| X11::connect(name.clone()));
    for error in futures_util::future::join_all(attempts).await {
        let error = error.err().unwrap();
        assert!(format!("{error:#}").contains("didn't answer"), "{error:#}");
    }
}
