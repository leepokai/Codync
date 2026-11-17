//! Where capture and input come from: the xdg `RemoteDesktop` + `ScreenCast` portals (approved
//! once), or, where there are no such portals (a cloud computer's virtual desktop), the X display
//! in `$DISPLAY` straight away.

use crate::Helper;
use crate::input::Event;
use crate::portal::{self, Portal};
use crate::x11::{self, X11};
use anyhow::Result;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::MutexGuard;

/// A screen as the host sees it. `width`/`height` are logical pixels: the coordinate space of
/// input events and of the host's `Display` points.
#[derive(Clone, Debug)]
pub struct Display {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone)]
pub enum Backend {
    Portal(Portal),
    X11(Arc<X11>),
}

impl Backend {
    pub fn is_active(&self) -> bool {
        match self {
            Self::Portal(p) => p.is_active(),
            Self::X11(_) => true,
        }
    }

    pub fn status(&self) -> Value {
        match self {
            Self::Portal(p) => p.status(),
            Self::X11(x) => x.status(),
        }
    }

    pub fn display(&self, id: Option<u64>) -> Result<Display> {
        match self {
            Self::Portal(p) => p.display(id),
            Self::X11(x) => x.display(id),
        }
    }

    /// A live video source for one display; each pipeline gets its own.
    pub async fn source(&self, d: &Display) -> Result<gst::Element> {
        match self {
            Self::Portal(p) => p.source(d).await,
            Self::X11(x) => x.source(),
        }
    }

    pub async fn input(&self, d: &Display, event: &Event) -> Result<()> {
        match self {
            Self::Portal(p) => crate::portal_input::perform(p, d, event).await,
            Self::X11(x) => {
                let (x, d, event) = (x.clone(), d.clone(), event.clone());
                tokio::task::spawn_blocking(move || x.perform(&d, &event)).await?
            }
        }
    }

    /// A command for this screen's session: on X11 it gets the display.
    pub fn command(&self, program: &str) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(program);
        if let Self::X11(x) = self {
            command.env("DISPLAY", x.name());
        }
        command
    }
}

impl Helper {
    /// The backend, locked: the portal once set up; else, on a session without screen sharing
    /// portals, the X display once it's reachable (it may start after the helper).
    pub async fn current_backend(&self) -> MutexGuard<'_, Option<Backend>> {
        let mut backend = self.backend.lock().await;
        if backend.is_none()
            && let Some(name) = x11::display_name()
            && !self.portals().await
        {
            *backend = X11::open(&name).map(Backend::X11);
        }
        backend
    }

    /// Whether this session has screen sharing portals. Asked again until it does: they may come
    /// up with the desktop session after the helper started.
    async fn portals(&self) -> bool {
        if !self.portals.load(Ordering::Relaxed) && portal::available().await {
            self.portals.store(true, Ordering::Relaxed);
        }
        self.portals.load(Ordering::Relaxed)
    }

    /// Opens the portal session (a system dialog the first time). A plain X display has nothing
    /// to approve, unless the session's portals came up after the helper chose it.
    pub async fn request_permission(&self) -> Result<()> {
        let mut backend = self.current_backend().await;
        let portal = match &*backend {
            Some(Backend::X11(_)) if !self.portals().await => return Ok(()),
            Some(Backend::Portal(portal)) => Some(portal.clone()),
            Some(Backend::X11(_)) | None => None,
        };
        if portal.as_ref().is_none_or(|p| !p.has_input()) {
            let restore = portal.as_ref().is_none_or(|p| !p.is_active());
            let next = Portal::start(&crate::data_dir(), restore).await?;
            if let Some(previous) = portal
                && previous.is_active()
            {
                previous.session().close().await?;
            }
            if backend.is_some() {
                self.sessions.close_all().await;
            }
            *backend = Some(Backend::Portal(next));
        }
        Ok(())
    }
}

/// The status the host reads (`HelperStatus` in `host/src/screen/protocol.rs`).
pub fn status(displays: &[Display], capture: bool, input: bool) -> Value {
    json!({
        "platform": "linux",
        "permissionApp": "Codync Screen",
        "version": env!("CARGO_PKG_VERSION"),
        "capture": capture,
        "trickle": true,
        "input": input,
        "displays": displays.iter().map(|d| json!({
            "id": d.id,
            "name": format!("Display {}", d.id + 1),
            "width": d.width,
            "height": d.height,
            "main": d.id == 0,
        })).collect::<Vec<_>>(),
    })
}
