//! X11 fallback for desktops without the `RemoteDesktop` + `ScreenCast` portals (for example
//! Xfce on xrdp): `ximagesrc` captures the root window and XTest injects input. X11 has no
//! consent prompt, so this backend starts only after a local setup (`consent.rs`).

pub mod consent;
pub mod display;
mod input;
mod keymap;

pub use input::X11Input;

use crate::input::{BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, MODIFIER_KEYSYMS};
use crate::portal::Display;
use anyhow::{Context, Result, anyhow, bail};
use keymap::Keymap;
use std::net::Shutdown;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use x11rb::connection::Connection as _;
use x11rb::protocol::xproto::{
    BUTTON_RELEASE_EVENT, ConnectionExt as _, KEY_PRESS_EVENT, KEY_RELEASE_EVENT,
    MOTION_NOTIFY_EVENT, Window,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::reexports::x11rb_protocol::parse_display::parse_display;
use x11rb::reexports::x11rb_protocol::xauth::get_auth;
use x11rb::rust_connection::{DefaultStream, RustConnection};

/// A local X server answers in milliseconds; one that takes this long counts as gone.
const X_TIMEOUT: Duration = Duration::from_secs(2);
/// Connection attempts to one display in flight before new ones to it are refused. A timed-out
/// attempt hangs up its socket and ends; one stuck in `connect(2)` (a frozen server whose listen
/// queue is full) can't, and this keeps those from filling tokio's blocking pool.
const MAX_HUNG_CONNECTS: usize = 4;
/// The display of each connection attempt in flight.
static CONNECTING: Mutex<Vec<String>> = Mutex::new(Vec::new());

#[derive(Clone)]
pub struct X11(Arc<Inner>);

struct Inner {
    /// The X display name, for `ximagesrc`.
    name: String,
    conn: RustConnection,
    root: Window,
    active: AtomicBool,
    /// Failed reconnections since the connection broke.
    failures: AtomicU32,
    /// Serializes key taps, which may remap a spare keycode.
    keys: Arc<tokio::sync::Mutex<()>>,
    /// Orders a temporary binding and its clearing, which run on blocking threads.
    remap: Mutex<()>,
    /// The screen size last read, for input between status reads.
    size: Mutex<Vec<Display>>,
    /// Smooth-scroll remainder (dx, dy) below one wheel click.
    scroll: Mutex<(f64, f64)>,
}

impl X11 {
    /// Connects to an X display and checks XTest. Xwayland is turned down: it shows only the
    /// X clients of a Wayland desktop, which the portals share instead.
    pub async fn connect(name: String) -> Result<Self> {
        let attempt = Attempt::start(&name)?;
        let hangup = Arc::new(Hangup::default());
        // Also when the caller stops waiting, for example when the host link drops.
        let abandon = Abandon(&hangup);
        let shown = name.clone();
        let mut connecting = tokio::task::spawn_blocking({
            let hangup = hangup.clone();
            move || {
                let _attempt = attempt;
                Self::connect_blocking(name, &hangup)
            }
        });
        if let Ok(connected) = tokio::time::timeout(X_TIMEOUT, &mut connecting).await {
            hangup.keep();
            return connected?;
        }
        drop(abandon);
        // The hung-up thread ends; wait for it so its slot is free for the next attempt.
        let _ = tokio::time::timeout(X_TIMEOUT, connecting).await;
        bail!("the X display {shown} didn't answer");
    }

    /// `x11rb::connect`, handing the socket to `hangup` before anything waits on the server.
    fn connect_blocking(name: String, hangup: &Hangup) -> Result<Self> {
        let open = || -> Result<_> {
            let parsed = parse_display(Some(&name))?;
            let mut last = anyhow!("no address to connect to");
            for address in parsed.connect_instruction() {
                match DefaultStream::connect(&address) {
                    Ok((stream, (family, peer))) => {
                        hangup.watch(&stream)?;
                        let (auth_name, auth_data) = get_auth(family, &peer, parsed.display)
                            .ok()
                            .flatten()
                            .unwrap_or_default();
                        let screen = usize::from(parsed.screen);
                        let conn = RustConnection::connect_to_stream_with_auth_info(
                            stream, screen, auth_name, auth_data,
                        )?;
                        return Ok((conn, screen));
                    }
                    Err(error) => last = error.into(),
                }
            }
            Err(last)
        };
        let (conn, screen) = open().with_context(|| format!("couldn't open X display {name}"))?;
        let root = conn.setup().roots[screen].root;
        conn.xtest_get_version(2, 2)?
            .reply()
            .context("the X server has no XTest extension")?;
        if conn.query_extension(b"XWAYLAND")?.reply()?.present {
            bail!("{name} is Xwayland, part of a Wayland desktop");
        }
        Ok(Self(Arc::new(Inner {
            name,
            conn,
            root,
            active: AtomicBool::new(true),
            failures: AtomicU32::new(0),
            keys: Arc::default(),
            remap: Mutex::default(),
            size: Mutex::default(),
            scroll: Mutex::new((0.0, 0.0)),
        })))
    }

    pub fn is_active(&self) -> bool {
        self.0.active.load(Ordering::Relaxed)
    }

    pub fn name(&self) -> &str {
        &self.0.name
    }

    /// Whether `other` is this same connection.
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// Counts one failed reconnection and returns how many there have been.
    pub fn reconnect_failed(&self) -> u32 {
        self.0.failures.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Runs X requests on a blocking thread. A server that doesn't answer in time counts as
    /// gone, like a broken connection.
    async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Self) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        if !self.is_active() {
            bail!("the X display {} is unreachable", self.name());
        }
        let x = self.clone();
        let Ok(done) =
            tokio::time::timeout(X_TIMEOUT, tokio::task::spawn_blocking(move || f(&x))).await
        else {
            self.0.active.store(false, Ordering::Relaxed);
            bail!("the X display {} stopped answering", self.name());
        };
        done?
    }

    /// Marks the backend inactive once the X connection breaks (for example, xrdp ended the session).
    fn check<T>(&self, result: Result<T, impl Into<anyhow::Error>>) -> Result<T> {
        result.map_err(|e| {
            let e: anyhow::Error = e.into();
            let broken = e.downcast_ref::<x11rb::errors::ConnectionError>().is_some()
                || matches!(
                    e.downcast_ref::<x11rb::errors::ReplyError>(),
                    Some(x11rb::errors::ReplyError::ConnectionError(_))
                );
            if broken {
                self.0.active.store(false, Ordering::Relaxed);
            }
            e
        })
    }

    /// The whole root window as one display. xrdp can resize it, so it is read each time.
    pub async fn displays(&self) -> Vec<Display> {
        let geometry = self
            .call(|x| {
                let cookie = x.check(x.0.conn.get_geometry(x.0.root))?;
                x.check(cookie.reply())
            })
            .await;
        match geometry {
            Ok(g) => {
                let displays = vec![Display {
                    id: 0,
                    node: 0,
                    x: 0,
                    y: 0,
                    width: f64::from(g.width),
                    height: f64::from(g.height),
                }];
                if let Ok(mut size) = self.0.size.lock() {
                    size.clone_from(&displays);
                }
                displays
            }
            Err(error) => {
                tracing::debug!(
                    error = format!("{error:#}"),
                    "couldn't read the X screen size"
                );
                Vec::new()
            }
        }
    }

    pub async fn display(&self, id: Option<u64>) -> Result<Display> {
        let id = id.map_or(0, |i| u32::try_from(i).unwrap_or(u32::MAX));
        if !self.is_active() {
            bail!("the X display {} is unreachable", self.name());
        }
        self.displays()
            .await
            .into_iter()
            .find(|d| d.id == id)
            .ok_or_else(|| anyhow!("no such display"))
    }

    /// The display as last read, so input costs no extra round trip. A resize ends the streams
    /// at the next status read, every 3 s, which also refreshes this.
    pub async fn input_display(&self, id: u32) -> Result<Display> {
        let cached = self
            .0
            .size
            .lock()
            .ok()
            .and_then(|size| size.iter().find(|d| d.id == id).cloned());
        match cached {
            Some(display) => Ok(display),
            None => self.display(Some(u64::from(id))).await,
        }
    }

    /// Releases the modifiers and buttons input may hold. A connection that timed out mid-gesture
    /// can't send the release, and XTest doesn't release on disconnect; releasing a key that isn't
    /// down does nothing.
    pub async fn release_held(&self) -> Result<()> {
        self.call(|x| {
            let map = x.keymap()?;
            let modifiers = MODIFIER_KEYSYMS
                .iter()
                .filter_map(|&sym| map.find(u32::try_from(sym).ok()?).map(|(code, _)| code));
            for code in modifiers {
                x.fake(KEY_RELEASE_EVENT, code, 0, 0)?;
            }
            for button in [BTN_LEFT, BTN_MIDDLE, BTN_RIGHT] {
                x.fake(BUTTON_RELEASE_EVENT, x_button(button), 0, 0)?;
            }
            x.flush()
        })
        .await
    }

    fn fake(&self, kind: u8, detail: u8, x: i16, y: i16) -> Result<()> {
        let root = if kind == MOTION_NOTIFY_EVENT {
            self.0.root
        } else {
            x11rb::NONE
        };
        self.check(self.0.conn.xtest_fake_input(kind, detail, 0, root, x, y, 0))?;
        Ok(())
    }

    fn flush(&self) -> Result<()> {
        self.check(self.0.conn.flush())
    }

    /// Round trip: the server has processed every request sent so far.
    fn sync(&self) -> Result<()> {
        let cookie = self.check(self.0.conn.get_input_focus())?;
        self.check(cookie.reply())?;
        // Nothing reads X events here; drop the MappingNotify and error events that piled up.
        while let Ok(Some(_)) = self.0.conn.poll_for_event() {}
        Ok(())
    }

    fn keymap(&self) -> Result<Keymap> {
        let setup = self.0.conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let cookie = self.check(self.0.conn.get_keyboard_mapping(min, max - min + 1))?;
        let reply = self.check(cookie.reply())?;
        Ok(Keymap {
            min,
            per: reply.keysyms_per_keycode,
            syms: reply.keysyms,
        })
    }

    fn remap(&self) -> std::sync::MutexGuard<'_, ()> {
        self.0
            .remap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn bind(&self, code: u8, row: &[u32], per: u8) -> Result<()> {
        self.check(self.0.conn.change_keyboard_mapping(1, code, per, row))?;
        self.sync()
    }

    fn press_release(&self, codes: &[u8]) -> Result<()> {
        for &code in codes {
            self.fake(KEY_PRESS_EVENT, code, 0, 0)?;
        }
        for &code in codes.iter().rev() {
            self.fake(KEY_RELEASE_EVENT, code, 0, 0)?;
        }
        self.flush()
    }
}

