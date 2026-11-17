//! One X display shared without a portal (a cloud computer's virtual desktop, an X server with
//! no portals): capture through GStreamer's `ximagesrc` (one, shared: `capture.rs`), input through
//! XTest. There is nothing to approve: whoever may connect to the display may share it.

mod capture;
mod input;
mod keymap;

use crate::backend::{self, Display};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::sync::{Arc, Mutex, PoisonError};
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{ConnectionExt as _, Window};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

pub struct X11 {
    /// The display's name, e.g. `:20`.
    name: String,
    conn: RustConnection,
    root: Window,
    /// What input keeps between events; also keeps one event's key remapping from interleaving
    /// with another's or with clearing spare keys.
    input: Mutex<input::State>,
    /// The display's capture while some pipeline takes its frames.
    capture: Mutex<Option<capture::Capture>>,
}

/// The X display to share where the session has no screen sharing portals: `$DISPLAY`, unless the session is
/// Wayland (its X server shows only X apps). Never a scan of the X sockets: one can be another user's desktop, such
/// as a cloud provider's, which runs as its sudo user.
pub fn display_name() -> Option<String> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return None;
    }
    std::env::var("DISPLAY").ok().filter(|d| !d.is_empty())
}

impl X11 {
    /// Display `name`, or None while it can't be used (it may start after the helper).
    pub fn open(name: &str) -> Option<Arc<Self>> {
        Self::connect(name)
            .inspect_err(|error| {
                tracing::debug!(
                    display = name,
                    error = format!("{error:#}"),
                    "can't use this X display"
                );
            })
            .ok()
    }

    fn connect(name: &str) -> Result<Arc<Self>> {
        let (conn, screen) = x11rb::connect(Some(name))?;
        conn.xtest_get_version(2, 2)?
            .reply()
            .context("the X server has no XTest extension")?;
        let root = conn
            .setup()
            .roots
            .get(screen)
            .context("the X server has no such screen")?
            .root;
        let x = Arc::new(Self {
            name: name.to_owned(),
            conn,
            root,
            input: Mutex::default(),
            capture: Mutex::default(),
        });
        x.watch();
        x.sweep();
        tracing::info!(display = name, "sharing the X display");
        Ok(x)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The whole X screen, sized live (a VNC client may resize it).
    pub fn display(&self, id: Option<u64>) -> Result<Display> {
        if id.is_some_and(|id| id != 0) {
            bail!("no such display");
        }
        let root = self.conn.get_geometry(self.root)?.reply()?;
        Ok(Display {
            id: 0,
            x: 0,
            y: 0,
            width: f64::from(root.width),
            height: f64::from(root.height),
        })
    }

    pub fn status(&self) -> Value {
        match self.display(None) {
            Ok(d) => backend::status(&[d], true, true),
            Err(error) => {
                tracing::warn!(
                    display = self.name,
                    error = format!("{error:#}"),
                    "can't read the X screen size"
                );
                backend::status(&[], false, false)
            }
        }
    }

    /// The whole screen with its pointer, from the display's one capture (started if none runs).
    pub fn source(&self) -> Result<gst::Element> {
        let mut capture = self.capture.lock().unwrap_or_else(PoisonError::into_inner);
        let capture = match &mut *capture {
            Some(c) => c,
            none @ None => none.insert(capture::Capture::start(&self.name)?),
        };
        Ok(capture.output())
    }

    /// Once a second: clears spare keys apps have read, and stops the capture nobody watches.
    fn sweep(self: &Arc<Self>) {
        let x = self.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(keymap::HOLD);
                if let Err(error) = x.clear_spares() {
                    tracing::debug!(error = format!("{error:#}"), "can't clear spare keys");
                }
                let mut capture = x.capture.lock().unwrap_or_else(PoisonError::into_inner);
                if capture.as_ref().is_some_and(capture::Capture::idle) {
                    *capture = None;
                }
            }
        });
    }

    /// Exits when the X server goes away: the host starts the helper again, which then finds
    /// the server that replaced it. Also logs X errors from requests nobody waits on.
    fn watch(self: &Arc<Self>) {
        let x = self.clone();
        std::thread::spawn(move || {
            loop {
                match x.conn.wait_for_event() {
                    Ok(Event::Error(error)) => tracing::debug!(?error, "X error"),
                    Ok(_) => {}
                    Err(error) => {
                        tracing::error!(display = x.name, %error, "lost the X display");
                        std::process::exit(1);
                    }
                }
            }
        });
    }
}
