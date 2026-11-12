//! One X display shared without a portal (a cloud computer's virtual desktop, an X server with
//! no portals): capture through GStreamer's `ximagesrc`, input through XTest. There is nothing to
//! approve: whoever may connect to the display may share it.

mod input;
mod keymap;

use crate::backend::{self, Display};
use anyhow::{Context, Result, bail};
use gst::prelude::*;
use serde_json::Value;
use std::sync::{Arc, Mutex};
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
        });
        x.watch();
        x.sweep_spares();
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

    /// The whole screen with its pointer. ponytail: 30 fps fixed (each frame copies the full
    /// screen, no damage tracking); follow the viewer's `maxFramerate` if 60 is ever needed here.
    pub fn source(&self) -> Result<gst::Element> {
        let bin = gst::parse::bin_from_description(
            "ximagesrc name=x use-damage=false show-pointer=true ! capsfilter caps=video/x-raw,framerate=30/1",
            true,
        )
        .context("GStreamer's X11 capture (ximagesrc, gstreamer1.0-plugins-good) is missing")?;
        bin.by_name("x")
            .context("ximagesrc is missing from its bin")?
            .set_property("display-name", &self.name);
        Ok(bin.upcast())
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