/// One connection attempt in flight, counted for its display until its thread returns or panics.
struct Attempt(String);

impl Attempt {
    fn start(name: &str) -> Result<Self> {
        let mut connecting = CONNECTING.lock().unwrap_or_else(PoisonError::into_inner);
        if connecting.iter().filter(|n| *n == name).count() >= MAX_HUNG_CONNECTS {
            bail!("earlier connections to the X display {name} are still hanging");
        }
        connecting.push(name.to_owned());
        Ok(Self(name.to_owned()))
    }
}

impl Drop for Attempt {
    fn drop(&mut self) {
        let mut connecting = CONNECTING.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(i) = connecting.iter().position(|n| *n == self.0) {
            connecting.swap_remove(i);
        }
    }
}

/// The socket of a connection attempt. Shutting it down when the attempt is abandoned wakes the
/// thread waiting on a frozen X server, so the attempt ends instead of hanging.
#[derive(Default)]
struct Hangup(Mutex<HangupState>);

#[derive(Default)]
enum HangupState {
    #[default]
    Connecting,
    Open(OwnedFd),
    Done,
}

/// Hangs up an attempt that didn't finish when dropped.
struct Abandon<'a>(&'a Hangup);

impl Drop for Abandon<'_> {
    fn drop(&mut self) {
        self.0.now();
    }
}

impl Hangup {
    fn lock(&self) -> std::sync::MutexGuard<'_, HangupState> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Keeps a copy of the socket to shut down later; fails if the attempt was abandoned.
    fn watch(&self, stream: &DefaultStream) -> Result<()> {
        let mut state = self.lock();
        if matches!(*state, HangupState::Done) {
            bail!("the X display didn't answer in time");
        }
        *state = HangupState::Open(AsFd::as_fd(stream).try_clone_to_owned()?);
        Ok(())
    }

    /// The attempt finished: closes the copy and leaves the connection alone.
    fn keep(&self) {
        *self.lock() = HangupState::Done;
    }

    fn now(&self) {
        if let HangupState::Open(fd) = std::mem::replace(&mut *self.lock(), HangupState::Done) {
            // `shutdown(2)` works on any socket, TCP included; UnixStream only wraps the call.
            let _ = UnixStream::from(fd).shutdown(Shutdown::Both);
        }
    }
}

/// Linux button code to X button number.
fn x_button(code: i32) -> u8 {
    match code {
        BTN_MIDDLE => 2,
        BTN_RIGHT => 3,
        _ => 1,
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
